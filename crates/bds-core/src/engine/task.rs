use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Unique task identifier.
pub type TaskId = u64;

/// Task status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

/// Immutable task state exposed to UI consumers.
#[derive(Debug, Clone)]
pub struct TaskSnapshot {
    pub id: TaskId,
    pub label: String,
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub status: TaskStatus,
    pub progress: Option<f32>,
    pub message: Option<String>,
    pub created_at: Instant,
}

/// Entry tracking a task.
#[derive(Debug)]
struct TaskEntry {
    id: TaskId,
    label: String,
    group_id: Option<String>,
    group_name: Option<String>,
    status: TaskStatus,
    cancel_flag: Arc<AtomicBool>,
    progress: Option<f32>,
    message: Option<String>,
    created_at: Instant,
    finished_at: Option<Instant>,
    last_progress_report: Option<Instant>,
}

/// Manages concurrent tasks with a max concurrency limit and FIFO queue.
pub struct TaskManager {
    max_concurrent: usize,
    next_id: Mutex<TaskId>,
    tasks: Mutex<Vec<TaskEntry>>,
    state_changed: Condvar,
}

impl TaskManager {
    /// Create a new task manager with the given concurrency limit.
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent,
            next_id: Mutex::new(1),
            tasks: Mutex::new(Vec::new()),
            state_changed: Condvar::new(),
        }
    }

    /// Submit a new task. Returns its unique identifier.
    pub fn submit(&self, label: &str) -> TaskId {
        let mut next = self.next_id.lock().unwrap();
        let id = *next;
        *next += 1;

        let entry = TaskEntry {
            id,
            label: label.to_owned(),
            group_id: None,
            group_name: None,
            status: TaskStatus::Pending,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            progress: None,
            message: None,
            created_at: Instant::now(),
            finished_at: None,
            last_progress_report: None,
        };

        let mut tasks = self.tasks.lock().unwrap();
        tasks.push(entry);
        // Auto-start if under capacity
        let running = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Running)
            .count();
        if running < self.max_concurrent
            && let Some(t) = tasks
                .iter_mut()
                .find(|t| t.id == id && t.status == TaskStatus::Pending)
        {
            t.status = TaskStatus::Running;
        }
        id
    }

    /// Submit a new task within a group. Returns its unique identifier.
    pub fn submit_grouped(&self, label: &str, group_id: &str, group_name: &str) -> TaskId {
        let id = self.submit(label);
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(entry) = tasks.iter_mut().find(|t| t.id == id) {
            entry.group_id = Some(group_id.to_owned());
            entry.group_name = Some(group_name.to_owned());
        }
        id
    }

    /// Block a worker until its task may run. Returns false if cancelled.
    pub fn wait_until_runnable(&self, task_id: TaskId) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        loop {
            match tasks
                .iter()
                .find(|task| task.id == task_id)
                .map(|task| &task.status)
            {
                Some(TaskStatus::Running) => return true,
                Some(TaskStatus::Pending) => tasks = self.state_changed.wait(tasks).unwrap(),
                _ => return false,
            }
        }
    }

    /// Mark a task as completed.
    pub fn complete(&self, task_id: TaskId) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(entry) = tasks.iter_mut().find(|t| t.id == task_id)
            && matches!(entry.status, TaskStatus::Running)
        {
            entry.status = TaskStatus::Completed;
            entry.progress = Some(1.0);
            entry.finished_at = Some(Instant::now());
        }
        Self::promote_next(&mut tasks, self.max_concurrent);
        self.state_changed.notify_all();
    }

    /// Mark a task as failed with an error message.
    pub fn fail(&self, task_id: TaskId, error: String) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(entry) = tasks.iter_mut().find(|t| t.id == task_id)
            && matches!(entry.status, TaskStatus::Running)
        {
            entry.message = Some(error.clone());
            entry.status = TaskStatus::Failed(error);
            entry.finished_at = Some(Instant::now());
        }
        Self::promote_next(&mut tasks, self.max_concurrent);
        self.state_changed.notify_all();
    }

    /// Cancel a task by setting its cancel flag and status.
    pub fn cancel(&self, task_id: TaskId) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(entry) = tasks.iter_mut().find(|t| t.id == task_id)
            && matches!(entry.status, TaskStatus::Running | TaskStatus::Pending)
        {
            entry.cancel_flag.store(true, Ordering::Release);
            entry.status = TaskStatus::Cancelled;
            entry.finished_at = Some(Instant::now());
        }
        Self::promote_next(&mut tasks, self.max_concurrent);
        self.state_changed.notify_all();
    }

    /// Check whether a task has been cancelled.
    pub fn is_cancelled(&self, task_id: TaskId) -> bool {
        let tasks = self.tasks.lock().unwrap();
        tasks
            .iter()
            .find(|t| t.id == task_id)
            .map(|t| t.cancel_flag.load(Ordering::Acquire))
            .unwrap_or(false)
    }

    /// Shared cancellation flag for a worker owned by this task.
    pub fn cancellation_flag(&self, task_id: TaskId) -> Option<Arc<AtomicBool>> {
        self.tasks
            .lock()
            .unwrap()
            .iter()
            .find(|task| task.id == task_id)
            .map(|task| Arc::clone(&task.cancel_flag))
    }

    /// Return the current status of a task.
    pub fn status(&self, task_id: TaskId) -> Option<TaskStatus> {
        let tasks = self.tasks.lock().unwrap();
        tasks
            .iter()
            .find(|t| t.id == task_id)
            .map(|t| t.status.clone())
    }

    /// Count tasks that are still queued.
    pub fn pending_count(&self) -> usize {
        let tasks = self.tasks.lock().unwrap();
        tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .count()
    }

    /// Count tasks that are currently running.
    pub fn running_count(&self) -> usize {
        let tasks = self.tasks.lock().unwrap();
        tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Running)
            .count()
    }

    /// Remove finished tasks older than the configured retention period.
    pub fn evict_expired(&self) {
        let cutoff = Instant::now() - FINISHED_TASK_TTL;
        let mut tasks = self.tasks.lock().unwrap();
        tasks.retain(|task| {
            task.finished_at
                .is_none_or(|finished_at| finished_at > cutoff)
        });
    }

    /// Remove every finished task while preserving running and queued work.
    pub fn clear_completed(&self) {
        self.tasks
            .lock()
            .unwrap()
            .retain(|task| matches!(task.status, TaskStatus::Pending | TaskStatus::Running));
    }

    /// Update progress for a running task. Throttled to at most once per 250ms.
    pub fn report_progress(&self, task_id: TaskId, progress: Option<f32>, message: Option<String>) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(entry) = tasks.iter_mut().find(|t| t.id == task_id)
            && entry.status == TaskStatus::Running
        {
            let now = Instant::now();
            let should_report = match entry.last_progress_report {
                Some(prev) => now.duration_since(prev).as_millis() >= PROGRESS_THROTTLE_MS as u128,
                None => true,
            };
            if should_report {
                entry.progress = progress;
                entry.message = message;
                entry.last_progress_report = Some(now);
            }
        }
    }

    /// Return the current progress of a task.
    pub fn progress(&self, task_id: TaskId) -> Option<f32> {
        let tasks = self.tasks.lock().unwrap();
        tasks
            .iter()
            .find(|t| t.id == task_id)
            .and_then(|t| t.progress)
    }

    /// Return a snapshot of all tasks for UI display.
    pub fn snapshots(&self) -> Vec<TaskSnapshot> {
        let tasks = self.tasks.lock().unwrap();
        let mut snapshots = tasks
            .iter()
            .filter(|task| task.finished_at.is_none())
            .chain(
                tasks
                    .iter()
                    .rev()
                    .filter(|task| task.finished_at.is_some())
                    .take(RECENT_FINISHED_LIMIT),
            )
            .map(|task| TaskSnapshot {
                id: task.id,
                label: task.label.clone(),
                group_id: task.group_id.clone(),
                group_name: task.group_name.clone(),
                status: task.status.clone(),
                progress: task.progress,
                message: task.message.clone(),
                created_at: task.created_at,
            })
            .collect::<Vec<_>>();
        snapshots.sort_by_key(|snapshot| snapshot.created_at);
        snapshots
    }

    /// Promote the next queued task to running if capacity allows.
    fn promote_next(tasks: &mut [TaskEntry], max_concurrent: usize) {
        let running = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Running)
            .count();
        if running < max_concurrent
            && let Some(t) = tasks.iter_mut().find(|t| t.status == TaskStatus::Pending)
        {
            t.status = TaskStatus::Running;
        }
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new(3)
    }
}

/// Default progress throttle interval (250ms per spec).
pub const PROGRESS_THROTTLE_MS: u64 = 250;
pub const RECENT_FINISHED_LIMIT: usize = 10;
pub const FINISHED_TASK_TTL: Duration = Duration::from_secs(60 * 60);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_and_start() {
        let mgr = TaskManager::default();
        let id = mgr.submit("build site");
        // Auto-started since capacity allows
        assert_eq!(mgr.status(id), Some(TaskStatus::Running));
    }

    #[test]
    fn max_concurrent_enforced() {
        let mgr = TaskManager::new(3);
        let ids: Vec<TaskId> = (0..4).map(|i| mgr.submit(&format!("task {i}"))).collect();

        // First 3 auto-started, 4th stays queued
        assert_eq!(mgr.running_count(), 3);
        assert_eq!(mgr.status(ids[3]), Some(TaskStatus::Pending));
    }

    #[test]
    fn fifo_order() {
        let mgr = TaskManager::new(1); // limit to 1 to test FIFO
        let a = mgr.submit("first"); // auto-starts
        let b = mgr.submit("second"); // queued
        let c = mgr.submit("third"); // queued

        assert_eq!(mgr.status(a), Some(TaskStatus::Running));
        mgr.complete(a); // should auto-promote b
        assert_eq!(mgr.status(b), Some(TaskStatus::Running));
        assert_eq!(mgr.status(c), Some(TaskStatus::Pending));
    }

    #[test]
    fn cancel_sets_flag() {
        let mgr = TaskManager::default();
        let id = mgr.submit("upload");
        // Task is auto-started (Running)
        assert!(!mgr.is_cancelled(id));
        mgr.cancel(id);
        assert!(mgr.is_cancelled(id));
        assert_eq!(mgr.status(id), Some(TaskStatus::Cancelled));
    }

    #[test]
    fn complete_and_fail() {
        let mgr = TaskManager::default();
        let ok = mgr.submit("good task");
        let bad = mgr.submit("bad task");

        // Both auto-started (capacity=3)
        mgr.complete(ok);
        mgr.fail(bad, "disk full".into());

        assert_eq!(mgr.status(ok), Some(TaskStatus::Completed));
        assert_eq!(
            mgr.status(bad),
            Some(TaskStatus::Failed("disk full".into()))
        );
        // Progress should be 1.0 on completed
        assert_eq!(mgr.progress(ok), Some(1.0));
    }

    #[test]
    fn eviction_removes_only_expired_finished_tasks() {
        let mgr = TaskManager::new(3);
        let a = mgr.submit("done"); // auto-starts
        let b = mgr.submit("broken"); // auto-starts
        let e = mgr.submit("busy"); // auto-starts
        let _c = mgr.submit("stopped"); // queued (at capacity)
        let _d = mgr.submit("waiting"); // queued

        mgr.complete(a);
        mgr.fail(b, "oops".into());
        // c should have been auto-promoted when a completed, and again when b failed
        // After a completes: c promoted to running
        // After b fails: d promoted to running

        {
            let mut tasks = mgr.tasks.lock().unwrap();
            for task in tasks.iter_mut().filter(|task| task.finished_at.is_some()) {
                task.finished_at =
                    Some(Instant::now() - FINISHED_TASK_TTL - Duration::from_secs(1));
            }
        }
        mgr.evict_expired();

        assert_eq!(mgr.status(a), None);
        assert_eq!(mgr.status(b), None);
        // c and d were promoted, e is still running
        assert_eq!(mgr.status(e), Some(TaskStatus::Running));
    }

    #[test]
    fn snapshots_retain_only_ten_finished_tasks() {
        let mgr = TaskManager::new(20);
        for index in 0..12 {
            let id = mgr.submit(&format!("task {index}"));
            mgr.complete(id);
        }

        assert_eq!(mgr.snapshots().len(), 10);
    }

    #[test]
    fn clear_completed_preserves_active_tasks() {
        let mgr = TaskManager::new(2);
        let done = mgr.submit("done");
        let running = mgr.submit("running");
        mgr.complete(done);
        mgr.clear_completed();
        assert_eq!(mgr.status(done), None);
        assert_eq!(mgr.status(running), Some(TaskStatus::Running));
    }

    #[test]
    fn completing_task_starts_next_queued() {
        let mgr = TaskManager::new(1);
        let a = mgr.submit("first"); // auto-starts
        let b = mgr.submit("second"); // queued

        assert_eq!(mgr.status(a), Some(TaskStatus::Running));
        assert_eq!(mgr.status(b), Some(TaskStatus::Pending));

        mgr.complete(a);
        assert_eq!(mgr.status(b), Some(TaskStatus::Running));
    }

    #[test]
    fn queued_task_waits_for_a_slot() {
        let mgr = std::sync::Arc::new(TaskManager::new(1));
        let running = mgr.submit("running");
        let queued = mgr.submit("queued");
        let waiter = {
            let mgr = mgr.clone();
            std::thread::spawn(move || mgr.wait_until_runnable(queued))
        };

        assert!(!waiter.is_finished());
        mgr.complete(running);
        assert!(waiter.join().unwrap());
    }

    #[test]
    fn cancelling_queued_task_stops_its_waiter() {
        let mgr = std::sync::Arc::new(TaskManager::new(1));
        let _running = mgr.submit("running");
        let queued = mgr.submit("queued");
        let waiter = {
            let mgr = mgr.clone();
            std::thread::spawn(move || mgr.wait_until_runnable(queued))
        };

        mgr.cancel(queued);
        assert!(!waiter.join().unwrap());
    }

    #[test]
    fn cancel_precondition_ignores_completed() {
        let mgr = TaskManager::default();
        let id = mgr.submit("task");
        mgr.complete(id);
        mgr.cancel(id); // should be no-op
        assert_eq!(mgr.status(id), Some(TaskStatus::Completed));
    }

    #[test]
    fn report_progress_updates_task() {
        let mgr = TaskManager::default();
        let id = mgr.submit("upload");
        mgr.report_progress(id, Some(0.5), Some("halfway".into()));
        assert_eq!(mgr.progress(id), Some(0.5));
        assert_eq!(mgr.snapshots()[0].message.as_deref(), Some("halfway"));
    }
}

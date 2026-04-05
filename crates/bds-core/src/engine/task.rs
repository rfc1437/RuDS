use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Unique task identifier.
pub type TaskId = u64;

/// Task status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

/// Progress update for a task.
#[derive(Debug, Clone)]
pub struct TaskProgress {
    pub task_id: TaskId,
    pub message: String,
    pub percent: Option<f32>,
}

/// Entry tracking a task.
#[derive(Debug)]
struct TaskEntry {
    id: TaskId,
    label: String,
    status: TaskStatus,
    cancel_flag: Arc<AtomicBool>,
    progress: Option<f32>,
    message: Option<String>,
    created_at: Instant,
}

/// Manages concurrent tasks with a max concurrency limit and FIFO queue.
pub struct TaskManager {
    max_concurrent: usize,
    next_id: Mutex<TaskId>,
    tasks: Mutex<Vec<TaskEntry>>,
}

impl TaskManager {
    /// Create a new task manager with the given concurrency limit.
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent,
            next_id: Mutex::new(1),
            tasks: Mutex::new(Vec::new()),
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
            status: TaskStatus::Queued,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            progress: None,
            message: None,
            created_at: Instant::now(),
        };

        let mut tasks = self.tasks.lock().unwrap();
        tasks.push(entry);
        // Auto-start if under capacity
        let running = tasks.iter().filter(|t| t.status == TaskStatus::Running).count();
        if running < self.max_concurrent {
            if let Some(t) = tasks.iter_mut().find(|t| t.id == id && t.status == TaskStatus::Queued) {
                t.status = TaskStatus::Running;
            }
        }
        id
    }

    /// Try to start a queued task. Returns true if the task was moved to
    /// Running, false if concurrency is at capacity or the task is not Queued.
    pub fn try_start(&self, task_id: TaskId) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        let running = tasks.iter().filter(|t| t.status == TaskStatus::Running).count();
        if running >= self.max_concurrent {
            return false;
        }
        if let Some(entry) = tasks.iter_mut().find(|t| t.id == task_id) {
            if entry.status == TaskStatus::Queued {
                entry.status = TaskStatus::Running;
                return true;
            }
        }
        false
    }

    /// Mark a task as completed.
    pub fn complete(&self, task_id: TaskId) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(entry) = tasks.iter_mut().find(|t| t.id == task_id) {
            if matches!(entry.status, TaskStatus::Running) {
                entry.status = TaskStatus::Completed;
                entry.progress = Some(1.0);
            }
        }
        Self::promote_next(&mut tasks, self.max_concurrent);
    }

    /// Mark a task as failed with an error message.
    pub fn fail(&self, task_id: TaskId, error: String) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(entry) = tasks.iter_mut().find(|t| t.id == task_id) {
            if matches!(entry.status, TaskStatus::Running) {
                entry.message = Some(error.clone());
                entry.status = TaskStatus::Failed(error);
            }
        }
        Self::promote_next(&mut tasks, self.max_concurrent);
    }

    /// Cancel a task by setting its cancel flag and status.
    pub fn cancel(&self, task_id: TaskId) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(entry) = tasks.iter_mut().find(|t| t.id == task_id) {
            if matches!(entry.status, TaskStatus::Running | TaskStatus::Queued) {
                entry.cancel_flag.store(true, Ordering::Release);
                entry.status = TaskStatus::Cancelled;
            }
        }
        Self::promote_next(&mut tasks, self.max_concurrent);
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

    /// Return the current status of a task.
    pub fn status(&self, task_id: TaskId) -> Option<TaskStatus> {
        let tasks = self.tasks.lock().unwrap();
        tasks.iter().find(|t| t.id == task_id).map(|t| t.status.clone())
    }

    /// Count tasks that are still queued.
    pub fn pending_count(&self) -> usize {
        let tasks = self.tasks.lock().unwrap();
        tasks.iter().filter(|t| t.status == TaskStatus::Queued).count()
    }

    /// Count tasks that are currently running.
    pub fn running_count(&self) -> usize {
        let tasks = self.tasks.lock().unwrap();
        tasks.iter().filter(|t| t.status == TaskStatus::Running).count()
    }

    /// Remove all completed, failed, and cancelled tasks.
    pub fn drain_completed(&self) {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.retain(|t| matches!(t.status, TaskStatus::Queued | TaskStatus::Running));
    }

    /// Return the label of a task.
    pub fn label(&self, task_id: TaskId) -> Option<String> {
        let tasks = self.tasks.lock().unwrap();
        tasks.iter().find(|t| t.id == task_id).map(|t| t.label.clone())
    }

    /// Return the id of the first queued task (FIFO order).
    pub fn next_queued(&self) -> Option<TaskId> {
        let tasks = self.tasks.lock().unwrap();
        tasks.iter().find(|t| t.status == TaskStatus::Queued).map(|t| t.id)
    }

    /// Update progress for a running task.
    pub fn report_progress(&self, task_id: TaskId, progress: Option<f32>, message: Option<String>) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(entry) = tasks.iter_mut().find(|t| t.id == task_id) {
            if entry.status == TaskStatus::Running {
                entry.progress = progress;
                entry.message = message;
            }
        }
    }

    /// Return the current progress of a task.
    pub fn progress(&self, task_id: TaskId) -> Option<f32> {
        let tasks = self.tasks.lock().unwrap();
        tasks.iter().find(|t| t.id == task_id).and_then(|t| t.progress)
    }

    /// Return the current message of a task.
    pub fn message(&self, task_id: TaskId) -> Option<String> {
        let tasks = self.tasks.lock().unwrap();
        tasks.iter().find(|t| t.id == task_id).and_then(|t| t.message.clone())
    }

    /// Return a snapshot of all tasks for UI display.
    pub fn snapshots(&self) -> Vec<(TaskId, String, TaskStatus, Option<f32>, Option<String>)> {
        let tasks = self.tasks.lock().unwrap();
        tasks
            .iter()
            .map(|t| (t.id, t.label.clone(), t.status.clone(), t.progress, t.message.clone()))
            .collect()
    }

    /// Promote the next queued task to running if capacity allows.
    fn promote_next(tasks: &mut Vec<TaskEntry>, max_concurrent: usize) {
        let running = tasks.iter().filter(|t| t.status == TaskStatus::Running).count();
        if running < max_concurrent {
            if let Some(t) = tasks.iter_mut().find(|t| t.status == TaskStatus::Queued) {
                t.status = TaskStatus::Running;
            }
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

/// Throttles progress reporting to avoid flooding.
pub struct ProgressThrottle {
    interval_ms: u64,
    last_report: Mutex<Option<Instant>>,
}

impl ProgressThrottle {
    /// Create a throttle with the given interval in milliseconds.
    pub fn new(interval_ms: u64) -> Self {
        Self {
            interval_ms,
            last_report: Mutex::new(None),
        }
    }

    /// Returns true if enough time has elapsed since the last report.
    pub fn should_report(&self) -> bool {
        let mut last = self.last_report.lock().unwrap();
        let now = Instant::now();
        match *last {
            Some(prev) if now.duration_since(prev).as_millis() < self.interval_ms as u128 => false,
            _ => {
                *last = Some(now);
                true
            }
        }
    }
}

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
        assert_eq!(mgr.status(ids[3]), Some(TaskStatus::Queued));
    }

    #[test]
    fn fifo_order() {
        let mgr = TaskManager::new(1); // limit to 1 to test FIFO
        let a = mgr.submit("first");  // auto-starts
        let b = mgr.submit("second"); // queued
        let c = mgr.submit("third");  // queued

        assert_eq!(mgr.status(a), Some(TaskStatus::Running));
        assert_eq!(mgr.next_queued(), Some(b));

        mgr.complete(a);  // should auto-promote b
        assert_eq!(mgr.status(b), Some(TaskStatus::Running));
        assert_eq!(mgr.next_queued(), Some(c));
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
        assert_eq!(mgr.status(bad), Some(TaskStatus::Failed("disk full".into())));
        // Progress should be 1.0 on completed
        assert_eq!(mgr.progress(ok), Some(1.0));
    }

    #[test]
    fn drain_removes_finished() {
        let mgr = TaskManager::new(3);
        let a = mgr.submit("done");    // auto-starts
        let b = mgr.submit("broken");  // auto-starts
        let e = mgr.submit("busy");    // auto-starts
        let _c = mgr.submit("stopped"); // queued (at capacity)
        let _d = mgr.submit("waiting"); // queued

        mgr.complete(a);
        mgr.fail(b, "oops".into());
        // c should have been auto-promoted when a completed, and again when b failed
        // After a completes: c promoted to running
        // After b fails: d promoted to running

        mgr.drain_completed();

        assert_eq!(mgr.status(a), None);
        assert_eq!(mgr.status(b), None);
        // c and d were promoted, e is still running
        assert_eq!(mgr.status(e), Some(TaskStatus::Running));
    }

    #[test]
    fn completing_task_starts_next_queued() {
        let mgr = TaskManager::new(1);
        let a = mgr.submit("first");  // auto-starts
        let b = mgr.submit("second"); // queued

        assert_eq!(mgr.status(a), Some(TaskStatus::Running));
        assert_eq!(mgr.status(b), Some(TaskStatus::Queued));

        mgr.complete(a);
        assert_eq!(mgr.status(b), Some(TaskStatus::Running));
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
        assert_eq!(mgr.message(id), Some("halfway".into()));
    }

    #[test]
    fn progress_throttle_initial_reports() {
        let throttle = ProgressThrottle::new(PROGRESS_THROTTLE_MS);
        assert!(throttle.should_report());
    }

    #[test]
    fn progress_throttle_suppresses_rapid() {
        let throttle = ProgressThrottle::new(PROGRESS_THROTTLE_MS);
        assert!(throttle.should_report());
        assert!(!throttle.should_report());
    }
}

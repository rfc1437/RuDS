use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::Notify;

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
    pub cancellation_requested: bool,
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
    worker_active: bool,
    worker_started: bool,
}

/// Manages concurrent tasks with a max concurrency limit and FIFO queue.
pub struct TaskManager {
    max_concurrent: usize,
    state: Mutex<TaskState>,
    state_changed: Condvar,
    async_changed: Notify,
}

struct TaskState {
    next_id: TaskId,
    tasks: HashMap<TaskId, TaskEntry>,
    order: VecDeque<TaskId>,
    pending: VecDeque<TaskId>,
    worker_count: usize,
}

/// Capacity reservation held from asynchronous admission until the worker exits.
pub struct TaskWorker {
    manager: Arc<TaskManager>,
    task_id: TaskId,
}

impl Drop for TaskWorker {
    fn drop(&mut self) {
        self.manager.worker_exited(self.task_id);
    }
}

impl TaskManager {
    /// Create a new task manager with the given concurrency limit.
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent: max_concurrent.max(1),
            state: Mutex::new(TaskState {
                next_id: 1,
                tasks: HashMap::new(),
                order: VecDeque::new(),
                pending: VecDeque::new(),
                worker_count: 0,
            }),
            state_changed: Condvar::new(),
            async_changed: Notify::new(),
        }
    }

    /// Submit a new task. Returns its unique identifier.
    pub fn submit(&self, label: &str) -> TaskId {
        self.submit_with_group(label, None, None)
    }

    fn submit_with_group(
        &self,
        label: &str,
        group_id: Option<&str>,
        group_name: Option<&str>,
    ) -> TaskId {
        let mut state = self.state.lock().unwrap();
        Self::prune_expired(&mut state);
        let id = state.next_id;
        state.next_id += 1;
        state.tasks.insert(
            id,
            TaskEntry {
                id,
                label: label.to_owned(),
                group_id: group_id.map(str::to_owned),
                group_name: group_name.map(str::to_owned),
                status: TaskStatus::Pending,
                cancel_flag: Arc::new(AtomicBool::new(false)),
                progress: None,
                message: None,
                created_at: Instant::now(),
                finished_at: None,
                last_progress_report: None,
                worker_active: false,
                worker_started: false,
            },
        );
        state.order.push_back(id);
        state.pending.push_back(id);
        Self::promote_next(&mut state, self.max_concurrent);
        drop(state);
        self.notify_changed();
        id
    }

    /// Submit a new task within a group. Returns its unique identifier.
    pub fn submit_grouped(&self, label: &str, group_id: &str, group_name: &str) -> TaskId {
        self.submit_with_group(label, Some(group_id), Some(group_name))
    }

    /// Wait synchronously for admission. Prefer [`Self::admit`] before spawning workers.
    pub fn wait_until_runnable(&self, task_id: TaskId) -> bool {
        let mut state = self.state.lock().unwrap();
        loop {
            match state.tasks.get(&task_id).map(|task| &task.status) {
                Some(TaskStatus::Running) => {
                    if let Some(task) = state.tasks.get_mut(&task_id) {
                        task.worker_started = true;
                    }
                    return true;
                }
                Some(TaskStatus::Pending) => state = self.state_changed.wait(state).unwrap(),
                _ => return false,
            }
        }
    }

    /// Admit without occupying a blocking-pool thread while queued.
    pub async fn admit(self: &Arc<Self>, task_id: TaskId) -> Option<TaskWorker> {
        loop {
            let notified = self.async_changed.notified();
            {
                let mut state = self.state.lock().unwrap();
                match state.tasks.get_mut(&task_id) {
                    Some(task) if task.status == TaskStatus::Running => {
                        task.worker_started = true;
                        return Some(TaskWorker {
                            manager: Arc::clone(self),
                            task_id,
                        });
                    }
                    Some(task) if task.status == TaskStatus::Pending => {}
                    _ => return None,
                }
            }
            notified.await;
        }
    }

    /// Admit a non-Tokio worker while retaining capacity until its guard drops.
    pub fn admit_blocking(self: &Arc<Self>, task_id: TaskId) -> Option<TaskWorker> {
        if self.wait_until_runnable(task_id) {
            Some(TaskWorker {
                manager: Arc::clone(self),
                task_id,
            })
        } else {
            None
        }
    }

    /// Mark a task as completed.
    pub fn complete(&self, task_id: TaskId) {
        let mut state = self.state.lock().unwrap();
        let released = if let Some(entry) = state.tasks.get_mut(&task_id) {
            if matches!(entry.status, TaskStatus::Running) {
                entry.status = TaskStatus::Completed;
                entry.progress = Some(1.0);
                entry.finished_at = Some(Instant::now());
            }
            let released = entry.worker_active;
            entry.worker_active = false;
            entry.worker_started = false;
            released
        } else {
            false
        };
        state.worker_count = state.worker_count.saturating_sub(usize::from(released));
        Self::promote_next(&mut state, self.max_concurrent);
        drop(state);
        self.notify_changed();
    }

    /// Mark a task as failed with an error message.
    pub fn fail(&self, task_id: TaskId, error: String) {
        let mut state = self.state.lock().unwrap();
        let released = if let Some(entry) = state.tasks.get_mut(&task_id) {
            if matches!(entry.status, TaskStatus::Running) {
                if entry.cancel_flag.load(Ordering::Acquire) {
                    entry.status = TaskStatus::Cancelled;
                } else {
                    entry.message = Some(error.clone());
                    entry.status = TaskStatus::Failed(error);
                }
                entry.finished_at = Some(Instant::now());
            }
            let released = entry.worker_active;
            entry.worker_active = false;
            entry.worker_started = false;
            released
        } else {
            false
        };
        state.worker_count = state.worker_count.saturating_sub(usize::from(released));
        Self::promote_next(&mut state, self.max_concurrent);
        drop(state);
        self.notify_changed();
    }

    /// Cancel queued work immediately, or request a cooperative stop from a worker.
    pub fn cancel(&self, task_id: TaskId) -> bool {
        let mut state = self.state.lock().unwrap();
        let mut cancelled = false;
        let mut released = false;
        if let Some(entry) = state.tasks.get_mut(&task_id)
            && matches!(entry.status, TaskStatus::Running | TaskStatus::Pending)
        {
            entry.cancel_flag.store(true, Ordering::Release);
            if !entry.worker_started {
                if entry.worker_active {
                    entry.worker_active = false;
                    released = true;
                }
                entry.status = TaskStatus::Cancelled;
                entry.finished_at = Some(Instant::now());
            }
            cancelled = true;
        }
        state.worker_count = state.worker_count.saturating_sub(usize::from(released));
        Self::promote_next(&mut state, self.max_concurrent);
        drop(state);
        self.notify_changed();
        cancelled
    }

    /// Cancel every active task in a group and release their workers.
    pub fn cancel_group(&self, group_id: &str) {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        let group_ids = state
            .tasks
            .values()
            .filter(|task| {
                task.group_id.as_deref() == Some(group_id)
                    && matches!(task.status, TaskStatus::Running | TaskStatus::Pending)
            })
            .map(|task| task.id)
            .collect::<Vec<_>>();
        let mut released = 0;
        for id in group_ids {
            let entry = state.tasks.get_mut(&id).unwrap();
            entry.cancel_flag.store(true, Ordering::Release);
            if !entry.worker_started {
                if entry.worker_active {
                    entry.worker_active = false;
                    released += 1;
                }
                entry.status = TaskStatus::Cancelled;
                entry.finished_at = Some(now);
            }
        }
        state.worker_count = state.worker_count.saturating_sub(released);
        Self::promote_next(&mut state, self.max_concurrent);
        drop(state);
        self.notify_changed();
    }

    /// Return the group containing a task, if any.
    pub fn group_id(&self, task_id: TaskId) -> Option<String> {
        let mut state = self.state.lock().unwrap();
        Self::prune_expired(&mut state);
        state
            .tasks
            .get(&task_id)
            .and_then(|task| task.group_id.clone())
    }

    /// Check whether a task has been cancelled.
    pub fn is_cancelled(&self, task_id: TaskId) -> bool {
        let mut state = self.state.lock().unwrap();
        Self::prune_expired(&mut state);
        state
            .tasks
            .get(&task_id)
            .map(|t| t.cancel_flag.load(Ordering::Acquire))
            .unwrap_or(false)
    }

    /// Shared cancellation flag for a worker owned by this task.
    pub fn cancellation_flag(&self, task_id: TaskId) -> Option<Arc<AtomicBool>> {
        let mut state = self.state.lock().unwrap();
        Self::prune_expired(&mut state);
        state
            .tasks
            .get(&task_id)
            .map(|task| Arc::clone(&task.cancel_flag))
    }

    /// Return the current status of a task.
    pub fn status(&self, task_id: TaskId) -> Option<TaskStatus> {
        self.get(task_id).map(|task| task.status)
    }

    /// Count tasks that are still queued.
    pub fn pending_count(&self) -> usize {
        let mut state = self.state.lock().unwrap();
        Self::prune_expired(&mut state);
        state
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::Pending)
            .count()
    }

    /// Count tasks that are currently running.
    pub fn running_count(&self) -> usize {
        let mut state = self.state.lock().unwrap();
        Self::prune_expired(&mut state);
        state
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::Running)
            .count()
    }

    /// Remove finished tasks older than the configured retention period.
    pub fn evict_expired(&self) {
        let mut state = self.state.lock().unwrap();
        Self::prune_expired(&mut state);
    }

    /// Remove every finished task while preserving running and queued work.
    pub fn clear_completed(&self) {
        let mut state = self.state.lock().unwrap();
        Self::remove_where(&mut state, |task| task.status == TaskStatus::Completed);
    }

    /// Remove all terminal task results while preserving active work.
    pub fn clear_finished(&self) {
        let mut state = self.state.lock().unwrap();
        Self::remove_where(&mut state, |task| {
            task.finished_at.is_some() && !task.worker_active
        });
    }

    /// Update progress for a running task. Throttled to at most once per 250ms.
    pub fn report_progress(&self, task_id: TaskId, progress: Option<f32>, message: Option<String>) {
        let mut state = self.state.lock().unwrap();
        if let Some(entry) = state.tasks.get_mut(&task_id)
            && entry.status == TaskStatus::Running
        {
            let now = Instant::now();
            let should_report = match entry.last_progress_report {
                Some(prev) => now.duration_since(prev).as_millis() >= PROGRESS_THROTTLE_MS as u128,
                None => true,
            };
            if should_report || progress.is_some_and(|value| value >= 1.0) {
                entry.progress = progress;
                entry.message = message;
                entry.last_progress_report = Some(now);
            }
        }
    }

    /// Return the current progress of a task.
    pub fn progress(&self, task_id: TaskId) -> Option<f32> {
        self.get(task_id).and_then(|task| task.progress)
    }

    /// Return active tasks plus ten recent finished tasks for status surfaces.
    pub fn snapshots(&self) -> Vec<TaskSnapshot> {
        let mut state = self.state.lock().unwrap();
        Self::prune_expired(&mut state);
        let mut active = state
            .tasks
            .values()
            .filter(|task| task.finished_at.is_none())
            .collect::<Vec<_>>();
        active.sort_by_key(|task| (task.status != TaskStatus::Running, task.created_at));
        let active_groups = active
            .iter()
            .filter_map(|task| task.group_id.as_deref())
            .collect::<HashSet<_>>();
        let mut finished = state
            .tasks
            .values()
            .filter(|task| task.finished_at.is_some())
            .collect::<Vec<_>>();
        finished.sort_by_key(|task| std::cmp::Reverse(task.finished_at));
        let recent_ids = finished
            .iter()
            .take(RECENT_FINISHED_LIMIT)
            .map(|task| task.id)
            .collect::<HashSet<_>>();
        active
            .into_iter()
            .chain(finished.into_iter().filter(|task| {
                recent_ids.contains(&task.id)
                    || task
                        .group_id
                        .as_deref()
                        .is_some_and(|group| active_groups.contains(group))
            }))
            .map(Self::snapshot)
            .collect()
    }

    /// Return one retained task by id.
    pub fn get(&self, task_id: TaskId) -> Option<TaskSnapshot> {
        let mut state = self.state.lock().unwrap();
        Self::prune_expired(&mut state);
        state.tasks.get(&task_id).map(Self::snapshot)
    }

    /// Return every retained task, newest first.
    pub fn all(&self) -> Vec<TaskSnapshot> {
        let mut state = self.state.lock().unwrap();
        Self::prune_expired(&mut state);
        state
            .order
            .iter()
            .rev()
            .filter_map(|id| state.tasks.get(id).map(Self::snapshot))
            .collect()
    }

    /// Return running tasks in start order.
    pub fn running(&self) -> Vec<TaskSnapshot> {
        let mut state = self.state.lock().unwrap();
        Self::prune_expired(&mut state);
        state
            .order
            .iter()
            .filter_map(|id| state.tasks.get(id))
            .filter(|task| task.status == TaskStatus::Running)
            .map(Self::snapshot)
            .collect()
    }

    fn worker_exited(&self, task_id: TaskId) {
        let mut state = self.state.lock().unwrap();
        let released = if let Some(task) = state.tasks.get_mut(&task_id) {
            if task.cancel_flag.load(Ordering::Acquire) {
                task.worker_started = false;
                drop(state);
                self.notify_changed();
                return;
            }
            let released = task.worker_active;
            task.worker_active = false;
            task.worker_started = false;
            released
        } else {
            false
        };
        state.worker_count = state.worker_count.saturating_sub(usize::from(released));
        Self::promote_next(&mut state, self.max_concurrent);
        drop(state);
        self.notify_changed();
    }

    fn promote_next(state: &mut TaskState, max_concurrent: usize) {
        while state.worker_count < max_concurrent {
            let Some(id) = state.pending.pop_front() else {
                break;
            };
            let Some(task) = state.tasks.get_mut(&id) else {
                continue;
            };
            if task.status != TaskStatus::Pending {
                continue;
            }
            task.status = TaskStatus::Running;
            task.worker_active = true;
            state.worker_count += 1;
        }
    }

    fn snapshot(task: &TaskEntry) -> TaskSnapshot {
        TaskSnapshot {
            id: task.id,
            label: task.label.clone(),
            group_id: task.group_id.clone(),
            group_name: task.group_name.clone(),
            status: task.status.clone(),
            progress: task.progress,
            message: task.message.clone(),
            cancellation_requested: task.cancel_flag.load(Ordering::Acquire),
            created_at: task.created_at,
        }
    }

    fn prune_expired(state: &mut TaskState) {
        let cutoff = Instant::now() - FINISHED_TASK_TTL;
        Self::remove_where(state, |task| {
            task.finished_at
                .is_some_and(|finished_at| finished_at <= cutoff)
                && !task.worker_active
        });
    }

    fn remove_where(state: &mut TaskState, predicate: impl Fn(&TaskEntry) -> bool) {
        let removed = state
            .tasks
            .values()
            .filter(|task| predicate(task))
            .map(|task| task.id)
            .collect::<HashSet<_>>();
        state.tasks.retain(|id, _| !removed.contains(id));
        state.order.retain(|id| !removed.contains(id));
        state.pending.retain(|id| !removed.contains(id));
    }

    fn notify_changed(&self) {
        self.state_changed.notify_all();
        self.async_changed.notify_waiters();
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new(
            std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
        )
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
    fn default_uses_every_online_worker_like_bds2() {
        let mgr = TaskManager::default();
        let expected = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1);
        for index in 0..expected {
            mgr.submit(&format!("task {index}"));
        }

        assert_eq!(mgr.running_count(), expected);
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
    fn cancelling_unstarted_task_settles_immediately() {
        let mgr = TaskManager::default();
        let id = mgr.submit("upload");
        // Task is auto-started (Running)
        assert!(!mgr.is_cancelled(id));
        mgr.cancel(id);
        assert!(mgr.is_cancelled(id));
        assert_eq!(mgr.status(id), Some(TaskStatus::Cancelled));
    }

    #[tokio::test]
    async fn cancelling_started_task_stays_running_until_worker_stops() {
        let mgr = Arc::new(TaskManager::new(1));
        let id = mgr.submit("upload");
        let worker = mgr.admit(id).await.unwrap();

        mgr.cancel(id);

        let snapshot = mgr.get(id).unwrap();
        assert_eq!(snapshot.status, TaskStatus::Running);
        assert!(snapshot.cancellation_requested);
        drop(worker);
        mgr.fail(id, "operation cancelled".into());
        assert_eq!(mgr.status(id), Some(TaskStatus::Cancelled));
    }

    #[tokio::test]
    async fn completed_work_wins_a_late_cancellation_request() {
        let mgr = Arc::new(TaskManager::new(1));
        let id = mgr.submit("atomic update");
        let worker = mgr.admit(id).await.unwrap();
        mgr.cancel(id);
        drop(worker);

        mgr.complete(id);

        assert_eq!(mgr.status(id), Some(TaskStatus::Completed));
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
            let mut state = mgr.state.lock().unwrap();
            for task in state
                .tasks
                .values_mut()
                .filter(|task| task.finished_at.is_some())
            {
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
    fn snapshots_retain_every_active_task_and_complete_active_groups() {
        let mgr = TaskManager::new(20);
        let finished_group_member = mgr.submit_grouped("finished", "group", "Group");
        mgr.complete(finished_group_member);
        for index in 0..12 {
            mgr.submit_grouped(&format!("active {index}"), "group", "Group");
        }
        for index in 0..12 {
            let id = mgr.submit(&format!("history {index}"));
            mgr.complete(id);
        }

        let snapshots = mgr.snapshots();
        assert_eq!(
            snapshots
                .iter()
                .filter(|task| matches!(task.status, TaskStatus::Pending | TaskStatus::Running))
                .count(),
            12
        );
        assert!(
            snapshots
                .iter()
                .any(|task| task.id == finished_group_member)
        );
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
    fn cancelling_group_settles_every_active_task() {
        let mgr = TaskManager::new(2);
        let first = mgr.submit_grouped("first", "generation-1", "Render Site");
        let second = mgr.submit_grouped("second", "generation-1", "Render Site");
        let third = mgr.submit_grouped("third", "generation-1", "Render Site");
        let unrelated = mgr.submit("unrelated");

        mgr.cancel_group("generation-1");

        assert_eq!(mgr.status(first), Some(TaskStatus::Cancelled));
        assert_eq!(mgr.status(second), Some(TaskStatus::Cancelled));
        assert_eq!(mgr.status(third), Some(TaskStatus::Cancelled));
        assert_ne!(mgr.status(unrelated), Some(TaskStatus::Cancelled));
    }

    #[test]
    fn report_progress_updates_task() {
        let mgr = TaskManager::default();
        let id = mgr.submit("upload");
        mgr.report_progress(id, Some(0.5), Some("halfway".into()));
        assert_eq!(mgr.progress(id), Some(0.5));
        assert_eq!(mgr.snapshots()[0].message.as_deref(), Some("halfway"));
    }

    #[tokio::test]
    async fn cancellation_holds_capacity_until_worker_exits() {
        let mgr = Arc::new(TaskManager::new(1));
        let running = mgr.submit("running");
        let queued = mgr.submit("queued");
        let worker = mgr.admit(running).await.unwrap();

        mgr.cancel(running);

        assert_eq!(mgr.status(running), Some(TaskStatus::Running));
        assert!(mgr.get(running).unwrap().cancellation_requested);
        assert_eq!(mgr.status(queued), Some(TaskStatus::Pending));
        drop(worker);
        assert_eq!(mgr.status(queued), Some(TaskStatus::Pending));
        mgr.fail(running, "operation cancelled".into());
        assert_eq!(mgr.status(queued), Some(TaskStatus::Running));
    }

    #[tokio::test]
    async fn panicking_worker_guard_releases_capacity() {
        let mgr = Arc::new(TaskManager::new(1));
        let panicking = mgr.submit("panicking");
        let queued = mgr.submit("queued");
        let worker = mgr.admit(panicking).await.unwrap();

        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _worker = worker;
            panic!("boom");
        }));

        assert_eq!(mgr.status(queued), Some(TaskStatus::Running));
    }

    #[test]
    fn terminal_progress_bypasses_throttle() {
        let mgr = TaskManager::new(1);
        let id = mgr.submit("work");
        mgr.report_progress(id, Some(0.5), Some("working".into()));
        mgr.report_progress(id, Some(1.0), Some("done".into()));

        let task = mgr
            .snapshots()
            .into_iter()
            .find(|task| task.id == id)
            .unwrap();
        assert_eq!(task.progress, Some(1.0));
        assert_eq!(task.message.as_deref(), Some("done"));
    }

    #[test]
    fn clear_completed_keeps_other_terminal_results() {
        let mgr = TaskManager::new(3);
        let completed = mgr.submit("completed");
        let failed = mgr.submit("failed");
        let cancelled = mgr.submit("cancelled");
        mgr.complete(completed);
        mgr.fail(failed, "failed".into());
        mgr.cancel(cancelled);

        mgr.clear_completed();

        assert_eq!(mgr.status(completed), None);
        assert!(matches!(mgr.status(failed), Some(TaskStatus::Failed(_))));
        assert_eq!(mgr.status(cancelled), Some(TaskStatus::Cancelled));
    }
}

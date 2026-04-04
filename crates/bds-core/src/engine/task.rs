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
        };

        self.tasks.lock().unwrap().push(entry);
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
            entry.status = TaskStatus::Completed;
        }
    }

    /// Mark a task as failed with an error message.
    pub fn fail(&self, task_id: TaskId, error: String) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(entry) = tasks.iter_mut().find(|t| t.id == task_id) {
            entry.status = TaskStatus::Failed(error);
        }
    }

    /// Cancel a task by setting its cancel flag and status.
    pub fn cancel(&self, task_id: TaskId) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(entry) = tasks.iter_mut().find(|t| t.id == task_id) {
            entry.cancel_flag.store(true, Ordering::Release);
            entry.status = TaskStatus::Cancelled;
        }
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
        assert_eq!(mgr.status(id), Some(TaskStatus::Queued));

        assert!(mgr.try_start(id));
        assert_eq!(mgr.status(id), Some(TaskStatus::Running));
    }

    #[test]
    fn max_concurrent_enforced() {
        let mgr = TaskManager::new(3);
        let ids: Vec<TaskId> = (0..4).map(|i| mgr.submit(&format!("task {i}"))).collect();

        assert!(mgr.try_start(ids[0]));
        assert!(mgr.try_start(ids[1]));
        assert!(mgr.try_start(ids[2]));
        assert!(!mgr.try_start(ids[3]));

        assert_eq!(mgr.running_count(), 3);
        assert_eq!(mgr.status(ids[3]), Some(TaskStatus::Queued));
    }

    #[test]
    fn fifo_order() {
        let mgr = TaskManager::default();
        let a = mgr.submit("first");
        let b = mgr.submit("second");
        let c = mgr.submit("third");

        assert_eq!(mgr.next_queued(), Some(a));
        mgr.try_start(a);
        assert_eq!(mgr.next_queued(), Some(b));
        mgr.try_start(b);
        assert_eq!(mgr.next_queued(), Some(c));
    }

    #[test]
    fn cancel_sets_flag() {
        let mgr = TaskManager::default();
        let id = mgr.submit("upload");
        mgr.try_start(id);

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

        mgr.try_start(ok);
        mgr.try_start(bad);

        mgr.complete(ok);
        mgr.fail(bad, "disk full".into());

        assert_eq!(mgr.status(ok), Some(TaskStatus::Completed));
        assert_eq!(mgr.status(bad), Some(TaskStatus::Failed("disk full".into())));
    }

    #[test]
    fn drain_removes_finished() {
        let mgr = TaskManager::default();
        let a = mgr.submit("done");
        let b = mgr.submit("broken");
        let c = mgr.submit("stopped");
        let d = mgr.submit("waiting");
        let e = mgr.submit("busy");

        mgr.try_start(a);
        mgr.try_start(b);
        mgr.try_start(e);
        mgr.complete(a);
        mgr.fail(b, "oops".into());
        mgr.cancel(c);

        mgr.drain_completed();

        assert_eq!(mgr.status(a), None);
        assert_eq!(mgr.status(b), None);
        assert_eq!(mgr.status(c), None);
        assert_eq!(mgr.status(d), Some(TaskStatus::Queued));
        assert_eq!(mgr.status(e), Some(TaskStatus::Running));
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

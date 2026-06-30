use std::time::{Duration, Instant};

use crate::task::group::GroupId;

/// Unique identifier for a task.
pub type TaskId = u64;

/// ETA smoothing window: rate is computed across progress samples within this
/// duration of "now".
pub(crate) const ETA_WINDOW: Duration = Duration::from_secs(5);
/// Hard cap on remembered progress samples per task.
pub(crate) const PROGRESS_HISTORY_CAP: usize = 32;

/// The status of a task in its lifecycle.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    /// Waiting for one or more dependencies to complete.
    Pending,
    /// Dependencies met, queued for execution.
    Ready,
    /// Currently executing on a worker thread.
    Running,
    /// Finished successfully.
    Completed,
    /// Finished with an error.
    Failed(String),
    /// Cancelled by the user or the manager.
    Cancelled,
}

impl TaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Completed | TaskStatus::Failed(_) | TaskStatus::Cancelled
        )
    }

    pub fn label(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "Pending",
            TaskStatus::Ready => "Ready",
            TaskStatus::Running => "Running",
            TaskStatus::Completed => "Completed",
            TaskStatus::Failed(_) => "Failed",
            TaskStatus::Cancelled => "Cancelled",
        }
    }
}

/// A sliding-window progress sample used to compute rate / ETA.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProgressSample {
    pub at: Instant,
    pub value: f32,
}

/// A unit of background work.
///
/// Built with the builder pattern; the work closure is consumed by
/// [`crate::task::manager::TaskManager::add_task`].
pub struct Task {
    pub(crate) id: TaskId,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) group: Option<GroupId>,
    pub(crate) dependencies: Vec<TaskId>,
    pub(crate) work: Option<WorkFn>,
    pub(crate) status: TaskStatus,
    pub(crate) progress: f32,
    pub(crate) message: String,
    pub(crate) started_at: Option<Instant>,
    pub(crate) completed_at: Option<Instant>,
    pub(crate) progress_history: Vec<ProgressSample>,
    pub(crate) cancelled: bool,
}

/// The work executed by a [`Task`].
///
/// The closure receives a [`crate::task::context::TaskContext`] which it can
/// use to report progress, update a status message, and check for cancellation.
pub type WorkFn =
    Box<dyn FnOnce(&crate::task::context::TaskContext) -> Result<(), String> + Send + 'static>;

impl Task {
    /// Create a new task with a name and description.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: 0, // assigned by TaskManager
            name: name.into(),
            description: description.into(),
            group: None,
            dependencies: Vec::new(),
            work: None,
            status: TaskStatus::Pending,
            progress: 0.0,
            message: String::new(),
            started_at: None,
            completed_at: None,
            progress_history: Vec::with_capacity(16),
            cancelled: false,
        }
    }

    /// Assign this task to a group.
    pub fn group(mut self, group: GroupId) -> Self {
        self.group = Some(group);
        self
    }

    /// Declare that this task cannot run until the given tasks complete.
    ///
    /// May be called multiple times to add several dependencies.
    pub fn depends_on(mut self, dep: TaskId) -> Self {
        self.dependencies.push(dep);
        self
    }

    /// Declare several dependencies at once.
    pub fn depends_on_all<I: IntoIterator<Item = TaskId>>(mut self, deps: I) -> Self {
        self.dependencies.extend(deps);
        self
    }

    /// Attach the work closure. The closure is consumed by the manager when
    /// the task is started.
    pub fn work<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&crate::task::context::TaskContext) -> Result<(), String> + Send + 'static,
    {
        self.work = Some(Box::new(f));
        self
    }

    pub fn id(&self) -> TaskId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn group_id(&self) -> Option<GroupId> {
        self.group
    }

    pub fn dependencies(&self) -> &[TaskId] {
        &self.dependencies
    }

    pub fn status(&self) -> &TaskStatus {
        &self.status
    }

    pub fn progress(&self) -> f32 {
        self.progress.clamp(0.0, 1.0)
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// Estimated time remaining, based on the sliding-window progress rate.
    /// Returns `None` if progress is stalled, unknown, or already complete.
    pub fn eta(&self) -> Option<Duration> {
        if !matches!(self.status, TaskStatus::Running) {
            return None;
        }
        if self.progress_history.len() < 2 {
            return None;
        }
        let now = Instant::now();
        let windowed: Vec<&ProgressSample> = self
            .progress_history
            .iter()
            .filter(|s| now.duration_since(s.at) <= ETA_WINDOW)
            .collect();
        if windowed.len() < 2 {
            return None;
        }
        let first = windowed.first().unwrap();
        let last = windowed.last().unwrap();
        let dp = last.value - first.value;
        let dt = last.at.duration_since(first.at);
        if dt.as_secs_f32() <= 0.0 || dp <= 0.0 {
            return None;
        }
        let rate = dp / dt.as_secs_f32();
        let remaining = (1.0 - self.progress) / rate;
        if !remaining.is_finite() || remaining < 0.0 {
            return None;
        }
        Some(Duration::from_secs_f32(remaining))
    }

    /// Total time spent running so far.
    pub fn elapsed(&self) -> Option<Duration> {
        let start = self.started_at?;
        let end = self.completed_at.unwrap_or_else(Instant::now);
        Some(end.duration_since(start))
    }
}

// Manual Debug -- `work` is a `Box<dyn FnOnce>` which has no Debug.
impl std::fmt::Debug for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Task")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("description", &self.description)
            .field("group", &self.group)
            .field("dependencies", &self.dependencies)
            .field("status", &self.status)
            .field("progress", &self.progress)
            .field("message", &self.message)
            .field("started_at", &self.started_at)
            .field("completed_at", &self.completed_at)
            .field("work", &"<closure>")
            .field("cancelled", &self.cancelled)
            .finish()
    }
}

// Snapshotting a Task clones everything except the single-use work closure.
impl Clone for Task {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            description: self.description.clone(),
            group: self.group,
            dependencies: self.dependencies.clone(),
            work: None,
            status: self.status.clone(),
            progress: self.progress,
            message: self.message.clone(),
            started_at: self.started_at,
            completed_at: self.completed_at,
            progress_history: self.progress_history.clone(),
            cancelled: self.cancelled,
        }
    }
}

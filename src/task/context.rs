use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::task::task::TaskId;

/// Messages a running task can send back to the [`crate::task::manager::TaskManager`].
#[derive(Debug, Clone)]
pub(crate) enum ProgressUpdate {
    /// Update progress to a value in `0.0..=1.0`.
    Progress(TaskId, f32),
    /// Update the human-readable status message.
    Message(TaskId, String),
    /// Task finished successfully.
    Done(TaskId),
    /// Task finished with an error.
    Failed(TaskId, String),
}

/// A handle handed to a work closure while a task is running.
///
/// Cloning is cheap -- each clone shares the same underlying cancellation flag.
#[derive(Clone)]
pub struct TaskContext {
    pub(crate) id: TaskId,
    pub(crate) tx: Sender<ProgressUpdate>,
    pub(crate) cancel_flag: Arc<AtomicBool>,
    pub(crate) last_progress: Arc<Mutex<f32>>,
}

impl TaskContext {
    /// Report progress as a fraction in `0.0..=1.0`. Values outside this range
    /// are clamped.
    ///
    /// Backed by the unbounded `std::sync::mpsc` channel, so this call is
    /// effectively non-blocking and never panics on a full channel.
    pub fn set_progress(&self, fraction: f32) {
        let v = fraction.clamp(0.0, 1.0);
        {
            let mut guard = self.last_progress.lock().expect("progress lock poisoned");
            *guard = v;
        }
        let _ = self.tx.send(ProgressUpdate::Progress(self.id, v));
    }

    /// Set a human-readable status message displayed next to the progress bar.
    pub fn set_message(&self, msg: impl Into<String>) {
        let _ = self.tx.send(ProgressUpdate::Message(self.id, msg.into()));
    }

    /// Convenience: increment progress by `delta` (clamped to `1.0`).
    pub fn advance(&self, delta: f32) {
        let cur = *self.last_progress.lock().expect("progress lock poisoned");
        self.set_progress(cur + delta);
    }

    /// Returns `true` if the user (or manager) has requested cancellation.
    ///
    /// Long-running work should poll this periodically.
    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::Relaxed)
    }

    /// The id of the task this context belongs to.
    pub fn task_id(&self) -> TaskId {
        self.id
    }
}

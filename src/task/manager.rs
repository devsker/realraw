use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::task::context::{ProgressUpdate, TaskContext};
use crate::task::group::{GroupId, GroupTree, TaskGroup};
use crate::task::task::{ProgressSample, PROGRESS_HISTORY_CAP, Task, TaskId, TaskStatus, WorkFn};

/// A read-only snapshot of the manager's state, suitable for the UI thread
/// to render without taking a lock.
#[derive(Debug, Clone, Default)]
pub struct TaskSnapshot {
    pub tasks: Vec<Task>,
    pub groups: Vec<TaskGroup>,
    /// `group_id -> aggregate (completed, total, fraction in 0..1)`
    pub group_progress: HashMap<GroupId, GroupProgress>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GroupProgress {
    pub completed: usize,
    pub total: usize,
    pub fraction: f32,
}

struct WorkerHandle {
    task_id: TaskId,
    join: Option<JoinHandle<()>>,
}

/// The task manager: holds tasks, schedules eligible ones onto worker
/// threads, and provides a [`TaskSnapshot`] for the UI.
///
/// Typical lifecycle:
/// ```ignore
/// let mut mgr = TaskManager::new();
/// mgr.add_group("Import", None);
/// let id = mgr.add_task(Task::new("copy", "...").work(|ctx| { ... }));
/// mgr.start();
/// // each frame:
/// mgr.sync();
/// let snap = mgr.snapshot();
/// ```
pub struct TaskManager {
    tasks: HashMap<TaskId, Task>,
    task_order: VecDeque<TaskId>,
    groups: GroupTree,
    next_task_id: u64,
    next_group_id: u64,
    progress_tx: Sender<ProgressUpdate>,
    progress_rx: Receiver<ProgressUpdate>,
    cancel_flags: HashMap<TaskId, Arc<AtomicBool>>,
    workers: Vec<WorkerHandle>,
    started: bool,
    max_concurrency: usize,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskManager {
    pub fn new() -> Self {
        let (progress_tx, progress_rx) = channel();
        Self {
            tasks: HashMap::new(),
            task_order: VecDeque::new(),
            groups: GroupTree::default(),
            next_task_id: 1,
            next_group_id: 1,
            progress_tx,
            progress_rx,
            cancel_flags: HashMap::new(),
            workers: Vec::new(),
            started: false,
            max_concurrency: 4,
        }
    }

    /// Set the maximum number of tasks that may execute concurrently.
    /// Must be called before [`start`](Self::start) to take effect on the
    /// initial scheduling pass.
    pub fn set_max_concurrency(mut self, n: usize) -> Self {
        self.max_concurrency = n.max(1);
        self
    }

    /// Add a group. Returns the group's id.
    pub fn add_group(&mut self, name: impl Into<String>, parent: Option<GroupId>) -> GroupId {
        let mut g = TaskGroup::new(name);
        g.id = self.next_group_id;
        g.parent = parent;
        self.next_group_id += 1;
        self.groups.insert(g)
    }

    /// Add a task. Returns the task's id.
    pub fn add_task(&mut self, mut task: Task) -> TaskId {
        assert!(task.work.is_some(), "task must have a work closure");
        let id = self.next_task_id;
        self.next_task_id += 1;
        task.id = id;
        self.tasks.insert(id, task);
        self.task_order.push_back(id);
        id
    }

    /// Begin scheduling. Tasks with no unmet dependencies are spawned
    /// immediately (up to `max_concurrency`).
    pub fn start(&mut self) {
        self.started = true;
        self.schedule_ready();
    }

    /// Drain progress / completion messages and update internal state.
    /// Should be called from the UI thread every frame.
    pub fn sync(&mut self) {
        // Non-blocking drain.
        while let Ok(update) = self.progress_rx.try_recv() {
            self.apply_update(update);
        }
        self.reap_finished_workers();
        self.schedule_ready();
    }

    /// Take a cheap, cloneable snapshot for the UI to render.
    pub fn snapshot(&self) -> TaskSnapshot {
        let mut group_progress: HashMap<GroupId, GroupProgress> = HashMap::new();

        // First pass: count completed/total per group.
        for task in self.tasks.values() {
            let Some(gid) = task.group else { continue };
            let entry = group_progress.entry(gid).or_default();
            entry.total += 1;
            if matches!(task.status, TaskStatus::Completed) {
                entry.completed += 1;
            }
        }

        // Second pass: fold counts upward through the group tree.
        let all_groups: Vec<GroupId> = self.groups.by_id.keys().copied().collect();
        for &gid in &all_groups {
            let own = group_progress.get(&gid).copied().unwrap_or_default();
            let mut completed = own.completed;
            let mut total = own.total;
            for &child in self.groups.children_of(gid) {
                let c = group_progress.get(&child).copied().unwrap_or_default();
                completed += c.completed;
                total += c.total;
            }
            let fraction = if total == 0 { 0.0 } else { completed as f32 / total as f32 };
            group_progress.insert(gid, GroupProgress { completed, total, fraction });
        }

        TaskSnapshot {
            tasks: self.tasks.values().cloned().collect(),
            groups: self.groups.by_id.values().cloned().collect(),
            group_progress,
        }
    }

    /// Request cancellation of a single task. The work closure will see
    /// `is_cancelled()` return `true` on its next poll.
    pub fn cancel(&mut self, id: TaskId) {
        if let Some(flag) = self.cancel_flags.get(&id) {
            flag.store(true, Ordering::Relaxed);
        }
        if let Some(t) = self.tasks.get_mut(&id)
            && !t.status.is_terminal()
        {
            t.status = TaskStatus::Cancelled;
            t.completed_at = Some(Instant::now());
        }
    }

    /// Request cancellation of every task in a group (and sub-groups).
    pub fn cancel_group(&mut self, gid: GroupId) {
        let descendants: Vec<GroupId> = self.groups.walk(gid);
        let tasks: Vec<TaskId> = self
            .tasks
            .values()
            .filter(|t| t.group.map(|g| descendants.contains(&g)).unwrap_or(false))
            .map(|t| t.id)
            .collect();
        for id in tasks {
            self.cancel(id);
        }
    }

    /// Toggle the collapsed state of a group in the UI.
    pub fn set_group_collapsed(&mut self, gid: GroupId, collapsed: bool) {
        if let Some(g) = self.groups.by_id.get_mut(&gid) {
            g.collapsed = collapsed;
        }
    }

    /// True if every task is in a terminal state.
    pub fn all_done(&self) -> bool {
        self.tasks.values().all(|t| t.status.is_terminal())
    }

    /// Look up a task by id (cloned).
    pub fn get_task(&self, id: TaskId) -> Option<Task> {
        self.tasks.get(&id).cloned()
    }

    /// Look up a group by id (cloned).
    pub fn get_group(&self, gid: GroupId) -> Option<TaskGroup> {
        self.groups.by_id.get(&gid).cloned()
    }

    // ---------- internals ----------

    fn apply_update(&mut self, update: ProgressUpdate) {
        match update {
            ProgressUpdate::Progress(id, v) => {
                if let Some(t) = self.tasks.get_mut(&id)
                    && !t.status.is_terminal()
                {
                    t.progress = v;
                    let now = Instant::now();
                    if t
                        .progress_history
                        .last()
                        .is_none_or(|s| now.duration_since(s.at) > Duration::from_millis(50))
                    {
                        t.progress_history.push(ProgressSample { at: now, value: v });
                        if t.progress_history.len() > PROGRESS_HISTORY_CAP {
                            t.progress_history.remove(0);
                        }
                    }
                }
            }
            ProgressUpdate::Message(id, msg) => {
                if let Some(t) = self.tasks.get_mut(&id) {
                    t.message = msg;
                }
            }
            ProgressUpdate::Done(id) => {
                if let Some(t) = self.tasks.get_mut(&id)
                    && !t.status.is_terminal()
                {
                    t.status = TaskStatus::Completed;
                    t.progress = 1.0;
                    t.completed_at = Some(Instant::now());
                }
            }
            ProgressUpdate::Failed(id, err) => {
                if let Some(t) = self.tasks.get_mut(&id)
                    && !t.status.is_terminal()
                {
                    t.status = TaskStatus::Failed(err);
                    t.completed_at = Some(Instant::now());
                }
            }
        }
    }

    fn reap_finished_workers(&mut self) {
        self.workers.retain_mut(|w| {
            if w.join.as_ref().is_some_and(|j| j.is_finished()) {
                if let Some(j) = w.join.take() {
                    let _ = j.join();
                }
                false
            } else {
                true
            }
        });
    }

    fn schedule_ready(&mut self) {
        // Free a slot if we are at capacity.
        self.reap_finished_workers();

        let active = self
            .workers
            .iter()
            .filter(|w| {
                self.tasks
                    .get(&w.task_id)
                    .is_some_and(|t| !t.status.is_terminal())
            })
            .count();

        let mut available_slots = self.max_concurrency.saturating_sub(active);
        if available_slots == 0 {
            return;
        }

        // Find tasks that are Pending, all dependencies are Completed,
        // and haven't been spawned yet. Iterate in insertion order for
        // deterministic scheduling.
        let mut to_spawn: Vec<TaskId> = Vec::new();
        for &id in &self.task_order {
            if available_slots == 0 {
                break;
            }
            let Some(task) = self.tasks.get(&id) else { continue };
            if !matches!(task.status, TaskStatus::Pending) {
                continue;
            }
            if !self.deps_satisfied(id) {
                continue;
            }
            // Already has a worker? skip.
            if self.workers.iter().any(|w| w.task_id == id) {
                continue;
            }
            to_spawn.push(id);
            available_slots -= 1;
        }

        for id in to_spawn {
            self.spawn(id);
        }
    }

    fn deps_satisfied(&self, id: TaskId) -> bool {
        let Some(task) = self.tasks.get(&id) else { return false };
        for &dep in &task.dependencies {
            match self.tasks.get(&dep) {
                Some(t) if matches!(t.status, TaskStatus::Completed) => {}
                _ => return false,
            }
        }
        true
    }

    fn spawn(&mut self, id: TaskId) {
        // Mark as Running and grab the work closure.
        let task = match self.tasks.get_mut(&id) {
            Some(t) => t,
            None => return,
        };
        task.status = TaskStatus::Running;
        task.started_at = Some(Instant::now());
        let work: WorkFn = match task.work.take() {
            Some(w) => w,
            None => return,
        };

        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.cancel_flags.insert(id, cancel_flag.clone());
        let ctx = TaskContext {
            id,
            tx: self.progress_tx.clone(),
            cancel_flag,
            last_progress: Arc::new(Mutex::new(0.0)),
        };

        let tx = self.progress_tx.clone();
        let join = thread::Builder::new()
            .name(format!("realraw-task-{id}"))
            .spawn(move || {
                let result = work(&ctx);
                match result {
                    Ok(()) => {
                        let _ = tx.send(ProgressUpdate::Done(id));
                    }
                    Err(e) => {
                        let _ = tx.send(ProgressUpdate::Failed(id, e));
                    }
                }
            })
            .expect("failed to spawn task worker");

        self.workers.push(WorkerHandle { task_id: id, join: Some(join) });
    }
}

// Implement Clone for Task (used by snapshot) -- the work closure is
// `Option<Box<dyn FnOnce ...>>` so we need a manual impl that drops it.

/// Format a `Duration` as `"1h 23m 04s"`, `"2m 34s"`, `"12.3s"`, or `"0ms"`.
pub fn format_eta(d: Duration) -> String {
    let total_ms = d.as_millis();
    if total_ms < 1000 {
        return format!("{total_ms}ms");
    }
    let total_secs = d.as_secs();
    let ms = total_ms % 1000;
    if total_secs < 60 {
        return format!("{}.{:02}s", total_secs, ms / 10);
    }
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    if mins < 60 {
        return format!("{mins}m {secs:02}s");
    }
    let hours = mins / 60;
    let mins = mins % 60;
    format!("{hours}h {mins:02}m {secs:02}s")
}

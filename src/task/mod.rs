//! Task system for running background work with progress reporting,
//! dependencies, smart grouping, and egui progress widgets.
//!
//! # Quick start
//!
//! ```
//! use realraw::task::{Task, TaskManager};
//!
//! let mut mgr = TaskManager::new();
//! let import_group = mgr.add_group("Import", None);
//! let tid = mgr.add_task(
//!     Task::new("Import photos", "Copying from /DCIM")
//!         .group(import_group)
//!         .work(|ctx| {
//!             for i in 0..=100 {
//!                 ctx.set_progress(i as f32 / 100.0);
//!                 std::thread::sleep(std::time::Duration::from_millis(20));
//!             }
//!             Ok(())
//!         }),
//! );
//! mgr.start();
//! ```

mod context;
mod group;
mod manager;
#[allow(clippy::module_inception)]
mod task;
mod widget;

pub use context::TaskContext;
pub use group::{GroupId, TaskGroup};
pub use manager::{format_eta, GroupProgress, TaskManager, TaskSnapshot};
pub use task::{Task, TaskId, TaskStatus, WorkFn};
pub use widget::{task_progress_bar, task_tree, TaskCommand, TaskViewOptions};

use std::collections::HashMap;

use eframe::egui::{self, Color32, RichText, Ui};

use crate::task::group::{GroupId, TaskGroup};
use crate::task::manager::{format_eta, GroupProgress, TaskSnapshot};
use crate::task::task::{Task, TaskId, TaskStatus};

/// User actions the UI can request from the host application.
#[derive(Debug, Clone)]
pub enum TaskCommand {
    /// Cancel a single task.
    CancelTask(TaskId),
    /// Cancel every task in a group (and its sub-groups).
    CancelGroup(GroupId),
    /// Toggle the collapsed state of a group header.
    ToggleGroup(GroupId, bool),
}

/// UI options for [`task_tree`].
#[derive(Debug, Clone)]
pub struct TaskViewOptions {
    /// Show the "Cancel" button next to tasks / groups.
    pub show_cancel: bool,
    /// Show elapsed time next to tasks.
    pub show_elapsed: bool,
    /// Show ETA next to running tasks.
    pub show_eta: bool,
    /// Show the description line below the name.
    pub show_description: bool,
    /// Auto-group tasks whose name contains `::`, `:` or `/` under a
    /// synthetic group based on the prefix.
    pub auto_group_by_separator: bool,
    /// Render each task on a single line (status dot, name, bar, %, eta,
    /// cancel button) with no description, no per-task indents, and no
    /// "Cancel group" button. Suitable for status-bar / dropdown UIs.
    pub compact: bool,
    /// Render tasks as a flat list with no group / category headers at all.
    pub flat: bool,
    /// If true, only tasks currently in [`TaskStatus::Running`] are shown.
    pub only_running: bool,
}

impl Default for TaskViewOptions {
    fn default() -> Self {
        Self {
            show_cancel: true,
            show_elapsed: true,
            show_eta: true,
            show_description: false,
            auto_group_by_separator: true,
            compact: false,
            flat: false,
            only_running: false,
        }
    }
}

/// Render a single progress bar for a task. Returns the bar's `Response` so
/// callers can detect clicks / hover.
pub fn task_progress_bar(ui: &mut Ui, task: &Task) -> egui::Response {
    let fraction = task.progress();
    let desired = if matches!(task.status, TaskStatus::Running) {
        egui::ProgressBar::new(fraction).show_percentage()
    } else if matches!(task.status, TaskStatus::Pending | TaskStatus::Ready) {
        egui::ProgressBar::new(0.0).show_percentage()
    } else {
        egui::ProgressBar::new(fraction).show_percentage()
    };
    ui.add(desired)
}

/// Render a tree view of all tasks and groups in the snapshot.
///
/// `on_command` is called whenever the user clicks a cancel / collapse
/// button. Callers route the command into their [`TaskManager`].
pub fn task_tree(
    ui: &mut Ui,
    snapshot: &TaskSnapshot,
    opts: &TaskViewOptions,
    on_command: &mut dyn FnMut(TaskCommand),
) {
    if opts.flat {
        render_flat(ui, snapshot, opts, on_command);
    } else if opts.auto_group_by_separator {
        render_with_auto_groups(ui, snapshot, opts, on_command);
    } else {
        render_plain(ui, snapshot, opts, on_command);
    }
}

fn render_flat(
    ui: &mut Ui,
    snapshot: &TaskSnapshot,
    opts: &TaskViewOptions,
    on_command: &mut dyn FnMut(TaskCommand),
) {
    for task in &snapshot.tasks {
        if opts.only_running && !matches!(task.status(), TaskStatus::Running) {
            continue;
        }
        render_task(ui, task, opts, on_command);
    }
}

fn render_plain(
    ui: &mut Ui,
    snapshot: &TaskSnapshot,
    opts: &TaskViewOptions,
    on_command: &mut dyn FnMut(TaskCommand),
) {
    let group_by_id: HashMap<GroupId, &TaskGroup> =
        snapshot.groups.iter().map(|g| (g.id(), g)).collect();
    let group_progress: HashMap<GroupId, GroupProgress> = snapshot.group_progress.clone();

    let roots: Vec<GroupId> = snapshot
        .groups
        .iter()
        .filter(|g| g.parent_id().is_none())
        .map(|g| g.id())
        .collect();

    for gid in roots {
        render_group(ui, gid, &group_by_id, &group_progress, snapshot, opts, on_command);
    }

    // Tasks not in any group:
    let orphan_tasks: Vec<&Task> = snapshot
        .tasks
        .iter()
        .filter(|t| t.group_id().is_none())
        .collect();
    for task in orphan_tasks {
        render_task(ui, task, opts, on_command);
    }
}

fn render_group(
    ui: &mut Ui,
    gid: GroupId,
    group_by_id: &HashMap<GroupId, &TaskGroup>,
    group_progress: &HashMap<GroupId, GroupProgress>,
    snapshot: &TaskSnapshot,
    opts: &TaskViewOptions,
    on_command: &mut dyn FnMut(TaskCommand),
) {
    let Some(group) = group_by_id.get(&gid).copied() else { return };
    let gp = group_progress.get(&gid).copied().unwrap_or_default();
    let header = format!("{}  ({}/{})", group.name(), gp.completed, gp.total);
    let id = ui.id().with(("group", gid));
    let collapsing = egui::CollapsingHeader::new(RichText::new(header).strong())
        .id_salt(id)
        .default_open(!group.is_collapsed())
        .show(ui, |ui| {
            for child in children_of(snapshot, gid) {
                render_group(ui, child, group_by_id, group_progress, snapshot, opts, on_command);
            }
            for task in snapshot.tasks.iter().filter(|t| t.group_id() == Some(gid)) {
                render_task(ui, task, opts, on_command);
            }
        });
    let was_open = !group.is_collapsed();
    if collapsing.header_response.clicked() {
        on_command(TaskCommand::ToggleGroup(gid, was_open));
    }
    if !opts.compact && opts.show_cancel && gp.total > gp.completed {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            if ui.small_button("Cancel group").clicked() {
                on_command(TaskCommand::CancelGroup(gid));
            }
        });
    }
}

fn children_of(snapshot: &TaskSnapshot, parent: GroupId) -> Vec<GroupId> {
    snapshot
        .groups
        .iter()
        .filter(|g| g.parent_id() == Some(parent))
        .map(|g| g.id())
        .collect()
}

fn render_task(
    ui: &mut Ui,
    task: &Task,
    opts: &TaskViewOptions,
    on_command: &mut dyn FnMut(TaskCommand),
) {
    if opts.compact {
        render_task_compact(ui, task, opts, on_command);
    } else {
        render_task_full(ui, task, opts, on_command);
    }
}

fn render_task_compact(
    ui: &mut Ui,
    task: &Task,
    opts: &TaskViewOptions,
    on_command: &mut dyn FnMut(TaskCommand),
) {
    ui.horizontal(|ui| {
        status_dot(ui, task.status());
        ui.label(RichText::new(task.name()).small());
        ui.add(
            egui::ProgressBar::new(task.progress())
                .desired_width(ui.available_width() * 0.5)
                .show_percentage(),
        );
        if matches!(task.status(), TaskStatus::Running) {
            if let Some(eta) = task.eta()
                && opts.show_eta
            {
                ui.label(RichText::new(format_eta(eta)).small().weak());
            }
        } else if let TaskStatus::Failed(err) = task.status() {
            ui.label(RichText::new(err).small().color(Color32::LIGHT_RED));
        }
        if opts.show_cancel
            && !task.status().is_terminal()
            && ui.small_button("x").clicked()
        {
            on_command(TaskCommand::CancelTask(task.id()));
        }
    });
}

fn render_task_full(
    ui: &mut Ui,
    task: &Task,
    opts: &TaskViewOptions,
    on_command: &mut dyn FnMut(TaskCommand),
) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        status_dot(ui, task.status());
        ui.label(task.name());
        if opts.show_cancel
            && !task.status().is_terminal()
            && ui.small_button("x").clicked()
        {
            on_command(TaskCommand::CancelTask(task.id()));
        }
    });
    if opts.show_description && !task.description().is_empty() {
        ui.indent(("desc", task.id()), |ui| {
            ui.label(RichText::new(task.description()).small().weak());
        });
    }
    ui.indent(("bar", task.id()), |ui| {
        task_progress_bar(ui, task);
        ui.horizontal(|ui| {
            if !task.message().is_empty() {
                ui.label(RichText::new(task.message()).small().italics());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if matches!(task.status(), TaskStatus::Running) {
                    if let Some(eta) = task.eta()
                        && opts.show_eta
                    {
                        ui.label(RichText::new(format!("eta {}", format_eta(eta))).small());
                    }
                } else if let Some(elapsed) = task.elapsed()
                    && opts.show_elapsed
                {
                    let label = match task.status() {
                        TaskStatus::Completed => format!("took {}", format_eta(elapsed)),
                        TaskStatus::Failed(_) => format!("failed after {}", format_eta(elapsed)),
                        TaskStatus::Cancelled => format!("cancelled after {}", format_eta(elapsed)),
                        _ => format_eta(elapsed),
                    };
                    ui.label(RichText::new(label).small().weak());
                }
                if let TaskStatus::Failed(err) = task.status() {
                    ui.label(RichText::new(err).small().color(Color32::LIGHT_RED));
                }
            });
        });
    });
}

fn status_dot(ui: &mut Ui, status: &TaskStatus) {
    let (color, glyph) = match status {
        TaskStatus::Pending => (Color32::GRAY, "..."),
        TaskStatus::Ready => (Color32::from_rgb(180, 180, 220), "->"),
        TaskStatus::Running => (Color32::from_rgb(255, 200, 80), ">>"),
        TaskStatus::Completed => (Color32::from_rgb(80, 200, 120), "OK"),
        TaskStatus::Failed(_) => (Color32::from_rgb(220, 80, 80), "!!"),
        TaskStatus::Cancelled => (Color32::from_rgb(160, 160, 160), "--"),
    };
    ui.label(RichText::new(glyph).monospace().color(color));
}

/// When `auto_group_by_separator` is enabled, tasks whose names contain
/// `"::"`, `":"` or `"/"` are visually grouped under a synthetic header
/// derived from the prefix. This lets callers throw a flat list of tasks
/// at the UI and get smart grouping for free.
fn render_with_auto_groups(
    ui: &mut Ui,
    snapshot: &TaskSnapshot,
    opts: &TaskViewOptions,
    on_command: &mut dyn FnMut(TaskCommand),
) {
    let group_by_id: HashMap<GroupId, &TaskGroup> =
        snapshot.groups.iter().map(|g| (g.id(), g)).collect();
    let group_progress: HashMap<GroupId, GroupProgress> = snapshot.group_progress.clone();
    let roots: Vec<GroupId> = snapshot
        .groups
        .iter()
        .filter(|g| g.parent_id().is_none())
        .map(|g| g.id())
        .collect();
    for gid in roots {
        render_group(ui, gid, &group_by_id, &group_progress, snapshot, opts, on_command);
    }

    // Bucket ungrouped tasks by prefix.
    let mut buckets: std::collections::BTreeMap<String, Vec<&Task>> =
        std::collections::BTreeMap::new();
    let mut loose: Vec<&Task> = Vec::new();
    for task in snapshot.tasks.iter().filter(|t| t.group_id().is_none()) {
        match extract_prefix(task.name()) {
            Some(p) => buckets.entry(p).or_default().push(task),
            None => loose.push(task),
        }
    }

    for (prefix, tasks) in buckets {
        let completed = tasks
            .iter()
            .filter(|t| matches!(t.status(), TaskStatus::Completed))
            .count();
        let header = format!("{prefix}  ({}/{})", completed, tasks.len());
        egui::CollapsingHeader::new(RichText::new(header).strong())
            .id_salt(ui.id().with(("autogroup", &prefix)))
            .show(ui, |ui| {
                for task in tasks {
                    render_task(ui, task, opts, on_command);
                }
            });
    }

    for task in loose {
        render_task(ui, task, opts, on_command);
    }
}

fn extract_prefix(name: &str) -> Option<String> {
    for sep in ["::", "/", ":"] {
        if let Some(idx) = name.find(sep) {
            return Some(name[..idx].to_string());
        }
    }
    None
}

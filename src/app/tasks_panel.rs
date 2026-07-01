use eframe::egui;

use crate::app::App;
use crate::task::{TaskCommand, TaskManager, TaskViewOptions};

pub(crate) fn render(
    app: &mut App,
    ctx: &egui::Context,
    has_running: bool,
    running: usize,
    total: usize,
) {
    egui::TopBottomPanel::bottom("background_tasks")
        .resizable(false)
        .show_animated(ctx, app.tasks_open, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Running");
                if has_running {
                    ui.weak(format!("({running})"));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("x").on_hover_text("Close").clicked() {
                        app.tasks_open = false;
                    }
                    if has_running && ui.small_button("Cancel all").clicked() {
                        cancel_all_non_terminal(app);
                    }
                    if total > 0 && !has_running && ui.small_button("Clear").clicked() {
                        app.task_manager = TaskManager::new().set_max_concurrency(4);
                    }
                });
            });
            ui.separator();

            let opts = TaskViewOptions {
                compact: true,
                flat: true,
                only_running: true,
                ..TaskViewOptions::default()
            };
            let mut on_command = |cmd: TaskCommand| match cmd {
                TaskCommand::CancelTask(id) => app.task_manager.cancel(id),
                TaskCommand::CancelGroup(gid) => app.task_manager.cancel_group(gid),
                TaskCommand::ToggleGroup(gid, collapsed) => {
                    app.task_manager.set_group_collapsed(gid, collapsed)
                }
            };
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if has_running {
                        crate::task::task_tree(
                            ui,
                            &app.last_snapshot,
                            &opts,
                            &mut on_command,
                        );
                    } else {
                        ui.weak("Nothing running.");
                    }
                });
        });
}

fn cancel_all_non_terminal(app: &mut App) {
    let ids: Vec<_> = app
        .last_snapshot
        .tasks
        .iter()
        .filter(|t| !t.status().is_terminal())
        .map(|t| t.id())
        .collect();
    for id in ids {
        app.task_manager.cancel(id);
    }
}

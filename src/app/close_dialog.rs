use eframe::egui;

use crate::app::App;

pub(crate) fn render(app: &mut App, ctx: &egui::Context) {
    if app.close_press_count >= 3 {
        app.show_close_dialog = false;
        app.closing = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        return;
    }

    let mut confirmed = false;
    let mut cancelled = false;

    egui::Modal::new(egui::Id::new("close_dialog"))
        .show(ctx, |ui| {
            ui.heading("Quit realraw");
            ui.label("Are you sure to quit realraw?");
            if app.close_press_count >= 2 {
                ui.label("(Press again to quit)");
            }
            ui.add_space(12.0);

            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    cancelled = true;
                }
                if ui.button("Quit").clicked() {
                    confirmed = true;
                }
            });
        });

    if cancelled {
        app.show_close_dialog = false;
        app.close_press_count = 0;
    } else if confirmed {
        app.show_close_dialog = false;
        app.closing = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    } else if app.show_close_dialog {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
    }
}

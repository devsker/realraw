use eframe::egui;

use crate::app::App;

pub(crate) fn render(app: &mut App, ctx: &egui::Context) {
    // let mut catalog_created = false;

    let mut confirmed = false;
    let mut cancelled = false;

    egui::Modal::new(egui::Id::new("close_dialog"))
        .show(ctx, |ui| {
            ui.heading("Quit realraw");
            ui.label("Are you sure to quit realraw?");
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
    } else if confirmed {
        app.show_close_dialog = false;
        app.closing = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    } else if app.show_close_dialog {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
    }
}

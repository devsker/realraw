use eframe::egui;

use crate::app::App;

pub(crate) fn render(app: &mut App, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if let Some(cat) = &app.catalog {
                if let Ok(counts) = cat.counts() {
                    app.catalog_counts = Some(counts);
                }
                let path = cat.display_path();
                ui.strong("catalog:");
                ui.label(&path);
                if let Some(c) = app.catalog_counts {
                    ui.separator();
                    ui.label(format!("photos: {}", c.photos));
                }
            } else if let Some(err) = &app.catalog_error {
                ui.colored_label(egui::Color32::LIGHT_RED, err);
            } else {
                ui.weak("no catalog open");
            }
        });
    });
}

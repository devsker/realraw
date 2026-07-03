use eframe::egui;

use crate::app::App;

/// Develop adjustment settings, matching Lightroom's basic panel.
#[derive(Debug, Clone, PartialEq)]
pub struct DevelopSettings {
    // Light
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    // Presence
    pub clarity: f32,
    pub vibrance: f32,
    pub saturation: f32,
    // Color
    pub temp: f32,
    pub tint: f32,
}

impl Default for DevelopSettings {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            clarity: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            temp: 0.0,
            tint: 0.0,
        }
    }
}

fn slider(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>) {
    ui.add(
        egui::Slider::new(value, range)
            .text(label)
            .show_value(true),
    );
}

fn section_header(ui: &mut egui::Ui, label: &str) {
    ui.label(egui::RichText::new(label).strong().size(13.0));
    ui.separator();
}

/// Render the Develop page with adjustment sliders in a side panel.
pub(crate) fn render(app: &mut App, ctx: &egui::Context) {
    // Right-side adjustment panel (rendered before CentralPanel so it
    // reserves space from the right edge).
    egui::SidePanel::right("develop_adjustments")
        .resizable(false)
        .default_width(260.0)
        .width_range(200.0..=400.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                let s = &mut app.develop;

                section_header(ui, "Light");
                slider(ui, "Exposure", &mut s.exposure, -5.0..=5.0);
                slider(ui, "Contrast", &mut s.contrast, -100.0..=100.0);
                slider(ui, "Highlights", &mut s.highlights, -100.0..=100.0);
                slider(ui, "Shadows", &mut s.shadows, -100.0..=100.0);
                slider(ui, "Whites", &mut s.whites, -100.0..=100.0);
                slider(ui, "Blacks", &mut s.blacks, -100.0..=100.0);

                ui.add_space(12.0);

                section_header(ui, "Presence");
                slider(ui, "Clarity", &mut s.clarity, -100.0..=100.0);
                slider(ui, "Vibrance", &mut s.vibrance, -100.0..=100.0);
                slider(ui, "Saturation", &mut s.saturation, -100.0..=100.0);

                ui.add_space(12.0);

                section_header(ui, "Color");
                slider(ui, "Temp", &mut s.temp, -100.0..=100.0);
                slider(ui, "Tint", &mut s.tint, -100.0..=100.0);
            });
        });

    // Central area (photo preview, empty for now).
    egui::CentralPanel::default().show(ctx, |_ui| {});
}

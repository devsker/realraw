use eframe::egui;

use crate::app::App;

pub(crate) fn render(app: &mut App, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        let mut needs_refresh = app.library_needs_refresh;
        if !needs_refresh
            && let Some(cat) = &app.catalog
        {
            let mtime_ms = std::fs::metadata(cat.path())
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64);
            if mtime_ms != app.library_last_refresh_mtime_ms {
                app.library_last_refresh_mtime_ms = mtime_ms;
                needs_refresh = true;
            }
        }
        if needs_refresh {
            app.library_needs_refresh = false;
            if let Some(cat) = &app.catalog {
                app.library.refresh(cat, None);
            }
        }
        if let Some(cat) = &app.catalog {
            app.library.importing = app.import_summary_rx.is_some();
            let _ = app.library.show(ctx, ui, cat);
        }
    });
}

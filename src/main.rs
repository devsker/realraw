use realraw::app::App;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "realraw",
        options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

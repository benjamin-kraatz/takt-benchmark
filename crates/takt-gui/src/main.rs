mod app;
mod palette;
mod views;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Takt")
            .with_inner_size([1180.0, 760.0]),
        ..Default::default()
    };

    eframe::run_native(
        "takt",
        options,
        Box::new(|creation_context| Ok(Box::new(app::TaktApp::new(creation_context)))),
    )
}

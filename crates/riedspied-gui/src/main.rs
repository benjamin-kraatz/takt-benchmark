mod app;
mod views;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("riedspied")
            .with_inner_size([1180.0, 760.0]),
        ..Default::default()
    };

    eframe::run_native(
        "riedspied",
        options,
        Box::new(|creation_context| Ok(Box::new(app::RiedspiedApp::new(creation_context)))),
    )
}

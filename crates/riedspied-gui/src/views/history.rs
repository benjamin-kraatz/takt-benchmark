use eframe::egui;
use riedspied_core::BenchmarkRunRecord;

pub fn show_history(ui: &mut egui::Ui, history: &[BenchmarkRunRecord]) {
    ui.heading("Local History");
    if history.is_empty() {
        ui.label("No saved benchmark runs yet.");
        return;
    }

    egui::ScrollArea::vertical()
        .max_height(240.0)
        .show(ui, |ui| {
            for record in history.iter().take(12) {
                ui.group(|ui| {
                    ui.label(format!(
                        "{}  {}  {}",
                        record.started_at.format("%Y-%m-%d %H:%M:%S"),
                        record.target.name,
                        record.profile.preset,
                    ));
                    for result in &record.results {
                        ui.label(format!(
                            "{} avg {:.1} MiB/s peak {:.1} MiB/s{}",
                            result.benchmark.label(),
                            result.average_mbps,
                            result.peak_mbps,
                            result
                                .iops
                                .map(|value| format!(" iops {:.0}", value))
                                .unwrap_or_default(),
                        ));
                    }
                });
            }
        });
}

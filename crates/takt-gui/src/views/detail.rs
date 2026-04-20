use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use takt_core::{BenchmarkRunRecord, BenchmarkType};

pub fn show_run_detail(ui: &mut egui::Ui, run: &BenchmarkRunRecord) {
    ui.heading("Run Details");
    ui.label(format!("Run ID: {}", run.run_id));
    ui.label(format!(
        "{} on {} using {} profile",
        run.target.name,
        run.target.mount_point.display(),
        run.profile.preset,
    ));

    let overview_points = PlotPoints::from_iter(BenchmarkType::ALL.iter().enumerate().filter_map(
        |(index, benchmark)| {
            run.result_for(*benchmark)
                .map(|result| [index as f64, result.average_mbps])
        },
    ));
    Plot::new(format!("detail-overview-{}", run.run_id))
        .height(180.0)
        .include_y(0.0)
        .show(ui, |plot_ui| {
            plot_ui.line(Line::new("Average throughput", overview_points));
        });

    egui::Grid::new("detail-grid").striped(true).show(ui, |ui| {
        ui.strong("Benchmark");
        ui.strong("Average MiB/s");
        ui.strong("Peak MiB/s");
        ui.strong("Min MiB/s");
        ui.strong("IOPS");
        ui.strong("P95 ms");
        ui.end_row();

        for result in &run.results {
            ui.label(result.benchmark.label());
            ui.label(format!("{:.1}", result.average_mbps));
            ui.label(format!("{:.1}", result.peak_mbps));
            ui.label(format!("{:.1}", result.minimum_mbps));
            ui.label(
                result
                    .iops
                    .map(|value| format!("{value:.0}"))
                    .unwrap_or_else(|| "-".to_string()),
            );
            ui.label(
                result
                    .latency_ms_p95
                    .map(|value| format!("{value:.2}"))
                    .unwrap_or_else(|| "-".to_string()),
            );
            ui.end_row();
        }
    });

    ui.separator();
    for result in &run.results {
        ui.collapsing(result.benchmark.label(), |ui| {
            ui.label(format!(
                "Duration {:.1}s, processed {:.1} MiB",
                result.duration_secs,
                result.bytes_processed as f64 / 1024.0 / 1024.0,
            ));
            if result.samples.is_empty() {
                ui.label("No time-series samples captured for this benchmark.");
                return;
            }

            let points = PlotPoints::from_iter(
                result
                    .samples
                    .iter()
                    .map(|sample| [sample.seconds, sample.throughput_mbps]),
            );
            Plot::new(format!("detail-{}-{}", run.run_id, result.benchmark.slug()))
                .height(160.0)
                .include_x(0.0)
                .include_y(0.0)
                .show(ui, |plot_ui| {
                    plot_ui.line(Line::new(result.benchmark.label(), points));
                });
        });
    }
}

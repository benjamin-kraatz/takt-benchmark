use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use riedspied_core::{BenchmarkRunRecord, BenchmarkType};

pub fn show_trend_view(ui: &mut egui::Ui, runs: &[BenchmarkRunRecord]) {
    ui.heading("Device Trend");
    if runs.is_empty() {
        ui.label("Select a device or run to see trend history.");
        return;
    }

    Plot::new("trend-plot")
        .height(220.0)
        .include_y(0.0)
        .show(ui, |plot_ui| {
            for benchmark in BenchmarkType::ALL {
                let points =
                    PlotPoints::from_iter(runs.iter().enumerate().filter_map(|(index, run)| {
                        run.result_for(benchmark)
                            .map(|result| [index as f64, result.average_mbps])
                    }));
                plot_ui.line(Line::new(benchmark.label(), points));
            }
        });
}

pub fn show_two_run_comparison(
    ui: &mut egui::Ui,
    left: &BenchmarkRunRecord,
    right: &BenchmarkRunRecord,
) {
    ui.heading("Direct Comparison");
    ui.label(format!(
        "{}  vs  {}",
        left.display_name(),
        right.display_name()
    ));

    egui::Grid::new("comparison-grid")
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Benchmark");
            ui.strong("Left Avg");
            ui.strong("Right Avg");
            ui.strong("Delta");
            ui.end_row();

            for benchmark in BenchmarkType::ALL {
                let left_value = left.result_for(benchmark).map(|result| result.average_mbps);
                let right_value = right
                    .result_for(benchmark)
                    .map(|result| result.average_mbps);
                ui.label(benchmark.label());
                ui.label(
                    left_value
                        .map(|value| format!("{value:.1}"))
                        .unwrap_or_else(|| "-".to_string()),
                );
                ui.label(
                    right_value
                        .map(|value| format!("{value:.1}"))
                        .unwrap_or_else(|| "-".to_string()),
                );
                ui.label(match (left_value, right_value) {
                    (Some(left_value), Some(right_value)) => {
                        format!("{:+.1}", right_value - left_value)
                    }
                    _ => "-".to_string(),
                });
                ui.end_row();
            }
        });

    for benchmark in BenchmarkType::ALL {
        let left_result = left.result_for(benchmark);
        let right_result = right.result_for(benchmark);
        if left_result.is_none() && right_result.is_none() {
            continue;
        }

        ui.collapsing(format!("{} overlay", benchmark.label()), |ui| {
            Plot::new(format!("compare-{}", benchmark.slug()))
                .height(160.0)
                .include_y(0.0)
                .show(ui, |plot_ui| {
                    if let Some(result) = left_result {
                        plot_ui.line(Line::new(
                            format!("Left: {}", left.started_at.format("%m-%d %H:%M")),
                            PlotPoints::from_iter(
                                result
                                    .samples
                                    .iter()
                                    .map(|sample| [sample.seconds, sample.throughput_mbps]),
                            ),
                        ));
                    }
                    if let Some(result) = right_result {
                        plot_ui.line(Line::new(
                            format!("Right: {}", right.started_at.format("%m-%d %H:%M")),
                            PlotPoints::from_iter(
                                result
                                    .samples
                                    .iter()
                                    .map(|sample| [sample.seconds, sample.throughput_mbps]),
                            ),
                        ));
                    }
                });
        });
    }
}

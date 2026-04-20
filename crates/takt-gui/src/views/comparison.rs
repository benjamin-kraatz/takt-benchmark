use std::path::Path;

use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use takt_core::{BenchmarkRunRecord, BenchmarkType, ExportFormat};

use crate::palette;
use super::{ExportAction, render_export_controls};

pub fn show_compare_tab(
    ui: &mut egui::Ui,
    trend_runs: &[BenchmarkRunRecord],
    comparison_runs: &[&BenchmarkRunRecord],
    selected_export_format: &mut ExportFormat,
    export_directory: &Path,
    export_path: &mut String,
    export_status: &mut Option<String>,
    controls_enabled: bool,
    picker_pending: bool,
) -> Option<ExportAction> {
    // Trend section
    egui::Frame::new()
        .fill(palette::BG_CARD)
        .stroke(egui::Stroke::new(1.0, palette::BG_BORDER))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Device Trend")
                    .strong()
                    .color(palette::TEXT_PRIMARY),
            );
            ui.add_space(4.0);

            if trend_runs.is_empty() {
                ui.label(
                    egui::RichText::new("Select a run from History to see trend.")
                        .color(palette::TEXT_DISABLED),
                );
            } else {
                let mut reset_zoom = false;
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{} runs", trend_runs.len()))
                            .size(11.0)
                            .color(palette::TEXT_SECONDARY),
                    );
                    if ui.small_button("Reset zoom").clicked() {
                        reset_zoom = true;
                    }
                });

                let trend_id = format!(
                    "trend-{}-{}-{}",
                    trend_runs.first().map(|r| r.run_id.as_str()).unwrap_or(""),
                    trend_runs.last().map(|r| r.run_id.as_str()).unwrap_or(""),
                    trend_runs.len(),
                );
                let mut plot = Plot::new(trend_id)
                    .height(220.0)
                    .include_x(0.0)
                    .include_y(0.0);
                if reset_zoom {
                    plot = plot.reset();
                }
                plot.show(ui, |plot_ui| {
                    for benchmark in BenchmarkType::ALL {
                        let points = PlotPoints::from_iter(
                            trend_runs.iter().enumerate().filter_map(|(i, run)| {
                                run.result_for(benchmark)
                                    .map(|result| [i as f64, result.average_mbps])
                            }),
                        );
                        plot_ui.line(Line::new(benchmark.label(), points));
                    }
                });
            }
        });

    if comparison_runs.len() < 2 {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Select exactly 2 runs from History to compare.")
                .color(palette::TEXT_DISABLED),
        );
        return None;
    }

    let left = comparison_runs[0];
    let right = comparison_runs[1];

    ui.add_space(8.0);

    // Direct comparison
    egui::Frame::new()
        .fill(palette::BG_CARD)
        .stroke(egui::Stroke::new(1.0, palette::BG_BORDER))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Direct Comparison")
                    .strong()
                    .color(palette::TEXT_PRIMARY),
            );
            ui.label(
                egui::RichText::new(format!(
                    "{}  vs  {}",
                    left.display_name(),
                    right.display_name()
                ))
                .size(11.0)
                .color(palette::TEXT_SECONDARY),
            );
            ui.add_space(6.0);

            egui::Grid::new("compare-grid").striped(true).show(ui, |ui| {
                ui.label(egui::RichText::new("Benchmark").strong().color(palette::ACCENT));
                ui.label(egui::RichText::new("Left Avg").strong().color(palette::ACCENT));
                ui.label(egui::RichText::new("Right Avg").strong().color(palette::ACCENT));
                ui.label(egui::RichText::new("Delta").strong().color(palette::ACCENT));
                ui.end_row();

                for benchmark in BenchmarkType::ALL {
                    let l = left.result_for(benchmark).map(|r| r.average_mbps);
                    let r = right.result_for(benchmark).map(|r| r.average_mbps);
                    let delta_color = match (l, r) {
                        (Some(lv), Some(rv)) if rv > lv => palette::SUCCESS,
                        (Some(_), Some(_)) => palette::WARNING,
                        _ => palette::TEXT_DISABLED,
                    };
                    ui.label(egui::RichText::new(benchmark.label()).color(palette::TEXT_PRIMARY));
                    ui.label(
                        egui::RichText::new(l.map(|v| format!("{v:.1}")).unwrap_or_else(|| "—".into()))
                            .color(palette::TEXT_SECONDARY),
                    );
                    ui.label(
                        egui::RichText::new(r.map(|v| format!("{v:.1}")).unwrap_or_else(|| "—".into()))
                            .color(palette::TEXT_SECONDARY),
                    );
                    ui.label(
                        egui::RichText::new(match (l, r) {
                            (Some(lv), Some(rv)) => format!("{:+.1}", rv - lv),
                            _ => "—".into(),
                        })
                        .color(delta_color),
                    );
                    ui.end_row();
                }
            });
        });

    ui.add_space(8.0);

    // Overlay plots
    for benchmark in BenchmarkType::ALL {
        let left_result = left.result_for(benchmark);
        let right_result = right.result_for(benchmark);
        if left_result.is_none() && right_result.is_none() {
            continue;
        }
        ui.collapsing(
            egui::RichText::new(format!("{} overlay", benchmark.label()))
                .color(palette::TEXT_PRIMARY),
            |ui| {
                let mut reset_zoom = false;
                ui.horizontal(|ui| {
                    if ui.small_button("Reset zoom").clicked() {
                        reset_zoom = true;
                    }
                });
                let mut plot = Plot::new(format!(
                    "compare-{}-{}-{}",
                    benchmark.slug(),
                    left.run_id,
                    right.run_id
                ))
                .height(160.0)
                .include_x(0.0)
                .include_y(0.0);
                if reset_zoom {
                    plot = plot.reset();
                }
                plot.show(ui, |plot_ui| {
                    if let Some(result) = left_result {
                        plot_ui.line(Line::new(
                            format!("Left: {}", left.started_at.format("%m-%d %H:%M")),
                            PlotPoints::from_iter(
                                result.samples.iter().map(|s| [s.seconds, s.throughput_mbps]),
                            ),
                        ).color(palette::ACCENT));
                    }
                    if let Some(result) = right_result {
                        plot_ui.line(Line::new(
                            format!("Right: {}", right.started_at.format("%m-%d %H:%M")),
                            PlotPoints::from_iter(
                                result.samples.iter().map(|s| [s.seconds, s.throughput_mbps]),
                            ),
                        ).color(palette::WARNING));
                    }
                });
            },
        );
    }

    ui.add_space(8.0);

    // Export
    egui::Frame::new()
        .fill(palette::BG_CARD)
        .stroke(egui::Stroke::new(1.0, palette::BG_BORDER))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Export Comparison")
                    .strong()
                    .color(palette::TEXT_PRIMARY),
            );
            ui.add_space(4.0);
            let action = render_export_controls(
                ui,
                selected_export_format,
                export_directory,
                export_path,
                export_status,
                &[left.clone(), right.clone()],
                controls_enabled,
                picker_pending,
            );
            if let Some(status) = export_status.as_deref() {
                ui.label(
                    egui::RichText::new(status)
                        .size(11.0)
                        .color(palette::TEXT_SECONDARY),
                );
            }
            action
        })
        .inner
}

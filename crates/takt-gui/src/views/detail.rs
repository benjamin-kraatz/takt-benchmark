use std::path::Path;

use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use takt_core::{BenchmarkRunRecord, BenchmarkType, ExportFormat};

use crate::palette;
use super::{ExportAction, render_export_controls};

pub struct ResultsTabResponse {
    pub export_action: Option<ExportAction>,
    pub save_annotations: bool,
}

pub fn show_results_tab(
    ui: &mut egui::Ui,
    run: Option<&BenchmarkRunRecord>,
    selected_export_format: &mut ExportFormat,
    export_directory: &Path,
    export_path: &mut String,
    export_status: &mut Option<String>,
    controls_enabled: bool,
    picker_pending: bool,
    tag_editor: &mut String,
    note_editor: &mut String,
) -> ResultsTabResponse {
    let mut response = ResultsTabResponse {
        export_action: None,
        save_annotations: false,
    };

    let Some(run) = run else {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("No run selected")
                    .size(20.0)
                    .color(palette::TEXT_SECONDARY),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(
                    "Run a benchmark or select a run from the History tab.",
                )
                .color(palette::TEXT_DISABLED),
            );
        });
        return response;
    };

    // Overview header
    egui::Frame::new()
        .fill(palette::BG_CARD)
        .stroke(egui::Stroke::new(1.0, palette::BG_BORDER))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(run.display_name())
                    .size(16.0)
                    .strong()
                    .color(palette::TEXT_PRIMARY),
            );
            ui.label(
                egui::RichText::new(format!(
                    "Run ID: {}  ·  {}  ·  {} profile",
                    run.run_id,
                    run.target.mount_point.display(),
                    run.profile.preset,
                ))
                .size(11.0)
                .color(palette::TEXT_SECONDARY),
            );
        });

    ui.add_space(8.0);

    // Overview plot
    let overview_points = PlotPoints::from_iter(
        BenchmarkType::ALL
            .iter()
            .enumerate()
            .filter_map(|(i, benchmark)| {
                run.result_for(*benchmark)
                    .map(|result| [i as f64, result.average_mbps])
            }),
    );
    let mut reset_overview_zoom = false;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Average Throughput Overview")
                .strong()
                .color(palette::TEXT_PRIMARY),
        );
        if ui.small_button("Reset zoom").clicked() {
            reset_overview_zoom = true;
        }
    });
    let mut overview_plot = Plot::new(format!("detail-overview-{}", run.run_id))
        .height(180.0)
        .include_x(0.0)
        .include_y(0.0);
    if reset_overview_zoom {
        overview_plot = overview_plot.reset();
    }
    overview_plot.show(ui, |plot_ui| {
        plot_ui.line(
            Line::new("Average throughput", overview_points).color(palette::ACCENT),
        );
    });

    ui.add_space(8.0);

    // Results grid
    egui::Frame::new()
        .fill(palette::BG_CARD)
        .stroke(egui::Stroke::new(1.0, palette::BG_BORDER))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            egui::Grid::new("detail-grid").striped(true).show(ui, |ui| {
                ui.label(egui::RichText::new("Benchmark").strong().color(palette::ACCENT));
                ui.label(egui::RichText::new("Avg MiB/s").strong().color(palette::ACCENT));
                ui.label(egui::RichText::new("Peak MiB/s").strong().color(palette::ACCENT));
                ui.label(egui::RichText::new("Min MiB/s").strong().color(palette::ACCENT));
                ui.label(egui::RichText::new("IOPS").strong().color(palette::ACCENT));
                ui.label(egui::RichText::new("P95 ms").strong().color(palette::ACCENT));
                ui.end_row();

                for result in &run.results {
                    ui.label(
                        egui::RichText::new(result.benchmark.label())
                            .color(palette::TEXT_PRIMARY),
                    );
                    ui.label(
                        egui::RichText::new(format!("{:.1}", result.average_mbps))
                            .color(palette::SUCCESS),
                    );
                    ui.label(
                        egui::RichText::new(format!("{:.1}", result.peak_mbps))
                            .color(palette::TEXT_PRIMARY),
                    );
                    ui.label(
                        egui::RichText::new(format!("{:.1}", result.minimum_mbps))
                            .color(palette::TEXT_SECONDARY),
                    );
                    ui.label(
                        egui::RichText::new(
                            result
                                .iops
                                .map(|v| format!("{v:.0}"))
                                .unwrap_or_else(|| "—".to_string()),
                        )
                        .color(palette::TEXT_SECONDARY),
                    );
                    ui.label(
                        egui::RichText::new(
                            result
                                .latency_ms_p95
                                .map(|v| format!("{v:.2}"))
                                .unwrap_or_else(|| "—".to_string()),
                        )
                        .color(palette::TEXT_SECONDARY),
                    );
                    ui.end_row();
                }
            });
        });

    ui.add_space(8.0);

    // Per-benchmark collapsible plots
    ui.separator();
    for result in &run.results {
        ui.collapsing(
            egui::RichText::new(result.benchmark.label()).color(palette::TEXT_PRIMARY),
            |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Duration {:.1}s  ·  {:.1} MiB processed",
                        result.duration_secs,
                        result.bytes_processed as f64 / 1024.0 / 1024.0,
                    ))
                    .size(11.0)
                    .color(palette::TEXT_SECONDARY),
                );
                if result.samples.is_empty() {
                    ui.label(
                        egui::RichText::new("No time-series samples captured.")
                            .color(palette::TEXT_DISABLED),
                    );
                    return;
                }
                let mut reset_zoom = false;
                ui.horizontal(|ui| {
                    ui.label("Throughput timeline");
                    if ui.small_button("Reset zoom").clicked() {
                        reset_zoom = true;
                    }
                });
                let points = PlotPoints::from_iter(
                    result
                        .samples
                        .iter()
                        .map(|s| [s.seconds, s.throughput_mbps]),
                );
                let mut plot = Plot::new(format!(
                    "detail-{}-{}",
                    run.run_id,
                    result.benchmark.slug()
                ))
                .height(160.0)
                .include_x(0.0)
                .include_y(0.0);
                if reset_zoom {
                    plot = plot.reset();
                }
                plot.show(ui, |plot_ui| {
                    plot_ui.line(
                        Line::new(result.benchmark.label(), points).color(palette::ACCENT),
                    );
                });
            },
        );
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);

    // Annotations
    egui::Frame::new()
        .fill(palette::BG_CARD)
        .stroke(egui::Stroke::new(1.0, palette::BG_BORDER))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Annotations")
                    .strong()
                    .color(palette::TEXT_PRIMARY),
            );
            ui.add_space(4.0);
            ui.add_enabled_ui(controls_enabled, |ui| {
                ui.label(
                    egui::RichText::new("Tags (comma-separated)")
                        .size(11.0)
                        .color(palette::TEXT_SECONDARY),
                );
                ui.text_edit_singleline(tag_editor);
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Notes")
                        .size(11.0)
                        .color(palette::TEXT_SECONDARY),
                );
                ui.text_edit_multiline(note_editor);
                ui.add_space(4.0);
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Save annotations")
                                .color(palette::TEXT_PRIMARY),
                        )
                        .fill(palette::BG_BORDER),
                    )
                    .clicked()
                {
                    response.save_annotations = true;
                }
            });
        });

    ui.add_space(8.0);

    // Export
    egui::Frame::new()
        .fill(palette::BG_CARD)
        .stroke(egui::Stroke::new(1.0, palette::BG_BORDER))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Export")
                    .strong()
                    .color(palette::TEXT_PRIMARY),
            );
            ui.add_space(4.0);
            response.export_action = render_export_controls(
                ui,
                selected_export_format,
                export_directory,
                export_path,
                export_status,
                std::slice::from_ref(run),
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
        });

    response
}

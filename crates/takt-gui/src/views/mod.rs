pub mod benchmark;
pub mod comparison;
pub mod detail;
pub mod history;

use std::path::{Path, PathBuf};

use eframe::egui;
use takt_core::{BenchmarkRunRecord, ExportFormat, describe_export};

use crate::palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportAction {
    Browse,
    Export,
}

pub(crate) fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(palette::BG_CARD)
        .corner_radius(egui::CornerRadius::same(6))
        .stroke(egui::Stroke::new(1.0, palette::BG_BORDER))
        .inner_margin(egui::Margin::same(10))
}

pub(crate) fn render_export_controls(
    ui: &mut egui::Ui,
    selected_export_format: &mut ExportFormat,
    export_directory: &Path,
    export_path: &mut String,
    export_status: &mut Option<String>,
    runs: &[BenchmarkRunRecord],
    controls_enabled: bool,
    picker_pending: bool,
) -> Option<ExportAction> {
    let mut action = None;
    let preview = describe_export(*selected_export_format, runs);
    let normalized_path =
        normalize_export_path(export_path, *selected_export_format, export_directory);

    ui.add_enabled_ui(controls_enabled, |ui| {
        ui.horizontal(|ui| {
            ui.label("Format");
            egui::ComboBox::from_id_salt(ui.next_auto_id())
                .selected_text(selected_export_format.label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        selected_export_format,
                        ExportFormat::Json,
                        ExportFormat::Json.label(),
                    );
                    ui.selectable_value(
                        selected_export_format,
                        ExportFormat::Markdown,
                        ExportFormat::Markdown.label(),
                    );
                    ui.selectable_value(
                        selected_export_format,
                        ExportFormat::Html,
                        ExportFormat::Html.label(),
                    );
                    ui.selectable_value(
                        selected_export_format,
                        ExportFormat::Png,
                        ExportFormat::Png.label(),
                    );
                });
            ui.label("Export path");
            ui.text_edit_singleline(export_path);
        });
    });

    ui.group(|ui| {
        ui.strong("Export Preview");
        ui.label(format!(
            "{} run(s) will be exported as {}.",
            preview.run_count,
            preview.format.label()
        ));
        ui.label(format!("Destination: {}", normalized_path.display()));
        if let Some(mode) = preview.png_mode {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label("PNG mode:");
                ui.strong(mode.label());
            });
            ui.label(mode.description());
        } else {
            ui.separator();
            ui.label("Text exports include benchmark metrics, annotations, and device context.");
        }
        if let Some(first_run) = runs.first() {
            let mut run_summary = vec![first_run.display_name()];
            if runs.len() > 1 {
                run_summary.push(format!("+ {} more run(s)", runs.len() - 1));
            }
            ui.label(format!("Selection: {}", run_summary.join(" ")));
        }
    });

    ui.add_enabled_ui(controls_enabled, |ui| {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !picker_pending,
                    egui::Button::new(if picker_pending {
                        "Choosing..."
                    } else {
                        "Browse..."
                    }),
                )
                .clicked()
            {
                action = Some(ExportAction::Browse);
            }
            if ui
                .button(format!("Export {}", selected_export_format.label()))
                .clicked()
            {
                action = Some(ExportAction::Export);
            }
            if ui.button("Clear status").clicked() {
                *export_status = None;
            }
        });
    });

    if !controls_enabled {
        ui.label("Export controls are disabled while a benchmark is running.");
    }

    action
}

pub(crate) fn normalize_export_path(
    path: &str,
    format: ExportFormat,
    export_directory: &Path,
) -> PathBuf {
    let trimmed = path.trim();
    let mut output = if trimmed.is_empty() {
        export_directory.join(format!("benchmark-export.{}", format.extension()))
    } else {
        PathBuf::from(trimmed)
    };
    if output.extension().is_none() {
        output.set_extension(format.extension());
    }
    if output.is_relative() {
        export_directory.join(output)
    } else {
        output
    }
}

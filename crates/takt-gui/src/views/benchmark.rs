use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use takt_core::{BenchmarkType, DeviceTarget, ProfilePreset};

use crate::palette;

pub struct RunProgressDisplay {
    pub status_line: String,
    pub detail_line: String,
    pub suite_label: String,
    pub suite_fraction: f32,
    pub benchmark_label: String,
    pub benchmark_fraction: Option<f32>,
    pub queue_line: Option<String>,
    pub cancelling: bool,
}

pub struct RunStatusBanner<'a> {
    pub kind: RunStatusKind,
    pub title: &'a str,
    pub detail: &'a str,
}

#[derive(Clone, Copy)]
pub enum RunStatusKind {
    Success,
    Warning,
    Error,
}

pub fn show_sidebar(
    ui: &mut egui::Ui,
    devices: &[DeviceTarget],
    selected_target: &mut Option<String>,
    profile: &mut ProfilePreset,
    selected_benchmarks: &mut Vec<BenchmarkType>,
    is_running: bool,
) {
    section_header(ui, "TARGET");
    ui.add_enabled_ui(!is_running, |ui| {
        let w = ui.available_width();
        egui::ComboBox::from_id_salt("target-combo")
            .width(w)
            .selected_text(selected_label(devices, selected_target.as_ref()))
            .show_ui(ui, |ui| {
                for device in devices {
                    ui.selectable_value(
                        selected_target,
                        Some(device.id.clone()),
                        format!(
                            "{} ({}, {} GiB free)",
                            device.name,
                            device.mount_point.display(),
                            device.available_bytes / 1024 / 1024 / 1024,
                        ),
                    );
                }
            });
    });

    ui.add_space(12.0);
    section_header(ui, "PROFILE");
    ui.add_enabled_ui(!is_running, |ui| {
        let w = ui.available_width();
        egui::ComboBox::from_id_salt("profile-combo")
            .width(w)
            .selected_text(profile.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(profile, ProfilePreset::Quick, ProfilePreset::Quick.label());
                ui.selectable_value(
                    profile,
                    ProfilePreset::Balanced,
                    ProfilePreset::Balanced.label(),
                );
                ui.selectable_value(
                    profile,
                    ProfilePreset::Thorough,
                    ProfilePreset::Thorough.label(),
                );
            });
    });

    ui.add_space(12.0);
    section_header(ui, "BENCHMARKS");
    ui.add_enabled_ui(!is_running, |ui| {
        for benchmark in BenchmarkType::ALL {
            let mut enabled = selected_benchmarks.contains(&benchmark);
            if ui
                .checkbox(
                    &mut enabled,
                    egui::RichText::new(benchmark.label()).color(palette::TEXT_PRIMARY),
                )
                .changed()
            {
                if enabled {
                    selected_benchmarks.push(benchmark);
                    selected_benchmarks.sort_by_key(|candidate| {
                        BenchmarkType::ALL
                            .iter()
                            .position(|item| item == candidate)
                            .unwrap_or_default()
                    });
                    selected_benchmarks.dedup();
                } else {
                    selected_benchmarks.retain(|candidate| candidate != &benchmark);
                }
            }
        }
    });
}

pub fn show_run_tab(
    ui: &mut egui::Ui,
    progress_display: Option<&RunProgressDisplay>,
    status_banner: Option<RunStatusBanner<'_>>,
    live_plot_revision: u64,
    live_samples: &[[f64; 2]],
) {
    let has_banner = status_banner.is_some();
    let has_progress = progress_display.is_some();

    // Empty state
    if !has_banner && !has_progress && live_samples.is_empty() {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("No benchmark running")
                    .size(20.0)
                    .color(palette::TEXT_SECONDARY),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(
                    "Configure a target and profile in the sidebar,\nthen click Run Benchmark.",
                )
                .color(palette::TEXT_DISABLED),
            );
        });
        return;
    }

    // Status banner
    if let Some(status) = status_banner {
        let (bg, border) = match status.kind {
            RunStatusKind::Success => (palette::SUCCESS.gamma_multiply(0.12), palette::SUCCESS),
            RunStatusKind::Warning => (palette::WARNING.gamma_multiply(0.12), palette::WARNING),
            RunStatusKind::Error => (palette::DANGER.gamma_multiply(0.12), palette::DANGER),
        };
        egui::Frame::new()
            .fill(bg)
            .stroke(egui::Stroke::new(1.0, border))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.label(egui::RichText::new(status.title).strong().color(border));
                ui.label(
                    egui::RichText::new(status.detail)
                        .size(12.0)
                        .color(palette::TEXT_SECONDARY),
                );
            });
        ui.add_space(8.0);
    }

    // Live throughput plot
    if !live_samples.is_empty() {
        let mut reset_zoom = false;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Live Throughput")
                    .strong()
                    .color(palette::TEXT_PRIMARY),
            );
            if ui.small_button("Reset zoom").clicked() {
                reset_zoom = true;
            }
        });
        let points = PlotPoints::from_iter(live_samples.iter().copied());
        let line = Line::new("Throughput", points).color(palette::ACCENT);
        let mut plot = Plot::new(format!("throughput-plot-{live_plot_revision}"))
            .height(240.0)
            .include_x(0.0)
            .include_y(0.0);
        if reset_zoom {
            plot = plot.reset();
        }
        plot.show(ui, |plot_ui| plot_ui.line(line));
        ui.add_space(8.0);
    }

    // Progress card
    if let Some(progress) = progress_display {
        egui::Frame::new()
            .fill(palette::BG_CARD)
            .stroke(egui::Stroke::new(1.0, palette::BG_BORDER))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(if progress.cancelling {
                            "Cancelling benchmark..."
                        } else {
                            "Benchmark in progress"
                        })
                        .strong()
                        .color(palette::WARNING),
                    );
                    if progress.benchmark_fraction.is_none() {
                        ui.add(egui::Spinner::new());
                    }
                });
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&progress.status_line).color(palette::TEXT_PRIMARY));
                ui.label(
                    egui::RichText::new(&progress.detail_line)
                        .size(11.0)
                        .color(palette::TEXT_SECONDARY),
                );
                ui.add_space(4.0);
                ui.add(
                    egui::ProgressBar::new(progress.suite_fraction)
                        .desired_width(f32::INFINITY)
                        .text(progress.suite_label.clone()),
                );
                if let Some(fraction) = progress.benchmark_fraction {
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .desired_width(f32::INFINITY)
                            .text(progress.benchmark_label.clone()),
                    );
                } else {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        ui.label(
                            egui::RichText::new(&progress.benchmark_label)
                                .color(palette::TEXT_SECONDARY),
                        );
                    });
                }
                if let Some(queue_line) = &progress.queue_line {
                    ui.label(
                        egui::RichText::new(queue_line)
                            .size(11.0)
                            .color(palette::TEXT_DISABLED),
                    );
                }
            });
    }
}

fn section_header(ui: &mut egui::Ui, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .size(10.0)
            .color(palette::TEXT_SECONDARY)
            .strong(),
    );
    ui.add_space(4.0);
}

fn selected_label(devices: &[DeviceTarget], selected_target: Option<&String>) -> String {
    selected_target
        .and_then(|selected| devices.iter().find(|device| &device.id == selected))
        .map(|device| format!("{} ({})", device.name, device.mount_point.display()))
        .unwrap_or_else(|| "Select a target".to_string())
}

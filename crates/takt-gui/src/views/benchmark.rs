use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use takt_core::{BenchmarkRunRecord, BenchmarkType, DeviceTarget, ProfilePreset};

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

pub fn show_controls(
    ui: &mut egui::Ui,
    devices: &[DeviceTarget],
    selected_target: &mut Option<String>,
    profile: &mut ProfilePreset,
    selected_benchmarks: &mut Vec<BenchmarkType>,
    controls_enabled: bool,
    progress_display: Option<&RunProgressDisplay>,
    status_banner: Option<RunStatusBanner<'_>>,
    live_plot_revision: u64,
    live_samples: &[[f64; 2]],
) {
    ui.heading("Benchmark Runner");
    ui.add_enabled_ui(controls_enabled, |ui| {
        ui.horizontal(|ui| {
            ui.label("Target");
            egui::ComboBox::from_id_salt("target-combo")
                .selected_text(selected_label(devices, selected_target.as_ref()))
                .show_ui(ui, |ui| {
                    for device in devices {
                        ui.selectable_value(
                            selected_target,
                            Some(device.id.clone()),
                            format!(
                                "{} ({:?}, {}, {} GiB free)",
                                device.name,
                                device.kind,
                                device.mount_point.display(),
                                device.available_bytes / 1024 / 1024 / 1024,
                            ),
                        );
                    }
                });
        });

        ui.horizontal(|ui| {
            ui.label("Profile");
            egui::ComboBox::from_id_salt("profile-combo")
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

        ui.label("Benchmarks");
        ui.horizontal_wrapped(|ui| {
            for benchmark in BenchmarkType::ALL {
                let mut enabled = selected_benchmarks.contains(&benchmark);
                if ui.checkbox(&mut enabled, benchmark.label()).changed() {
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
    });

    if let Some(progress) = progress_display {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.strong(if progress.cancelling {
                    "Cancelling benchmark..."
                } else {
                    "Benchmark in progress"
                });
                if progress.benchmark_fraction.is_none() {
                    ui.add(egui::Spinner::new());
                }
            });
            ui.label(&progress.status_line);
            ui.label(&progress.detail_line);
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
                    ui.label(&progress.benchmark_label);
                });
            }
            if let Some(queue_line) = &progress.queue_line {
                ui.label(queue_line);
            }
        });
    } else if !controls_enabled {
        ui.group(|ui| {
            ui.strong("Benchmark in progress");
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label("Preparing benchmark worker...");
            });
        });
    }

    if let Some(status) = status_banner {
        let fill = match status.kind {
            RunStatusKind::Success => egui::Color32::from_rgb(223, 245, 229),
            RunStatusKind::Warning => egui::Color32::from_rgb(250, 238, 208),
            RunStatusKind::Error => egui::Color32::from_rgb(248, 220, 218),
        };
        let stroke = match status.kind {
            RunStatusKind::Success => egui::Color32::from_rgb(54, 110, 74),
            RunStatusKind::Warning => egui::Color32::from_rgb(142, 101, 22),
            RunStatusKind::Error => egui::Color32::from_rgb(158, 54, 46),
        };
        egui::Frame::group(ui.style())
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, stroke))
            .show(ui, |ui| {
                ui.strong(status.title);
                ui.label(status.detail);
            });
    }

    if !live_samples.is_empty() {
        let points = PlotPoints::from_iter(live_samples.iter().copied());
        let line = Line::new("Throughput", points);
        Plot::new(format!("throughput-plot-{live_plot_revision}"))
            .height(180.0)
            .include_x(0.0)
            .include_y(0.0)
            .show(ui, |plot_ui| plot_ui.line(line));
    }

    ui.separator();
    ui.label(
        "Targets include built-in system storage, removable media, and mounted network shares.",
    );
    if !controls_enabled {
        ui.label("Selection controls are disabled until the current benchmark run finishes.");
    }
    if let Some(selected_target) = selected_target.as_ref() {
        if let Some(device) = devices.iter().find(|device| &device.id == selected_target) {
            ui.label(format!(
                "Selected target: {} | id={} | readonly={} | transport={} | storage={} | model={} | vendor={} | usb={} | volume uuid={} | partition uuid={} | mount options={}",
                device.mount_point.display(),
                device.id,
                device.metadata.is_read_only,
                device.transport_hint().unwrap_or("unknown"),
                device.storage_hint().unwrap_or("unknown"),
                device.metadata.model.as_deref().unwrap_or("-"),
                device.metadata.vendor.as_deref().unwrap_or("-"),
                device.metadata.usb_generation.as_deref().unwrap_or("-"),
                device.metadata.volume_uuid.as_deref().unwrap_or("-"),
                device.metadata.partition_uuid.as_deref().unwrap_or("-"),
                if device.metadata.mount_options.is_empty() {
                    "-".to_string()
                } else {
                    device.metadata.mount_options.join(",")
                }
            ));
        }
    }
}

pub fn show_run_summary(ui: &mut egui::Ui, run: &BenchmarkRunRecord) {
    ui.heading("Latest Result");
    ui.label(format!(
        "{} at {} using {} profile",
        run.target.name,
        run.started_at.format("%Y-%m-%d %H:%M:%S"),
        run.profile.preset,
    ));

    egui::Grid::new("results-grid")
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Benchmark");
            ui.strong("Average MiB/s");
            ui.strong("Peak MiB/s");
            ui.strong("P95 latency ms");
            ui.end_row();

            for result in &run.results {
                ui.label(result.benchmark.label());
                ui.label(format!("{:.1}", result.average_mbps));
                ui.label(format!("{:.1}", result.peak_mbps));
                ui.label(
                    result
                        .latency_ms_p95
                        .map(|value| format!("{value:.2}"))
                        .unwrap_or_else(|| "-".to_string()),
                );
                ui.end_row();
            }
        });
}

fn selected_label(devices: &[DeviceTarget], selected_target: Option<&String>) -> String {
    selected_target
        .and_then(|selected| devices.iter().find(|device| &device.id == selected))
        .map(|device| format!("{} ({})", device.name, device.mount_point.display()))
        .unwrap_or_else(|| "Select a target".to_string())
}

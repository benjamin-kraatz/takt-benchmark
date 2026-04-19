use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use takt_core::{BenchmarkRunRecord, BenchmarkType, DeviceTarget, ProfilePreset, ProgressUpdate};

pub fn show_controls(
    ui: &mut egui::Ui,
    devices: &[DeviceTarget],
    selected_target: &mut Option<String>,
    profile: &mut ProfilePreset,
    selected_benchmarks: &mut Vec<BenchmarkType>,
    last_progress: Option<&ProgressUpdate>,
    live_samples: &[[f64; 2]],
) {
    ui.heading("Benchmark Runner");
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

    if let Some(progress) = last_progress {
        ui.label(format!(
            "Current phase: {} {} {:.1} MiB/s after {:.1}s",
            progress.benchmark.label(),
            progress.phase,
            progress.current_mbps,
            progress.elapsed.as_secs_f64(),
        ));
    }

    if !live_samples.is_empty() {
        let points = PlotPoints::from_iter(live_samples.iter().copied());
        let line = Line::new("Throughput", points);
        Plot::new("throughput-plot")
            .height(180.0)
            .include_y(0.0)
            .show(ui, |plot_ui| plot_ui.line(line));
    }

    ui.separator();
    ui.label(
        "Targets include built-in system storage, removable media, and mounted network shares.",
    );
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

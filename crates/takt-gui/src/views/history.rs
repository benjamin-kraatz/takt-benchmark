use eframe::egui;
use takt_core::{BenchmarkRunRecord, ProfilePreset};

pub fn show_history(
    ui: &mut egui::Ui,
    history: &[BenchmarkRunRecord],
    selected_run_id: &mut Option<String>,
    comparison_run_ids: &mut Vec<String>,
    device_filter: &mut Option<String>,
    profile_filter: &mut Option<ProfilePreset>,
) {
    ui.heading("Local History");
    if history.is_empty() {
        ui.label("No saved benchmark runs yet.");
        return;
    }

    ui.horizontal(|ui| {
        ui.label("Device filter");
        egui::ComboBox::from_id_salt("history-device-filter")
            .selected_text(device_filter.as_deref().unwrap_or("All devices"))
            .show_ui(ui, |ui| {
                ui.selectable_value(device_filter, None, "All devices");
                let mut unique_devices = history
                    .iter()
                    .map(|record| record.target.name.clone())
                    .collect::<Vec<_>>();
                unique_devices.sort();
                unique_devices.dedup();
                for device in unique_devices {
                    ui.selectable_value(device_filter, Some(device.clone()), device);
                }
            });

        ui.label("Profile filter");
        egui::ComboBox::from_id_salt("history-profile-filter")
            .selected_text(
                profile_filter
                    .as_ref()
                    .map(ProfilePreset::label)
                    .unwrap_or("All profiles"),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(profile_filter, None, "All profiles");
                ui.selectable_value(profile_filter, Some(ProfilePreset::Quick), "Quick");
                ui.selectable_value(profile_filter, Some(ProfilePreset::Balanced), "Balanced");
                ui.selectable_value(profile_filter, Some(ProfilePreset::Thorough), "Thorough");
            });
    });

    let filtered = history.iter().filter(|record| {
        device_filter
            .as_ref()
            .is_none_or(|device| &record.target.name == device)
            && profile_filter
                .as_ref()
                .is_none_or(|profile| &record.profile.preset == profile)
    });

    egui::ScrollArea::vertical()
        .max_height(320.0)
        .show(ui, |ui| {
            for record in filtered.take(30) {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                selected_run_id.as_ref() == Some(&record.run_id),
                                format!(
                                    "{}  {}  {}",
                                    record.started_at.format("%Y-%m-%d %H:%M:%S"),
                                    record.target.name,
                                    record.profile.preset,
                                ),
                            )
                            .clicked()
                        {
                            *selected_run_id = Some(record.run_id.clone());
                        }

                        let compare_selected = comparison_run_ids.contains(&record.run_id);
                        let compare_label = if compare_selected {
                            "Remove from compare"
                        } else {
                            "Compare"
                        };
                        if ui.button(compare_label).clicked() {
                            if compare_selected {
                                comparison_run_ids.retain(|run_id| run_id != &record.run_id);
                            } else {
                                if comparison_run_ids.len() == 2 {
                                    comparison_run_ids.remove(0);
                                }
                                comparison_run_ids.push(record.run_id.clone());
                            }
                        }
                    });
                    ui.label(format!("run id {}", record.run_id));
                    if !record.tags.is_empty() {
                        ui.label(format!("tags: {}", record.tags.join(", ")));
                    }
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

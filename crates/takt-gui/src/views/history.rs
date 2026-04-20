use eframe::egui;
use takt_core::{BenchmarkRunRecord, ProfilePreset};

use crate::palette;

pub fn show_history_tab(
    ui: &mut egui::Ui,
    history: &[BenchmarkRunRecord],
    selected_run_id: &mut Option<String>,
    comparison_run_ids: &mut Vec<String>,
    device_filter: &mut Option<String>,
    profile_filter: &mut Option<ProfilePreset>,
    controls_enabled: bool,
) {
    if history.is_empty() {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("No benchmark runs yet")
                    .size(20.0)
                    .color(palette::TEXT_SECONDARY),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Run a benchmark to populate history.")
                    .color(palette::TEXT_DISABLED),
            );
        });
        return;
    }

    // Filter row
    egui::Frame::new()
        .fill(palette::BG_CARD)
        .stroke(egui::Stroke::new(1.0, palette::BG_BORDER))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.add_enabled_ui(controls_enabled, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Device")
                            .size(11.0)
                            .color(palette::TEXT_SECONDARY),
                    );
                    egui::ComboBox::from_id_salt("history-device-filter")
                        .selected_text(device_filter.as_deref().unwrap_or("All devices"))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(device_filter, None, "All devices");
                            let mut unique_devices = history
                                .iter()
                                .map(|r| r.target.name.clone())
                                .collect::<Vec<_>>();
                            unique_devices.sort();
                            unique_devices.dedup();
                            for device in unique_devices {
                                ui.selectable_value(
                                    device_filter,
                                    Some(device.clone()),
                                    device,
                                );
                            }
                        });

                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Profile")
                            .size(11.0)
                            .color(palette::TEXT_SECONDARY),
                    );
                    egui::ComboBox::from_id_salt("history-profile-filter")
                        .selected_text(
                            profile_filter
                                .as_ref()
                                .map(ProfilePreset::label)
                                .unwrap_or("All profiles"),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(profile_filter, None, "All profiles");
                            ui.selectable_value(
                                profile_filter,
                                Some(ProfilePreset::Quick),
                                "Quick",
                            );
                            ui.selectable_value(
                                profile_filter,
                                Some(ProfilePreset::Balanced),
                                "Balanced",
                            );
                            ui.selectable_value(
                                profile_filter,
                                Some(ProfilePreset::Thorough),
                                "Thorough",
                            );
                        });
                });
            });
        });

    ui.add_space(8.0);

    let filtered: Vec<&BenchmarkRunRecord> = history
        .iter()
        .filter(|r| {
            device_filter
                .as_ref()
                .is_none_or(|d| &r.target.name == d)
                && profile_filter
                    .as_ref()
                    .is_none_or(|p| &r.profile.preset == p)
        })
        .take(50)
        .collect();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for record in filtered {
                let is_selected = selected_run_id.as_ref() == Some(&record.run_id);
                let compare_selected = comparison_run_ids.contains(&record.run_id);

                let border_color = if is_selected {
                    palette::ACCENT
                } else {
                    palette::BG_BORDER
                };
                let card_fill = if is_selected {
                    egui::Color32::from_rgb(30, 45, 60)
                } else {
                    palette::BG_CARD
                };

                egui::Frame::new()
                    .fill(card_fill)
                    .stroke(egui::Stroke::new(1.0, border_color))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.add_enabled_ui(controls_enabled, |ui| {
                            ui.horizontal(|ui| {
                                // Select button
                                let select_text = format!(
                                    "{}  {}  {}",
                                    record.started_at.format("%Y-%m-%d %H:%M"),
                                    record.target.name,
                                    record.profile.preset,
                                );
                                if ui
                                    .selectable_label(
                                        is_selected,
                                        egui::RichText::new(&select_text)
                                            .color(if is_selected {
                                                palette::ACCENT
                                            } else {
                                                palette::TEXT_PRIMARY
                                            })
                                            .strong(),
                                    )
                                    .clicked()
                                {
                                    *selected_run_id = Some(record.run_id.clone());
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let compare_label = if compare_selected {
                                            egui::RichText::new("− Compare")
                                                .size(11.0)
                                                .color(palette::WARNING)
                                        } else {
                                            egui::RichText::new("+ Compare")
                                                .size(11.0)
                                                .color(palette::TEXT_SECONDARY)
                                        };
                                        if ui.button(compare_label).clicked() {
                                            if compare_selected {
                                                comparison_run_ids
                                                    .retain(|id| id != &record.run_id);
                                            } else {
                                                if comparison_run_ids.len() == 2 {
                                                    comparison_run_ids.remove(0);
                                                }
                                                comparison_run_ids.push(record.run_id.clone());
                                            }
                                        }
                                    },
                                );
                            });

                            ui.label(
                                egui::RichText::new(format!("ID: {}", record.run_id))
                                    .size(10.0)
                                    .color(palette::TEXT_DISABLED),
                            );

                            if !record.tags.is_empty() {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Tags: {}",
                                        record.tags.join(", ")
                                    ))
                                    .size(11.0)
                                    .color(palette::TEXT_SECONDARY),
                                );
                            }

                            ui.horizontal_wrapped(|ui| {
                                for result in &record.results {
                                    egui::Frame::new()
                                        .fill(palette::BG_BORDER)
                                        .corner_radius(egui::CornerRadius::same(4))
                                        .inner_margin(egui::Margin::symmetric(6, 2))
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{} {:.0}",
                                                    result.benchmark.label(),
                                                    result.average_mbps,
                                                ))
                                                .size(11.0)
                                                .color(palette::SUCCESS),
                                            );
                                        });
                                }
                            });
                        });
                    });

                ui.add_space(4.0);
            }
        });

    if !controls_enabled {
        ui.label(
            egui::RichText::new("History controls disabled while benchmark is running.")
                .size(11.0)
                .color(palette::TEXT_DISABLED),
        );
    }
}

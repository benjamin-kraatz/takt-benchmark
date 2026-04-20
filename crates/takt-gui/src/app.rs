use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use directories::{ProjectDirs, UserDirs};
use eframe::egui;
use rfd::FileDialog;
use takt_core::{
    BenchmarkProfile, BenchmarkRunRecord, BenchmarkType, DeviceTarget, ExportFormat, HistoryStore,
    ProfilePreset, ProgressUpdate, RunConfiguration, cleanup_benchmark_temp_dirs,
    describe_export, discover_devices, export_runs_to_path, run_benchmark_suite,
};

use crate::views::{benchmark, comparison, detail, history};

pub struct TaktApp {
    devices: Vec<DeviceTarget>,
    selected_target: Option<String>,
    profile: ProfilePreset,
    selected_benchmarks: Vec<BenchmarkType>,
    history: Vec<BenchmarkRunRecord>,
    latest_run: Option<BenchmarkRunRecord>,
    last_progress: Option<ProgressUpdate>,
    live_samples: Vec<[f64; 2]>,
    selected_run_id: Option<String>,
    comparison_run_ids: Vec<String>,
    history_device_filter: Option<String>,
    history_profile_filter: Option<ProfilePreset>,
    selected_export_format: ExportFormat,
    export_directory: PathBuf,
    export_path: String,
    export_status: Option<String>,
    picker_receiver: Option<Receiver<Option<PathBuf>>>,
    picker_pending: bool,
    tag_editor: String,
    note_editor: String,
    auto_cleanup_temp_dirs: bool,
    worker: Option<WorkerState>,
    pending_dialog: Option<PendingDialog>,
    benchmark_status: Option<BenchmarkStatusBanner>,
    cleanup_status: Option<String>,
    live_plot_revision: u64,
    error_message: Option<String>,
}

struct WorkerState {
    receiver: Receiver<WorkerEvent>,
    cancel_flag: Arc<AtomicBool>,
    target: DeviceTarget,
    benchmarks: Vec<BenchmarkType>,
    profile: BenchmarkProfile,
    started_at: Instant,
    cancel_requested: bool,
}

enum WorkerEvent {
    Progress(ProgressUpdate),
    Finished(Box<Result<BenchmarkRunRecord, String>>),
}

#[derive(Debug, Clone)]
enum PendingDialog {
    ConfirmCleanup { target: DeviceTarget },
    ConfirmBuiltInRun { target: DeviceTarget, step: BuiltInRunStep },
}

#[derive(Debug, Clone, Copy)]
enum BuiltInRunStep {
    Initial,
    Final,
}

#[derive(Debug, Clone)]
struct BenchmarkStatusBanner {
    kind: BenchmarkStatusKind,
    title: String,
    detail: String,
}

#[derive(Debug, Clone, Copy)]
enum BenchmarkStatusKind {
    Success,
    Cancelled,
    Failure,
}

#[derive(Debug, Clone, Copy)]
enum ExportControlAction {
    Browse,
    Export,
}

impl Default for TaktApp {
    fn default() -> Self {
        let devices = discover_devices().unwrap_or_default();
        let selected_target = devices.first().map(|device| device.id.clone());
        let history = HistoryStore::default_store()
            .and_then(|store| store.load())
            .unwrap_or_default();
        let export_directory = load_export_directory().unwrap_or_else(default_export_directory);

        Self {
            devices,
            selected_target,
            profile: ProfilePreset::Balanced,
            selected_benchmarks: BenchmarkType::ALL.to_vec(),
            history,
            latest_run: None,
            last_progress: None,
            live_samples: Vec::new(),
            selected_run_id: None,
            comparison_run_ids: Vec::new(),
            history_device_filter: None,
            history_profile_filter: None,
            selected_export_format: ExportFormat::Json,
            export_directory: export_directory.clone(),
            export_path: export_directory
                .join(format!(
                    "benchmark-export.{}",
                    ExportFormat::Json.extension()
                ))
                .display()
                .to_string(),
            export_status: None,
            picker_receiver: None,
            picker_pending: false,
            tag_editor: String::new(),
            note_editor: String::new(),
            auto_cleanup_temp_dirs: true,
            worker: None,
            pending_dialog: None,
            benchmark_status: None,
            cleanup_status: None,
            live_plot_revision: 0,
            error_message: None,
        }
    }
}

impl TaktApp {
    pub fn new(_creation_context: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    fn refresh_devices(&mut self) {
        match discover_devices() {
            Ok(devices) => {
                self.selected_target = self
                    .selected_target
                    .take()
                    .or_else(|| devices.first().map(|device| device.id.clone()));
                self.devices = devices;
            }
            Err(error) => self.error_message = Some(error.to_string()),
        }
    }

    fn selected_device(&self) -> Option<&DeviceTarget> {
        let selected = self.selected_target.as_ref()?;
        self.devices.iter().find(|device| &device.id == selected)
    }

    fn start_run_for_target(&mut self, target: DeviceTarget) {
        if self.worker.is_some() {
            return;
        }

        self.last_progress = None;
        self.live_samples.clear();
        self.error_message = None;
        self.export_status = None;
        self.benchmark_status = None;
        self.cleanup_status = None;
        self.live_plot_revision = self.live_plot_revision.wrapping_add(1);

        let (sender, receiver) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let worker_cancel_flag = Arc::clone(&cancel_flag);
        let profile = BenchmarkProfile::from_preset(self.profile.clone());
        let benchmarks = if self.selected_benchmarks.is_empty() {
            BenchmarkType::ALL.to_vec()
        } else {
            self.selected_benchmarks.clone()
        };
        let worker_target = target.clone();
        let worker_profile = profile.clone();
        let worker_benchmarks = benchmarks.clone();

        std::thread::spawn(move || {
            let configuration = RunConfiguration {
                profile,
                benchmarks,
                keep_temp_files: false,
            };

            let run =
                run_benchmark_suite(&target, configuration, Some(worker_cancel_flag), |update| {
                    let _ = sender.send(WorkerEvent::Progress(update));
                })
                .and_then(|run| {
                    if let Ok(store) = HistoryStore::default_store() {
                        store.save(&run)?;
                    }
                    Ok(run)
                })
                .map_err(|error| error.to_string());

            let _ = sender.send(WorkerEvent::Finished(Box::new(run)));
        });

        self.worker = Some(WorkerState {
            receiver,
            cancel_flag,
            target: worker_target,
            benchmarks: worker_benchmarks,
            profile: worker_profile,
            started_at: Instant::now(),
            cancel_requested: false,
        });
    }

    fn cancel_run(&mut self) {
        if let Some(worker) = &mut self.worker {
            worker.cancel_requested = true;
            worker.cancel_flag.store(true, Ordering::Relaxed);
        }
    }

    fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    fn poll_worker(&mut self) {
        let mut finished = false;
        let mut completion_status = None;
        let mut cleanup_target = None;

        if let Some(worker) = &self.worker {
            while let Ok(event) = worker.receiver.try_recv() {
                match event {
                    WorkerEvent::Progress(progress) => {
                        self.live_samples
                            .push([progress.elapsed.as_secs_f64(), progress.current_mbps]);
                        self.last_progress = Some(progress);
                    }
                    WorkerEvent::Finished(result) => {
                        finished = true;
                        cleanup_target = Some(worker.target.clone());
                        match *result {
                            Ok(run) => {
                                self.live_plot_revision = self.live_plot_revision.wrapping_add(1);
                                completion_status = Some(BenchmarkStatusBanner {
                                    kind: BenchmarkStatusKind::Success,
                                    title: "Benchmark run completed".to_string(),
                                    detail: format!(
                                        "{} finished with {} benchmark result(s).",
                                        run.display_name(),
                                        run.results.len(),
                                    ),
                                });
                                self.latest_run = Some(run.clone());
                                self.selected_run_id = Some(run.run_id.clone());
                                self.history.insert(0, run.clone());
                                self.tag_editor = run.tags.join(", ");
                                self.note_editor = run.notes.clone().unwrap_or_default();
                                self.error_message = None;
                            }
                            Err(error) => {
                                let cancelled = worker.cancel_requested
                                    || error.eq_ignore_ascii_case("benchmark cancelled");
                                completion_status = Some(BenchmarkStatusBanner {
                                    kind: if cancelled {
                                        BenchmarkStatusKind::Cancelled
                                    } else {
                                        BenchmarkStatusKind::Failure
                                    },
                                    title: if cancelled {
                                        "Benchmark run cancelled".to_string()
                                    } else {
                                        "Benchmark run failed".to_string()
                                    },
                                    detail: if cancelled {
                                        "Cancellation was requested before the benchmark suite finished. Partial live throughput data may still be shown above.".to_string()
                                    } else {
                                        error.clone()
                                    },
                                });
                                self.error_message = if cancelled { None } else { Some(error) };
                            }
                        }
                    }
                }
            }
        }

        if finished {
            if self.auto_cleanup_temp_dirs {
                if let Some(target) = cleanup_target.as_ref() {
                    match cleanup_benchmark_temp_dirs(target) {
                        Ok(removed) => {
                            let detail = cleanup_message(target, removed);
                            self.cleanup_status = Some(detail.clone());
                            if let Some(status) = completion_status.as_mut() {
                                status.detail = format!("{} {}", status.detail, detail);
                            }
                        }
                        Err(error) => self.error_message = Some(error.to_string()),
                    }
                }
            }
            self.benchmark_status = completion_status;
            self.worker = None;
        }
    }

    fn cleanup_selected_temp_dirs(&mut self) {
        let Some(target) = self.selected_device().cloned() else {
            self.error_message = Some("select a benchmark target first".to_string());
            return;
        };

        match cleanup_benchmark_temp_dirs(&target) {
            Ok(removed) => {
                self.cleanup_status = Some(cleanup_message(&target, removed));
                self.error_message = None;
            }
            Err(error) => self.error_message = Some(error.to_string()),
        }
    }

    fn request_cleanup_selected_temp_dirs(&mut self) {
        let Some(target) = self.selected_device().cloned() else {
            self.error_message = Some("select a benchmark target first".to_string());
            return;
        };

        self.pending_dialog = Some(PendingDialog::ConfirmCleanup { target });
    }

    fn request_start_run(&mut self) {
        let Some(target) = self.selected_device().cloned() else {
            self.error_message = Some("select a benchmark target first".to_string());
            return;
        };

        if is_high_risk_benchmark_target(&target) {
            self.pending_dialog = Some(PendingDialog::ConfirmBuiltInRun {
                target,
                step: BuiltInRunStep::Initial,
            });
        } else {
            self.start_run_for_target(target);
        }
    }

    fn show_pending_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.pending_dialog.clone() else {
            return;
        };

        match dialog {
            PendingDialog::ConfirmCleanup { target } => {
                egui::Window::new("Confirm Cleanup")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!(
                            "Remove leftover .takt-* directories from {}?",
                            target.mount_point.display()
                        ));
                        ui.label("This only affects benchmark temp directories on the selected target.");
                        ui.horizontal(|ui| {
                            if ui.button("Cancel").clicked() {
                                self.pending_dialog = None;
                            }
                            if ui.button("Clean Temp Dirs").clicked() {
                                self.pending_dialog = None;
                                self.cleanup_selected_temp_dirs();
                            }
                        });
                    });
            }
            PendingDialog::ConfirmBuiltInRun { target, step } => {
                let (title, warning, confirm_label) = match step {
                    BuiltInRunStep::Initial => (
                        "Confirm Built-In Benchmark",
                        format!(
                            "{} is built-in storage at {}. Running benchmarks here can stress your system volume and affect the machine while the test runs.",
                            target.name,
                            target.mount_point.display()
                        ),
                        "Continue",
                    ),
                    BuiltInRunStep::Final => (
                        "Confirm Again",
                        "This is the second confirmation for a built-in storage benchmark. Only continue if you explicitly want to benchmark this internal volume.".to_string(),
                        "Run Benchmark",
                    ),
                };
                egui::Window::new(title)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(warning);
                        ui.horizontal(|ui| {
                            if ui.button("Cancel").clicked() {
                                self.pending_dialog = None;
                            }
                            if ui.button(confirm_label).clicked() {
                                match step {
                                    BuiltInRunStep::Initial => {
                                        self.pending_dialog = Some(PendingDialog::ConfirmBuiltInRun {
                                            target,
                                            step: BuiltInRunStep::Final,
                                        });
                                    }
                                    BuiltInRunStep::Final => {
                                        self.pending_dialog = None;
                                        self.start_run_for_target(target);
                                    }
                                }
                            }
                        });
                    });
            }
        }
    }

    fn poll_picker(&mut self) {
        let mut clear_receiver = false;
        if let Some(receiver) = &self.picker_receiver {
            while let Ok(path) = receiver.try_recv() {
                clear_receiver = true;
                self.picker_pending = false;
                if let Some(path) = path {
                    self.export_path = path.display().to_string();
                    if let Some(parent) = path.parent() {
                        self.export_directory = parent.to_path_buf();
                        let _ = save_export_directory(parent);
                    }
                    self.export_status = Some(format!("Selected export path {}", path.display()));
                    self.error_message = None;
                }
            }
        }
        if clear_receiver {
            self.picker_receiver = None;
        }
    }

    fn selected_run(&self) -> Option<&BenchmarkRunRecord> {
        let run_id = self.selected_run_id.as_ref()?;
        self.history.iter().find(|record| &record.run_id == run_id)
    }

    fn comparison_runs(&self) -> Vec<&BenchmarkRunRecord> {
        self.comparison_run_ids
            .iter()
            .filter_map(|run_id| self.history.iter().find(|record| &record.run_id == run_id))
            .collect()
    }

    fn trend_runs(&self) -> Vec<BenchmarkRunRecord> {
        let device_name = self
            .history_device_filter
            .clone()
            .or_else(|| self.selected_run().map(|run| run.target.name.clone()));

        let mut runs = self
            .history
            .iter()
            .filter(|record| {
                device_name
                    .as_ref()
                    .is_none_or(|name| &record.target.name == name)
                    && self
                        .history_profile_filter
                        .as_ref()
                        .is_none_or(|profile| &record.profile.preset == profile)
            })
            .cloned()
            .collect::<Vec<_>>();
        runs.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        runs
    }

    fn sync_annotation_editors(&mut self) {
        if let Some((tags, notes)) = self
            .selected_run()
            .map(|run| (run.tags.join(", "), run.notes.clone().unwrap_or_default()))
        {
            self.tag_editor = tags;
            self.note_editor = notes;
        }
    }

    fn save_annotations(&mut self) {
        let Some(run_id) = self.selected_run_id.clone() else {
            return;
        };
        let tags = self
            .tag_editor
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        match HistoryStore::default_store().and_then(|store| {
            store.update_annotations(&run_id, tags, Some(self.note_editor.clone()))
        }) {
            Ok(Some(updated)) => {
                if let Some(existing) = self
                    .history
                    .iter_mut()
                    .find(|record| record.run_id == run_id)
                {
                    *existing = updated.clone();
                }
                if self
                    .latest_run
                    .as_ref()
                    .is_some_and(|record| record.run_id == updated.run_id)
                {
                    self.latest_run = Some(updated);
                }
                self.export_status = Some("Saved tags and notes.".to_string());
            }
            Ok(None) => {
                self.error_message = Some("Selected run no longer exists in history.".to_string())
            }
            Err(error) => self.error_message = Some(error.to_string()),
        }
    }

    fn begin_export_picker(&mut self, suggestion: &str) {
        if self.picker_pending {
            return;
        }

        self.picker_pending = true;
        self.error_message = None;
        let directory = self.export_directory.clone();
        let file_name = suggested_file_name(suggestion, self.selected_export_format);
        let format = self.selected_export_format;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let dialog = FileDialog::new()
                .set_directory(directory)
                .set_file_name(&file_name)
                .add_filter(format.label(), &[format.extension()]);
            let _ = sender.send(dialog.save_file());
        });
        self.picker_receiver = Some(receiver);
    }

    fn export_runs(&mut self, format: ExportFormat, title: &str, runs: &[BenchmarkRunRecord]) {
        if runs.is_empty() {
            self.error_message = Some("No benchmark run is available for export.".to_string());
            return;
        }
        let output_path = normalize_export_path(&self.export_path, format, &self.export_directory);
        match export_runs_to_path(format, title, runs, &output_path) {
            Ok(()) => {
                if let Some(parent) = output_path.parent() {
                    self.export_directory = parent.to_path_buf();
                    let _ = save_export_directory(parent);
                }
                self.export_path = output_path.display().to_string();
                self.export_status = Some(format!(
                    "Exported {} run(s) to {}",
                    runs.len(),
                    output_path.display()
                ));
                self.error_message = None;
            }
            Err(error) => self.error_message = Some(error.to_string()),
        }
    }

    fn progress_display(&self) -> Option<benchmark::RunProgressDisplay> {
        let worker = self.worker.as_ref()?;
        let total_benchmarks = worker.benchmarks.len().max(1);
        let last_progress = self.last_progress.as_ref();
        let current_benchmark = last_progress
            .map(|progress| progress.benchmark)
            .or_else(|| worker.benchmarks.first().copied())?;
        let current_index = worker
            .benchmarks
            .iter()
            .position(|benchmark| *benchmark == current_benchmark)
            .unwrap_or_default();
        let benchmark_fraction = last_progress
            .and_then(|progress| benchmark_fraction(progress, &worker.profile));
        let suite_fraction = (((current_index as f32) + benchmark_fraction.unwrap_or(0.0))
            / total_benchmarks as f32)
            .clamp(0.0, 1.0);
        let elapsed = last_progress
            .map(|progress| progress.elapsed)
            .unwrap_or_else(|| worker.started_at.elapsed());
        let status_line = if let Some(progress) = last_progress {
            format!(
                "Running {}/{}: {} {}",
                current_index + 1,
                total_benchmarks,
                progress.benchmark.label(),
                progress.phase,
            )
        } else {
            format!(
                "Preparing benchmark 1/{}: {}",
                total_benchmarks,
                current_benchmark.label(),
            )
        };
        let detail_line = if let Some(progress) = last_progress {
            let processed = format_mib(progress.bytes_processed);
            let current_rate = format!("{:.1} MiB/s", progress.current_mbps);
            if let Some(total) = progress.bytes_total {
                format!(
                    "{} / {} processed, current throughput {}, elapsed {}",
                    processed,
                    format_mib(total),
                    current_rate,
                    format_duration(progress.elapsed),
                )
            } else {
                format!(
                    "{} processed, current throughput {}, elapsed {}",
                    processed,
                    current_rate,
                    format_duration(progress.elapsed),
                )
            }
        } else {
            format!("Starting worker thread, elapsed {}", format_duration(elapsed))
        };
        let remaining = worker
            .benchmarks
            .iter()
            .skip(current_index.saturating_add(1))
            .map(|benchmark| benchmark.label())
            .collect::<Vec<_>>();
        let queue_line = if remaining.is_empty() {
            None
        } else {
            Some(format!("Remaining: {}", remaining.join(" -> ")))
        };

        Some(benchmark::RunProgressDisplay {
            status_line,
            detail_line,
            suite_label: format!("Benchmark suite {:.0}%", suite_fraction * 100.0),
            suite_fraction,
            benchmark_label: format!("{} progress", current_benchmark.label()),
            benchmark_fraction,
            queue_line,
            cancelling: worker.cancel_requested,
        })
    }
}

impl eframe::App for TaktApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_worker();
        self.poll_picker();
        let is_running = self.is_running();
        let progress_display = self.progress_display();

        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("Takt");
                if ui
                    .add_enabled(!is_running, egui::Button::new("Refresh Devices"))
                    .clicked()
                {
                    self.refresh_devices();
                }
                if ui
                    .add_enabled(!is_running, egui::Button::new("Clean Temp Dirs"))
                    .clicked()
                {
                    self.request_cleanup_selected_temp_dirs();
                }
                let run_label = if is_running {
                    "Running..."
                } else {
                    "Run Benchmark"
                };
                if ui
                    .add_enabled(!is_running, egui::Button::new(run_label))
                    .clicked()
                {
                    self.request_start_run();
                }
                let cancel_requested = self
                    .worker
                    .as_ref()
                    .is_some_and(|worker| worker.cancel_requested);
                if ui
                    .add_enabled(
                        is_running && !cancel_requested,
                        egui::Button::new(if cancel_requested {
                            "Cancelling..."
                        } else {
                            "Cancel"
                        }),
                    )
                    .clicked()
                {
                    self.cancel_run();
                }
                ui.add_enabled_ui(!is_running, |ui| {
                    ui.checkbox(&mut self.auto_cleanup_temp_dirs, "Auto-clean temp dirs");
                });
            });
            if let Some(status) = &self.cleanup_status {
                ui.label(status);
            }
        });
        ui.separator();

        egui::ScrollArea::vertical()
            .scroll_source(
                egui::scroll_area::ScrollSource::SCROLL_BAR
                    | egui::scroll_area::ScrollSource::MOUSE_WHEEL,
            )
            .auto_shrink([false, false])
            .show(ui, |ui| {

                benchmark::show_controls(
                    ui,
                    &self.devices,
                    &mut self.selected_target,
                    &mut self.profile,
                    &mut self.selected_benchmarks,
                    !is_running,
                    progress_display.as_ref(),
                    self.benchmark_status.as_ref().map(|status| benchmark::RunStatusBanner {
                        kind: match status.kind {
                            BenchmarkStatusKind::Success => benchmark::RunStatusKind::Success,
                            BenchmarkStatusKind::Cancelled => benchmark::RunStatusKind::Warning,
                            BenchmarkStatusKind::Failure => benchmark::RunStatusKind::Error,
                        },
                        title: status.title.as_str(),
                        detail: status.detail.as_str(),
                    }),
                    self.live_plot_revision,
                    &self.live_samples,
                );

                if let Some(error_message) = &self.error_message {
                    ui.colored_label(egui::Color32::from_rgb(176, 58, 46), error_message);
                }

                if let Some(run) = &self.latest_run {
                    ui.separator();
                    benchmark::show_run_summary(ui, run);
                    match render_export_controls(
                        ui,
                        &mut self.selected_export_format,
                        &self.export_directory,
                        &mut self.export_path,
                        &mut self.export_status,
                        std::slice::from_ref(run),
                        !is_running,
                        self.picker_pending,
                    ) {
                        Some(ExportControlAction::Browse) => {
                            self.begin_export_picker(&run.display_name())
                        }
                        Some(ExportControlAction::Export) => {
                            let runs = vec![run.clone()];
                            self.export_runs(
                                self.selected_export_format,
                                "Immediate benchmark export",
                                &runs,
                            );
                        }
                        None => {}
                    }
                }

                ui.separator();
                let previous_selected_run = self.selected_run_id.clone();
                history::show_history(
                    ui,
                    &self.history,
                    &mut self.selected_run_id,
                    &mut self.comparison_run_ids,
                    &mut self.history_device_filter,
                    &mut self.history_profile_filter,
                    !is_running,
                );
                if self.selected_run_id != previous_selected_run {
                    self.sync_annotation_editors();
                }

                if let Some(selected_run) = self.selected_run().cloned() {
                    ui.separator();
                    ui.heading("Annotations and Export");
                    ui.add_enabled_ui(!is_running, |ui| {
                        ui.label("Tags (comma separated)");
                        ui.text_edit_singleline(&mut self.tag_editor);
                        ui.label("Notes");
                        ui.text_edit_multiline(&mut self.note_editor);
                        if ui.button("Save tags and notes").clicked() {
                            self.save_annotations();
                        }
                    });
                    match render_export_controls(
                        ui,
                        &mut self.selected_export_format,
                        &self.export_directory,
                        &mut self.export_path,
                        &mut self.export_status,
                        std::slice::from_ref(&selected_run),
                        !is_running,
                        self.picker_pending,
                    ) {
                        Some(ExportControlAction::Browse) => {
                            self.begin_export_picker(&selected_run.display_name())
                        }
                        Some(ExportControlAction::Export) => {
                            let runs = vec![selected_run.clone()];
                            self.export_runs(
                                self.selected_export_format,
                                "Detailed benchmark export",
                                &runs,
                            );
                        }
                        None => {}
                    }
                    ui.separator();
                    detail::show_run_detail(ui, &selected_run);
                }

                let comparison_runs = self.comparison_runs();
                let trend_runs = self.trend_runs();
                ui.separator();
                comparison::show_trend_view(ui, &trend_runs);
                if comparison_runs.len() == 2 {
                    ui.separator();
                    comparison::show_two_run_comparison(ui, comparison_runs[0], comparison_runs[1]);
                    let runs = vec![comparison_runs[0].clone(), comparison_runs[1].clone()];
                    match render_export_controls(
                        ui,
                        &mut self.selected_export_format,
                        &self.export_directory,
                        &mut self.export_path,
                        &mut self.export_status,
                        &runs,
                        !is_running,
                        self.picker_pending,
                    ) {
                        Some(ExportControlAction::Browse) => {
                            self.begin_export_picker("comparison-export")
                        }
                        Some(ExportControlAction::Export) => {
                            self.export_runs(self.selected_export_format, "Comparison export", &runs);
                        }
                        None => {}
                    }
                }

                if let Some(status) = &self.export_status {
                    ui.separator();
                    ui.label(status);
                }
            });

        self.show_pending_dialog(ui.ctx());

        if is_running {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

fn cleanup_message(target: &DeviceTarget, removed: usize) -> String {
    if removed == 0 {
        format!(
            "No leftover benchmark temp directories found on {}.",
            target.mount_point.display()
        )
    } else {
        format!(
            "Removed {} leftover benchmark temp director{} from {}.",
            removed,
            if removed == 1 { "y" } else { "ies" },
            target.mount_point.display()
        )
    }
}

fn is_high_risk_benchmark_target(target: &DeviceTarget) -> bool {
    matches!(target.kind, takt_core::DeviceKind::BuiltIn)
}

fn render_export_controls(
    ui: &mut egui::Ui,
    selected_export_format: &mut ExportFormat,
    export_directory: &Path,
    export_path: &mut String,
    export_status: &mut Option<String>,
    runs: &[BenchmarkRunRecord],
    controls_enabled: bool,
    picker_pending: bool,
) -> Option<ExportControlAction> {
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
            ui.label("Text exports include benchmark metrics, annotations, and device context. PNG-specific layout selection is only used when PNG is chosen.");
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
                action = Some(ExportControlAction::Browse);
            }
            if ui
                .button(format!("Export {}", selected_export_format.label()))
                .clicked()
            {
                action = Some(ExportControlAction::Export);
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

fn benchmark_fraction(progress: &ProgressUpdate, profile: &BenchmarkProfile) -> Option<f32> {
    if let Some(total) = progress.bytes_total {
        if total == 0 {
            return None;
        }
        return Some((progress.bytes_processed as f32 / total as f32).clamp(0.0, 1.0));
    }

    match progress.benchmark {
        BenchmarkType::SustainedWrite => Some(
            (progress.elapsed.as_secs_f32() / profile.sustained_seconds.max(1) as f32)
                .clamp(0.0, 1.0),
        ),
        _ => None,
    }
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60,
    )
}

fn format_mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / 1024.0 / 1024.0)
}

fn normalize_export_path(path: &str, format: ExportFormat, export_directory: &Path) -> PathBuf {
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

fn suggested_file_name(suggestion: &str, format: ExportFormat) -> String {
    let stem = suggestion
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!(
        "{}.{}",
        if stem.is_empty() {
            "benchmark-export"
        } else {
            &stem
        },
        format.extension()
    )
}

fn default_export_directory() -> PathBuf {
    if let Some(user_dirs) = UserDirs::new() {
        if let Some(documents_dir) = user_dirs.document_dir() {
            return documents_dir.to_path_buf();
        }
        return user_dirs.home_dir().to_path_buf();
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn export_settings_path() -> Option<PathBuf> {
    let project_dirs = ProjectDirs::from("com", "takt", "takt")?;
    Some(project_dirs.config_local_dir().join("gui-export-dir.txt"))
}

fn load_export_directory() -> Option<PathBuf> {
    let path = export_settings_path()?;
    let content = fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn save_export_directory(directory: &Path) -> std::io::Result<()> {
    let Some(path) = export_settings_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, directory.display().to_string())
}

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use eframe::egui;
use riedspied_core::{
    BenchmarkProfile, BenchmarkRunRecord, BenchmarkType, DeviceTarget, ExportFormat, HistoryStore,
    ProfilePreset, ProgressUpdate, RunConfiguration, discover_devices, export_runs_to_path,
    run_benchmark_suite,
};

use crate::views::{benchmark, comparison, detail, history};

pub struct RiedspiedApp {
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
    export_path: String,
    export_status: Option<String>,
    tag_editor: String,
    note_editor: String,
    worker: Option<WorkerState>,
    error_message: Option<String>,
}

struct WorkerState {
    receiver: Receiver<WorkerEvent>,
    cancel_flag: Arc<AtomicBool>,
}

enum WorkerEvent {
    Progress(ProgressUpdate),
    Finished(Box<Result<BenchmarkRunRecord, String>>),
}

impl Default for RiedspiedApp {
    fn default() -> Self {
        let devices = discover_devices().unwrap_or_default();
        let selected_target = devices.first().map(|device| device.id.clone());
        let history = HistoryStore::default_store()
            .and_then(|store| store.load())
            .unwrap_or_default();

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
            export_path: "benchmark-export".to_string(),
            export_status: None,
            tag_editor: String::new(),
            note_editor: String::new(),
            worker: None,
            error_message: None,
        }
    }
}

impl RiedspiedApp {
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

    fn start_run(&mut self) {
        if self.worker.is_some() {
            return;
        }

        let Some(target) = self.selected_device().cloned() else {
            self.error_message = Some("select a benchmark target first".to_string());
            return;
        };

        self.last_progress = None;
        self.live_samples.clear();
        self.error_message = None;
        self.export_status = None;

        let (sender, receiver) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let worker_cancel_flag = Arc::clone(&cancel_flag);
        let profile = BenchmarkProfile::from_preset(self.profile.clone());
        let benchmarks = if self.selected_benchmarks.is_empty() {
            BenchmarkType::ALL.to_vec()
        } else {
            self.selected_benchmarks.clone()
        };

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
        });
    }

    fn cancel_run(&mut self) {
        if let Some(worker) = &self.worker {
            worker.cancel_flag.store(true, Ordering::Relaxed);
        }
    }

    fn poll_worker(&mut self) {
        let mut finished = false;

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
                        match *result {
                            Ok(run) => {
                                self.latest_run = Some(run.clone());
                                self.selected_run_id = Some(run.run_id.clone());
                                self.history.insert(0, run.clone());
                                self.tag_editor = run.tags.join(", ");
                                self.note_editor = run.notes.clone().unwrap_or_default();
                                self.error_message = None;
                            }
                            Err(error) => self.error_message = Some(error),
                        }
                    }
                }
            }
        }

        if finished {
            self.worker = None;
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

    fn export_runs(&mut self, format: ExportFormat, title: &str, runs: &[BenchmarkRunRecord]) {
        if runs.is_empty() {
            self.error_message = Some("No benchmark run is available for export.".to_string());
            return;
        }
        let output_path = normalize_export_path(&self.export_path, format);
        match export_runs_to_path(format, title, runs, &output_path) {
            Ok(()) => {
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
}

impl eframe::App for RiedspiedApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_worker();

        ui.horizontal(|ui| {
            ui.heading("riedspied");
            if ui.button("Refresh Devices").clicked() {
                self.refresh_devices();
            }
            let run_label = if self.worker.is_some() {
                "Running..."
            } else {
                "Run Benchmark"
            };
            if ui
                .add_enabled(self.worker.is_none(), egui::Button::new(run_label))
                .clicked()
            {
                self.start_run();
            }
            if ui
                .add_enabled(self.worker.is_some(), egui::Button::new("Cancel"))
                .clicked()
            {
                self.cancel_run();
            }
        });
        ui.separator();

        benchmark::show_controls(
            ui,
            &self.devices,
            &mut self.selected_target,
            &mut self.profile,
            &mut self.selected_benchmarks,
            self.last_progress.as_ref(),
            &self.live_samples,
        );

        if let Some(error_message) = &self.error_message {
            ui.colored_label(egui::Color32::from_rgb(176, 58, 46), error_message);
        }

        if let Some(run) = &self.latest_run {
            ui.separator();
            benchmark::show_run_summary(ui, run);
            if let Some(format) =
                render_export_controls(ui, &mut self.export_path, &mut self.export_status)
            {
                let runs = vec![run.clone()];
                self.export_runs(format, "Immediate benchmark export", &runs);
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
        );
        if self.selected_run_id != previous_selected_run {
            self.sync_annotation_editors();
        }

        if let Some(selected_run) = self.selected_run().cloned() {
            ui.separator();
            ui.heading("Annotations and Export");
            ui.label("Tags (comma separated)");
            ui.text_edit_singleline(&mut self.tag_editor);
            ui.label("Notes");
            ui.text_edit_multiline(&mut self.note_editor);
            if ui.button("Save tags and notes").clicked() {
                self.save_annotations();
            }
            if let Some(format) =
                render_export_controls(ui, &mut self.export_path, &mut self.export_status)
            {
                let runs = vec![selected_run.clone()];
                self.export_runs(format, "Detailed benchmark export", &runs);
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
            if let Some(format) =
                render_export_controls(ui, &mut self.export_path, &mut self.export_status)
            {
                self.export_runs(format, "Comparison export", &runs);
            }
        }

        if let Some(status) = &self.export_status {
            ui.separator();
            ui.label(status);
        }

        if self.worker.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

fn render_export_controls(
    ui: &mut egui::Ui,
    export_path: &mut String,
    export_status: &mut Option<String>,
) -> Option<ExportFormat> {
    let mut requested_export = None;
    ui.horizontal(|ui| {
        ui.label("Export path");
        ui.text_edit_singleline(export_path);
    });
    ui.horizontal_wrapped(|ui| {
        if ui.button("Export JSON").clicked() {
            requested_export = Some(ExportFormat::Json);
        }
        if ui.button("Export Markdown").clicked() {
            requested_export = Some(ExportFormat::Markdown);
        }
        if ui.button("Export HTML").clicked() {
            requested_export = Some(ExportFormat::Html);
        }
        if ui.button("Export PNG").clicked() {
            requested_export = Some(ExportFormat::Png);
        }
        if ui.button("Clear status").clicked() {
            *export_status = None;
        }
    });

    requested_export
}

fn normalize_export_path(path: &str, format: ExportFormat) -> PathBuf {
    let mut output = PathBuf::from(path.trim());
    if output.extension().is_none() {
        output.set_extension(format.extension());
    }
    if output.as_os_str().is_empty() {
        output = PathBuf::from(format!("benchmark-export.{}", format.extension()));
    }
    if output.is_relative() {
        Path::new(".").join(output)
    } else {
        output
    }
}

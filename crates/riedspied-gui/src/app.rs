use std::sync::mpsc::{self, Receiver};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use eframe::egui;
use riedspied_core::{
    BenchmarkProfile, BenchmarkRunRecord, BenchmarkType, DeviceTarget, HistoryStore, ProfilePreset,
    ProgressUpdate, RunConfiguration, discover_devices, run_benchmark_suite,
};

use crate::views::{benchmark, history};

pub struct RiedspiedApp {
    devices: Vec<DeviceTarget>,
    selected_target: Option<String>,
    profile: ProfilePreset,
    history: Vec<BenchmarkRunRecord>,
    latest_run: Option<BenchmarkRunRecord>,
    last_progress: Option<ProgressUpdate>,
    live_samples: Vec<[f64; 2]>,
    worker: Option<WorkerState>,
    error_message: Option<String>,
}

struct WorkerState {
    receiver: Receiver<WorkerEvent>,
    cancel_flag: Arc<AtomicBool>,
}

enum WorkerEvent {
    Progress(ProgressUpdate),
    Finished(Result<BenchmarkRunRecord, String>),
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
            history,
            latest_run: None,
            last_progress: None,
            live_samples: Vec::new(),
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

        let (sender, receiver) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let worker_cancel_flag = Arc::clone(&cancel_flag);
        let profile = BenchmarkProfile::from_preset(self.profile.clone());

        std::thread::spawn(move || {
            let configuration = RunConfiguration {
                profile,
                benchmarks: BenchmarkType::ALL.to_vec(),
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

            let _ = sender.send(WorkerEvent::Finished(run));
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
                        match result {
                            Ok(run) => {
                                self.latest_run = Some(run.clone());
                                self.history.insert(0, run);
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
            self.last_progress.as_ref(),
            &self.live_samples,
        );

        if let Some(error_message) = &self.error_message {
            ui.colored_label(egui::Color32::from_rgb(176, 58, 46), error_message);
        }

        if let Some(run) = &self.latest_run {
            ui.separator();
            benchmark::show_run_summary(ui, run);
        }

        ui.separator();
        history::show_history(ui, &self.history);

        if self.worker.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

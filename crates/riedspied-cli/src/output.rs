use indicatif::{ProgressBar, ProgressStyle};
use riedspied_core::{BenchmarkRunRecord, BenchmarkType, ProgressUpdate};

pub struct TerminalReporter {
    progress_bar: ProgressBar,
    current_benchmark: Option<BenchmarkType>,
}

impl TerminalReporter {
    pub fn new() -> Self {
        let progress_bar = ProgressBar::new_spinner();
        progress_bar.enable_steady_tick(std::time::Duration::from_millis(120));
        progress_bar.set_style(
            ProgressStyle::with_template("{spinner:.green} {msg}")
                .expect("valid progress template"),
        );

        Self {
            progress_bar,
            current_benchmark: None,
        }
    }

    pub fn update(&mut self, update: ProgressUpdate) {
        if self.current_benchmark != Some(update.benchmark) {
            self.current_benchmark = Some(update.benchmark);
            if let Some(total) = update.bytes_total {
                self.progress_bar.set_length(total);
            } else {
                self.progress_bar.set_length(0);
            }
        }

        if let Some(total) = update.bytes_total {
            self.progress_bar.set_length(total);
            self.progress_bar
                .set_position(update.bytes_processed.min(total));
        }

        self.progress_bar.set_message(format!(
            "{} {} {:.1} MiB/s elapsed {:.1}s",
            update.benchmark.label(),
            update.phase,
            update.current_mbps,
            update.elapsed.as_secs_f64(),
        ));
    }

    pub fn finish(&self) {
        self.progress_bar.finish_and_clear();
    }
}

pub fn print_device_table(devices: &[riedspied_core::DeviceTarget]) {
    for device in devices {
        println!(
            "{:<20} {:<12} {:<12} {:>8} GiB free {:>8} GiB total {}",
            device.name,
            format!("{:?}", device.kind),
            device.filesystem,
            gib(device.available_bytes),
            gib(device.total_bytes),
            device.mount_point.display(),
        );
    }
}

pub fn print_run_summary(run: &BenchmarkRunRecord) {
    println!(
        "Benchmark completed for {} ({}) with {} profile",
        run.target.name,
        run.target.mount_point.display(),
        run.profile.preset,
    );
    for result in &run.results {
        println!(
            "- {:<18} avg {:>8.1} MiB/s peak {:>8.1} MiB/s min {:>8.1} MiB/s{}{}",
            result.benchmark.label(),
            result.average_mbps,
            result.peak_mbps,
            result.minimum_mbps,
            result
                .iops
                .map(|value| format!(" iops {:>8.0}", value))
                .unwrap_or_default(),
            result
                .latency_ms_p95
                .map(|value| format!(" p95 {:>6.2} ms", value))
                .unwrap_or_default(),
        );
    }
}

pub fn print_history(records: &[BenchmarkRunRecord]) {
    for record in records {
        println!(
            "{}  {}  {}  {} results",
            record.started_at.format("%Y-%m-%d %H:%M:%S"),
            record.target.name,
            record.profile.preset,
            record.results.len(),
        );
        for result in &record.results {
            println!(
                "  {:<18} avg {:>8.1} MiB/s peak {:>8.1} MiB/s{}",
                result.benchmark.label(),
                result.average_mbps,
                result.peak_mbps,
                result
                    .iops
                    .map(|value| format!(" iops {:>8.0}", value))
                    .unwrap_or_default(),
            );
        }
    }
}

fn gib(bytes: u64) -> u64 {
    bytes / 1024 / 1024 / 1024
}

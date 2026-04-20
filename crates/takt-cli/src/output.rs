use indicatif::{ProgressBar, ProgressStyle};
use takt_core::{BenchmarkRunRecord, BenchmarkType, ProgressUpdate};

pub struct TerminalReporter {
    progress_bar: ProgressBar,
    benchmarks: Vec<BenchmarkType>,
    current_benchmark: Option<BenchmarkType>,
}

impl TerminalReporter {
    pub fn new(benchmarks: &[BenchmarkType]) -> Self {
        let progress_bar = ProgressBar::new_spinner();
        progress_bar.enable_steady_tick(std::time::Duration::from_millis(120));
        progress_bar.set_style(
            ProgressStyle::with_template("{spinner:.green} {msg}")
                .expect("valid progress template"),
        );
        progress_bar.set_message(format!(
            "Preparing {} benchmark(s)...",
            benchmarks.len().max(1)
        ));

        Self {
            progress_bar,
            benchmarks: benchmarks.to_vec(),
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

        let benchmark_index = self
            .benchmarks
            .iter()
            .position(|benchmark| *benchmark == update.benchmark)
            .map(|index| index + 1)
            .unwrap_or(1);
        self.progress_bar.set_message(format!(
            "[{}/{}] {} {} {:.1} MiB/s elapsed {:.1}s",
            benchmark_index,
            self.benchmarks.len().max(1),
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

pub fn print_device_table(devices: &[takt_core::DeviceTarget], verbose: bool) {
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
        if verbose {
            println!(
                "  id={} source={} readonly={} removable={} storage={} bus={} protocol={} model={} vendor={} usb={} volume-uuid={} partition-uuid={} mount-options={}",
                device.id,
                device.source,
                device.metadata.is_read_only,
                device
                    .metadata
                    .is_removable
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                device.storage_hint().unwrap_or("unknown"),
                device.transport_hint().unwrap_or("unknown"),
                device.metadata.network_protocol.as_deref().unwrap_or("-"),
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
            );
        }
    }
}

pub fn print_run_summary(run: &BenchmarkRunRecord) {
    println!(
        "Benchmark completed for {} ({}) with {} profile",
        run.target.name,
        run.target.mount_point.display(),
        run.profile.preset,
    );
    println!("Run ID: {}", run.run_id);
    if !run.tags.is_empty() {
        println!("Tags: {}", run.tags.join(", "));
    }
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

pub fn print_history(records: &[BenchmarkRunRecord], verbose: bool) {
    for record in records {
        println!(
            "{}  {}  {}  {} results  id={}{}",
            record.started_at.format("%Y-%m-%d %H:%M:%S"),
            record.target.name,
            record.profile.preset,
            record.results.len(),
            record.run_id,
            if record.tags.is_empty() {
                String::new()
            } else {
                format!("  tags={}", record.tags.join(","))
            }
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
        if verbose {
            println!(
                "  target={} target-id={} fs={} readonly={} transport={} model={} vendor={}",
                record.target.mount_point.display(),
                record.target.id,
                record.target.filesystem,
                record.target.metadata.is_read_only,
                record.target.transport_hint().unwrap_or("unknown"),
                record.target.metadata.model.as_deref().unwrap_or("-"),
                record.target.metadata.vendor.as_deref().unwrap_or("-")
            );
            if let Some(notes) = &record.notes {
                println!("  notes={notes}");
            }
        }
    }
}

pub fn print_export_notice(format: &str, path: &std::path::Path, run_count: usize) {
    println!(
        "Exported {} run(s) as {} to {}",
        run_count,
        format,
        path.display()
    );
}

fn gib(bytes: u64) -> u64 {
    bytes / 1024 / 1024 / 1024
}

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use riedspied_core::{
    BenchmarkProfile, BenchmarkRunRecord, BenchmarkType, DeviceTarget, ExportFormat, HistoryStore,
    ProfilePreset, RunConfiguration, discover_devices, export_runs_to_path, run_benchmark_suite,
};

use crate::output::TerminalReporter;

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ProfileChoice {
    Quick,
    Balanced,
    Thorough,
}

impl From<ProfileChoice> for ProfilePreset {
    fn from(value: ProfileChoice) -> Self {
        match value {
            ProfileChoice::Quick => ProfilePreset::Quick,
            ProfileChoice::Balanced => ProfilePreset::Balanced,
            ProfileChoice::Thorough => ProfilePreset::Thorough,
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum BenchmarkChoice {
    SequentialWrite,
    SequentialRead,
    SustainedWrite,
    RandomIops,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ExportFormatChoice {
    Json,
    Markdown,
    Html,
    Png,
}

impl From<ExportFormatChoice> for ExportFormat {
    fn from(value: ExportFormatChoice) -> Self {
        match value {
            ExportFormatChoice::Json => ExportFormat::Json,
            ExportFormatChoice::Markdown => ExportFormat::Markdown,
            ExportFormatChoice::Html => ExportFormat::Html,
            ExportFormatChoice::Png => ExportFormat::Png,
        }
    }
}

impl From<BenchmarkChoice> for BenchmarkType {
    fn from(value: BenchmarkChoice) -> Self {
        match value {
            BenchmarkChoice::SequentialWrite => BenchmarkType::SequentialWrite,
            BenchmarkChoice::SequentialRead => BenchmarkType::SequentialRead,
            BenchmarkChoice::SustainedWrite => BenchmarkType::SustainedWrite,
            BenchmarkChoice::RandomIops => BenchmarkType::RandomIops,
        }
    }
}

pub fn list_targets() -> Result<Vec<DeviceTarget>> {
    discover_devices()
}

pub fn find_target(target: &str) -> Result<DeviceTarget> {
    let target_path = Path::new(target);
    let devices = discover_devices().context("failed to discover mounted devices")?;

    devices
        .into_iter()
        .find(|device| device.mount_point == target_path || device.matches_reference(target))
        .with_context(|| format!("no benchmark target matched {target}"))
}

pub fn run_benchmark(
    target: &str,
    profile: ProfileChoice,
    requested_benchmarks: Vec<BenchmarkChoice>,
    keep_temp_files: bool,
    store_history: bool,
    tags: Vec<String>,
) -> Result<BenchmarkRunRecord> {
    let target = find_target(target)?;
    let profile = BenchmarkProfile::from_preset(profile.into());
    let benchmarks = if requested_benchmarks.is_empty() {
        BenchmarkType::ALL.to_vec()
    } else {
        requested_benchmarks.into_iter().map(Into::into).collect()
    };
    let configuration = RunConfiguration {
        profile,
        benchmarks,
        keep_temp_files,
    };

    let mut reporter = TerminalReporter::new();
    let mut run = run_benchmark_suite(&target, configuration, None, |update| {
        reporter.update(update);
    })?;
    reporter.finish();
    run.tags = tags;

    if store_history {
        let store = HistoryStore::default_store()?;
        store.save(&run)?;
    }

    Ok(run)
}

pub fn load_history(
    limit: usize,
    target_filter: Option<&str>,
    profile_filter: Option<ProfileChoice>,
) -> Result<Vec<BenchmarkRunRecord>> {
    if limit == 0 {
        bail!("history limit must be greater than zero");
    }

    let store = HistoryStore::default_store()?;
    let mut records = store.load()?;
    if let Some(target_filter) = target_filter {
        records.retain(|record| record.target.matches_reference(target_filter));
    }
    if let Some(profile_filter) = profile_filter {
        let selected_profile: ProfilePreset = profile_filter.into();
        records.retain(|record| record.profile.preset == selected_profile);
    }
    records.truncate(limit);
    Ok(records)
}

pub fn export_runs(
    run_ids: Vec<String>,
    latest: bool,
    format: ExportFormatChoice,
    output: PathBuf,
    title: Option<String>,
) -> Result<usize> {
    let store = HistoryStore::default_store()?;
    let runs = if latest {
        store.load()?.into_iter().take(1).collect::<Vec<_>>()
    } else {
        store.load_selected(&run_ids)?
    };

    if runs.is_empty() {
        bail!("no benchmark runs matched the export selection");
    }

    let export_title = title.unwrap_or_else(|| {
        if runs.len() == 1 {
            format!("Benchmark export: {}", runs[0].display_name())
        } else {
            format!("Benchmark comparison export ({})", runs.len())
        }
    });
    export_runs_to_path(format.into(), &export_title, &runs, &output)?;
    Ok(runs.len())
}

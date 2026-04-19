use std::path::Path;

use anyhow::{Context, Result, bail};
use riedspied_core::{
    BenchmarkProfile, BenchmarkType, DeviceTarget, HistoryStore, ProfilePreset, RunConfiguration,
    discover_devices, run_benchmark_suite,
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
        .find(|device| {
            device.mount_point == target_path
                || device.id == target
                || device.name.eq_ignore_ascii_case(target)
        })
        .with_context(|| format!("no benchmark target matched {target}"))
}

pub fn run_benchmark(
    target: &str,
    profile: ProfileChoice,
    requested_benchmarks: Vec<BenchmarkChoice>,
    keep_temp_files: bool,
    store_history: bool,
) -> Result<riedspied_core::BenchmarkRunRecord> {
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
    let run = run_benchmark_suite(&target, configuration, None, |update| {
        reporter.update(update);
    })?;
    reporter.finish();

    if store_history {
        let store = HistoryStore::default_store()?;
        store.save(&run)?;
    }

    Ok(run)
}

pub fn load_history(limit: usize) -> Result<Vec<riedspied_core::BenchmarkRunRecord>> {
    if limit == 0 {
        bail!("history limit must be greater than zero");
    }

    let store = HistoryStore::default_store()?;
    let mut records = store.load()?;
    records.truncate(limit);
    Ok(records)
}

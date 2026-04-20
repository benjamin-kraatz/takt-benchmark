use std::cmp::Ordering;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering as AtomicOrdering},
};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use rand::{Rng, SeedableRng, rngs::SmallRng};
use serde::{Deserialize, Serialize};

use crate::device::DeviceTarget;

mod random;
mod sequential;
mod sustained;

pub use random::run_random_iops;
pub use sequential::{run_sequential_read, run_sequential_write};
pub use sustained::run_sustained_write;

const MIB: u64 = 1024 * 1024;
const KIB: u64 = 1024;
const TEMP_DIR_PREFIX: &str = ".takt-";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum BenchmarkType {
    SequentialWrite,
    SequentialRead,
    SustainedWrite,
    RandomIops,
}

impl BenchmarkType {
    pub const ALL: [BenchmarkType; 4] = [
        BenchmarkType::SequentialWrite,
        BenchmarkType::SequentialRead,
        BenchmarkType::SustainedWrite,
        BenchmarkType::RandomIops,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BenchmarkType::SequentialWrite => "Sequential Write",
            BenchmarkType::SequentialRead => "Sequential Read",
            BenchmarkType::SustainedWrite => "Sustained Write",
            BenchmarkType::RandomIops => "Random IOPS",
        }
    }
    pub fn slug(self) -> &'static str {
        match self {
            BenchmarkType::SequentialWrite => "sequential-write",
            BenchmarkType::SequentialRead => "sequential-read",
            BenchmarkType::SustainedWrite => "sustained-write",
            BenchmarkType::RandomIops => "random-iops",
        }
    }
}

impl fmt::Display for BenchmarkType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProfilePreset {
    Quick,
    Balanced,
    Thorough,
}

impl ProfilePreset {
    pub fn label(&self) -> &'static str {
        match self {
            ProfilePreset::Quick => "Quick",
            ProfilePreset::Balanced => "Balanced",
            ProfilePreset::Thorough => "Thorough",
        }
    }
}

impl fmt::Display for ProfilePreset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkProfile {
    pub preset: ProfilePreset,
    pub sequential_bytes: u64,
    pub sustained_seconds: u64,
    pub random_file_bytes: u64,
    pub random_operations: u64,
    pub chunk_bytes: usize,
    pub block_bytes: usize,
    pub minimum_free_ratio: f64,
}

impl BenchmarkProfile {
    pub fn quick() -> Self {
        Self {
            preset: ProfilePreset::Quick,
            sequential_bytes: 128 * MIB,
            sustained_seconds: 10,
            random_file_bytes: 64 * MIB,
            random_operations: 2_000,
            chunk_bytes: MIB as usize,
            block_bytes: 4 * KIB as usize,
            minimum_free_ratio: 0.10,
        }
    }

    pub fn balanced() -> Self {
        Self {
            preset: ProfilePreset::Balanced,
            sequential_bytes: 512 * MIB,
            sustained_seconds: 20,
            random_file_bytes: 128 * MIB,
            random_operations: 5_000,
            chunk_bytes: MIB as usize,
            block_bytes: 4 * KIB as usize,
            minimum_free_ratio: 0.10,
        }
    }

    pub fn thorough() -> Self {
        Self {
            preset: ProfilePreset::Thorough,
            sequential_bytes: 1024 * MIB,
            sustained_seconds: 45,
            random_file_bytes: 256 * MIB,
            random_operations: 12_000,
            chunk_bytes: MIB as usize,
            block_bytes: 4 * KIB as usize,
            minimum_free_ratio: 0.10,
        }
    }

    pub fn from_preset(preset: ProfilePreset) -> Self {
        match preset {
            ProfilePreset::Quick => Self::quick(),
            ProfilePreset::Balanced => Self::balanced(),
            ProfilePreset::Thorough => Self::thorough(),
        }
    }

    pub fn estimated_required_bytes(&self) -> u64 {
        self.sequential_bytes.saturating_mul(2)
            + self.random_file_bytes
            + (self.chunk_bytes as u64).saturating_mul(4)
    }

    pub fn validate_for(&self, device: &DeviceTarget) -> Result<()> {
        if device.free_ratio() < self.minimum_free_ratio {
            bail!(
                "device {} does not meet minimum free-space ratio of {:.0}%",
                device.mount_point.display(),
                self.minimum_free_ratio * 100.0
            );
        }

        if device.available_bytes < self.estimated_required_bytes() {
            bail!(
                "device {} only has {} MiB free but profile {} needs about {} MiB",
                device.mount_point.display(),
                device.available_bytes / MIB,
                self.preset,
                self.estimated_required_bytes() / MIB,
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfiguration {
    pub profile: BenchmarkProfile,
    pub benchmarks: Vec<BenchmarkType>,
    pub keep_temp_files: bool,
}

impl Default for RunConfiguration {
    fn default() -> Self {
        Self {
            profile: BenchmarkProfile::balanced(),
            benchmarks: BenchmarkType::ALL.to_vec(),
            keep_temp_files: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplePoint {
    pub seconds: f64,
    pub throughput_mbps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub benchmark: BenchmarkType,
    pub bytes_processed: u64,
    pub duration_secs: f64,
    pub average_mbps: f64,
    pub peak_mbps: f64,
    pub minimum_mbps: f64,
    pub iops: Option<f64>,
    pub latency_ms_p50: Option<f64>,
    pub latency_ms_p95: Option<f64>,
    pub samples: Vec<SamplePoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRunRecord {
    #[serde(default)]
    pub run_id: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub target: DeviceTarget,
    pub profile: BenchmarkProfile,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
    pub results: Vec<BenchmarkResult>,
}

impl BenchmarkRunRecord {
    pub fn ensure_defaults(&mut self) {
        if self.run_id.is_empty() {
            self.run_id = build_run_id(&self.started_at, &self.target.id);
        }
    }

    pub fn display_name(&self) -> String {
        format!(
            "{} · {} · {}",
            self.target.name,
            self.profile.preset,
            self.started_at.format("%Y-%m-%d %H:%M:%S")
        )
    }

    pub fn series_label(&self) -> String {
        let mut label = format!(
            "{} {}",
            self.started_at.format("%m-%d %H:%M"),
            self.profile.preset,
        );
        if !self.tags.is_empty() {
            label.push_str(&format!(" [{}]", self.tags.join(",")));
        }
        label
    }

    pub fn result_for(&self, benchmark: BenchmarkType) -> Option<&BenchmarkResult> {
        self.results
            .iter()
            .find(|result| result.benchmark == benchmark)
    }
}

#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub benchmark: BenchmarkType,
    pub phase: &'static str,
    pub bytes_processed: u64,
    pub bytes_total: Option<u64>,
    pub elapsed: Duration,
    pub current_mbps: f64,
}

#[derive(Debug, Clone)]
pub struct BenchmarkContext {
    pub temp_dir: PathBuf,
    pub profile: BenchmarkProfile,
    pub cancel_flag: Option<Arc<AtomicBool>>,
}

impl BenchmarkContext {
    pub fn check_cancelled(&self) -> Result<()> {
        if self
            .cancel_flag
            .as_ref()
            .is_some_and(|flag| flag.load(AtomicOrdering::Relaxed))
        {
            bail!("benchmark cancelled")
        } else {
            Ok(())
        }
    }
}

pub fn run_benchmark_suite(
    target: &DeviceTarget,
    configuration: RunConfiguration,
    cancel_flag: Option<Arc<AtomicBool>>,
    mut progress: impl FnMut(ProgressUpdate),
) -> Result<BenchmarkRunRecord> {
    configuration.profile.validate_for(target)?;
    let started_at = Utc::now();
    let temp_dir = benchmark_temp_dir(target, &started_at);
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("failed to create temp dir {}", temp_dir.display()))?;

    let context = BenchmarkContext {
        temp_dir: temp_dir.clone(),
        profile: configuration.profile.clone(),
        cancel_flag,
    };
    let run_result = (|| {
        let mut results = Vec::new();

        for benchmark in &configuration.benchmarks {
            let result = match benchmark {
                BenchmarkType::SequentialWrite => {
                    run_sequential_write(target, &context, &mut progress)
                }
                BenchmarkType::SequentialRead => run_sequential_read(target, &context, &mut progress),
                BenchmarkType::SustainedWrite => run_sustained_write(target, &context, &mut progress),
                BenchmarkType::RandomIops => run_random_iops(target, &context, &mut progress),
            }?;
            results.push(result);
        }

        Ok(BenchmarkRunRecord {
            run_id: build_run_id(&started_at, &target.id),
            started_at,
            finished_at: Utc::now(),
            target: target.clone(),
            profile: configuration.profile.clone(),
            tags: Vec::new(),
            notes: None,
            results,
        })
    })();

    if !configuration.keep_temp_files {
        let _ = remove_temp_dir_if_present(&temp_dir);
    }

    run_result
}

pub fn cleanup_benchmark_temp_dirs(target: &DeviceTarget) -> Result<usize> {
    cleanup_benchmark_temp_dirs_in_path(&target.mount_point)
}

fn cleanup_benchmark_temp_dirs_in_path(path: &Path) -> Result<usize> {
    let mut removed = 0;
    let entries = fs::read_dir(path)
        .with_context(|| format!("failed to read benchmark target {}", path.display()))?;

    for entry in entries {
        let entry = entry.with_context(|| format!("failed to inspect {}", path.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", entry.path().display()))?;
        if !file_type.is_dir() {
            continue;
        }

        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if !file_name.starts_with(TEMP_DIR_PREFIX) {
            continue;
        }

        fs::remove_dir_all(entry.path())
            .with_context(|| format!("failed to remove {}", entry.path().display()))?;
        removed += 1;
    }

    Ok(removed)
}

fn benchmark_temp_dir(target: &DeviceTarget, started_at: &DateTime<Utc>) -> PathBuf {
    target
        .mount_point
        .join(format!("{}{}", TEMP_DIR_PREFIX, started_at.format("%Y%m%d%H%M%S")))
}

fn remove_temp_dir_if_present(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

pub fn build_run_id(started_at: &DateTime<Utc>, target_id: &str) -> String {
    let suffix = target_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .filter(|character| *character != '-')
        .take(12)
        .collect::<String>();

    format!("{}-{}", started_at.format("%Y%m%d%H%M%S"), suffix)
}

pub(crate) fn benchmark_file(context: &BenchmarkContext, name: &str) -> PathBuf {
    context.temp_dir.join(name)
}

pub(crate) fn ensure_fixture(path: &Path, bytes: u64, chunk_bytes: usize) -> Result<()> {
    if path.exists() && path.metadata()?.len() >= bytes {
        return Ok(());
    }

    let mut file = File::create(path)
        .with_context(|| format!("failed to create fixture file {}", path.display()))?;
    let buffer = vec![0x5A; chunk_bytes.max(4096)];
    let mut written = 0_u64;
    while written < bytes {
        let next_chunk = (bytes - written).min(buffer.len() as u64) as usize;
        file.write_all(&buffer[..next_chunk])?;
        written += next_chunk as u64;
    }
    file.sync_all()?;
    Ok(())
}

pub(crate) fn emit_progress(
    progress: &mut impl FnMut(ProgressUpdate),
    benchmark: BenchmarkType,
    phase: &'static str,
    bytes_processed: u64,
    bytes_total: Option<u64>,
    elapsed: Duration,
    current_mbps: f64,
) {
    progress(ProgressUpdate {
        benchmark,
        phase,
        bytes_processed,
        bytes_total,
        elapsed,
        current_mbps,
    });
}

pub(crate) fn build_result(
    benchmark: BenchmarkType,
    bytes_processed: u64,
    duration: Duration,
    samples: Vec<SamplePoint>,
    iops: Option<f64>,
    latencies_ms: Option<&[f64]>,
) -> BenchmarkResult {
    let duration_secs = duration.as_secs_f64().max(f64::EPSILON);
    let average_mbps = bytes_to_mbps(bytes_processed, duration_secs);
    let peak_mbps = samples
        .iter()
        .map(|sample| sample.throughput_mbps)
        .fold(0.0, f64::max);
    let minimum_mbps = if samples.is_empty() {
        average_mbps
    } else {
        samples
            .iter()
            .map(|sample| sample.throughput_mbps)
            .fold(f64::INFINITY, f64::min)
    };
    let latency_ms_p50 = latencies_ms.map(|values| percentile(values, 0.50));
    let latency_ms_p95 = latencies_ms.map(|values| percentile(values, 0.95));

    BenchmarkResult {
        benchmark,
        bytes_processed,
        duration_secs,
        average_mbps,
        peak_mbps,
        minimum_mbps,
        iops,
        latency_ms_p50,
        latency_ms_p95,
        samples,
    }
}

pub(crate) fn bytes_to_mbps(bytes: u64, duration_secs: f64) -> f64 {
    if duration_secs <= 0.0 {
        return 0.0;
    }

    bytes as f64 / MIB as f64 / duration_secs
}

pub(crate) fn percentile(values: &[f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index]
}

pub(crate) fn write_chunk(file: &mut File, buffer: &[u8], bytes: usize) -> Result<()> {
    file.write_all(&buffer[..bytes])
        .context("failed to write benchmark chunk")?;
    Ok(())
}

pub(crate) fn open_rw(path: &Path) -> Result<File> {
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .with_context(|| format!("failed to open {} for read/write", path.display()))
}

pub(crate) fn sample_tick(last_tick: &mut Instant, sample_interval: Duration) -> bool {
    if last_tick.elapsed() >= sample_interval {
        *last_tick = Instant::now();
        true
    } else {
        false
    }
}

pub(crate) fn reset_cursor(file: &mut File) -> Result<()> {
    file.seek(SeekFrom::Start(0))
        .context("failed to reset file cursor")?;
    Ok(())
}

pub(crate) fn read_exact_chunk(file: &mut File, buffer: &mut [u8], bytes: usize) -> Result<()> {
    file.read_exact(&mut buffer[..bytes])
        .context("failed to read benchmark chunk")?;
    Ok(())
}

pub(crate) fn seeded_rng() -> SmallRng {
    SmallRng::seed_from_u64(0xDEC0DED)
}

pub(crate) fn random_offset(rng: &mut SmallRng, file_bytes: u64, block_bytes: usize) -> u64 {
    let max_offset = file_bytes.saturating_sub(block_bytes as u64);
    if max_offset == 0 {
        return 0;
    }

    let block = rng.random_range(0..=(max_offset / block_bytes as u64));
    block * block_bytes as u64
}

#[cfg(test)]
mod tests {
    use super::{BenchmarkProfile, ProfilePreset};
    use crate::device::{DeviceKind, DeviceTarget};

    #[test]
    fn balanced_profile_is_default() {
        let configuration = super::RunConfiguration::default();
        assert_eq!(configuration.profile.preset, ProfilePreset::Balanced);
    }

    #[test]
    fn validation_rejects_tiny_drives() {
        let target = DeviceTarget {
            id: "tiny".to_string(),
            name: "tiny".to_string(),
            mount_point: "/tmp".into(),
            source: "/dev/disk1".to_string(),
            filesystem: "apfs".to_string(),
            kind: DeviceKind::BuiltIn,
            total_bytes: 64 * 1024 * 1024,
            available_bytes: 32 * 1024 * 1024,
            metadata: Default::default(),
        };

        let result = BenchmarkProfile::balanced().validate_for(&target);
        assert!(result.is_err());
    }
}

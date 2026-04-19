use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;

use crate::bench::BenchmarkRunRecord;

#[derive(Debug, Clone)]
pub struct HistoryStore {
    path: PathBuf,
}

impl HistoryStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn default_path() -> Result<PathBuf> {
        let project_dirs = ProjectDirs::from("com", "riedspied", "riedspied")
            .context("failed to resolve application data directory")?;
        Ok(project_dirs.data_local_dir().join("history.jsonl"))
    }

    pub fn default_store() -> Result<Self> {
        Ok(Self::new(Self::default_path()?))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn save(&self, record: &BenchmarkRunRecord) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).context("failed to create history directory")?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("failed to open history file at {}", self.path.display()))?;

        let encoded =
            serde_json::to_string(record).context("failed to serialize benchmark record")?;
        writeln!(file, "{encoded}").context("failed to append history record")?;
        Ok(())
    }

    pub fn load(&self) -> Result<Vec<BenchmarkRunRecord>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let file = OpenOptions::new()
            .read(true)
            .open(&self.path)
            .with_context(|| format!("failed to open history file at {}", self.path.display()))?;
        let reader = BufReader::new(file);
        let mut records = Vec::new();

        for line in reader.lines() {
            let line = line.context("failed to read history line")?;
            if line.trim().is_empty() {
                continue;
            }
            let record: BenchmarkRunRecord =
                serde_json::from_str(&line).context("failed to decode history entry")?;
            records.push(record);
        }

        records.sort_by(|left, right| right.started_at.cmp(&left.started_at));
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::HistoryStore;
    use crate::bench::{
        BenchmarkProfile, BenchmarkResult, BenchmarkRunRecord, BenchmarkType, SamplePoint,
    };
    use crate::device::{DeviceKind, DeviceTarget};

    #[test]
    fn saves_and_loads_history_records() {
        let temp_dir = tempdir().expect("tempdir");
        let store = HistoryStore::new(temp_dir.path().join("history.jsonl"));
        let record = BenchmarkRunRecord {
            started_at: chrono::Utc::now(),
            finished_at: chrono::Utc::now(),
            target: DeviceTarget {
                id: "/tmp".to_string(),
                name: "tmp".to_string(),
                mount_point: "/tmp".into(),
                source: "/dev/disk1s1".to_string(),
                filesystem: "apfs".to_string(),
                kind: DeviceKind::BuiltIn,
                total_bytes: 1024,
                available_bytes: 512,
            },
            profile: BenchmarkProfile::balanced(),
            results: vec![BenchmarkResult {
                benchmark: BenchmarkType::SequentialWrite,
                bytes_processed: 1024,
                duration_secs: 1.0,
                average_mbps: 8.0,
                peak_mbps: 8.0,
                minimum_mbps: 8.0,
                iops: None,
                latency_ms_p50: None,
                latency_ms_p95: None,
                samples: vec![SamplePoint {
                    seconds: 1.0,
                    throughput_mbps: 8.0,
                }],
            }],
        };

        store.save(&record).expect("save history");
        let records = store.load().expect("load history");

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].target.name, "tmp");
        assert_eq!(
            records[0].results[0].benchmark,
            BenchmarkType::SequentialWrite
        );
    }
}

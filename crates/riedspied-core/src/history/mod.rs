use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
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

        let mut record = record.clone();
        record.ensure_defaults();

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("failed to open history file at {}", self.path.display()))?;

        let encoded =
            serde_json::to_string(&record).context("failed to serialize benchmark record")?;
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
            let mut record: BenchmarkRunRecord =
                serde_json::from_str(&line).context("failed to decode history entry")?;
            record.ensure_defaults();
            records.push(record);
        }

        records.sort_by(|left, right| right.started_at.cmp(&left.started_at));
        Ok(records)
    }

    pub fn load_selected(&self, run_ids: &[String]) -> Result<Vec<BenchmarkRunRecord>> {
        if run_ids.is_empty() {
            bail!("at least one run id is required");
        }

        let records = self.load()?;
        Ok(records
            .into_iter()
            .filter(|record| run_ids.iter().any(|run_id| run_id == &record.run_id))
            .collect())
    }

    pub fn update_annotations(
        &self,
        run_id: &str,
        tags: Vec<String>,
        notes: Option<String>,
    ) -> Result<Option<BenchmarkRunRecord>> {
        let mut records = self.load()?;
        let mut updated = None;

        for record in &mut records {
            if record.run_id == run_id {
                record.tags = tags.clone();
                record.notes = notes.clone().filter(|value| !value.trim().is_empty());
                updated = Some(record.clone());
                break;
            }
        }

        if updated.is_some() {
            self.write_all(&records)?;
        }

        Ok(updated)
    }

    fn write_all(&self, records: &[BenchmarkRunRecord]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).context("failed to create history directory")?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)
            .with_context(|| {
                format!("failed to rewrite history file at {}", self.path.display())
            })?;

        for record in records {
            let encoded = serde_json::to_string(record)
                .context("failed to serialize benchmark record during rewrite")?;
            writeln!(file, "{encoded}").context("failed to rewrite history record")?;
        }

        Ok(())
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
            run_id: "run-1".to_string(),
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
                metadata: Default::default(),
            },
            profile: BenchmarkProfile::balanced(),
            tags: vec!["baseline".to_string()],
            notes: Some("initial run".to_string()),
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
        assert_eq!(records[0].run_id, "run-1");
        assert_eq!(
            records[0].results[0].benchmark,
            BenchmarkType::SequentialWrite
        );
    }

    #[test]
    fn updates_annotations() {
        let temp_dir = tempdir().expect("tempdir");
        let store = HistoryStore::new(temp_dir.path().join("history.jsonl"));
        let mut record = BenchmarkRunRecord {
            run_id: "run-2".to_string(),
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
                metadata: Default::default(),
            },
            profile: BenchmarkProfile::balanced(),
            tags: Vec::new(),
            notes: None,
            results: Vec::new(),
        };
        record.ensure_defaults();

        store.save(&record).expect("save history");
        let updated = store
            .update_annotations(
                &record.run_id,
                vec!["after-update".to_string()],
                Some("note".to_string()),
            )
            .expect("update")
            .expect("record updated");

        assert_eq!(updated.tags, vec!["after-update".to_string()]);
        assert_eq!(updated.notes.as_deref(), Some("note"));
    }

    #[test]
    fn loads_legacy_history_without_new_fields() {
        let temp_dir = tempdir().expect("tempdir");
        let path = temp_dir.path().join("history.jsonl");
        std::fs::write(
            &path,
            "{\"started_at\":\"2026-04-20T00:00:00Z\",\"finished_at\":\"2026-04-20T00:00:01Z\",\"target\":{\"id\":\"/tmp\",\"name\":\"tmp\",\"mount_point\":\"/tmp\",\"source\":\"/dev/disk1s1\",\"filesystem\":\"apfs\",\"kind\":\"BuiltIn\",\"total_bytes\":1024,\"available_bytes\":512},\"profile\":{\"preset\":\"Balanced\",\"sequential_bytes\":1,\"sustained_seconds\":1,\"random_file_bytes\":1,\"random_operations\":1,\"chunk_bytes\":1,\"block_bytes\":1,\"minimum_free_ratio\":0.1},\"results\":[]}\n",
        )
        .expect("write legacy history");

        let store = HistoryStore::new(path);
        let records = store.load().expect("load history");

        assert_eq!(records.len(), 1);
        assert!(!records[0].run_id.is_empty());
        assert!(records[0].tags.is_empty());
        assert!(records[0].target.metadata.mount_options.is_empty());
    }
}

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeviceKind {
    BuiltIn,
    External,
    Network,
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceMetadata {
    #[serde(default)]
    pub mount_options: Vec<String>,
    #[serde(default)]
    pub is_read_only: bool,
    #[serde(default)]
    pub is_removable: Option<bool>,
    #[serde(default)]
    pub is_rotational: Option<bool>,
    #[serde(default)]
    pub vendor: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub bus: Option<String>,
    #[serde(default)]
    pub network_protocol: Option<String>,
    #[serde(default)]
    pub usb_generation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTarget {
    pub id: String,
    pub name: String,
    pub mount_point: PathBuf,
    pub source: String,
    pub filesystem: String,
    pub kind: DeviceKind,
    pub total_bytes: u64,
    pub available_bytes: u64,
    #[serde(default)]
    pub metadata: DeviceMetadata,
}

impl DeviceTarget {
    pub fn free_ratio(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }

        self.available_bytes as f64 / self.total_bytes as f64
    }

    pub fn storage_hint(&self) -> Option<&'static str> {
        match self.metadata.is_rotational {
            Some(true) => Some("HDD"),
            Some(false) => Some("SSD/flash"),
            None => None,
        }
    }

    pub fn transport_hint(&self) -> Option<&str> {
        self.metadata
            .bus
            .as_deref()
            .or(self.metadata.network_protocol.as_deref())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MountEntry {
    source: String,
    mount_point: PathBuf,
    filesystem: String,
    mount_options: Vec<String>,
}

pub fn discover_devices() -> Result<Vec<DeviceTarget>> {
    #[cfg(target_os = "macos")]
    {
        return macos::discover_devices();
    }

    #[cfg(target_os = "linux")]
    {
        return linux::discover_devices();
    }

    #[allow(unreachable_code)]
    Ok(Vec::new())
}

fn resolve_space_map() -> Result<HashMap<PathBuf, (u64, u64)>> {
    let output = Command::new("df")
        .args(["-kP"])
        .output()
        .context("failed to run df -kP")?;

    let stdout = String::from_utf8(output.stdout).context("df output was not valid UTF-8")?;
    let mut space_map = HashMap::new();

    for line in stdout.lines().skip(1) {
        let mut columns = line.split_whitespace();
        let _filesystem = columns.next();
        let total_kib = columns.next().and_then(|value| value.parse::<u64>().ok());
        let _used = columns.next();
        let available_kib = columns.next().and_then(|value| value.parse::<u64>().ok());
        let _capacity = columns.next();
        let mount_point = columns.next();

        if let (Some(total_kib), Some(available_kib), Some(mount_point)) =
            (total_kib, available_kib, mount_point)
        {
            space_map.insert(
                PathBuf::from(mount_point),
                (
                    total_kib.saturating_mul(1024),
                    available_kib.saturating_mul(1024),
                ),
            );
        }
    }

    Ok(space_map)
}

fn device_name(mount_point: &Path, source: &str) -> String {
    if mount_point == Path::new("/") {
        return "System Volume".to_string();
    }

    mount_point
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| source.to_string())
}

fn classify_kind(mount_point: &Path, source: &str, filesystem: &str) -> DeviceKind {
    if source.starts_with("//")
        || source.contains(":/")
        || matches!(filesystem, "nfs" | "smbfs" | "cifs" | "afpfs" | "webdav")
    {
        return DeviceKind::Network;
    }

    if mount_point == Path::new("/") {
        return DeviceKind::BuiltIn;
    }

    if mount_point.starts_with("/Volumes")
        || mount_point.starts_with("/media")
        || mount_point.starts_with("/run/media")
        || mount_point.starts_with("/mnt")
    {
        return DeviceKind::External;
    }

    DeviceKind::Unknown
}

pub(crate) fn hydrate_targets(entries: Vec<MountEntry>) -> Result<Vec<DeviceTarget>> {
    let space_map = resolve_space_map()?;
    let mut devices = Vec::new();

    for entry in entries {
        if let Some((total_bytes, available_bytes)) = space_map.get(&entry.mount_point) {
            let mount_point = entry.mount_point.clone();
            devices.push(DeviceTarget {
                id: mount_point.display().to_string(),
                name: device_name(&mount_point, &entry.source),
                mount_point,
                source: entry.source.clone(),
                filesystem: entry.filesystem.clone(),
                kind: classify_kind(&entry.mount_point, &entry.source, &entry.filesystem),
                total_bytes: *total_bytes,
                available_bytes: *available_bytes,
                metadata: DeviceMetadata {
                    is_read_only: is_read_only_mount(&entry.mount_options),
                    mount_options: entry.mount_options.clone(),
                    network_protocol: network_protocol(&entry.source, &entry.filesystem),
                    ..Default::default()
                },
            });
        }
    }

    devices.sort_by(|left, right| left.mount_point.cmp(&right.mount_point));
    devices.dedup_by(|left, right| left.mount_point == right.mount_point);
    Ok(devices)
}

pub(crate) fn is_read_only_mount(options: &[String]) -> bool {
    options.iter().any(|option| {
        matches!(
            option.as_str(),
            "ro" | "read-only" | "read-only volume" | "read-only media"
        )
    })
}

pub(crate) fn network_protocol(source: &str, filesystem: &str) -> Option<String> {
    if filesystem.eq_ignore_ascii_case("nfs") || source.contains(":/") {
        return Some("NFS".to_string());
    }
    if matches!(filesystem, "smbfs" | "cifs") || source.starts_with("//") {
        return Some("SMB".to_string());
    }
    if filesystem.eq_ignore_ascii_case("afpfs") {
        return Some("AFP".to_string());
    }
    if filesystem.eq_ignore_ascii_case("webdav") {
        return Some("WebDAV".to_string());
    }

    None
}

pub(crate) fn capture_command(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

pub(crate) fn parse_key_value_lines(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            let value = value.trim();
            if value.is_empty() {
                None
            } else {
                Some((key.trim().to_string(), value.to_string()))
            }
        })
        .collect()
}

pub(crate) fn normalize_bool(value: Option<&str>) -> Option<bool> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "yes" | "true" | "1" => Some(true),
        "no" | "false" | "0" => Some(false),
        _ => None,
    }
}

pub(crate) fn clean_value(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() || value == "-" || value == "(null)" {
        None
    } else {
        Some(value.to_string())
    }
}

pub(crate) fn base_device_name(source: &str) -> Option<String> {
    let name = Path::new(source).file_name()?.to_str()?.to_string();

    if let Some((prefix, suffix)) = name.rsplit_once('s') {
        if prefix.starts_with("disk") && suffix.chars().all(|character| character.is_ascii_digit())
        {
            return Some(prefix.to_string());
        }
    }

    if let Some((prefix, suffix)) = name.rsplit_once('p') {
        if prefix
            .chars()
            .last()
            .is_some_and(|character| character.is_ascii_digit())
            && suffix.chars().all(|character| character.is_ascii_digit())
        {
            return Some(prefix.to_string());
        }
    }

    let trimmed = name.trim_end_matches(|character: char| character.is_ascii_digit());
    if trimmed.is_empty() {
        Some(name)
    } else {
        Some(trimmed.to_string())
    }
}

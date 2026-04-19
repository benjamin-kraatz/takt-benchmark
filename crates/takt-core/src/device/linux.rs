use std::path::PathBuf;
use std::process::Command;
use std::{fs, path::Path};

use anyhow::{Context, Result};

use super::{
    MountEntry, base_device_name, build_device_id, capture_command, clean_value, hydrate_targets,
    normalize_bool,
};

pub fn discover_devices() -> Result<Vec<super::DeviceTarget>> {
    let output = Command::new("mount")
        .output()
        .context("failed to run mount on Linux")?;
    let stdout = String::from_utf8(output.stdout).context("mount output was not valid UTF-8")?;

    let entries: Vec<_> = stdout
        .lines()
        .filter_map(parse_mount_line)
        .filter(is_supported_mount)
        .collect();
    let mut devices = hydrate_targets(entries)?;
    for device in &mut devices {
        enrich_device(device);
    }
    Ok(devices)
}

fn parse_mount_line(line: &str) -> Option<MountEntry> {
    let (source, remainder) = line.split_once(" on ")?;
    let (mount_point, details) = remainder.split_once(" type ")?;
    let (filesystem, options) = details.split_once(" (")?;

    Some(MountEntry {
        source: source.to_string(),
        mount_point: PathBuf::from(mount_point),
        filesystem: filesystem.to_string(),
        mount_options: options
            .trim_end_matches(')')
            .split(',')
            .map(|value| value.trim().to_string())
            .collect(),
    })
}

fn is_supported_mount(entry: &MountEntry) -> bool {
    let ignored_filesystems = [
        "proc",
        "sysfs",
        "tmpfs",
        "devtmpfs",
        "cgroup",
        "cgroup2",
        "overlay",
        "squashfs",
        "nsfs",
        "tracefs",
        "fusectl",
        "configfs",
        "ramfs",
        "debugfs",
        "securityfs",
        "mqueue",
        "hugetlbfs",
        "pstore",
        "efivarfs",
    ];

    if ignored_filesystems.contains(&entry.filesystem.as_str()) {
        return false;
    }

    if entry.mount_point == std::path::Path::new("/") {
        return true;
    }

    entry.mount_point.starts_with("/media")
        || entry.mount_point.starts_with("/run/media")
        || entry.mount_point.starts_with("/mnt")
        || entry.filesystem == "nfs"
        || entry.filesystem == "cifs"
        || entry.source.starts_with("//")
        || entry.source.contains(":/")
}

fn enrich_device(device: &mut super::DeviceTarget) {
    let mut unique_hint = None;

    if !device.source.starts_with("/dev/") {
        device.id = build_device_id(&device.source, &device.mount_point, None, None, None);
        return;
    }

    let primary = lsblk_properties(&device.source);
    let parent = primary
        .get("PKNAME")
        .map(|value| format!("/dev/{value}"))
        .and_then(|path| lsblk_properties(&path).into());

    let merged = merged_property(primary.as_ref(), parent.as_ref(), "RM");
    if let Some(removable) = merged.and_then(|value| normalize_bool(Some(value))) {
        device.metadata.is_removable = Some(removable);
    }
    let merged = merged_property(primary.as_ref(), parent.as_ref(), "ROTA");
    if let Some(rotational) = merged.and_then(|value| normalize_bool(Some(value))) {
        device.metadata.is_rotational = Some(rotational);
    }
    let merged = merged_property(primary.as_ref(), parent.as_ref(), "RO");
    if let Some(read_only) = merged.and_then(|value| normalize_bool(Some(value))) {
        device.metadata.is_read_only |= read_only;
    }
    device.metadata.vendor =
        clean_value(merged_property(primary.as_ref(), parent.as_ref(), "VENDOR"));
    device.metadata.model =
        clean_value(merged_property(primary.as_ref(), parent.as_ref(), "MODEL"));
    device.metadata.bus = clean_value(merged_property(primary.as_ref(), parent.as_ref(), "TRAN"));
    device.metadata.volume_uuid =
        clean_value(merged_property(primary.as_ref(), parent.as_ref(), "UUID"));
    device.metadata.partition_uuid = clean_value(merged_property(
        primary.as_ref(),
        parent.as_ref(),
        "PARTUUID",
    ));
    unique_hint = clean_value(merged_property(primary.as_ref(), parent.as_ref(), "WWN"))
        .or_else(|| clean_value(merged_property(primary.as_ref(), parent.as_ref(), "SERIAL")));

    if device.metadata.bus.as_deref() == Some("usb") {
        if let Some(base_name) = base_device_name(&device.source) {
            device.metadata.usb_generation = usb_generation_hint(&base_name);
        }
    }

    device.id = build_device_id(
        &device.source,
        &device.mount_point,
        device.metadata.volume_uuid.as_deref(),
        device.metadata.partition_uuid.as_deref(),
        unique_hint.as_deref(),
    );
}

fn lsblk_properties(device: &str) -> std::collections::HashMap<String, String> {
    let Some(output) = capture_command(
        "lsblk",
        &[
            "-P",
            "-o",
            "PATH,PKNAME,RM,RO,ROTA,MODEL,VENDOR,TRAN,UUID,PARTUUID,WWN,SERIAL",
            device,
        ],
    ) else {
        return std::collections::HashMap::new();
    };

    let line = output.lines().next().unwrap_or_default();
    line.split_whitespace()
        .filter_map(|segment| {
            let (key, value) = segment.split_once('=')?;
            Some((key.to_string(), value.trim_matches('"').to_string()))
        })
        .collect()
}

fn merged_property<'a>(
    primary: Option<&'a std::collections::HashMap<String, String>>,
    parent: Option<&'a std::collections::HashMap<String, String>>,
    key: &str,
) -> Option<&'a str> {
    primary
        .and_then(|map| map.get(key))
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            parent
                .and_then(|map| map.get(key))
                .map(String::as_str)
                .filter(|value| !value.is_empty())
        })
}

fn usb_generation_hint(base_name: &str) -> Option<String> {
    let device_path = Path::new("/sys/class/block").join(base_name).join("device");
    let canonical = fs::canonicalize(device_path).ok()?;

    for ancestor in canonical.ancestors() {
        let speed_path = ancestor.join("speed");
        let Ok(speed) = fs::read_to_string(speed_path) else {
            continue;
        };
        let value = speed.trim().parse::<f64>().ok()?;
        return Some(if value <= 480.0 {
            "USB 2.0".to_string()
        } else if value <= 5_000.0 {
            "USB 3.2 Gen 1".to_string()
        } else if value <= 10_000.0 {
            "USB 3.2 Gen 2".to_string()
        } else if value <= 20_000.0 {
            "USB 3.2 Gen 2x2".to_string()
        } else {
            "USB4 or higher".to_string()
        });
    }

    None
}

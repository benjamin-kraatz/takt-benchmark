use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

use super::{
    MountEntry, base_device_name, capture_command, clean_value, hydrate_targets, normalize_bool,
    parse_key_value_lines,
};

pub fn discover_devices() -> Result<Vec<super::DeviceTarget>> {
    let output = Command::new("mount")
        .output()
        .context("failed to run mount on macOS")?;
    let stdout = String::from_utf8(output.stdout).context("mount output was not valid UTF-8")?;

    let entries: Vec<_> = stdout
        .lines()
        .filter_map(parse_mount_line)
        .filter(is_supported_mount)
        .collect();
    let mut devices = hydrate_targets(entries)?;
    let usb_speed_map = usb_speed_map();
    for device in &mut devices {
        enrich_device(device, &usb_speed_map);
    }
    Ok(devices)
}

fn parse_mount_line(line: &str) -> Option<MountEntry> {
    let (source, remainder) = line.split_once(" on ")?;
    let (mount_point, details) = remainder.split_once(" (")?;
    let details = details.trim_end_matches(')');
    let mut segments = details.split(',').map(|value| value.trim().to_string());
    let filesystem = segments.next()?;

    Some(MountEntry {
        source: source.to_string(),
        mount_point: PathBuf::from(mount_point),
        filesystem,
        mount_options: segments.collect(),
    })
}

fn is_supported_mount(entry: &MountEntry) -> bool {
    let ignored_filesystems = ["autofs", "devfs", "map", "procfs"];
    let ignored_prefixes = ["/System", "/private/var", "/dev"];

    if ignored_filesystems.contains(&entry.filesystem.as_str()) {
        return false;
    }

    if entry.mount_point == std::path::Path::new("/") {
        return true;
    }

    if ignored_prefixes
        .iter()
        .any(|prefix| entry.mount_point.starts_with(prefix))
    {
        return false;
    }

    entry.mount_point.starts_with("/Volumes")
        || entry.source.starts_with("//")
        || entry.source.contains(":/")
}

fn enrich_device(device: &mut super::DeviceTarget, usb_speed_map: &HashMap<String, String>) {
    let mount_path = device.mount_point.to_string_lossy().to_string();
    if let Some(output) = capture_command("diskutil", &["info", &mount_path]) {
        let values = parse_key_value_lines(&output);
        if let Some(removable) = normalize_bool(
            values
                .get("Removable Media")
                .or_else(|| values.get("Removable"))
                .map(String::as_str),
        ) {
            device.metadata.is_removable = Some(removable);
        }
        if let Some(read_only) = normalize_bool(
            values
                .get("Read-Only Volume")
                .or_else(|| values.get("Read-Only Media"))
                .map(String::as_str),
        ) {
            device.metadata.is_read_only |= read_only;
        }
        if let Some(solid_state) = normalize_bool(
            values
                .get("Solid State")
                .or_else(|| values.get("Solid State Media"))
                .map(String::as_str),
        ) {
            device.metadata.is_rotational = Some(!solid_state);
        }
        device.metadata.bus = clean_value(values.get("Protocol").map(String::as_str));
        device.metadata.vendor = clean_value(
            values
                .get("Device Manufacturer")
                .or_else(|| values.get("Manufacturer"))
                .map(String::as_str),
        );
        device.metadata.model = clean_value(
            values
                .get("Device / Media Name")
                .or_else(|| values.get("Media Name"))
                .map(String::as_str),
        );
    }

    if let Some(base_device) = base_device_name(&device.source) {
        if let Some(speed) = usb_speed_map.get(&base_device) {
            device.metadata.usb_generation = Some(speed.clone());
            if device.metadata.bus.is_none() {
                device.metadata.bus = Some("USB".to_string());
            }
        }
    }
}

fn usb_speed_map() -> HashMap<String, String> {
    let Some(output) = capture_command(
        "system_profiler",
        &["SPUSBDataType", "-detailLevel", "mini"],
    ) else {
        return HashMap::new();
    };

    let mut current_bsd = None;
    let mut current_speed = None;
    let mut speeds = HashMap::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "BSD Name" => current_bsd = Some(value.to_string()),
                "Speed" => current_speed = Some(value.to_string()),
                _ => {}
            }

            if let (Some(bsd), Some(speed)) = (&current_bsd, &current_speed) {
                speeds.insert(bsd.clone(), speed.clone());
            }
        }
    }

    speeds
}

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

use super::{MountEntry, hydrate_targets};

pub fn discover_devices() -> Result<Vec<super::DeviceTarget>> {
    let output = Command::new("mount")
        .output()
        .context("failed to run mount on macOS")?;
    let stdout = String::from_utf8(output.stdout).context("mount output was not valid UTF-8")?;

    let entries = stdout
        .lines()
        .filter_map(parse_mount_line)
        .filter(is_supported_mount)
        .collect();

    hydrate_targets(entries)
}

fn parse_mount_line(line: &str) -> Option<MountEntry> {
    let (source, remainder) = line.split_once(" on ")?;
    let (mount_point, details) = remainder.split_once(" (")?;
    let filesystem = details.split(',').next()?.trim_end_matches(')');

    Some(MountEntry {
        source: source.to_string(),
        mount_point: PathBuf::from(mount_point),
        filesystem: filesystem.to_string(),
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

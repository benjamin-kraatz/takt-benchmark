# Platform Notes

## macOS

- Device discovery is based on mounted filesystems reported by `mount` and capacity data from `df -kP`.
- The root system volume is included so the built-in storage medium can be benchmarked.
- External removable volumes typically appear under `/Volumes`.
- Results can be influenced by APFS behavior, cache warmup, and USB power management.

## Linux

- Device discovery is based on `mount` plus `df -kP`.
- The root filesystem is included for benchmarking built-in storage.
- Removable drives and SD cards usually appear under `/media`, `/run/media`, or `/mnt`.
- Mounted NFS and SMB shares are treated as filesystem targets, not protocol-level throughput tests.

## Current limitations

- Cache flushing is not forced. Reported read throughput can therefore benefit from page cache after earlier passes.
- The benchmark engine does not yet attempt privileged raw-device access.
- MTP/PTP devices are intentionally excluded from v1 because they do not behave like normal mounted filesystems.

## Recommended validation targets

- One built-in system volume.
- One removable USB or SD device.
- One mounted network share.

This combination is enough to validate discovery, temp-file lifecycle, history persistence, and the main accuracy caveats in the current design.

# Benchmark Methodology

## Profiles

riedspied currently ships with three benchmark presets:

- `quick`: 128 MiB sequential pass, 10 second sustained write, 64 MiB random file, 2,000 random operations
- `balanced`: 512 MiB sequential pass, 20 second sustained write, 128 MiB random file, 5,000 random operations
- `thorough`: 1024 MiB sequential pass, 45 second sustained write, 256 MiB random file, 12,000 random operations

Balanced is the default because it gives useful sustained-write behavior without turning every run into a long soak test.

## Sequential throughput

Sequential write creates a fresh file and writes fixed-size chunks until the selected profile size is reached. Sequential read reads a dedicated fixture file in fixed-size chunks until the requested size is consumed.

## Sustained throughput

Sustained throughput writes continuous 1 MiB chunks for the selected duration and samples throughput once per second. This is intended to expose thermal throttling, SLC cache exhaustion, and controller slowdown patterns that short burst tests hide.

## Random IOPS and latency

Random IOPS uses a fixed-size scratch file, seeks to random block offsets, and performs mixed 4 KiB reads and writes. It records per-operation latency and reports IOPS plus p50 and p95 latency.

## Cleanup

Each benchmark run uses a hidden `.riedspied-*` directory on the target. The directory is removed after completion unless the caller explicitly requests retention.

## Interpretation caveats

The current implementation benchmarks the mounted filesystem path. Results therefore include:

- filesystem cache effects
- OS scheduling noise
- NAS client and mount-option behavior for network shares
- formatting choices such as APFS, exFAT, ext4, or NFS

That is correct for a mounted-filesystem benchmark, but it is not the same as raw block-device measurement.

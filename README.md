# riedspied

riedspied is a Rust workspace for benchmarking mounted storage targets on macOS and Linux. It currently supports built-in system volumes, removable drives, SD cards, and mounted network shares that appear as normal filesystems.

## Workspace

- `crates/riedspied-core`: shared benchmark engine, device discovery, and local history persistence.
- `crates/riedspied-cli`: command-line interface for listing targets, running benchmarks, and inspecting local history.
- `crates/riedspied-gui`: native desktop application built with `eframe` and `egui`.

## Current v1 capabilities

- Sequential write throughput.
- Sequential read throughput.
- Sustained write throughput.
- Random small-block latency and IOPS.
- Local JSONL history store.
- macOS and Linux mounted-filesystem discovery.
- Benchmark subset selection in the CLI and GUI.
- JSON, Markdown, HTML, and PNG export.
- GUI history filtering, per-run detail views, direct comparison, and same-device trend charts.
- Richer device metadata such as read-only hints, removable hints, transport hints, vendor or model when available, and network protocol hints.

## Deferred from v1

- MTP/PTP phone and camera transport.
- Raw block-device benchmarking.
- Windows support.
- Cross-machine result synchronization.

## Running

```bash
cargo run -p riedspied-cli -- list --verbose
cargo run -p riedspied-cli -- bench --target /Volumes/MyDrive --profile balanced --bench sequential-write --bench sustained-write --tag baseline
cargo run -p riedspied-cli -- bench --target /Volumes/MyDrive --profile balanced --export-format html --export-path ./latest-report.html
cargo run -p riedspied-cli -- history --limit 5 --profile balanced --verbose
cargo run -p riedspied-cli -- export --latest --format png --output ./latest-chart.png
cargo run -p riedspied-gui
```

## Safety model

The benchmark engine writes temporary files into a hidden `.riedspied-*` directory inside the selected mount point. Files are deleted after a run unless `--keep-temp-files` is used in the CLI. The engine refuses to start when the target does not meet the configured free-space requirement for the selected profile.

## Device metadata

Mounted devices now include optional metadata gathered from platform tools when available:

- read-only mount status and mount options
- removable vs fixed hints
- SSD or HDD rotational hints
- vendor and model names where discoverable
- transport or bus hints such as USB or NVMe
- network protocol hints such as SMB or NFS
- USB generation hints where the platform exposes them

Missing metadata is treated as optional context, not a fatal error.

## GUI analysis workflow

The GUI now supports:

- selecting benchmark subsets before a run
- exporting the latest run or selected history runs
- filtering history by device and profile
- opening a detailed run view with overview and drill-down charts
- comparing two runs directly
- viewing same-device trends over time
- tagging and annotating saved runs

## Verification

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
cargo run -p riedspied-cli -- list
```

See `docs/architecture.md`, `docs/benchmark-methodology.md`, and `docs/platform-notes.md` for implementation details and caveats.

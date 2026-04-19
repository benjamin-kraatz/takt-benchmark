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
- Native GUI save-file picker with remembered export directory.
- GUI history filtering, per-run detail views, direct comparison, and same-device trend charts.
- Richer device metadata such as read-only hints, removable hints, transport hints, vendor or model when available, and network protocol hints.

## Deferred from v1

- MTP/PTP phone and camera transport.
- Raw block-device benchmarking.
- Windows support.
- Cross-machine result synchronization.

## Running the Apps

Build and run commands are executed from the workspace root.

### Run the CLI

Use the CLI when you want scripted runs, terminal output, or direct export from the command line.

List discovered benchmark targets:

```bash
cargo run -p riedspied-cli -- list --verbose
```

The verbose listing now includes an explicit device ID for each detected target. That ID is intended to be stable across mount-point changes when the platform exposes a volume or partition UUID.

Run a benchmark against a mounted target:

```bash
cargo run -p riedspied-cli -- bench \
  --target /Volumes/MyDrive \
  --profile balanced \
  --bench sequential-write \
  --bench sustained-write \
  --tag baseline
```

You can also target a device explicitly by ID instead of by mount path or display name:

```bash
cargo run -p riedspied-cli -- bench \
  --target volume-uuid:c4dd4c01-f913-301b-8a1c-701332af5b53 \
  --profile balanced
```

Run a benchmark and export the result immediately:

```bash
cargo run -p riedspied-cli -- bench \
  --target /Volumes/MyDrive \
  --profile balanced \
  --export-format html \
  --export-path ./latest-report.html
```

Inspect saved history and export previous runs:

```bash
cargo run -p riedspied-cli -- history --limit 5 --profile balanced --verbose
cargo run -p riedspied-cli -- history --target volume-uuid:c4dd4c01-f913-301b-8a1c-701332af5b53 --verbose
cargo run -p riedspied-cli -- export --latest --format png --output ./latest-chart.png
```

For CLI target resolution, `--target` accepts any of the following:

- display name such as `RetroPie`
- mount path such as `/Volumes/RetroPie`
- source path such as `/dev/disk19s1`
- explicit device ID such as `volume-uuid:...`

### Run the GUI

Use the GUI when you want interactive target selection, live progress, history filtering, comparison views, annotations, and export previews.

Start the desktop app:

```bash
cargo run -p riedspied-gui
```

Inside the GUI you can:

- select a mounted target and benchmark profile
- choose which benchmarks to run
- inspect the latest run and saved history
- compare two runs or review same-device trends
- export runs with the built-in preview panel and native save dialog

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
- volume UUID and partition UUID hints where the platform exposes them

Missing metadata is treated as optional context, not a fatal error.

Each detected target also has an explicit device ID used by the CLI and GUI selection state. The ID prefers a volume UUID, then a partition UUID, then a device-specific fallback such as the source path.

## GUI analysis workflow

The GUI now supports:

- selecting benchmark subsets before a run
- exporting the latest run or selected history runs
- browsing for export destinations with a native save dialog and remembered folder
- filtering history by device and profile
- opening a detailed run view with overview and drill-down charts
- comparing two runs directly
- viewing same-device trends over time
- tagging and annotating saved runs

PNG exports now render as richer report images instead of a single benchmark summary line:

- single-run exports use a 2x2 benchmark panel layout
- two-run exports use per-benchmark overlay panels
- same-device multi-run exports automatically switch to trend-style panels

## Verification

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
cargo run -p riedspied-cli -- list --verbose
```

See `docs/architecture.md`, `docs/benchmark-methodology.md`, and `docs/platform-notes.md` for implementation details and caveats.

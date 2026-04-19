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

## Deferred from v1

- MTP/PTP phone and camera transport.
- Raw block-device benchmarking.
- Windows support.
- Cross-machine result synchronization.

## Running

```bash
cargo run -p riedspied-cli -- list
cargo run -p riedspied-cli -- bench --target /Volumes/MyDrive --profile balanced
cargo run -p riedspied-cli -- history --limit 5
cargo run -p riedspied-gui
```

## Safety model

The benchmark engine writes temporary files into a hidden `.riedspied-*` directory inside the selected mount point. Files are deleted after a run unless `--keep-temp-files` is used in the CLI. The engine refuses to start when the target does not meet the configured free-space requirement for the selected profile.

## Verification

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
cargo run -p riedspied-cli -- list
```

See `docs/architecture.md`, `docs/benchmark-methodology.md`, and `docs/platform-notes.md` for implementation details and caveats.

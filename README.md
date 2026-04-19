# Takt

**Takt** is a Rust workspace for benchmarking **mounted filesystems** on **macOS** and **Linux**: internal volumes, removable drives, SD cards, and network shares that appear as normal paths.

The name *Takt* (German for beat or clock cycle) nods to processor **clocking**—here applied to **storage throughput and latency** on a chosen mount.

## Crates

| Crate | Role |
|--------|------|
| [`crates/takt-core`](crates/takt-core) | Benchmark engine, device discovery, history store, shared export (JSON, Markdown, HTML, PNG) |
| [`crates/takt-cli`](crates/takt-cli) | Command-line interface |
| [`crates/takt-gui`](crates/takt-gui) | Native desktop app (`eframe` / `egui`) |

## Features

- Sequential read/write and sustained write throughput
- Random small-block IOPS and latency (p50 / p95)
- Profiles: `quick`, `balanced`, `thorough`
- Per-run tagging and local JSONL history
- CLI and GUI benchmark subset selection
- Exports: JSON, Markdown, HTML, PNG (layout-aware reports)
- GUI: history filters, per-run detail, two-run comparison, same-device trends, annotations, native save dialog with remembered export folder
- Device discovery with stable **device IDs** (volume UUID when available), verbose metadata (read-only, removable, transport, vendor/model, network protocol hints, etc.)

See [`docs/architecture.md`](docs/architecture.md), [`docs/benchmark-methodology.md`](docs/benchmark-methodology.md), and [`docs/platform-notes.md`](docs/platform-notes.md) for design and caveats.

## Requirements

- **Rust** toolchain matching `rust-version` in the root [`Cargo.toml`](Cargo.toml) (currently **1.85+**)
- **macOS** or **Linux** (Windows is not supported yet)

## Build and run

From the repository root:

### CLI (`takt`)

```bash
cargo run -p takt-cli -- list --verbose
```

```bash
cargo run -p takt-cli -- bench \
  --target /Volumes/MyDrive \
  --profile balanced \
  --bench sequential-write \
  --bench sustained-write \
  --tag baseline
```

Target by stable device ID:

```bash
cargo run -p takt-cli -- bench \
  --target volume-uuid:c4dd4c01-f913-301b-8a1c-701332af5b53 \
  --profile balanced
```

Bench with immediate HTML export:

```bash
cargo run -p takt-cli -- bench \
  --target /Volumes/MyDrive \
  --profile balanced \
  --export-format html \
  --export-path ./latest-report.html
```

History and export:

```bash
cargo run -p takt-cli -- history --limit 5 --profile balanced --verbose
cargo run -p takt-cli -- history --target volume-uuid:c4dd4c01-f913-301b-8a1c-701332af5b53 --verbose
cargo run -p takt-cli -- export --latest --format png --output ./latest-chart.png
```

`--target` accepts a display name, mount path, source device path, or `volume-uuid:…` / similar **device ID** strings printed by `list --verbose`.

### GUI

```bash
cargo run -p takt-gui
```

## Safety model

Benchmarks write under a hidden **`.takt-<timestamp>`** directory on the **selected mount**. That directory is removed after a successful run unless **`--keep-temp-files`** is set in the CLI. Runs are blocked if free space is below the requirement for the chosen profile.

## Migrating local history

If you previously ran an older build that stored history under another application ID, history lives under a different user data path. Renaming or copying JSONL data into Takt’s new directory is possible but manual; most users can start fresh.

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
cargo run -p takt-cli -- list --verbose
```

## Contributing

Issues and pull requests are welcome. Please run `fmt`, `clippy`, and `test` before submitting.

## License

Licensed under the **Apache License, Version 2.0**. See [`LICENSE`](LICENSE). Attribution and dependency notes are in [`NOTICE`](NOTICE).

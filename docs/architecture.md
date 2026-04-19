# Architecture

riedspied is structured as a Cargo workspace so both front ends consume the same core benchmark engine and result model.

## Crates

### `riedspied-core`

The core crate owns:

- mounted-device discovery for macOS and Linux
- benchmark profile presets
- benchmark execution and progress events
- local history persistence as JSONL

Discovery also assigns each `DeviceTarget` an explicit machine-friendly ID. When the platform exposes a stable volume UUID or partition UUID, that value is used as the primary identifier so CLI targeting, GUI selection state, and persisted run history are less dependent on the current mount path.

The benchmark runner creates a dedicated temporary directory on the selected target and executes the requested benchmark suite inside that directory. Progress is streamed through a callback that both the CLI and GUI consume.

The core crate also owns the shared export pipeline so JSON, Markdown, HTML, and PNG reports are generated once and reused by both front ends.

PNG export is now layout-aware inside the core export module. It chooses between single-run detail panels, two-run overlay panels, and same-device trend panels based on the export set, so report rendering does not need to be reimplemented in the GUI.

### `riedspied-cli`

The CLI is intentionally thin. It only:

- resolves a target from discovered devices
- constructs the selected profile and benchmark list
- renders progress and summary output
- saves completed runs into the history store
- exports selected runs through the shared export layer

CLI target resolution accepts device name, mount path, source path, or explicit device ID. The list command exposes those IDs in verbose mode so users can reference a specific volume directly.

### `riedspied-gui`

The GUI runs the same benchmark suite inside a background thread. Progress events are sent through an `mpsc` channel into the UI state, where they drive the live throughput chart and current-phase display.

History analysis stays GUI-side. The app reads the same persisted `BenchmarkRunRecord` values and builds filtering, per-run detail views, same-device trend charts, direct run comparison, and annotation editing on top of those records.

The GUI stores the selected target by `DeviceTarget.id`, so a stable identifier matters for preserving the intended selection across device refreshes.

GUI export is now picker-aware. The app keeps an editable export path, but it can also open a native save dialog through a GUI-only integration layer and remembers the last successful export directory separately from benchmark history.

## Execution flow

1. Discover mounted benchmark targets.
2. Validate free space against the selected profile.
3. Create a temporary hidden benchmark directory on the target.
4. Run sequential write, sequential read, sustained write, and random IOPS benchmarks.
5. Emit progress updates with current throughput and elapsed time.
6. Save the completed run to the local history store.
7. Remove temporary files unless the caller explicitly requests they be retained.

## Why mounted filesystems first

Mounted filesystems give the project a safe v1 boundary:

- no root privileges are required for normal usage
- the same path-based I/O model works for internal disks, removable media, and mounted NAS shares
- CLI and GUI can share one execution backend without transport-specific adapters

MTP/PTP and raw-device benchmarking should stay separate backends because they do not follow normal filesystem semantics.

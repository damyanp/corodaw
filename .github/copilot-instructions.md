# Copilot Instructions for Corodaw

## Build & Test

```sh
# Build the full workspace
cargo check --workspace

# Build and run the main app (default member is crates/corodaw)
cargo run

# Run tests (audio-graph and project crates have tests)
cargo test --workspace

# Run a single test
cargo test -p audio-graph single_node_process

# Lint
cargo clippy --workspace

# ASIO: cpal requires the ASIO SDK. Set CPAL_ASIO_DIR to the SDK path with valid headers.
```

No CI, clippy config, or rustfmt config exists. Use default `cargo clippy` and `cargo fmt`.

## Architecture

Corodaw is a DAW (Digital Audio Workstation) built on **Bevy ECS** for state management and **egui** (via bevy_egui) for UI rendering.

### Crate dependency graph

```
corodaw  (main app — UI, menus, async task runner)
├── corodaw-widgets  (pure egui widgets, no ECS dependency)
├── project          (domain model, persistence, undo/redo)
│   ├── engine       (CLAP plugin host, audio I/O, MIDI, builtin DSP nodes)
│   │   └── audio-graph  (real-time DSP graph — foundational, no workspace deps)
│   └── audio-graph
└── engine
```

- **audio-graph**: Real-time audio DSP graph. Nodes are Bevy ECS components (`GraphNodeDesc`); topology syncs to a `GraphWorker` on the CPAL audio callback thread via messages. Implements `GraphProcessor` trait for DSP.
- **engine**: CLAP plugin host via the `clack` crate. `ClapManager` bridges ECS and a dedicated plugin host thread using message-passing (MPSC). Contains builtin node owners: `GainNodeOwner`, `SummerOwner`, `MidiInputOwner`. Audio I/O via cpal (`AudioOutput`), MIDI via midir.
- **project**: Domain model (`ProjectInfo`, `ChannelOrder`, `ChannelMixerState`, `ChannelPluginBinding`). Persistence with serde JSON + base64 for plugin state. Undo/redo via `EditHistory` (command pattern with `EditCommand` trait).
- **corodaw-widgets**: Pure egui widgets (`ArrangerWidget`, `Meter`). Decoupled from ECS via the `ArrangerDataProvider` trait.
- **corodaw**: Main app. Bevy systems render egui panels. Uses `AsyncTaskRunner` (wrapping `smol::LocalExecutor`) for async tasks (file dialogs) that return `CommandQueue` applied back to the world.

### Key patterns

**Entity identification**: `StableId` component wraps a UUID for stable entity references across serialization and undo/redo. Use `StableId::find_entity(world)` to resolve a `StableId` back to an `Entity`. Never store raw `Entity` values in serialized or undo state.

**Undo/redo**: `EditHistory` (non-send resource) holds undo/redo stacks of `Box<dyn EditCommand>`. Each `EditCommand::execute()` returns its inverse command. Trigger undo/redo via `UndoRedoEvent` observer.

**Audio–UI state sync**: `GraphStateWriter` (audio thread) / `GraphStateReader` (UI thread) provide double-buffered metrics (peak values). Call `swap_buffers()` each frame to read latest values.

**Plugin lifecycle**: ECS inserts `ChannelPluginBinding` → `set_plugins_system` detects change → creates plugin via `ClapManager` → audio graph node wired automatically in `update_channels_system`. Plugin GUIs are native OS windows (not embedded in egui).

**Async task runner**: `AsyncTaskRunner` wraps `smol::LocalExecutor` to run async tasks (e.g., file dialogs via `rfd`). Tasks produce a `CommandQueue` that is applied to the Bevy `World` when the task completes.

**Widget abstraction**: `ArrangerDataProvider` trait decouples the arranger widget from Bevy. The `corodaw` crate implements it with live ECS queries; the widget examples use mock data.

**Bevy plugin registration**: Each subsystem registers as a Bevy `Plugin` (e.g., `ChannelPlugin`, `EditHistoryPlugin`). The app is assembled in `project::build_app()`.

## Naming Conventions

See [`docs/naming-conventions.md`](../docs/naming-conventions.md) for the full naming scheme. Key rules:

- **Update the naming doc** whenever types are added, removed, or renamed.
- All public types must be self-describing without relying on module paths.
- Use `Graph` prefix for audio-graph types, `Clap` prefix for CLAP types.
- Use `*Owner` suffix for components that own an audio graph node entity.
- Use `*Edit` suffix for undo/redo command structs (implementing `EditCommand`).
- Use `*Processor` suffix for audio-thread DSP implementations.

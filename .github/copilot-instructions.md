# Copilot Instructions for Corodaw

## Build & Test

```sh
# Build the full workspace
cargo check --workspace

# Build and run the main app (default member is crates/corodaw)
cargo run

# Run tests (only audio-graph has tests currently)
cargo test --workspace

# Run a single test
cargo test -p audio-graph single_node_process

# ASIO: cpal requires the ASIO SDK. Set CPAL_ASIO_DIR to the SDK path with valid headers.
```

No CI, clippy config, or rustfmt config exists. Use default `cargo clippy` and `cargo fmt`.

## Architecture

Corodaw is a DAW (Digital Audio Workstation) built on **Bevy ECS** for state management and **egui** (via bevy_egui) for UI rendering.

### Crate dependency graph

```
corodaw  (main app — UI, menus, executor)
├── corodaw-widgets  (pure egui widgets, no ECS dependency)
├── project          (domain model, persistence, undo/redo)
│   ├── engine       (CLAP plugin host, audio I/O, MIDI, builtin DSP nodes)
│   │   └── audio-graph  (real-time DSP graph — foundational, no workspace deps)
│   └── audio-graph
└── engine
```

- **audio-graph**: Real-time audio DSP graph. Nodes are Bevy ECS components; topology syncs to a single-threaded `AudioGraphWorker` on the CPAL audio callback thread via messages. Implements `Processor` trait for DSP.
- **engine**: CLAP plugin host via the `clack` crate. `ClapPluginManager` bridges ECS and a dedicated plugin host thread using message-passing (MPSC). Contains builtin nodes: `GainControl`, `Summer`, `MidiInputNode`. Audio I/O via cpal, MIDI via midir.
- **project**: Domain model (`Project`, `ChannelOrder`, `ChannelState`, `ChannelData`). Persistence with serde JSON + base64 for plugin state. Undo/redo via `CommandManager` (command pattern).
- **corodaw-widgets**: Pure egui widgets (`ArrangerWidget`, `Meter`). Decoupled from ECS via the `ArrangerDataProvider` trait.
- **corodaw**: Main app. Bevy systems render egui panels. Uses `smol::LocalExecutor` for async tasks (file dialogs) that return `CommandQueue` applied back to the world.

### Key patterns

**Entity identification**: `Id` component wraps a UUID for stable entity references across serialization and undo/redo. Use `Id::find_entity(world)` to resolve an `Id` back to an `Entity`. Never store raw `Entity` values in serialized or undo state.

**Undo/redo**: `CommandManager` (non-send resource) holds undo/redo stacks of `Box<dyn Command>`. Each `Command::execute()` returns its inverse command. Trigger undo/redo via `UndoRedoEvent` observer. Currently only `RenameChannelCommand` and `ChannelButtonCommand` are implemented.

**Audio–UI state sync**: `StateWriter` (audio thread) / `StateReader` (UI thread) provide double-buffered metrics (peak values). Call `swap_buffers()` each frame to read latest values.

**Plugin lifecycle**: ECS inserts `ChannelData` → `set_plugins_system` detects change → creates plugin via `ClapPluginManager` → audio graph node wired automatically in `update_channels_system`. Plugin GUIs are native OS windows (not embedded in egui).

**Async executor**: `smol::LocalExecutor` runs async tasks (e.g., file dialogs via `rfd`). Tasks produce a `CommandQueue` that is applied to the Bevy `World` when the task completes. See `Executor` in `main.rs`.

**Widget abstraction**: `ArrangerDataProvider` trait decouples the arranger widget from Bevy. The `corodaw` crate implements it with live ECS queries; the widget examples use mock data.

**Bevy plugin registration**: Each subsystem registers as a Bevy `Plugin` (e.g., `ChannelBevyPlugin`, `CommandManagerBevyPlugin`). The app is assembled in `project::make_app()`.

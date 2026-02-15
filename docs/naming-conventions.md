# Naming Conventions

This document describes the naming conventions used throughout the Corodaw
codebase. All new types should follow these conventions, and this document
should be updated whenever types are added, removed, or renamed.

## Principles

1. **Self-describing names** — every public type should be unambiguous without
   relying on its module path. Importing `GraphNode` is clear;
   importing a bare `Node` is not.
2. **Crate-based prefixes** — types in `audio-graph` use the `Graph` prefix;
   CLAP-related types in `engine` use the `Clap` prefix.
3. **Role suffixes** — suffixes like `Owner`, `Processor`, `Edit`, `Desc`
   indicate what a type *does* rather than repeating the domain noun.

## Type Categories

Every domain concept may have representations in up to four architectural
layers:

| Layer | Suffix / Pattern | Lives on | Example |
|---|---|---|---|
| **A — Node Owner** | `*Owner` | Main thread (ECS component) | `GainNodeOwner`, `SummerOwner`, `MidiInputOwner` |
| **B — Graph Description** | `GraphNodeDesc`, `GraphConnection`, … | Main thread (ECS component) | `GraphNodeDesc`, `GraphPorts` |
| **C — Audio-Thread Graph** | `GraphNode`, `GraphState`, … | Audio thread | `GraphNode`, `GraphState` |
| **D — Processor** | `*Processor` | Audio thread | `GainProcessor`, `ClapProcessor`, `SummerProcessor` |
| **E — Channel Model** | `Channel*` | Main thread (ECS) | `ChannelMixerState`, `ChannelPluginBinding` |
| **F — Infrastructure** | Varies | Varies | `GraphPlugin`, `EditHistory`, `StableId` |

## audio-graph crate

| Type | Kind | Description |
|---|---|---|
| `GraphPlugin` | Bevy Plugin | Registers the audio graph systems and resources |
| `GraphController` | Resource (NonSend) | Main-thread controller; sends graph updates to the worker |
| `GraphWorker` | Resource (NonSend) | Audio-thread side; owns the processing graph, calls `tick()` |
| `GraphNodeDesc` | Component | Declarative description of a node (ports, connections) |
| `GraphOutputNode` | Component (marker) | Marks the entity whose output feeds the audio device |
| `GraphPorts` | Struct | Port counts (audio in/out, event in/out) |
| `GraphConnection` | Struct | A single port-to-port connection |
| `GraphError` | Enum | Errors from graph description operations |
| `GraphEvent` | Struct | A timestamped MIDI event flowing through the graph |
| `GraphNode` | Struct | Audio-thread mirror of a node (holds processor + buffers) |
| `GraphState` | Struct | The full audio-thread processing graph |
| `GraphProcessContext` | Struct | Per-node context passed to `GraphProcessor::process()` |
| `GraphProcessor` | Trait | Trait implemented by anything that processes audio/events |
| `GraphAudioBuffers` | Struct | Audio buffer accessor for a node during processing |
| `GraphEventBuffers` | Struct | Event buffer accessor for a node during processing |
| `GraphStateReader` | Struct | Reader end of the triple-buffer state channel |
| `GraphStateWriter` | Struct | Writer end of the triple-buffer state channel |
| `GraphStateValue` | Enum | A value that can be communicated via the state channel |
| `GraphStateBuffer` | Struct | A key-value buffer of `GraphStateValue` entries |
| `graph_state_tracker()` | Free fn | Creates a `(GraphStateReader, GraphStateWriter)` pair |
| `graph_connect_audio()` | Free fn | Connects an audio output port to an input port |
| `graph_connect_event()` | Free fn | Connects an event output port to an input port |
| `graph_disconnect_event_input()` | Free fn | Disconnects all event inputs from a given source node |
| `graph_set_processor()` | Free fn | Assigns a `GraphProcessor` to a node entity |

## engine crate

### Audio output

| Type | Kind | Description |
|---|---|---|
| `AudioOutput` | Struct | Manages the CPAL audio output stream |
| `AudioOutputThread` | Struct | The audio callback thread; calls `GraphWorker::tick()` |

### Built-in nodes

| Type | Kind | Description |
|---|---|---|
| `SummerOwner` | Component | Owns a summing node in the audio graph |
| `SummerProcessor` | Struct | Audio-thread processor that sums inputs |
| `GainNodeOwner` | Component | Owns a gain node; holds a channel sender for gain updates |
| `GainProcessor` | Struct | Audio-thread processor that applies gain + reports peak |
| `MidiInputOwner` | Component | Owns a MIDI input node in the audio graph |
| `MidiInputProcessor` | Struct | Audio-thread processor that injects MIDI events |
| `PeakMeter` | Component | Stores peak level read from the state channel |

### MIDI

| Type | Kind | Description |
|---|---|---|
| `MidiEvent` | Struct | A timestamped MIDI message from a MIDI input device |
| `MidiReceiver` | Resource | Receives MIDI events from the MIDI worker thread |
| `MidiReceiverWorker` | Struct | Background thread that reads from MIDI input ports |

### CLAP plugin hosting

| Type | Kind | Description |
|---|---|---|
| `ClapManager` | Resource (NonSend) | Manages all loaded CLAP plugin instances |
| `ClapInstance` | Struct | A loaded CLAP plugin instance on the host thread |
| `ClapProxy` | Component (Clone) | Proxy handle that forwards requests to the plugin host thread |
| `ClapMainThread` | Struct | CLAP main-thread callback handler |
| `ClapId` | Struct | Index into the `ClapManager`'s plugin list |
| `ClapProcessor` | Struct | Audio-thread adapter; implements `GraphProcessor` for a CLAP plugin |
| `ClapExtensions` | Struct | Tracks which CLAP extensions a plugin supports |
| `Timers` | Struct | Timer infrastructure for CLAP timer extension (not yet wired up) |

### Plugin system

| Type | Kind | Description |
|---|---|---|
| `PluginManager` | Trait | Trait for plugin lifecycle management (load, create, GUI, state) |
| `PluginDescriptor` | Struct | Describes a discovered plugin (id, name, vendor, path) |
| `PluginUiHost` | Struct | Hosts a plugin's GUI window |
| `PluginGuiHandle` | Struct | Weak handle to a plugin GUI window |

## project crate

### Undo/redo system

| Type | Kind | Description |
|---|---|---|
| `EditCommand` | Trait | An undoable edit operation |
| `EditHistory` | Resource (NonSend) | Manages undo/redo stacks |
| `EditHistoryPlugin` | Bevy Plugin | Registers `EditHistory` and undo/redo observers |
| `UndoRedoEvent` | Event | Triggers undo or redo |
| `StableId` | Component | UUID that survives undo/redo (stable across entity respawns) |

### Project model

| Type | Kind | Description |
|---|---|---|
| `ProjectInfo` | Resource | Project metadata (file path) |
| `ChannelOrder` | Resource | Ordered list of channel entity IDs |
| `ChannelPlugin<T>` | Bevy Plugin | Registers channel systems for a given `PluginManager` impl |

### Channel types

| Type | Kind | Description |
|---|---|---|
| `ChannelMixerState` | Component | Mixer-strip state: gain, mute, solo, record arm |
| `ChannelPluginBinding` | Component | Which plugin is bound to a channel + serialized state |
| `ChannelPluginInstance<P>` | Component | Live plugin instance associated with a channel |
| `ChannelGain` | Component | Wraps a `GainNodeOwner` for a channel's gain stage |
| `ChannelSourceNode` | Component | Wraps a `MidiInputOwner` for a channel's input |
| `ChannelSnapshot` | Struct | Serializable snapshot of a channel for undo/redo |
| `ChannelButton` | Enum | Mute / Solo / RecordArm button identifiers |
| `channel_bundle()` | Free fn | Creates the ECS bundle for a new channel |
| `AvailablePlugin` | Component | Wraps a `PluginDescriptor` for UI display |

### Edit commands (all implement `EditCommand`)

| Type | Description |
|---|---|
| `RenameChannelEdit` | Renames a channel |
| `ChannelButtonEdit` | Toggles a channel button (mute/solo/arm) |
| `AddChannelEdit` | Adds a new channel |
| `DeleteChannelEdit` | Deletes a channel |
| `MoveChannelEdit` | Reorders a channel |
| `SetPluginEdit` | Sets or changes a channel's plugin |
| `SetGainEdit` | Changes a channel's gain value |

### App construction

| Type | Kind | Description |
|---|---|---|
| `build_app()` | Free fn | Constructs the full Bevy `App` with all plugins |

## corodaw crate (app)

| Type | Kind | Description |
|---|---|---|
| `AsyncTaskRunner` | Resource (NonSend) | Runs one-shot async tasks (e.g. file dialogs) |
| `FileAction` | Event | Open / Save file actions |
| `InspectorEnabled` | Resource | Toggles the world inspector window |
| `ArrangerData` | SystemParam | Collected query data for the arranger UI |

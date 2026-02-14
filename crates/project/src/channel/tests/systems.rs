use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use audio_graph::Processor;
use bevy_app::prelude::*;
use engine::plugins::ClapPluginId;

use super::*;

static NEXT_MOCK_PLUGIN_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug)]
struct NoOpProcessor;
impl Processor for NoOpProcessor {
    fn process(&mut self, _ctx: audio_graph::ProcessContext) {}
}

#[derive(Component)]
pub(super) struct MockPlugin {
    plugin_id: ClapPluginId,
    plugin_name: String,
}

struct MockPluginFactory {
    plugins_created: Cell<usize>,
}

impl MockPluginFactory {
    fn new() -> Self {
        Self {
            plugins_created: Cell::new(0),
        }
    }
}

impl PluginFactory for MockPluginFactory {
    type Plugin = MockPlugin;

    fn create_plugin_sync(&self, plugin: FoundPlugin) -> MockPlugin {
        let id = NEXT_MOCK_PLUGIN_ID.fetch_add(1, Ordering::Relaxed);
        self.plugins_created.set(self.plugins_created.get() + 1);
        MockPlugin {
            plugin_id: ClapPluginId::from_raw(id),
            plugin_name: plugin.name,
        }
    }

    fn plugin_id(plugin: &MockPlugin) -> ClapPluginId {
        plugin.plugin_id
    }

    fn plugin_name(plugin: &MockPlugin) -> &str {
        &plugin.plugin_name
    }

    fn load_plugin_state(
        &self,
        _clap_plugin_id: ClapPluginId,
        _data: Vec<u8>,
    ) -> futures::channel::oneshot::Receiver<Result<(), String>> {
        let (sender, receiver) = futures::channel::oneshot::channel();
        sender.send(Ok(())).unwrap();
        receiver
    }

    fn set_title(&self, _clap_plugin_id: ClapPluginId, _title: String) {}

    fn show_gui(
        &self,
        _clap_plugin_id: ClapPluginId,
        _title: String,
    ) -> futures::channel::oneshot::Receiver<engine::plugins::GuiHandle> {
        unimplemented!("MockPluginFactory does not support show_gui")
    }

    fn save_plugin_state(
        &self,
        _clap_plugin_id: ClapPluginId,
    ) -> futures::channel::oneshot::Receiver<Option<Vec<u8>>> {
        let (sender, receiver) = futures::channel::oneshot::channel();
        sender.send(None).unwrap();
        receiver
    }

    fn create_audio_graph_node(
        &self,
        _plugin: &MockPlugin,
    ) -> (audio_graph::Node, Box<dyn Processor>) {
        let node = audio_graph::Node::default().audio(0, 2).event(1, 0);
        (node, Box::new(NoOpProcessor))
    }
}

fn setup_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(audio_graph::AudioGraphPlugin);

    let summer = Summer::new(app.world_mut(), 2);
    let midi_input = MidiInputNode::new(app.world_mut());
    app.insert_non_send_resource(summer);
    app.insert_non_send_resource(midi_input);
    app.insert_non_send_resource(MockPluginFactory::new());

    app.add_systems(
        Update,
        (
            remove_plugins_system::<MockPluginFactory>,
            set_plugins_system::<MockPluginFactory>,
            update_channels_system,
            sync_channel_order_system,
            sync_plugin_window_titles_system::<MockPluginFactory>,
        )
            .chain(),
    );

    app.world_mut().spawn(ChannelOrder::default());

    // Register test plugins
    app.world_mut().spawn(AvailablePlugin(FoundPlugin {
        id: "com.test.synth-a".to_owned(),
        name: "Test Synth A".to_owned(),
        path: Default::default(),
    }));
    app.world_mut().spawn(AvailablePlugin(FoundPlugin {
        id: "com.test.synth-b".to_owned(),
        name: "Test Synth B".to_owned(),
        path: Default::default(),
    }));

    app
}

fn spawn_channel(app: &mut App) -> Id {
    let id = Id::new();
    let snapshot = ChannelSnapshot {
        id,
        ..Default::default()
    };
    AddChannelCommand::new(0, snapshot).execute(app.world_mut());
    id
}

fn get_entity(app: &mut App, id: Id) -> Entity {
    id.find_entity(app.world_mut()).unwrap()
}

#[test]
fn set_plugin_creates_components() {
    let mut app = setup_test_app();
    let id = spawn_channel(&mut app);

    let data = make_channel_data("com.test.synth-a");
    SetPluginCommand::new(id, Some(data)).execute(app.world_mut());
    app.update();

    let entity = get_entity(&mut app, id);
    let world = app.world();
    assert!(world.get::<ChannelAudioView<MockPlugin>>(entity).is_some());
    assert!(world.get::<InputNode>(entity).is_some());
    assert!(world.get::<ChannelGainControl>(entity).is_some());
}

#[test]
fn remove_plugin_removes_components() {
    let mut app = setup_test_app();
    let id = spawn_channel(&mut app);

    let data = make_channel_data("com.test.synth-a");
    SetPluginCommand::new(id, Some(data)).execute(app.world_mut());
    app.update();

    // Remove ChannelData
    let entity = get_entity(&mut app, id);
    app.world_mut().entity_mut(entity).remove::<ChannelData>();
    app.update();

    let world = app.world();
    assert!(world.get::<ChannelAudioView<MockPlugin>>(entity).is_none());
    assert!(world.get::<InputNode>(entity).is_none());
    assert!(world.get::<ChannelGainControl>(entity).is_none());
}

#[test]
fn replace_plugin_despawns_old_node() {
    let mut app = setup_test_app();
    let id = spawn_channel(&mut app);

    // Set plugin A
    let data_a = make_channel_data("com.test.synth-a");
    SetPluginCommand::new(id, Some(data_a)).execute(app.world_mut());
    app.update();

    let entity = get_entity(&mut app, id);
    let old_input_node_entity = app.world().get::<InputNode>(entity).unwrap().0;

    // Replace with plugin B
    let data_b = make_channel_data("com.test.synth-b");
    SetPluginCommand::new(id, Some(data_b)).execute(app.world_mut());
    app.update();

    let new_input_node_entity = app.world().get::<InputNode>(entity).unwrap().0;

    // New plugin should have a different node entity
    assert_ne!(old_input_node_entity, new_input_node_entity);

    // InputNode should reference a living entity
    assert!(
        app.world().get_entity(new_input_node_entity).is_ok(),
        "InputNode should reference a valid entity after replacement"
    );

    // Old node entity should be despawned
    assert!(
        app.world().get_entity(old_input_node_entity).is_err(),
        "Old plugin's audio graph node should be despawned after replacement"
    );

    // Second update lets the audio graph's pre_update_system clean up
    // stale connections from the despawned node.
    app.update();

    // New plugin node should be connected to gain, gain to summer
    let gain_entity = app
        .world()
        .get::<ChannelGainControl>(entity)
        .unwrap()
        .0
        .entity;
    let summer_entity = app.world().non_send_resource::<Summer>().entity;

    let gain_node = app.world().get::<Node>(gain_entity).unwrap();
    assert!(
        gain_node.inputs.contains(&new_input_node_entity),
        "Gain should be connected to new plugin node"
    );
    assert!(
        !gain_node.inputs.contains(&old_input_node_entity),
        "Gain should not be connected to old plugin node"
    );

    let summer_node = app.world().get::<Node>(summer_entity).unwrap();
    assert!(
        summer_node.inputs.contains(&gain_entity),
        "Summer should still be connected to gain"
    );
}

#[test]
fn replace_plugin_reuses_gain_control() {
    let mut app = setup_test_app();
    let id = spawn_channel(&mut app);

    // Set plugin A
    let data_a = make_channel_data("com.test.synth-a");
    SetPluginCommand::new(id, Some(data_a)).execute(app.world_mut());
    app.update();

    let entity = get_entity(&mut app, id);
    let gain_entity_before = app
        .world()
        .get::<ChannelGainControl>(entity)
        .unwrap()
        .0
        .entity;

    // Replace with plugin B
    let data_b = make_channel_data("com.test.synth-b");
    SetPluginCommand::new(id, Some(data_b)).execute(app.world_mut());
    app.update();

    let gain_entity_after = app
        .world()
        .get::<ChannelGainControl>(entity)
        .unwrap()
        .0
        .entity;

    // Gain control entity should be reused, not recreated
    assert_eq!(
        gain_entity_before, gain_entity_after,
        "Gain control should be reused across plugin replacement"
    );
}

#[test]
fn undo_plugin_set_despawns_node() {
    let mut app = setup_test_app();
    let id = spawn_channel(&mut app);

    // Set plugin A
    let data_a = make_channel_data("com.test.synth-a");
    let undo = SetPluginCommand::new(id, Some(data_a)).execute(app.world_mut());
    app.update();

    let entity = get_entity(&mut app, id);
    let input_node_entity = app.world().get::<InputNode>(entity).unwrap().0;

    // Undo (removes ChannelData)
    undo.unwrap().execute(app.world_mut());
    app.update();

    // Plugin components should be removed
    assert!(
        app.world()
            .get::<ChannelAudioView<MockPlugin>>(entity)
            .is_none()
    );
    assert!(app.world().get::<InputNode>(entity).is_none());

    // The audio graph node entity should be despawned
    assert!(
        app.world().get_entity(input_node_entity).is_err(),
        "Plugin's audio graph node should be despawned after undo"
    );
}

#[test]
fn undo_redo_plugin_set_reconnects_fresh() {
    let mut app = setup_test_app();
    let id = spawn_channel(&mut app);

    // Set plugin A
    let data_a = make_channel_data("com.test.synth-a");
    let undo = SetPluginCommand::new(id, Some(data_a))
        .execute(app.world_mut())
        .unwrap();
    app.update();

    let entity = get_entity(&mut app, id);
    let original_input_node = app.world().get::<InputNode>(entity).unwrap().0;

    // Undo
    let redo = undo.execute(app.world_mut()).unwrap();
    app.update();

    // Redo
    redo.execute(app.world_mut());
    app.update();

    // Should have a fresh plugin node (possibly new entity)
    let new_input_node = app.world().get::<InputNode>(entity).unwrap().0;

    // InputNode should reference a living entity
    assert!(
        app.world().get_entity(new_input_node).is_ok(),
        "InputNode should reference a valid entity after redo"
    );

    // Second update for audio graph cleanup
    app.update();

    // The new node should be properly wired
    let gain_entity = app
        .world()
        .get::<ChannelGainControl>(entity)
        .unwrap()
        .0
        .entity;
    let summer_entity = app.world().non_send_resource::<Summer>().entity;

    let gain_node = app.world().get::<Node>(gain_entity).unwrap();
    assert!(
        gain_node.inputs.contains(&new_input_node),
        "After redo, gain should be connected to the plugin node"
    );

    // Old node should not be connected
    if original_input_node != new_input_node {
        assert!(
            !gain_node.inputs.contains(&original_input_node),
            "Old plugin node should not be connected after undo/redo"
        );
    }

    let summer_node = app.world().get::<Node>(summer_entity).unwrap();
    assert!(
        summer_node.inputs.contains(&gain_entity),
        "Summer should be connected to gain after redo"
    );
}

#[test]
fn set_plugin_wires_audio_graph() {
    let mut app = setup_test_app();
    let id = spawn_channel(&mut app);

    let data = make_channel_data("com.test.synth-a");
    SetPluginCommand::new(id, Some(data)).execute(app.world_mut());
    app.update();

    let entity = get_entity(&mut app, id);
    let world = app.world();

    let input_node_entity = world.get::<InputNode>(entity).unwrap().0;
    let gain_entity = world.get::<ChannelGainControl>(entity).unwrap().0.entity;
    let summer_entity = world.non_send_resource::<Summer>().entity;

    // Gain control should be connected to plugin node (2 stereo ports)
    let gain_node = world.get::<Node>(gain_entity).unwrap();
    assert!(gain_node.inputs.contains(&input_node_entity));

    // Summer should be connected to gain control
    let summer_node = world.get::<Node>(summer_entity).unwrap();
    assert!(summer_node.inputs.contains(&gain_entity));
}

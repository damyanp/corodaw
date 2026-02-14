use audio_graph::{Connection, Node};
use bevy_app::prelude::*;
use bevy_ecs::{name::Name, prelude::*};
use bevy_reflect::Reflect;

use engine::builtin::{MidiInputNode, Summer};
use engine::plugins::{ClapPluginId, GuiHandle};
use engine::{
    builtin::GainControl,
    plugins::{PluginFactory, discovery::FoundPlugin},
};
use serde::{Deserialize, Serialize};

use base64::{Engine, engine::general_purpose};

use crate::commands::Command;
use crate::{AvailablePlugin, ChannelOrder, Id};

pub struct ChannelBevyPlugin<T: PluginFactory>(std::marker::PhantomData<fn() -> T>);

impl<T: PluginFactory> Default for ChannelBevyPlugin<T> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<T: PluginFactory + 'static> Plugin for ChannelBevyPlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                remove_plugins_system::<T>,
                set_plugins_system::<T>,
                update_channels_system,
                sync_channel_order_system,
                sync_plugin_window_titles_system::<T>,
            )
                .chain(),
        );
    }
}

pub fn new_channel() -> impl Bundle {
    (
        ChannelState::default(),
        Name::new("unnamed channel"),
        Id::new(),
    )
}

#[allow(clippy::type_complexity)]
fn sync_plugin_window_titles_system<T: PluginFactory>(
    channels: Query<
        (&Name, &ChannelAudioView<T::Plugin>),
        Or<(Changed<Name>, Changed<ChannelAudioView<T::Plugin>>)>,
    >,
    plugin_factory: NonSend<T>,
) {
    for (name, view) in &channels {
        if view.has_gui() {
            let title = view.window_title::<T>(name.as_str());
            plugin_factory.set_title(view.plugin_id::<T>(), title);
        }
    }
}

fn remove_plugins_system<T: PluginFactory>(
    mut commands: Commands,
    mut removed: RemovedComponents<ChannelData>,
    channels: Query<Entity, (With<ChannelState>, Without<ChannelData>)>,
) {
    for entity in removed.read() {
        if channels.get(entity).is_ok() {
            commands
                .entity(entity)
                .remove::<(ChannelAudioView<T::Plugin>, ChannelGainControl, InputNode)>();
        }
    }
}

fn set_plugins_system<T: PluginFactory>(
    mut commands: Commands,
    available_plugins: Query<&AvailablePlugin>,
    plugin_factory: NonSend<T>,
    channels: Query<
        (
            Entity,
            &ChannelState,
            &ChannelData,
            Option<&ChannelGainControl>,
        ),
        Changed<ChannelData>,
    >,
    summer: NonSend<Summer>,
) {
    for (entity, state, data, gain_control) in &channels {
        let found_plugin = available_plugins
            .iter()
            .find(|p| p.0.id == data.plugin_id)
            .unwrap();
        let found_plugin = &found_plugin.0;

        let channel_entity = commands.get_entity(entity).unwrap();

        let plugin_state_bytes = data
            .plugin_state
            .as_deref()
            .and_then(|s| general_purpose::STANDARD.decode(s).ok());

        set_plugin(
            &*plugin_factory,
            &summer,
            state,
            channel_entity,
            found_plugin,
            gain_control,
            plugin_state_bytes.as_deref(),
        );
    }
}

fn set_plugin<T: PluginFactory>(
    plugin_factory: &T,
    summer: &Summer,
    state: &ChannelState,
    mut channel_entity: EntityCommands<'_>,
    found_plugin: &FoundPlugin,
    gain_control: Option<&ChannelGainControl>,
    plugin_state_data: Option<&[u8]>,
) {
    let plugin = plugin_factory.create_plugin_sync(found_plugin.clone());

    if let Some(state_data) = plugin_state_data {
        let plugin_id = T::plugin_id(&plugin);
        let result = futures::executor::block_on(async {
            plugin_factory
                .load_plugin_state(plugin_id, state_data.to_vec())
                .await
                .unwrap()
        });
        if let Err(e) = result {
            eprintln!("Warning: failed to load plugin state: {e}");
        }
    }

    // TODO: destroy the old plugin and its graph node!

    let (plugin_node, plugin_processor) = plugin_factory.create_audio_graph_node(&plugin);

    let commands = channel_entity.commands_mut();
    let plugin_node_id = commands.spawn(plugin_node).id();
    commands.queue(move |world: &mut World| {
        audio_graph::set_processor(world, plugin_node_id, plugin_processor);
    });

    let mut gain_control = gain_control;
    let mut new_gain_control = None;
    if gain_control.is_none() {
        new_gain_control = Some(ChannelGainControl(GainControl::new(
            commands,
            state.gain_value,
        )));
        gain_control = new_gain_control.as_ref();
    }
    let gain_control = gain_control.unwrap();

    let gain_control_entity = gain_control.0.entity;
    let summer_entity = summer.entity;
    commands.queue(move |world: &mut World| {
        for port in 0..2 {
            audio_graph::connect_audio(
                world,
                gain_control_entity,
                Connection::new(port, plugin_node_id, port),
            )
            .unwrap();
            audio_graph::connect_audio(
                world,
                summer_entity,
                Connection::new(port, gain_control_entity, port),
            )
            .unwrap();
        }
    });

    if let Some(new_gain_control) = new_gain_control {
        channel_entity.add_child(new_gain_control.0.entity);
        channel_entity.insert(new_gain_control);
    }

    let channel_audio_view = ChannelAudioView {
        plugin,
        gui_handle: Default::default(),
    };

    channel_entity.add_child(plugin_node_id);
    channel_entity.insert((channel_audio_view, InputNode(plugin_node_id)));
}

fn sync_channel_order_system(
    mut orders: Query<&mut ChannelOrder>,
    channels: Query<Entity, With<ChannelState>>,
) {
    let mut order = orders
        .single_mut()
        .expect("Expected exactly one ChannelOrder");

    order
        .channel_order
        .retain(|entity| channels.get(*entity).is_ok());

    // TODO: what is there are channels that aren't listed in channel_order?
}

fn update_channels_system(
    mut commands: Commands,
    channels: Query<(&ChannelState, &InputNode, &ChannelGainControl)>,
    nodes: Query<&Node>,
    midi_input: NonSend<MidiInputNode>,
) {
    let has_soloed = channels.iter().any(|(d, _, _)| d.soloed);
    for (state, input_node, gain_control) in &channels {
        let muted = state.muted || (has_soloed && !state.soloed);
        let gain = if muted { 0.0 } else { state.gain_value };
        gain_control.0.set_gain(gain);

        let input_node_id = input_node.0;
        let Ok(input_node) = nodes.get(input_node_id) else {
            continue;
        };
        let midi_input = midi_input.entity;
        if state.armed {
            if !input_node.has_event_connected(midi_input) {
                commands.queue(move |world: &mut World| {
                    audio_graph::connect_event(
                        world,
                        input_node_id,
                        Connection::new(0, midi_input, 0),
                    )
                    .unwrap();
                });
            }
        } else if input_node.has_event_connected(midi_input) {
            commands.queue(move |world: &mut World| {
                audio_graph::disconnect_event_input_from_node(world, input_node_id, midi_input)
                    .unwrap();
            });
        }
    }
}

#[derive(Component, Reflect)]
struct InputNode(pub Entity);

#[derive(Component, Debug, Clone, Serialize, Deserialize, Reflect)]
#[require(ChannelState)]
pub struct ChannelData {
    pub plugin_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_state: Option<String>,
}

#[derive(Component, Debug, Clone, Serialize, Deserialize, Reflect)]
#[require(Id=Id::new(), Name)]
pub struct ChannelState {
    pub gain_value: f32,
    pub muted: bool,
    pub soloed: bool,
    pub armed: bool,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            gain_value: 1.0,
            muted: false,
            soloed: false,
            armed: false,
        }
    }
}

impl ChannelState {
    pub fn get_button(&self, button: ChannelButton) -> bool {
        match button {
            ChannelButton::Mute => self.muted,
            ChannelButton::Solo => self.soloed,
            ChannelButton::Arm => self.armed,
        }
    }

    pub fn set_button(&mut self, button: ChannelButton, value: bool) {
        match button {
            ChannelButton::Mute => self.muted = value,
            ChannelButton::Solo => self.soloed = value,
            ChannelButton::Arm => self.armed = value,
        }
    }
}

#[derive(Component)]
pub struct ChannelAudioView<P: Component> {
    plugin: P,
    gui_handle: Option<GuiHandle>,
}

#[derive(Component, Debug, Reflect)]
#[reflect(from_reflect = false)]
#[require(ChannelState)]
pub struct ChannelGainControl(#[reflect(ignore)] pub GainControl);

impl<P: Component> ChannelAudioView<P> {
    pub fn has_gui(&self) -> bool {
        self.gui_handle
            .as_ref()
            .map(|h| h.is_visible())
            .unwrap_or(false)
    }

    pub fn plugin_id<T: PluginFactory<Plugin = P>>(&self) -> ClapPluginId {
        T::plugin_id(&self.plugin)
    }

    pub fn window_title<T: PluginFactory<Plugin = P>>(&self, channel_name: &str) -> String {
        format!("{}: {channel_name}", T::plugin_name(&self.plugin))
    }

    pub fn set_gui_handle(&mut self, gui_handle: GuiHandle) {
        self.gui_handle = Some(gui_handle);
    }
}

#[derive(Debug)]
pub struct RenameChannelCommand {
    channel: Id,
    name: String,
}

impl RenameChannelCommand {
    pub fn new(channel: Id, name: String) -> Self {
        Self { channel, name }
    }
}

impl Command for RenameChannelCommand {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>> {
        let entity = self.channel.find_entity(world)?;
        let mut name = world.get_mut::<Name>(entity)?;
        let old_name = name.as_str().to_owned();
        name.set(self.name.clone());
        Some(Box::new(RenameChannelCommand::new(self.channel, old_name)))
    }
}

#[derive(Debug)]
pub struct ChannelButtonCommand {
    channel: Id,
    button: ChannelButton,
    value: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ChannelButton {
    Mute,
    Solo,
    Arm,
}

impl ChannelButtonCommand {
    pub fn new(channel: Id, button: ChannelButton, value: bool) -> Self {
        Self {
            channel,
            button,
            value,
        }
    }
}

impl Command for ChannelButtonCommand {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>> {
        let entity = self.channel.find_entity(world)?;
        let mut state = world.get_mut::<ChannelState>(entity)?;
        let old_value = state.get_button(self.button);
        state.set_button(self.button, self.value);
        Some(Box::new(ChannelButtonCommand::new(
            self.channel,
            self.button,
            old_value,
        )))
    }
}

#[derive(Debug, Clone)]
pub struct ChannelSnapshot {
    pub name: Name,
    pub state: ChannelState,
    pub data: Option<ChannelData>,
    pub id: Id,
}

impl Default for ChannelSnapshot {
    fn default() -> Self {
        Self {
            name: Name::new("unnamed channel"),
            state: ChannelState::default(),
            data: None,
            id: Id::new(),
        }
    }
}

#[derive(Debug)]
pub struct AddChannelCommand {
    index: usize,
    snapshot: ChannelSnapshot,
}

impl AddChannelCommand {
    pub fn new(index: usize, snapshot: ChannelSnapshot) -> Self {
        Self { index, snapshot }
    }
}

impl Command for AddChannelCommand {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>> {
        let mut entity = world.spawn((
            self.snapshot.state.clone(),
            self.snapshot.name.clone(),
            self.snapshot.id,
        ));
        if let Some(data) = &self.snapshot.data {
            entity.insert(data.clone());
        }
        let entity_id = entity.id();

        let mut query = world.query::<&mut ChannelOrder>();
        let mut channel_order = query.single_mut(world).ok()?;
        channel_order.channel_order.insert(self.index, entity_id);

        Some(Box::new(DeleteChannelCommand::new(
            self.snapshot.id,
            self.index,
        )))
    }
}

#[derive(Debug)]
pub struct DeleteChannelCommand {
    channel: Id,
    index: usize,
}

impl DeleteChannelCommand {
    pub fn new(channel: Id, index: usize) -> Self {
        Self { channel, index }
    }
}

impl Command for DeleteChannelCommand {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>> {
        let entity = self.channel.find_entity(world)?;

        let name = world.get::<Name>(entity)?.clone();
        let state = world.get::<ChannelState>(entity)?.clone();
        let data = world.get::<ChannelData>(entity).cloned();
        let id = *world.get::<Id>(entity)?;

        let mut query = world.query::<&mut ChannelOrder>();
        let mut channel_order = query.single_mut(world).ok()?;
        channel_order.channel_order.retain(|&e| e != entity);

        world.despawn(entity);

        let snapshot = ChannelSnapshot {
            name,
            state,
            data,
            id,
        };

        Some(Box::new(AddChannelCommand::new(self.index, snapshot)))
    }
}

#[derive(Debug)]
pub struct MoveChannelCommand {
    from: usize,
    to: usize,
}

impl MoveChannelCommand {
    pub fn new(from: usize, to: usize) -> Self {
        Self { from, to }
    }

    pub fn apply(&self, channel_order: &mut ChannelOrder) -> Box<dyn Command> {
        channel_order.move_channel(self.from, self.to);
        let undo = if self.from < self.to {
            Self::new(self.to - 1, self.from)
        } else if self.from > self.to {
            Self::new(self.to, self.from + 1)
        } else {
            Self::new(self.from, self.to)
        };
        Box::new(undo)
    }
}

impl Command for MoveChannelCommand {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>> {
        let mut query = world.query::<&mut ChannelOrder>();
        let mut channel_order = query.single_mut(world).ok()?;
        let undo = self.apply(&mut channel_order);
        Some(undo)
    }
}

#[derive(Debug)]
pub struct SetPluginCommand {
    channel: Id,
    data: Option<ChannelData>,
}

impl SetPluginCommand {
    pub fn new(channel: Id, data: Option<ChannelData>) -> Self {
        Self { channel, data }
    }
}

impl Command for SetPluginCommand {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>> {
        let entity = self.channel.find_entity(world)?;
        let old_data = world.entity_mut(entity).take::<ChannelData>();
        if let Some(data) = &self.data {
            world.entity_mut(entity).insert(data.clone());
        }
        Some(Box::new(SetPluginCommand::new(self.channel, old_data)))
    }
}

#[derive(Debug)]
pub struct SetGainCommand {
    channel: Id,
    gain_value: f32,
}

impl SetGainCommand {
    pub fn new(channel: Id, gain_value: f32) -> Self {
        Self {
            channel,
            gain_value,
        }
    }
}

impl Command for SetGainCommand {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>> {
        let entity = self.channel.find_entity(world)?;
        let mut state = world.get_mut::<ChannelState>(entity)?;
        let old_value = state.gain_value;
        state.gain_value = self.gain_value;
        Some(Box::new(SetGainCommand::new(self.channel, old_value)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChannelOrder;

    fn setup_world() -> World {
        let mut world = World::new();
        world.spawn(ChannelOrder::default());
        world
    }

    fn get_channel_order(world: &mut World) -> Vec<Entity> {
        let mut query = world.query::<&ChannelOrder>();
        query.single(world).unwrap().channel_order.clone()
    }

    #[test]
    fn add_channel() {
        let mut world = setup_world();
        let snapshot = ChannelSnapshot::default();
        let id = snapshot.id;

        AddChannelCommand::new(0, snapshot).execute(&mut world);

        let order = get_channel_order(&mut world);
        assert_eq!(order.len(), 1);

        let entity = id.find_entity(&mut world).unwrap();
        assert_eq!(order[0], entity);
        assert_eq!(
            world.get::<Name>(entity).unwrap().as_str(),
            "unnamed channel"
        );
        assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 1.0);
        assert_eq!(*world.get::<Id>(entity).unwrap(), id);
    }

    #[test]
    fn add_channel_returns_delete_undo() {
        let mut world = setup_world();
        let snapshot = ChannelSnapshot::default();

        let undo = AddChannelCommand::new(0, snapshot)
            .execute(&mut world)
            .unwrap();
        undo.execute(&mut world);

        let order = get_channel_order(&mut world);
        assert_eq!(order.len(), 0);
    }

    #[test]
    fn delete_channel() {
        let mut world = setup_world();
        let snapshot = ChannelSnapshot::default();
        let id = snapshot.id;

        AddChannelCommand::new(0, snapshot).execute(&mut world);

        DeleteChannelCommand::new(id, 0).execute(&mut world);

        let order = get_channel_order(&mut world);
        assert_eq!(order.len(), 0);
        assert!(id.find_entity(&mut world).is_none());
    }

    #[test]
    fn delete_channel_returns_add_undo() {
        let mut world = setup_world();
        let mut snapshot = ChannelSnapshot::default();
        snapshot.name = Name::new("my channel");
        snapshot.state.gain_value = 0.5;
        let id = snapshot.id;

        AddChannelCommand::new(0, snapshot).execute(&mut world);

        let undo = DeleteChannelCommand::new(id, 0)
            .execute(&mut world)
            .unwrap();
        undo.execute(&mut world);

        let order = get_channel_order(&mut world);
        assert_eq!(order.len(), 1);

        let entity = id.find_entity(&mut world).unwrap();
        assert_eq!(world.get::<Name>(entity).unwrap().as_str(), "my channel");
        assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 0.5);
        assert_eq!(*world.get::<Id>(entity).unwrap(), id);
    }

    #[test]
    fn add_delete_roundtrip() {
        let mut world = setup_world();
        let snapshot = ChannelSnapshot::default();
        let id = snapshot.id;

        // Add
        let delete_cmd = AddChannelCommand::new(0, snapshot)
            .execute(&mut world)
            .unwrap();
        assert_eq!(get_channel_order(&mut world).len(), 1);

        // Delete (undo add)
        let add_cmd = delete_cmd.execute(&mut world).unwrap();
        assert_eq!(get_channel_order(&mut world).len(), 0);

        // Re-add (redo add)
        let delete_cmd = add_cmd.execute(&mut world).unwrap();
        assert_eq!(get_channel_order(&mut world).len(), 1);
        assert!(id.find_entity(&mut world).is_some());

        // Re-delete (undo again)
        delete_cmd.execute(&mut world);
        assert_eq!(get_channel_order(&mut world).len(), 0);
        assert!(id.find_entity(&mut world).is_none());
    }

    #[test]
    fn delete_channel_with_data() {
        let mut world = setup_world();
        let snapshot = ChannelSnapshot {
            data: Some(ChannelData {
                plugin_id: "com.test.plugin".to_owned(),
                plugin_state: Some("dGVzdA==".to_owned()),
            }),
            ..Default::default()
        };
        let id = snapshot.id;

        AddChannelCommand::new(0, snapshot).execute(&mut world);

        let undo = DeleteChannelCommand::new(id, 0)
            .execute(&mut world)
            .unwrap();
        undo.execute(&mut world);

        let entity = id.find_entity(&mut world).unwrap();
        let data = world.get::<ChannelData>(entity).unwrap();
        assert_eq!(data.plugin_id, "com.test.plugin");
        assert_eq!(data.plugin_state.as_deref(), Some("dGVzdA=="));
    }

    fn setup_world_with_4_channels() -> (World, [Id; 4]) {
        let mut world = setup_world();
        let ids: [Id; 4] = std::array::from_fn(|_| Id::new());
        let names = ["A", "B", "C", "D"];
        for (i, (id, name)) in ids.iter().zip(names.iter()).enumerate() {
            let snapshot = ChannelSnapshot {
                name: Name::new(*name),
                id: *id,
                ..Default::default()
            };
            AddChannelCommand::new(i, snapshot).execute(&mut world);
        }
        (world, ids)
    }

    fn get_channel_ids(world: &mut World) -> Vec<Id> {
        let order = get_channel_order(world);
        order
            .iter()
            .map(|e| *world.get::<Id>(*e).unwrap())
            .collect()
    }

    #[test]
    fn move_forward() {
        let (mut world, [a, b, c, d]) = setup_world_with_4_channels();
        MoveChannelCommand::new(0, 2).execute(&mut world);
        assert_eq!(get_channel_ids(&mut world), vec![b, a, c, d]);
    }

    #[test]
    fn move_backward() {
        let (mut world, [a, b, c, d]) = setup_world_with_4_channels();
        MoveChannelCommand::new(2, 0).execute(&mut world);
        assert_eq!(get_channel_ids(&mut world), vec![c, a, b, d]);
    }

    #[test]
    fn move_forward_undo() {
        let (mut world, [a, b, c, d]) = setup_world_with_4_channels();
        let undo = MoveChannelCommand::new(0, 2).execute(&mut world).unwrap();
        undo.execute(&mut world);
        assert_eq!(get_channel_ids(&mut world), vec![a, b, c, d]);
    }

    #[test]
    fn move_backward_undo() {
        let (mut world, [a, b, c, d]) = setup_world_with_4_channels();
        let undo = MoveChannelCommand::new(2, 0).execute(&mut world).unwrap();
        undo.execute(&mut world);
        assert_eq!(get_channel_ids(&mut world), vec![a, b, c, d]);
    }

    #[test]
    fn move_roundtrip() {
        let (mut world, [a, b, c, d]) = setup_world_with_4_channels();

        // move(1, 3): [A, B, C, D] → [A, C, B, D]
        let undo = MoveChannelCommand::new(1, 3).execute(&mut world).unwrap();
        assert_eq!(get_channel_ids(&mut world), vec![a, c, b, d]);

        // undo
        let redo = undo.execute(&mut world).unwrap();
        assert_eq!(get_channel_ids(&mut world), vec![a, b, c, d]);

        // redo
        let undo = redo.execute(&mut world).unwrap();
        assert_eq!(get_channel_ids(&mut world), vec![a, c, b, d]);

        // undo again
        undo.execute(&mut world);
        assert_eq!(get_channel_ids(&mut world), vec![a, b, c, d]);
    }

    #[test]
    fn move_same_position() {
        let (mut world, [a, b, c, d]) = setup_world_with_4_channels();
        MoveChannelCommand::new(1, 1).execute(&mut world);
        assert_eq!(get_channel_ids(&mut world), vec![a, b, c, d]);
    }

    #[test]
    fn move_to_beginning() {
        let (mut world, [a, b, c, d]) = setup_world_with_4_channels();
        let undo = MoveChannelCommand::new(3, 0).execute(&mut world).unwrap();
        assert_eq!(get_channel_ids(&mut world), vec![d, a, b, c]);
        undo.execute(&mut world);
        assert_eq!(get_channel_ids(&mut world), vec![a, b, c, d]);
    }

    #[test]
    fn move_to_end() {
        let (mut world, [a, b, c, d]) = setup_world_with_4_channels();
        let undo = MoveChannelCommand::new(0, 4).execute(&mut world).unwrap();
        assert_eq!(get_channel_ids(&mut world), vec![b, c, d, a]);
        undo.execute(&mut world);
        assert_eq!(get_channel_ids(&mut world), vec![a, b, c, d]);
    }

    fn make_channel_data(plugin_id: &str) -> ChannelData {
        ChannelData {
            plugin_id: plugin_id.to_owned(),
            plugin_state: None,
        }
    }

    #[test]
    fn set_plugin() {
        let mut world = setup_world();
        let snapshot = ChannelSnapshot::default();
        let id = snapshot.id;
        AddChannelCommand::new(0, snapshot).execute(&mut world);

        let data = make_channel_data("com.test.synth");
        SetPluginCommand::new(id, Some(data)).execute(&mut world);

        let entity = id.find_entity(&mut world).unwrap();
        let data = world.get::<ChannelData>(entity).unwrap();
        assert_eq!(data.plugin_id, "com.test.synth");
    }

    #[test]
    fn set_plugin_undo() {
        let mut world = setup_world();
        let snapshot = ChannelSnapshot::default();
        let id = snapshot.id;
        AddChannelCommand::new(0, snapshot).execute(&mut world);

        let data = make_channel_data("com.test.synth");
        let undo = SetPluginCommand::new(id, Some(data))
            .execute(&mut world)
            .unwrap();
        undo.execute(&mut world);

        let entity = id.find_entity(&mut world).unwrap();
        assert!(world.get::<ChannelData>(entity).is_none());
    }

    #[test]
    fn set_plugin_replace() {
        let mut world = setup_world();
        let data_a = make_channel_data("com.test.synth-a");
        let snapshot = ChannelSnapshot {
            data: Some(data_a),
            ..Default::default()
        };
        let id = snapshot.id;
        AddChannelCommand::new(0, snapshot).execute(&mut world);

        let data_b = make_channel_data("com.test.synth-b");
        let undo = SetPluginCommand::new(id, Some(data_b))
            .execute(&mut world)
            .unwrap();

        let entity = id.find_entity(&mut world).unwrap();
        assert_eq!(
            world.get::<ChannelData>(entity).unwrap().plugin_id,
            "com.test.synth-b"
        );

        undo.execute(&mut world);
        assert_eq!(
            world.get::<ChannelData>(entity).unwrap().plugin_id,
            "com.test.synth-a"
        );
    }

    #[test]
    fn set_plugin_roundtrip() {
        let mut world = setup_world();
        let snapshot = ChannelSnapshot::default();
        let id = snapshot.id;
        AddChannelCommand::new(0, snapshot).execute(&mut world);

        let data = make_channel_data("com.test.synth");
        let undo = SetPluginCommand::new(id, Some(data))
            .execute(&mut world)
            .unwrap();

        let entity = id.find_entity(&mut world).unwrap();
        assert!(world.get::<ChannelData>(entity).is_some());

        // undo
        let redo = undo.execute(&mut world).unwrap();
        assert!(world.get::<ChannelData>(entity).is_none());

        // redo
        let undo = redo.execute(&mut world).unwrap();
        assert_eq!(
            world.get::<ChannelData>(entity).unwrap().plugin_id,
            "com.test.synth"
        );

        // undo again
        undo.execute(&mut world);
        assert!(world.get::<ChannelData>(entity).is_none());
    }

    #[test]
    fn set_gain() {
        let mut world = setup_world();
        let snapshot = ChannelSnapshot::default();
        let id = snapshot.id;
        AddChannelCommand::new(0, snapshot).execute(&mut world);

        SetGainCommand::new(id, 0.5).execute(&mut world);

        let entity = id.find_entity(&mut world).unwrap();
        assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 0.5);
    }

    #[test]
    fn set_gain_undo() {
        let mut world = setup_world();
        let snapshot = ChannelSnapshot::default();
        let id = snapshot.id;
        AddChannelCommand::new(0, snapshot).execute(&mut world);

        let undo = SetGainCommand::new(id, 0.3).execute(&mut world).unwrap();
        undo.execute(&mut world);

        let entity = id.find_entity(&mut world).unwrap();
        assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 1.0);
    }

    #[test]
    fn set_gain_roundtrip() {
        let mut world = setup_world();
        let snapshot = ChannelSnapshot::default();
        let id = snapshot.id;
        AddChannelCommand::new(0, snapshot).execute(&mut world);

        let entity = id.find_entity(&mut world).unwrap();

        let undo = SetGainCommand::new(id, 0.7).execute(&mut world).unwrap();
        assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 0.7);

        // undo
        let redo = undo.execute(&mut world).unwrap();
        assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 1.0);

        // redo
        let undo = redo.execute(&mut world).unwrap();
        assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 0.7);

        // undo again
        undo.execute(&mut world);
        assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 1.0);
    }

    // --- System-level test infrastructure ---

    use std::cell::Cell;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use audio_graph::Processor;
    use engine::plugins::ClapPluginId;

    static NEXT_MOCK_PLUGIN_ID: AtomicUsize = AtomicUsize::new(1);

    #[derive(Debug)]
    struct NoOpProcessor;
    impl Processor for NoOpProcessor {
        fn process(&mut self, _ctx: audio_graph::ProcessContext) {}
    }

    #[derive(Component)]
    struct MockPlugin {
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
}

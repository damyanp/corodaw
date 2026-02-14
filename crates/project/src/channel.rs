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

#[allow(clippy::type_complexity)]
fn remove_plugins_system<T: PluginFactory>(
    mut commands: Commands,
    mut removed: RemovedComponents<ChannelData>,
    channels: Query<
        (Entity, Option<&ChannelAudioView<T::Plugin>>),
        (With<ChannelState>, Without<ChannelData>),
    >,
) {
    for entity in removed.read() {
        if let Ok((entity, audio_view)) = channels.get(entity) {
            // Despawn the plugin's audio graph node entity. The audio graph's
            // pre_update_system will automatically disconnect it from any
            // remaining nodes (e.g. the summer) on the next frame.
            if let Some(audio_view) = audio_view {
                commands.entity(audio_view.plugin_node).despawn();
            }
            commands
                .entity(entity)
                .remove::<(ChannelAudioView<T::Plugin>, ChannelGainControl, InputNode)>();
        }
    }
}

#[allow(clippy::type_complexity)]
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
            Option<&ChannelAudioView<T::Plugin>>,
        ),
        Changed<ChannelData>,
    >,
    summer: NonSend<Summer>,
) {
    for (entity, state, data, gain_control, old_audio_view) in &channels {
        let found_plugin = available_plugins
            .iter()
            .find(|p| p.0.id == data.plugin_id)
            .unwrap();
        let found_plugin = &found_plugin.0;

        // Despawn the old plugin's audio graph node. The audio graph's
        // pre_update_system will disconnect it from other nodes next frame.
        if let Some(old_audio_view) = old_audio_view {
            commands.entity(old_audio_view.plugin_node).despawn();
        }

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
        plugin_node: plugin_node_id,
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
    plugin_node: Entity,
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
mod tests;

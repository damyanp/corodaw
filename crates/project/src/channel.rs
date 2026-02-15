use audio_graph::{GraphConnection, GraphNodeDesc};
use bevy_app::prelude::*;
use bevy_ecs::{name::Name, prelude::*};

use engine::builtin::{MidiInputOwner, SummerOwner};
use engine::{
    builtin::GainNodeOwner,
    plugins::{ClapManager, PluginManager, discovery::PluginDescriptor},
};

use base64::{Engine, engine::general_purpose};

use crate::{AvailablePlugin, ChannelOrder, StableId};

mod components;
mod edits;

pub use components::*;
pub use edits::*;

// Re-export so tests and downstream code can call EditCommand methods on edit types.
pub use crate::commands::EditCommand;

pub struct ChannelPlugin<T: PluginManager = ClapManager>(std::marker::PhantomData<fn() -> T>);

impl ChannelPlugin {
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<T: PluginManager> Default for ChannelPlugin<T> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<T: PluginManager + 'static> Plugin for ChannelPlugin<T> {
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

pub fn channel_bundle() -> impl Bundle {
    (
        ChannelMixerState::default(),
        Name::new("unnamed channel"),
        StableId::new(),
    )
}

#[allow(clippy::type_complexity)]
fn sync_plugin_window_titles_system<T: PluginManager>(
    channels: Query<
        (&Name, &ChannelPluginInstance<T::Plugin>),
        Or<(Changed<Name>, Changed<ChannelPluginInstance<T::Plugin>>)>,
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
fn remove_plugins_system<T: PluginManager>(
    mut commands: Commands,
    mut removed: RemovedComponents<ChannelPluginBinding>,
    channels: Query<
        (Entity, Option<&ChannelPluginInstance<T::Plugin>>),
        (With<ChannelMixerState>, Without<ChannelPluginBinding>),
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
            commands.entity(entity).remove::<(
                ChannelPluginInstance<T::Plugin>,
                ChannelGain,
                ChannelSourceNode,
            )>();
        }
    }
}

#[allow(clippy::type_complexity)]
fn set_plugins_system<T: PluginManager>(
    mut commands: Commands,
    available_plugins: Query<&AvailablePlugin>,
    plugin_factory: NonSend<T>,
    channels: Query<
        (
            Entity,
            &ChannelMixerState,
            &ChannelPluginBinding,
            Option<&ChannelGain>,
            Option<&ChannelPluginInstance<T::Plugin>>,
        ),
        Changed<ChannelPluginBinding>,
    >,
    summer: NonSend<SummerOwner>,
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

fn set_plugin<T: PluginManager>(
    plugin_factory: &T,
    summer: &SummerOwner,
    state: &ChannelMixerState,
    mut channel_entity: EntityCommands<'_>,
    found_plugin: &PluginDescriptor,
    gain_control: Option<&ChannelGain>,
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
        audio_graph::graph_set_processor(world, plugin_node_id, plugin_processor);
    });

    let mut gain_control = gain_control;
    let mut new_gain_control = None;
    if gain_control.is_none() {
        new_gain_control = Some(ChannelGain(GainNodeOwner::new(commands, state.gain_value)));
        gain_control = new_gain_control.as_ref();
    }
    let gain_control = gain_control.unwrap();

    let gain_control_entity = gain_control.0.entity;
    let summer_entity = summer.entity;
    commands.queue(move |world: &mut World| {
        for port in 0..2 {
            audio_graph::graph_connect_audio(
                world,
                gain_control_entity,
                GraphConnection::new(port, plugin_node_id, port),
            )
            .unwrap();
            audio_graph::graph_connect_audio(
                world,
                summer_entity,
                GraphConnection::new(port, gain_control_entity, port),
            )
            .unwrap();
        }
    });

    if let Some(new_gain_control) = new_gain_control {
        channel_entity.add_child(new_gain_control.0.entity);
        channel_entity.insert(new_gain_control);
    }

    let channel_audio_view = ChannelPluginInstance {
        plugin,
        plugin_node: plugin_node_id,
        gui_handle: Default::default(),
    };

    channel_entity.add_child(plugin_node_id);
    channel_entity.insert((channel_audio_view, ChannelSourceNode(plugin_node_id)));
}

fn sync_channel_order_system(
    mut orders: Query<&mut ChannelOrder>,
    channels: Query<Entity, With<ChannelMixerState>>,
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
    channels: Query<(&ChannelMixerState, &ChannelSourceNode, &ChannelGain)>,
    nodes: Query<&GraphNodeDesc>,
    midi_input: NonSend<MidiInputOwner>,
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
                    audio_graph::graph_connect_event(
                        world,
                        input_node_id,
                        GraphConnection::new(0, midi_input, 0),
                    )
                    .unwrap();
                });
            }
        } else if input_node.has_event_connected(midi_input) {
            commands.queue(move |world: &mut World| {
                audio_graph::graph_disconnect_event_input(world, input_node_id, midi_input)
                    .unwrap();
            });
        }
    }
}

#[cfg(test)]
mod tests;

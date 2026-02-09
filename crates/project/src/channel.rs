use audio_graph::{Connection, Node};
use bevy_app::prelude::*;
use bevy_ecs::{name::Name, prelude::*};

use engine::builtin::{MidiInputNode, Summer};
use engine::plugins::GuiHandle;
use engine::{
    builtin::GainControl,
    plugins::{ClapPluginManager, ClapPluginShared, discovery::FoundPlugin},
};
use serde::{Deserialize, Serialize};

use base64::{Engine, engine::general_purpose};

use crate::commands::Command;
use crate::{AvailablePlugin, ChannelOrder, Id};

pub struct ChannelBevyPlugin;
impl Plugin for ChannelBevyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                set_plugins_system,
                update_channels_system,
                sync_channel_order_system,
                sync_plugin_window_titles_system,
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
fn sync_plugin_window_titles_system(
    channels: Query<(&Name, &ChannelAudioView), Or<(Changed<Name>, Changed<ChannelAudioView>)>>,
    clap_plugin_manager: NonSend<ClapPluginManager>,
) {
    for (name, view) in &channels {
        if view.has_gui() {
            let title = view.window_title(name.as_str());
            clap_plugin_manager.set_title(view.clap_plugin.plugin_id, title);
        }
    }
}

fn set_plugins_system(
    mut commands: Commands,
    available_plugins: Query<&AvailablePlugin>,
    clap_plugin_manager: NonSend<ClapPluginManager>,
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
            &clap_plugin_manager,
            &summer,
            state,
            channel_entity,
            found_plugin,
            gain_control,
            plugin_state_bytes.as_deref(),
        );
    }
}

fn set_plugin(
    clap_plugin_manager: &ClapPluginManager,
    summer: &Summer,
    state: &ChannelState,
    mut channel_entity: EntityCommands<'_>,
    found_plugin: &FoundPlugin,
    gain_control: Option<&ChannelGainControl>,
    plugin_state_data: Option<&[u8]>,
) {
    let clap_plugin = clap_plugin_manager.create_plugin_sync(found_plugin.clone());

    if let Some(state_data) = plugin_state_data {
        let result = futures::executor::block_on(async {
            clap_plugin_manager
                .load_plugin_state(clap_plugin.plugin_id, state_data.to_vec())
                .await
                .unwrap()
        });
        if let Err(e) = result {
            eprintln!("Warning: failed to load plugin state: {e}");
        }
    }

    // TODO: destroy the old plugin and its graph node!

    let (plugin_node, plugin_processor) = clap_plugin.create_audio_graph_node_sync();

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
        clap_plugin,
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

#[derive(Component)]
struct InputNode(pub Entity);

#[derive(Component, Debug, Clone, Serialize, Deserialize)]
#[require(ChannelState)]
pub struct ChannelData {
    pub plugin_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_state: Option<String>,
}

#[derive(Component, Debug, Clone, Serialize, Deserialize)]
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

#[derive(Component)]
#[require(ChannelState)]
pub struct ChannelAudioView {
    clap_plugin: ClapPluginShared,
    gui_handle: Option<GuiHandle>,
}

#[derive(Component, Debug)]
#[require(ChannelState)]
pub struct ChannelGainControl(pub GainControl);

impl ChannelAudioView {
    pub fn has_gui(&self) -> bool {
        self.gui_handle
            .as_ref()
            .map(|h| h.is_visible())
            .unwrap_or(false)
    }

    pub fn plugin_id(&self) -> engine::plugins::ClapPluginId {
        self.clap_plugin.plugin_id
    }

    pub fn window_title(&self, channel_name: &str) -> String {
        format!("{}: {channel_name}", self.clap_plugin.plugin_name)
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

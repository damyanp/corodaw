use audio_graph::Connection;
use bevy_app::prelude::*;
use bevy_ecs::{name::Name, prelude::*};

use engine::builtin::{MidiInputNode, Summer};
use engine::plugins::GuiHandle;
use engine::{
    builtin::GainControl,
    plugins::{ClapPluginManager, ClapPluginShared, discovery::FoundPlugin},
};
use uuid::Uuid;

use crate::{AvailablePlugin, ChannelOrder, Id};

pub struct ChannelBevyPlugin;
impl Plugin for ChannelBevyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ChannelMessage>().add_systems(
            Update,
            (
                handle_channel_messages,
                set_plugins,
                update_channels,
                sync_channel_order,
            ),
        );
    }
}

pub fn new_channel() -> impl Bundle {
    (
        ChannelState::default(),
        Name::new("unnamed channel"),
        Id(Uuid::new_v4()),
    )
}

fn handle_channel_messages(
    mut channels: Query<(&mut ChannelState, &mut Name, Option<&mut ChannelAudioView>)>,
    mut messages: MessageReader<ChannelMessage>,
    clap_plugin_manager: NonSend<ClapPluginManager>,
) {
    if messages.is_empty() {
        return;
    }

    for message in messages.read() {
        if let Ok((mut state, mut name, view)) = channels.get_mut(message.channel) {
            match &message.control {
                ChannelControl::SetGain(value) => state.gain_value = *value,
                ChannelControl::SetName(value) => *name = Name::new(value.clone()),
                ChannelControl::Mute(value) => state.muted = *value,
                ChannelControl::Solo(value) => state.soloed = *value,
                ChannelControl::Armed(value) => state.armed = *value,
                ChannelControl::ShowGui => {
                    if let Some(mut channel_view) = view {
                        let gui_handle = futures::executor::block_on(async {
                            clap_plugin_manager
                                .show_gui(channel_view.clap_plugin.plugin_id)
                                .await
                                .unwrap()
                        });
                        channel_view.gui_handle = Some(gui_handle);
                    }
                }
            }
        }
    }
}

fn set_plugins(
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

        set_plugin(
            &clap_plugin_manager,
            &summer,
            state,
            channel_entity,
            found_plugin,
            gain_control,
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
) {
    let clap_plugin = clap_plugin_manager.create_plugin_sync(found_plugin.clone());

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
        channel_entity.insert(new_gain_control);
    }

    let channel_audio_view = ChannelAudioView {
        clap_plugin,
        gui_handle: Default::default(),
    };

    channel_entity.insert((channel_audio_view, InputNode(plugin_node_id)));
}

fn sync_channel_order(
    mut orders: Query<&mut ChannelOrder>,
    channels: Query<Entity, With<ChannelState>>,
) {
    let mut order = orders
        .single_mut()
        .expect("Expected exactly one ChannelOrder");

    order
        .channel_order
        .retain(|entity| channels.get(*entity).is_ok());
}

fn update_channels(
    mut commands: Commands,
    channels: Query<(&ChannelState, &InputNode, &ChannelGainControl)>,
    midi_input: NonSend<MidiInputNode>,
) {
    let has_soloed = channels.iter().any(|(d, _, _)| d.soloed);
    for (state, input_node, gain_control) in &channels {
        let muted = state.muted || (has_soloed && !state.soloed);
        let gain = if muted { 0.0 } else { state.gain_value };
        gain_control.0.set_gain(gain);

        let input_node = input_node.0;
        if state.armed {
            let midi_input = midi_input.entity;
            commands.queue(move |world: &mut World| {
                audio_graph::connect_event(world, input_node, Connection::new(0, midi_input, 0))
                    .unwrap();
            });
        } else {
            commands.queue(move |world: &mut World| {
                audio_graph::disconnect_event_input_channel(world, input_node, 0).unwrap();
            });
        }
    }
}

#[derive(Component)]
struct InputNode(pub Entity);

#[derive(Component, Debug, Clone)]
#[require(ChannelState)]
pub struct ChannelData {
    pub plugin_id: String,
}

#[derive(Component, Debug, Clone)]
#[require(Id=Id(Uuid::new_v4()), Name)]
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

#[derive(Component)]
#[require(ChannelState)]
pub struct ChannelGainControl(GainControl);

impl ChannelAudioView {
    pub fn has_gui(&self) -> bool {
        self.gui_handle
            .as_ref()
            .map(|h| h.is_visible())
            .unwrap_or(false)
    }
}

#[derive(Message, Debug)]
pub struct ChannelMessage {
    pub channel: Entity,
    pub control: ChannelControl,
}

#[derive(Debug)]
pub enum ChannelControl {
    SetGain(f32),
    SetName(String),
    Mute(bool),
    Solo(bool),
    Armed(bool),
    ShowGui,
}

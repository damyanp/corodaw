use audio_graph::{AudioGraph, NodeId};
use bevy_app::prelude::*;
use bevy_ecs::{name::Name, prelude::*};

use engine::builtin::{MidiInputNode, Summer};
use engine::plugins::GuiHandle;
use engine::{
    builtin::GainControl,
    plugins::{ClapPluginManager, ClapPluginShared, discovery::FoundPlugin},
};
use uuid::Uuid;

use crate::{AvailablePlugin, Id};

pub struct ChannelBevyPlugin;
impl Plugin for ChannelBevyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ChannelMessage>()
            .add_systems(Update, handle_channel_messages);
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
    mut commands: Commands,
    mut channels: Query<(
        &mut ChannelState,
        Option<&mut ChannelAudioView>,
        Option<&InputNode>,
        Option<&ChannelGainControl>,
    )>,
    available_plugins: Query<&AvailablePlugin>,
    mut messages: MessageReader<ChannelMessage>,
    clap_plugin_manager: NonSend<ClapPluginManager>,
    mut audio_graph: NonSendMut<AudioGraph>,
    summer: NonSend<Summer>,
    midi_input: NonSend<MidiInputNode>,
) {
    if messages.is_empty() {
        return;
    }

    for message in messages.read() {
        if let Ok((mut state, view, _, gain_control)) = channels.get_mut(message.channel) {
            match message.control {
                ChannelControl::SetGain(value) => state.gain_value = value,
                ChannelControl::ToggleMute => state.muted = !state.muted,
                ChannelControl::ToggleSolo => state.soloed = !state.soloed,
                ChannelControl::ToggleArmed => state.armed = !state.armed,
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
                ChannelControl::SetPlugin(entity) => {
                    let plugin = available_plugins.get(entity).unwrap();
                    let entity = commands.get_spawned_entity(message.channel).unwrap();

                    set_plugin(
                        &clap_plugin_manager,
                        &mut audio_graph,
                        &summer,
                        &state,
                        entity,
                        &plugin.0,
                        gain_control,
                    );
                }
            }
        }
    }

    let has_soloed = channels.iter().any(|(d, _, _, _)| d.soloed);
    for (state, _, input_node, gain_control) in &channels {
        if let Some(input_node) = input_node
            && let Some(ChannelGainControl(gain_control)) = gain_control
        {
            let muted = state.muted || (has_soloed && !state.soloed);
            let gain = if muted { 0.0 } else { state.gain_value };
            gain_control.set_gain(gain);

            if state.armed {
                audio_graph
                    .connect_event(input_node.0, 0, midi_input.node_id, 0)
                    .unwrap();
            } else {
                audio_graph.disconnect_event(input_node.0, 0).unwrap();
            }
        }
    }
    audio_graph.update();
}

fn set_plugin(
    clap_plugin_manager: &ClapPluginManager,
    audio_graph: &mut AudioGraph,
    summer: &Summer,
    state: &ChannelState,
    mut channel_entity: EntityCommands<'_>,
    found_plugin: &FoundPlugin,
    gain_control: Option<&ChannelGainControl>,
) {
    // Instantiate the plugin
    let clap_plugin = {
        let receiver = clap_plugin_manager.create_plugin(found_plugin.clone());
        futures::executor::block_on(async { receiver.await.unwrap() })
    };

    // TODO: destroy the old plugin and its graph node!

    let plugin_node_id = futures::executor::block_on(async {
        clap_plugin.create_audio_graph_node(audio_graph).await
    });

    let mut gain_control = gain_control;
    let mut new_gain_control = None;
    if gain_control.is_none() {
        new_gain_control = Some(ChannelGainControl(GainControl::new(
            audio_graph,
            state.gain_value,
        )));
        gain_control = new_gain_control.as_ref();
    }
    let gain_control = gain_control.unwrap();

    for port in 0..2 {
        audio_graph
            .connect_audio(gain_control.0.node_id, port, plugin_node_id, port)
            .unwrap();
        audio_graph
            .connect_audio_add_input(summer.node_id, gain_control.0.node_id, port)
            .unwrap();
    }

    if let Some(new_gain_control) = new_gain_control {
        channel_entity.insert(new_gain_control);
    }

    let channel_data = ChannelData {
        plugin_id: found_plugin.id.clone(),
    };

    let channel_audio_view = ChannelAudioView {
        clap_plugin,
        gui_handle: Default::default(),
    };

    channel_entity.insert((channel_data, channel_audio_view, InputNode(plugin_node_id)));
}

#[derive(Component)]
struct InputNode(pub NodeId);

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
    ToggleMute,
    ToggleSolo,
    ToggleArmed,
    ShowGui,
    SetPlugin(Entity),
}

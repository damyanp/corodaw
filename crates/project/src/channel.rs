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

use crate::Id;

pub struct ChannelBevyPlugin;
impl Plugin for ChannelBevyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ChannelMessage>()
            .add_observer(on_add_channel)
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

fn on_add_channel(
    add_channel: On<AddChannel>,
    mut commands: Commands,
    mut clap_plugin_manager: NonSendMut<ClapPluginManager>,
    mut audio_graph: NonSendMut<AudioGraph>,
    summer: NonSend<Summer>,
) {
    let clap_plugin_manager = &mut clap_plugin_manager;
    let receiver = clap_plugin_manager.create_plugin(add_channel.0.clone());
    let clap_plugin = futures::executor::block_on(async { receiver.await.unwrap() });

    let plugin_node_id = futures::executor::block_on(async {
        clap_plugin.create_audio_graph_node(&mut audio_graph).await
    });

    let gain_value = 1.0;
    let gain_control = GainControl::new(&mut audio_graph, gain_value);

    // TODO: this assumes ports 0 & 1 are the right ones to connect!
    for port in 0..2 {
        audio_graph
            .connect_audio(gain_control.node_id, port, plugin_node_id, port)
            .unwrap();
        audio_graph
            .connect_audio_add_input(summer.node_id, gain_control.node_id, port)
            .unwrap();
    }

    let input_node = InputNode(plugin_node_id);
    let channel_data = ChannelData {
        plugin_id: add_channel.0.id.clone(),
    };

    let channel_state = ChannelState {
        gain_value,
        muted: false,
        soloed: false,
        armed: false,
    };
    let channel_audio_view = ChannelAudioView {
        clap_plugin,
        gain_control,
        gui_handle: Default::default(),
    };

    commands.spawn((
        Name::new("unnamed"),
        Id(Uuid::new_v4()),
        input_node,
        channel_data,
        channel_state,
        channel_audio_view,
    ));

    audio_graph.update();
}

fn handle_channel_messages(
    mut channels: Query<(
        &mut ChannelState,
        Option<&mut ChannelAudioView>,
        Option<&InputNode>,
    )>,
    mut messages: MessageReader<ChannelMessage>,
    clap_plugin_manager: NonSend<ClapPluginManager>,
    mut audio_graph: NonSendMut<AudioGraph>,
    midi_input: NonSend<MidiInputNode>,
) {
    if messages.is_empty() {
        return;
    }

    for message in messages.read() {
        if let Ok(channel) = channels.get_mut(message.channel) {
            let mut channel_state = channel.0;
            let channel_view = channel.1;

            match message.control {
                ChannelControl::SetGain(value) => channel_state.gain_value = value,
                ChannelControl::ToggleMute => channel_state.muted = !channel_state.muted,
                ChannelControl::ToggleSolo => channel_state.soloed = !channel_state.soloed,
                ChannelControl::ToggleArmed => channel_state.armed = !channel_state.armed,
                ChannelControl::ShowGui => {
                    if let Some(mut channel_view) = channel_view {
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

    let has_soloed = channels.iter().any(|(d, _, _)| d.soloed);
    for channel in &channels {
        if let Some(channel_view) = channel.1
            && let Some(input_node) = channel.2
        {
            let muted = channel.0.muted || (has_soloed && !channel.0.soloed);
            let gain = if muted { 0.0 } else { channel.0.gain_value };
            channel_view.gain_control.set_gain(gain);

            if channel.0.armed {
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

#[derive(Component)]
struct InputNode(pub NodeId);

#[derive(Event)]
pub struct AddChannel(pub FoundPlugin);

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
    gain_control: GainControl,
    gui_handle: Option<GuiHandle>,
}

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
}

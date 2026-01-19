use std::marker::PhantomData;

use audio_graph::{AudioGraph, NodeDescBuilder, NodeId};
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use engine::{
    audio::Audio,
    builtin::{GainControl, MidiInputNode, Summer},
    plugins::{ClapPluginManager, ClapPluginShared, discovery::FoundPlugin},
};

#[derive(Serialize)]
pub struct Project {
    channels: Vec<Channel>,

    #[serde(skip)]
    audio_graph: AudioGraph,

    #[serde(skip)]
    clap_plugin_manager: ClapPluginManager,

    #[serde(skip)]
    midi_input: MidiInputNode,

    #[serde(skip)]
    summer: NodeId,

    #[serde(skip)]
    _audio: Audio,
}

impl Default for Project {
    fn default() -> Self {
        let (audio_graph, audio_graph_worker) = AudioGraph::new();
        let audio = Audio::new(audio_graph_worker).unwrap();

        let midi_input = MidiInputNode::new(&audio_graph);

        let summer = audio_graph.add_node(NodeDescBuilder::default().audio(0, 2), Box::new(Summer));
        audio_graph
            .set_output_node(summer)
            .expect("output node was just created and must be valid");

        audio_graph
            .add_input_node(summer, midi_input.node_id)
            .expect("nodes were just created and must be valid");

        audio_graph.update();

        let clap_plugin_manager = ClapPluginManager::default();

        Self {
            channels: Vec::default(),
            audio_graph,
            clap_plugin_manager,
            midi_input,
            summer,
            _audio: audio,
        }
    }
}

impl Project {
    pub fn num_channels(&self) -> usize {
        self.channels.len()
    }

    pub fn audio_graph(&self) -> AudioGraph {
        self.audio_graph.clone()
    }

    pub fn clap_plugin_manager(&self) -> ClapPluginManager {
        self.clap_plugin_manager.clone()
    }

    pub fn add_channel(&mut self, channel: Channel) -> ChannelId {
        let channel_id = channel.id();

        for port in 0..2 {
            self.audio_graph
                .connect_audio_grow_inputs(
                    self.summer,
                    self.num_channels() * 2 + port,
                    channel.output_node(),
                    port,
                )
                .unwrap();
        }

        self.channels.push(channel);

        self.audio_graph.update();

        channel_id
    }

    pub fn channel(&self, id: &ChannelId) -> Option<&Channel> {
        self.channels.iter().find(|m| m.id == *id)
    }

    pub fn channel_mut(&mut self, id: &ChannelId) -> Option<&mut Channel> {
        self.channels.iter_mut().find(|m| m.id == *id)
    }

    pub fn channel_control(&mut self, id: &ChannelId, control: ChannelControl) {
        self.channel_mut(id).unwrap().control(control);

        let has_soloed = self.channels.iter().any(|m| m.is_soloed());
        for channel in self.channels.iter_mut() {
            let muted = channel.is_muted() || (has_soloed && !channel.is_soloed());
            channel.update_gain(muted);

            if channel.is_armed() {
                self.audio_graph
                    .connect_event(channel.input_node(), 0, self.midi_input.node_id, 0)
                    .unwrap();
            } else {
                self.audio_graph
                    .disconnect_event(channel.input_node(), 0)
                    .unwrap();
            }
        }

        self.audio_graph().update();
    }

    pub fn show_gui(&self, id: ChannelId) -> impl Future<Output = ()> + 'static {
        self.channel(&id)
            .unwrap()
            .show_gui(self.clap_plugin_manager.clone())
    }

    pub fn has_gui(&self, id: &ChannelId) -> bool {
        self.channel(id).unwrap().has_gui(&self.clap_plugin_manager)
    }
}

pub enum ChannelControl {
    SetGain(f32),
    ToggleMute,
    ToggleSolo,
    ToggleArmed,
}

pub type ChannelId = Id<Channel>;

#[derive(Serialize, Deserialize, Debug)]
pub struct Channel {
    id: ChannelId,
    name: String,
    plugin_id: String,
    gain_value: f32,
    muted: bool,
    soloed: bool,
    armed: bool,

    #[serde(skip)]
    clap_plugin: Option<ClapPluginShared>,

    #[serde(skip)]
    plugin_node_id: Option<NodeId>,

    #[serde(skip)]
    gain_control: Option<GainControl>,
}

impl Channel {
    pub async fn new(
        name: String,
        audio_graph: &AudioGraph,
        clap_plugin_manager: &ClapPluginManager,
        found_plugin: &FoundPlugin,
        gain_value: f32,
    ) -> Channel {
        let clap_plugin = clap_plugin_manager
            .create_plugin(found_plugin.clone())
            .await;

        let gain_control = GainControl::new(audio_graph, gain_value);
        let plugin_node_id = clap_plugin.create_audio_graph_node(audio_graph).await;

        // TODO: this assumes ports 0 & 1 are the right ones to connect!
        for port in 0..2 {
            audio_graph
                .connect_audio(gain_control.node_id, port, plugin_node_id, port)
                .unwrap();
        }

        Self {
            id: Id::new(),
            name,
            plugin_id: found_plugin.id.clone(),
            gain_value,
            muted: false,
            soloed: false,
            armed: false,
            clap_plugin: Some(clap_plugin),
            plugin_node_id: Some(plugin_node_id),
            gain_control: Some(gain_control),
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn id(&self) -> ChannelId {
        self.id
    }

    fn control(&mut self, control: ChannelControl) {
        match control {
            ChannelControl::SetGain(gain) => self.gain_value = gain,
            ChannelControl::ToggleMute => self.muted = !self.muted,
            ChannelControl::ToggleSolo => self.soloed = !self.soloed,
            ChannelControl::ToggleArmed => self.armed = !self.armed,
        }
    }

    fn update_gain(&self, muted: bool) {
        if let Some(gain_control) = &self.gain_control {
            gain_control.set_gain(if muted { 0.0 } else { self.gain_value });
        }
    }

    pub fn gain(&self) -> f32 {
        self.gain_value
    }

    pub fn input_node(&self) -> NodeId {
        self.plugin_node_id.unwrap()
    }

    pub fn output_node(&self) -> NodeId {
        self.gain_control.as_ref().unwrap().node_id
    }

    fn show_gui(
        &self,
        clap_plugin_manager: ClapPluginManager,
    ) -> impl Future<Output = ()> + 'static {
        let clap_plugin_id = self.clap_plugin.as_ref().unwrap().plugin_id;
        async move {
            clap_plugin_manager.show_gui(clap_plugin_id).await;
        }
    }

    fn has_gui(&self, clap_plugin_manager: &ClapPluginManager) -> bool {
        clap_plugin_manager.has_gui(&self.clap_plugin.as_ref().unwrap().plugin_id)
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn is_soloed(&self) -> bool {
        self.soloed
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }
}

#[derive(Derivative, Serialize, Deserialize, Debug)]
#[derivative(Copy, Clone, Eq, PartialEq)]
pub struct Id<T> {
    uuid: Uuid,
    _phantom: PhantomData<T>,
}

impl<T> Id<T> {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            uuid: Uuid::new_v4(),
            _phantom: PhantomData,
        }
    }
}

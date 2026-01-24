use bevy_app::prelude::*;
use bevy_ecs::prelude::*;

use engine::{
    audio::Audio,
    builtin::{MidiInputNode, Summer},
    plugins::ClapPluginManager,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod channel;
mod found_plugin;
mod project;

#[derive(Component, Serialize, Deserialize)]
struct Id(Uuid);

pub use channel::{
    ChannelAudioView, ChannelControl, ChannelData, ChannelMessage, ChannelState, new_channel,
};
pub use found_plugin::AvailablePlugin;
pub use project::Project;

use crate::found_plugin::add_available_plugins;

pub fn make_app() -> App {
    let (mut audio_graph, audio_graph_worker) = audio_graph::AudioGraph::new();
    let audio = Audio::new(audio_graph_worker).unwrap();

    let midi_input = MidiInputNode::new(&mut audio_graph);
    let summer = Summer::new(&mut audio_graph, 2);
    audio_graph
        .set_output_node(summer.node_id)
        .expect("node was just created and must be valid");

    audio_graph.update();

    let mut app = App::new();

    app.insert_non_send_resource(audio_graph)
        .insert_non_send_resource(ClapPluginManager::default())
        .insert_non_send_resource(midi_input)
        .insert_non_send_resource(summer)
        .insert_non_send_resource(audio)
        .add_plugins(channel::ChannelBevyPlugin);

    app.world_mut().spawn(project::Project);
    app.world_mut().spawn(channel::new_channel());

    add_available_plugins(app.world_mut());

    app
}

use audio_graph::GraphOutputNode;
use bevy_app::prelude::*;
use engine::{
    audio::AudioOutput,
    builtin::{MidiInputOwner, SummerOwner},
    plugins::ClapManager,
};

use super::*;
use crate::{found_plugin::add_available_plugins, project::ProjectPlugin};

pub fn build_app() -> App {
    let mut app = App::new();

    app.add_plugins((
        audio_graph::GraphPlugin,
        ProjectPlugin::<ClapManager>::default(),
    ));

    let audio_graph_worker = app.world_mut().remove_non_send_resource().unwrap();
    let audio = AudioOutput::new(audio_graph_worker).unwrap();

    let midi_input = MidiInputOwner::new(app.world_mut());
    let summer = SummerOwner::new(app.world_mut(), 2);
    app.world_mut()
        .entity_mut(summer.entity)
        .insert(GraphOutputNode);

    app.insert_non_send_resource(ClapManager::default())
        .insert_non_send_resource(midi_input)
        .insert_non_send_resource(summer)
        .insert_non_send_resource(audio)
        .add_plugins((
            channel::ChannelPlugin::<ClapManager>::default(),
            EditHistoryPlugin,
        ));

    // Register types for bevy-inspector-egui
    app.register_type::<StableId>()
        .register_type::<ProjectInfo>()
        .register_type::<ChannelOrder>()
        .register_type::<ChannelPluginBinding>()
        .register_type::<ChannelMixerState>()
        .register_type::<ChannelGain>()
        .register_type::<AvailablePlugin>()
        .register_type::<audio_graph::GraphOutputNode>()
        .register_type::<audio_graph::GraphNodeDesc>()
        .register_type::<audio_graph::GraphConnection>()
        .register_type::<audio_graph::GraphPorts>();

    add_available_plugins(app.world_mut());

    app
}

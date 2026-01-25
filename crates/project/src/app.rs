use audio_graph::OutputNode;
use bevy_app::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use engine::{
    audio::Audio,
    builtin::{MidiInputNode, Summer},
    plugins::ClapPluginManager,
};

use super::*;
use crate::found_plugin::add_available_plugins;

pub fn make_app() -> App {
    let mut app = App::new();

    app.add_plugins(audio_graph::AudioGraphPlugin);

    let audio_graph_worker = app.world_mut().remove_non_send_resource().unwrap();
    let audio = Audio::new(audio_graph_worker).unwrap();

    let midi_input = MidiInputNode::new(app.world_mut());
    let summer = Summer::new(app.world_mut(), 2);
    app.world_mut().entity_mut(summer.entity).insert(OutputNode);

    app.insert_non_send_resource(ClapPluginManager::default())
        .insert_non_send_resource(midi_input)
        .insert_non_send_resource(summer)
        .insert_non_send_resource(audio)
        .add_plugins(channel::ChannelBevyPlugin);

    app.world_mut()
        .spawn((project::Project, project::ChannelOrder::default()));

    app.world_mut()
        .run_system_once(
            |mut commands: Commands, mut channel_order: Single<&mut project::ChannelOrder>| {
                channel_order.as_mut().add_channel(&mut commands);
            },
        )
        .unwrap();

    add_available_plugins(app.world_mut());

    app
}

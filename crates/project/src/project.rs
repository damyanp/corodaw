use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_ecs::system::ScheduleSystem;

use engine::builtin::{MidiInputNode, Summer};
use engine::{
    audio::Audio,
    plugins::{ClapPluginManager, discovery::FoundPlugin},
};

use crate::channel;

pub struct Project {
    app: App,
    _audio: Audio,
}

impl Default for Project {
    fn default() -> Self {
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
            .add_plugins(channel::ChannelBevyPlugin);

        Self { app, _audio: audio }
    }
}

impl Project {
    pub fn update(&mut self) {
        self.app.update();
    }

    pub fn add_channel(&mut self, plugin: &FoundPlugin) {
        self.app
            .world_mut()
            .trigger(channel::AddChannel(plugin.clone()));
    }

    pub fn get_world(&self) -> &World {
        self.app.world()
    }

    pub fn get_world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }

    pub fn add_systems<M>(&mut self, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) {
        self.app.add_systems(Update, systems);
    }

    pub fn write_message<M: Message>(&mut self, message: M) {
        self.app.world_mut().write_message(message);
    }
}

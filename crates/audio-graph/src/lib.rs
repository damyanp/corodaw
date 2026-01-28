use bevy_app::prelude::*;
use bevy_ecs::prelude::*;

mod audio_graph;
mod events;
mod node;
mod worker;

pub use audio_graph::{AudioGraph, AudioGraphWorker};
pub use events::AgEvent;
pub use node::{
    Connection, Node, OutputNode, connect_audio, connect_event, disconnect_event_input_port,
    set_processor,
};
pub use worker::{AgNode, Graph, Processor};

pub struct AudioGraphPlugin;
impl Plugin for AudioGraphPlugin {
    fn build(&self, app: &mut App) {
        let (audio_graph, audio_graph_worker) = AudioGraph::new();
        app.insert_non_send_resource(audio_graph)
            .insert_non_send_resource(audio_graph_worker)
            .add_systems(Update, audio_graph::update);
    }
}

#[cfg(test)]
mod tests;

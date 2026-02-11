use bevy_app::prelude::*;
use bevy_ecs::prelude::*;

mod audio_graph;
mod events;
mod node;
mod worker;

pub use audio_graph::{AudioGraph, AudioGraphWorker};
pub use events::AgEvent;
pub use node::{
    Connection, Node, OutputNode, Ports, connect_audio, connect_event,
    disconnect_event_input_from_node, set_processor,
};
pub use worker::{
    AgNode, Graph, ProcessContext, Processor, StateReader, StateValue, StateWriter, state_tracker,
};

pub struct AudioGraphPlugin;
impl Plugin for AudioGraphPlugin {
    fn build(&self, app: &mut App) {
        let (state_reader, state_writer) = state_tracker();

        let (audio_graph, audio_graph_worker) = AudioGraph::new(state_writer);
        app.insert_non_send_resource(audio_graph)
            .insert_non_send_resource(audio_graph_worker)
            .insert_non_send_resource(state_reader)
            .add_systems(
                Update,
                (audio_graph::pre_update_system, audio_graph::update_system),
            );
    }
}

#[cfg(test)]
mod tests;

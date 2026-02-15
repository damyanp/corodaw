use bevy_app::prelude::*;

mod audio_graph;
mod events;
mod node;
mod worker;

pub use audio_graph::{GraphController, GraphWorker};
pub use events::GraphEvent;
pub use node::{
    GraphConnection, GraphNodeDesc, GraphOutputNode, GraphPorts, graph_connect_audio,
    graph_connect_event, graph_disconnect_event_input, graph_set_processor,
};
pub use worker::{
    GraphNode, GraphProcessContext, GraphProcessor, GraphState, GraphStateReader, GraphStateValue,
    GraphStateWriter, graph_state_tracker,
};

pub struct GraphPlugin;
impl Plugin for GraphPlugin {
    fn build(&self, app: &mut App) {
        let (state_reader, state_writer) = graph_state_tracker();

        let (audio_graph, audio_graph_worker) = GraphController::new(state_writer);
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

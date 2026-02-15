use bevy_ecs::{prelude::*, query::QueryEntityError};
use bevy_reflect::Reflect;

use crate::{GraphController, worker::GraphProcessor};
use thiserror::Error;

#[derive(Component, Reflect)]
pub struct GraphOutputNode;

#[derive(Component, Clone, Debug, Default, Reflect)]
pub struct GraphNodeDesc {
    pub inputs: Vec<Entity>,
    pub audio_channels: GraphPorts,
    pub event_channels: GraphPorts,
    pub always_run: bool,
}

#[derive(Clone, Debug, Default, Reflect)]
pub struct GraphPorts {
    pub connections: Vec<GraphConnection>,
    pub num_inputs: u16,
    pub num_outputs: u16,
}

impl GraphPorts {
    fn new(num_inputs: u16, num_outputs: u16) -> Self {
        Self {
            num_inputs,
            num_outputs,
            ..Default::default()
        }
    }

    fn connect(&mut self, src: &Self, connection: GraphConnection) -> Result<(), GraphError> {
        if connection.channel >= self.num_inputs {
            return Err(GraphError::DestPortOutOfBounds);
        }

        if connection.src_channel >= src.num_outputs {
            return Err(GraphError::SrcPortOutOfBounds);
        }

        if !self.connections.contains(&connection) {
            self.connections.push(connection);
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Reflect)]
pub struct GraphConnection {
    pub channel: u16,
    pub src: Entity,
    pub src_channel: u16,
}

impl GraphConnection {
    pub fn new(channel: u16, src: Entity, src_channel: u16) -> Self {
        Self {
            channel,
            src,
            src_channel,
        }
    }
}

impl GraphNodeDesc {
    pub fn audio(self, num_audio_input_channels: u16, num_audio_output_channels: u16) -> Self {
        Self {
            audio_channels: GraphPorts::new(num_audio_input_channels, num_audio_output_channels),
            ..self
        }
    }

    pub fn event(self, num_event_input_channels: u16, num_event_output_channels: u16) -> Self {
        Self {
            event_channels: GraphPorts::new(num_event_input_channels, num_event_output_channels),
            ..self
        }
    }

    pub fn always_run(self) -> Self {
        Self {
            always_run: true,
            ..self
        }
    }

    pub(crate) fn update_input_nodes(&mut self) {
        let audio_channels = self.audio_channels.connections.iter();
        let event_channels = self.event_channels.connections.iter();
        let ports = audio_channels.chain(event_channels);

        let mut nodes: Vec<_> = ports.map(|c| c.src).collect();
        nodes.sort();
        nodes.dedup();
        self.inputs = nodes;
    }

    pub(crate) fn disconnect_node(&mut self, node: &Entity) -> bool {
        self.audio_channels
            .connections
            .retain(|connection| connection.src != *node);
        self.event_channels
            .connections
            .retain(|connection| connection.src != *node);

        let before = self.inputs.len();

        self.update_input_nodes();

        before != self.inputs.len()
    }

    pub fn has_event_connected(&self, input_node: Entity) -> bool {
        self.event_channels
            .connections
            .iter()
            .any(|connection| connection.src == input_node)
    }
}

pub fn graph_set_processor(
    world_mut: &mut World,
    entity: Entity,
    processor: Box<dyn GraphProcessor>,
) {
    // It's hard to put dyn GraphProcessor's into components (they don't naturally
    // want to be sync), so this is working around that.

    let audio_graph = world_mut
        .get_non_send_resource_mut::<GraphController>()
        .unwrap();
    audio_graph.set_processor(entity, processor);
}

pub fn graph_connect_audio(
    world: &mut World,
    dst: Entity,
    connection: GraphConnection,
) -> Result<(), GraphError> {
    connect_channels(world, dst, connection, |node| &mut node.audio_channels)?;
    Ok(())
}

pub fn graph_connect_event(
    world: &mut World,
    dst: Entity,
    connection: GraphConnection,
) -> Result<(), GraphError> {
    connect_channels(world, dst, connection, |node| &mut node.event_channels)?;
    Ok(())
}

fn connect_channels<F>(
    world: &mut World,
    dst: Entity,
    connection: GraphConnection,
    get_channels: F,
) -> Result<(), GraphError>
where
    F: Fn(&mut GraphNodeDesc) -> &mut GraphPorts,
{
    let mut nodes = world.query::<&mut GraphNodeDesc>();

    let [mut dst_node, mut src_node] =
        nodes
            .get_many_mut(world, [dst, connection.src])
            .map_err(|err| match err {
                QueryEntityError::QueryDoesNotMatch(entity, _) => GraphError::InvalidEntity(entity),
                QueryEntityError::NotSpawned(e) => GraphError::InvalidEntity(e.entity()),
                QueryEntityError::AliasedMutability(_) => GraphError::DestEqualsSrc,
            })?;

    get_channels(&mut dst_node).connect(get_channels(&mut src_node), connection)?;
    dst_node.update_input_nodes();

    Ok(())
}

pub fn graph_disconnect_event_input(
    world: &mut World,
    node: Entity,
    input_node: Entity,
) -> Result<(), GraphError> {
    disconnect_channel_from_node(world, node, input_node, |node| &mut node.event_channels)?;
    Ok(())
}

fn disconnect_channel_from_node<F>(
    world: &mut World,
    node: Entity,
    input_node: Entity,
    get_channels: F,
) -> Result<(), GraphError>
where
    F: Fn(&mut GraphNodeDesc) -> &mut GraphPorts,
{
    let mut dest = world
        .get_mut::<GraphNodeDesc>(node)
        .ok_or(GraphError::InvalidEntity(node))?;
    get_channels(&mut dest)
        .connections
        .retain(|connection| connection.src != input_node);
    Ok(())
}

#[derive(Error, Debug)]
pub enum GraphError {
    #[error("entity doesn't exist")]
    InvalidEntity(Entity),

    #[error("dest_node must not equal src_node")]
    DestEqualsSrc,

    #[error("dest_channel out of bounds")]
    DestPortOutOfBounds,

    #[error("src_channel out of bounds")]
    SrcPortOutOfBounds,
}

#[cfg(test)]
mod test {
    //use bevy_ecs::prelude::*;
    use super::*;

    fn get_node(world: &mut World, node: Entity) -> GraphNodeDesc {
        world
            .query::<&GraphNodeDesc>()
            .get(world, node)
            .unwrap()
            .clone()
    }

    #[test]
    fn test_connect_audio() {
        let mut world = World::new();
        let world = &mut world;
        let a = world.spawn(GraphNodeDesc::default().audio(0, 2)).id();
        let b = world.spawn(GraphNodeDesc::default().audio(2, 0)).id();
        let c = world.spawn(GraphNodeDesc::default().audio(0, 1)).id();

        // out of bound ports
        assert!(graph_connect_audio(world, b, GraphConnection::new(2, a, 0)).is_err());
        assert!(graph_connect_audio(world, b, GraphConnection::new(0, a, 2)).is_err());
        assert!(get_node(world, b).audio_channels.connections.is_empty());

        // simple connections
        graph_connect_audio(world, b, GraphConnection::new(0, a, 0)).unwrap();
        let n = get_node(world, b);
        assert_eq!(n.inputs, vec![a]);
        assert_eq!(
            n.audio_channels.connections,
            vec![GraphConnection::new(0, a, 0)]
        );

        graph_connect_audio(world, b, GraphConnection::new(1, a, 1)).unwrap();
        let mut n = get_node(world, b);
        assert_eq!(n.inputs, vec![a]);
        n.audio_channels.connections.sort();
        assert_eq!(
            n.audio_channels.connections,
            vec![GraphConnection::new(0, a, 0), GraphConnection::new(1, a, 1)]
        );

        // connections to same port
        graph_connect_audio(world, b, GraphConnection::new(0, c, 0)).unwrap();
        graph_connect_audio(world, b, GraphConnection::new(1, c, 0)).unwrap();
        let mut n = get_node(world, b);
        n.inputs.sort();
        n.audio_channels.connections.sort();
        assert_eq!(n.inputs, vec![c, a]);
        assert_eq!(
            n.audio_channels.connections,
            vec![
                GraphConnection::new(0, c, 0),
                GraphConnection::new(0, a, 0),
                GraphConnection::new(1, c, 0),
                GraphConnection::new(1, a, 1),
            ]
        );

        // duplicate connections don't actually duplicate
        graph_connect_audio(world, b, GraphConnection::new(0, a, 0)).unwrap();
        graph_connect_audio(world, b, GraphConnection::new(1, a, 1)).unwrap();
        graph_connect_audio(world, b, GraphConnection::new(0, c, 0)).unwrap();
        graph_connect_audio(world, b, GraphConnection::new(1, c, 0)).unwrap();
        let mut n = get_node(world, b);
        n.inputs.sort();
        n.audio_channels.connections.sort();
        assert_eq!(n.inputs, vec![c, a]);
        assert_eq!(
            n.audio_channels.connections,
            vec![
                GraphConnection::new(0, c, 0),
                GraphConnection::new(0, a, 0),
                GraphConnection::new(1, c, 0),
                GraphConnection::new(1, a, 1),
            ]
        );
    }
}

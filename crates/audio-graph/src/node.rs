#![allow(unused)]

use bevy_ecs::{
    lifecycle::HookContext,
    prelude::*,
    query::QueryEntityError,
    world::{DeferredWorld, error::EntityMutableFetchError},
};

use crate::{AudioGraph, worker::Processor};
use thiserror::Error;

#[derive(Component)]
pub struct OutputNode;

#[derive(Component, Clone, Debug, Default)]
pub struct Node {
    pub inputs: Vec<Entity>,
    pub audio_channels: Ports,
    pub event_channels: Ports,
    pub always_run: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Ports {
    pub connections: Vec<Connection>,
    pub num_inputs: u16,
    pub num_outputs: u16,
}

impl Ports {
    fn new(num_inputs: u16, num_outputs: u16) -> Self {
        Self {
            num_inputs,
            num_outputs,
            ..Default::default()
        }
    }

    fn connect(&mut self, src: &Self, connection: Connection) -> Result<(), AudioGraphDescError> {
        if connection.channel >= self.num_inputs {
            return Err(AudioGraphDescError::DestPortOutOfBounds);
        }

        if connection.src_channel >= src.num_outputs {
            return Err(AudioGraphDescError::SrcPortOutOfBounds);
        }

        if !self.connections.contains(&connection) {
            self.connections.push(connection);
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Connection {
    pub channel: u16,
    pub src: Entity,
    pub src_channel: u16,
}

impl Connection {
    pub fn new(channel: u16, src: Entity, src_channel: u16) -> Self {
        Self {
            channel,
            src,
            src_channel,
        }
    }
}

impl Node {
    pub fn audio(self, num_audio_input_channels: u16, num_audio_output_channels: u16) -> Self {
        Self {
            audio_channels: Ports::new(num_audio_input_channels, num_audio_output_channels),
            ..self
        }
    }

    pub fn event(self, num_event_input_channels: u16, num_event_output_channels: u16) -> Self {
        Self {
            event_channels: Ports::new(num_event_input_channels, num_event_output_channels),
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
}

pub fn set_processor(world_mut: &mut World, entity: Entity, processor: Box<dyn Processor>) {
    // It's hard to put dyn Processor's into components (they don't naturally
    // want to be sync), so this is working around that.

    let audio_graph = world_mut.get_non_send_resource_mut::<AudioGraph>().unwrap();
    audio_graph.set_processor(entity, processor);
}

pub fn connect_audio(
    world: &mut World,
    dst: Entity,
    connection: Connection,
) -> Result<(), AudioGraphDescError> {
    connect_channels(world, dst, connection, |node| &mut node.audio_channels)?;
    Ok(())
}

pub fn connect_event(
    world: &mut World,
    dst: Entity,
    connection: Connection,
) -> Result<(), AudioGraphDescError> {
    connect_channels(world, dst, connection, |node| &mut node.event_channels)?;
    Ok(())
}

fn connect_channels<F>(
    world: &mut World,
    dst: Entity,
    connection: Connection,
    get_channels: F,
) -> Result<(), AudioGraphDescError>
where
    F: Fn(&mut Node) -> &mut Ports,
{
    let mut nodes = world.query::<&mut Node>();

    let [mut dst_node, mut src_node] =
        nodes
            .get_many_mut(world, [dst, connection.src])
            .map_err(|err| match err {
                QueryEntityError::QueryDoesNotMatch(entity, _) => {
                    AudioGraphDescError::InvalidEntity(entity)
                }
                QueryEntityError::NotSpawned(e) => AudioGraphDescError::InvalidEntity(e.entity()),
                QueryEntityError::AliasedMutability(_) => AudioGraphDescError::DestEqualsSrc,
            })?;

    get_channels(&mut dst_node).connect(get_channels(&mut src_node), connection)?;
    dst_node.update_input_nodes();

    Ok(())
}

pub fn disconnect_event_input_channel(
    world: &mut World,
    node: Entity,
    input_channel: u16,
) -> Result<(), AudioGraphDescError> {
    disconnect_channel(world, node, input_channel, |node| &mut node.event_channels)?;
    Ok(())
}

fn disconnect_channel<F>(
    world: &mut World,
    node: Entity,
    input_channel: u16,
    get_channels: F,
) -> Result<(), AudioGraphDescError>
where
    F: Fn(&mut Node) -> &mut Ports,
{
    let mut dest = world
        .get_mut::<Node>(node)
        .ok_or(AudioGraphDescError::InvalidEntity(node))?;
    get_channels(&mut dest)
        .connections
        .retain(|connection| connection.channel != input_channel);
    Ok(())
}

#[derive(Error, Debug)]
pub enum AudioGraphDescError {
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

    fn get_node(mut world: &mut World, node: Entity) -> Node {
        world.query::<&Node>().get(world, node).unwrap().clone()
    }

    #[test]
    fn test_connect_audio() {
        let mut world = World::new();
        let mut world = &mut world;
        let a = world.spawn(Node::default().audio(0, 2)).id();
        let b = world.spawn(Node::default().audio(2, 0)).id();
        let c = world.spawn(Node::default().audio(0, 1)).id();

        // out of bound ports
        assert!(connect_audio(world, b, Connection::new(2, a, 0)).is_err());
        assert!(connect_audio(world, b, Connection::new(0, a, 2)).is_err());
        assert!(get_node(world, b).audio_channels.connections.is_empty());

        // simple connections
        connect_audio(world, b, Connection::new(0, a, 0)).unwrap();
        let n = get_node(world, b);
        assert_eq!(n.inputs, vec![a]);
        assert_eq!(n.audio_channels.connections, vec![Connection::new(0, a, 0)]);

        connect_audio(world, b, Connection::new(1, a, 1)).unwrap();
        let mut n = get_node(world, b);
        assert_eq!(n.inputs, vec![a]);
        n.audio_channels.connections.sort();
        assert_eq!(
            n.audio_channels.connections,
            vec![Connection::new(0, a, 0), Connection::new(1, a, 1)]
        );

        // connections to same port
        connect_audio(world, b, Connection::new(0, c, 0)).unwrap();
        connect_audio(world, b, Connection::new(1, c, 0)).unwrap();
        let mut n = get_node(world, b);
        n.inputs.sort();
        n.audio_channels.connections.sort();
        assert_eq!(n.inputs, vec![c, a]);
        assert_eq!(
            n.audio_channels.connections,
            vec![
                Connection::new(0, c, 0),
                Connection::new(0, a, 0),
                Connection::new(1, c, 0),
                Connection::new(1, a, 1),
            ]
        );

        // duplicate connections don't actually duplicate
        connect_audio(world, b, Connection::new(0, a, 0)).unwrap();
        connect_audio(world, b, Connection::new(1, a, 1)).unwrap();
        connect_audio(world, b, Connection::new(0, c, 0)).unwrap();
        connect_audio(world, b, Connection::new(1, c, 0)).unwrap();
        let mut n = get_node(world, b);
        n.inputs.sort();
        n.audio_channels.connections.sort();
        assert_eq!(n.inputs, vec![c, a]);
        assert_eq!(
            n.audio_channels.connections,
            vec![
                Connection::new(0, c, 0),
                Connection::new(0, a, 0),
                Connection::new(1, c, 0),
                Connection::new(1, a, 1),
            ]
        );
    }
}

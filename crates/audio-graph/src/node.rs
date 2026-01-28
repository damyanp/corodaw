#![allow(unused)]

use bevy_ecs::{prelude::*, query::QueryEntityError, world::error::EntityMutableFetchError};

use crate::{AudioGraph, worker::Processor};
use thiserror::Error;

#[derive(Component)]
pub struct OutputNode;

#[derive(Component, Clone, Debug, Default)]
pub struct Node {
    pub inputs: Vec<Entity>,
    pub audio_ports: Ports,
    pub event_ports: Ports,
}

#[derive(Clone, Debug, Default)]
pub struct Ports {
    pub connections: Vec<Connection>,
    pub num_inputs: usize,
    pub num_outputs: usize,
}

impl Ports {
    fn new(num_inputs: usize, num_outputs: usize) -> Self {
        Self {
            num_inputs,
            num_outputs,
            ..Default::default()
        }
    }

    fn connect(&mut self, src: &Self, connection: Connection) -> Result<(), AudioGraphDescError> {
        if connection.port >= self.num_inputs {
            return Err(AudioGraphDescError::DestPortOutOfBounds);
        }

        if connection.src_port >= src.num_outputs {
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
    pub port: usize,
    pub src: Entity,
    pub src_port: usize,
}

impl Connection {
    pub fn new(port: usize, src: Entity, src_port: usize) -> Self {
        Self {
            port,
            src,
            src_port,
        }
    }
}

impl Node {
    pub fn audio(self, num_audio_input_ports: usize, num_audio_output_ports: usize) -> Self {
        Self {
            audio_ports: Ports::new(num_audio_input_ports, num_audio_output_ports),
            ..self
        }
    }

    pub fn event(self, num_event_input_ports: usize, num_event_output_ports: usize) -> Self {
        Self {
            event_ports: Ports::new(num_event_input_ports, num_event_output_ports),
            ..self
        }
    }

    pub(crate) fn update_input_nodes(&mut self) {
        let audio_ports = self.audio_ports.connections.iter();
        let event_ports = self.event_ports.connections.iter();
        let ports = audio_ports.chain(event_ports);

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
    connect_ports(world, dst, connection, |node| &mut node.audio_ports)?;
    Ok(())
}

pub fn connect_event(
    world: &mut World,
    dst: Entity,
    connection: Connection,
) -> Result<(), AudioGraphDescError> {
    connect_ports(world, dst, connection, |node| &mut node.event_ports)?;
    Ok(())
}

fn connect_ports<F>(
    world: &mut World,
    dst: Entity,
    connection: Connection,
    get_ports: F,
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

    get_ports(&mut dst_node).connect(get_ports(&mut src_node), connection)?;
    dst_node.update_input_nodes();

    Ok(())
}

pub fn disconnect_event_input_port(
    world: &mut World,
    node: Entity,
    input_port: usize,
) -> Result<(), AudioGraphDescError> {
    disconnect_port(world, node, input_port, |node| &mut node.event_ports)?;
    Ok(())
}

fn disconnect_port<F>(
    world: &mut World,
    node: Entity,
    input_port: usize,
    get_ports: F,
) -> Result<(), AudioGraphDescError>
where
    F: Fn(&mut Node) -> &mut Ports,
{
    let mut dest = world
        .get_mut::<Node>(node)
        .ok_or(AudioGraphDescError::InvalidEntity(node))?;
    get_ports(&mut dest)
        .connections
        .retain(|connection| connection.port != input_port);
    Ok(())
}

#[derive(Error, Debug)]
pub enum AudioGraphDescError {
    #[error("entity doesn't exist")]
    InvalidEntity(Entity),

    #[error("dest_node must not equal src_node")]
    DestEqualsSrc,

    #[error("dest_port out of bounds")]
    DestPortOutOfBounds,

    #[error("src_port out of bounds")]
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
        assert!(get_node(world, b).audio_ports.connections.is_empty());

        // simple connections
        connect_audio(world, b, Connection::new(0, a, 0)).unwrap();
        let n = get_node(world, b);
        assert_eq!(n.inputs, vec![a]);
        assert_eq!(n.audio_ports.connections, vec![Connection::new(0, a, 0)]);

        connect_audio(world, b, Connection::new(1, a, 1)).unwrap();
        let mut n = get_node(world, b);
        assert_eq!(n.inputs, vec![a]);
        n.audio_ports.connections.sort();
        assert_eq!(
            n.audio_ports.connections,
            vec![Connection::new(0, a, 0), Connection::new(1, a, 1)]
        );

        // connections to same port
        connect_audio(world, b, Connection::new(0, c, 0)).unwrap();
        connect_audio(world, b, Connection::new(1, c, 0)).unwrap();
        let mut n = get_node(world, b);
        n.inputs.sort();
        n.audio_ports.connections.sort();
        assert_eq!(n.inputs, vec![c, a]);
        assert_eq!(
            n.audio_ports.connections,
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
        n.audio_ports.connections.sort();
        assert_eq!(n.inputs, vec![c, a]);
        assert_eq!(
            n.audio_ports.connections,
            vec![
                Connection::new(0, c, 0),
                Connection::new(0, a, 0),
                Connection::new(1, c, 0),
                Connection::new(1, a, 1),
            ]
        );
    }
}

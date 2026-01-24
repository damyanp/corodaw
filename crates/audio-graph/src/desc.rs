#![allow(unused)]

use bevy_ecs::{prelude::*, query::QueryEntityError, world::error::EntityMutableFetchError};

use crate::{AudioGraph, worker::Processor};
use thiserror::Error;

#[derive(Component)]
pub struct OutputNode;

#[derive(Component, Clone, Debug, Default)]
pub struct Node {
    pub inputs: Vec<Entity>,
    pub audio_input_connections: Vec<InputConnection>,
    pub event_input_connections: Vec<InputConnection>,
    pub num_audio_outputs: usize,
    pub num_event_outputs: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum InputConnection {
    #[default]
    Disconnected,
    Connected(Entity, usize),
}

impl Node {
    pub fn audio(self, num_audio_inputs: usize, num_audio_outputs: usize) -> Self {
        let mut audio_input_connections = Vec::default();
        audio_input_connections.resize_with(num_audio_inputs, Default::default);

        Self {
            audio_input_connections,
            num_audio_outputs,
            ..self
        }
    }

    pub fn event(self, num_event_inputs: usize, num_event_outputs: usize) -> Self {
        let mut event_input_connections = Vec::default();
        event_input_connections.resize_with(num_event_inputs, Default::default);

        Self {
            event_input_connections,
            num_event_outputs,
            ..self
        }
    }

    pub(crate) fn update_input_nodes(&mut self) {
        let mut nodes: Vec<_> = self
            .audio_input_connections
            .iter()
            .chain(self.event_input_connections.iter())
            .filter_map(|c| {
                if let InputConnection::Connected(node, _) = c {
                    Some(*node)
                } else {
                    None
                }
            })
            .collect();
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
    dst_port: usize,
    src: Entity,
    src_port: usize,
) -> Result<(), AudioGraphDescError> {
    let mut nodes = world.query::<&mut Node>();

    let [mut dst_node, src_node] =
        nodes
            .get_many_mut(world, [dst, src])
            .map_err(|err| match err {
                QueryEntityError::QueryDoesNotMatch(entity, _) => {
                    AudioGraphDescError::InvalidEntity(entity)
                }
                QueryEntityError::NotSpawned(e) => AudioGraphDescError::InvalidEntity(e.entity()),
                QueryEntityError::AliasedMutability(_) => AudioGraphDescError::DestEqualsSrc,
            })?;

    if dst_port >= dst_node.audio_input_connections.len() {
        return Err(AudioGraphDescError::DestPortOutOfBounds);
    }

    if src_port >= src_node.num_audio_outputs {
        return Err(AudioGraphDescError::SrcPortOutOfBounds);
    }

    dst_node.audio_input_connections[dst_port] = InputConnection::Connected(src, src_port);
    dst_node.update_input_nodes();

    Ok(())
}

pub fn add_audio_input(
    world: &mut World,
    dst: Entity,
    src: Entity,
    src_port: usize,
) -> Result<(), AudioGraphDescError> {
    let mut dst_node = world
        .get_mut::<Node>(dst)
        .ok_or(AudioGraphDescError::InvalidEntity(dst))?;
    let dst_port = dst_node.audio_input_connections.len();
    dst_node
        .audio_input_connections
        .push(InputConnection::Disconnected);
    connect_audio(world, dst, dst_port, src, src_port)?;
    Ok(())
}

pub fn connect_event(
    world: &mut World,
    dst: Entity,
    dst_port: usize,
    src: Entity,
    src_port: usize,
) -> Result<(), AudioGraphDescError> {
    let mut nodes = world.query::<&mut Node>();

    let [mut dst_node, src_node] =
        nodes
            .get_many_mut(world, [dst, src])
            .map_err(|err| match err {
                QueryEntityError::QueryDoesNotMatch(entity, _) => {
                    AudioGraphDescError::InvalidEntity(entity)
                }
                QueryEntityError::NotSpawned(e) => AudioGraphDescError::InvalidEntity(e.entity()),
                QueryEntityError::AliasedMutability(_) => AudioGraphDescError::DestEqualsSrc,
            })?;

    if dst_port >= dst_node.event_input_connections.len() {
        return Err(AudioGraphDescError::DestPortOutOfBounds);
    }

    if src_port >= src_node.num_event_outputs {
        return Err(AudioGraphDescError::SrcPortOutOfBounds);
    }

    dst_node.event_input_connections[dst_port] = InputConnection::Connected(src, src_port);
    dst_node.update_input_nodes();

    Ok(())
}

pub fn disconnect_event(
    world: &mut World,
    dest_node: Entity,
    dest_port: usize,
) -> Result<(), AudioGraphDescError> {
    let mut dest = world
        .get_mut::<Node>(dest_node)
        .ok_or(AudioGraphDescError::InvalidEntity(dest_node))?;
    dest.event_input_connections[dest_port] = InputConnection::Disconnected;
    dest.update_input_nodes();
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

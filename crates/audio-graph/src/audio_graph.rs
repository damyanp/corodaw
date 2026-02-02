#![allow(unused)]
use bevy_ecs::prelude::*;

use audio_blocks::{AudioBlock, AudioBlockInterleavedViewMut, AudioBlockMut, AudioBlockOpsMut};

use crate::{
    Node, Processor,
    node::{self, OutputNode},
    state_tracker,
    worker::{Graph, StateReader, StateWriter},
};
use std::{
    cell::RefCell,
    ops::DerefMut,
    sync::mpsc::{Receiver, Sender, channel},
    time::Duration,
};

pub struct AudioGraph {
    modified: bool,
    sender: Sender<AudioGraphMessage>,
}

/// This is the part of the audio graph that does audio processing, so it lives
/// on the audio thread.
pub struct AudioGraphWorker {
    receiver: Receiver<AudioGraphMessage>,
    state_writer: StateWriter,
    num_channels: u16,
    sample_rate: u32,
    pub(crate) graph: Graph,
    output: Option<Entity>,
}

enum AudioGraphMessage {
    SetProcessor(Entity, Box<dyn Processor>),
    UpdateGraph {
        changed: Vec<(Entity, Node)>,
        removed: Vec<Entity>,
        output_node: Option<Entity>,
    },
}

impl AudioGraph {
    pub fn new(state_writer: StateWriter) -> (AudioGraph, AudioGraphWorker) {
        let (sender, receiver) = channel();

        let audio_graph = AudioGraph {
            modified: false,
            sender,
        };

        (audio_graph, AudioGraphWorker::new(receiver, state_writer))
    }

    pub fn set_processor(&self, entity: Entity, processor: Box<dyn Processor>) {
        self.sender
            .send(AudioGraphMessage::SetProcessor(entity, processor));
    }
}

pub(crate) fn pre_update(
    mut removed_nodes: RemovedComponents<Node>,
    mut nodes: Query<(Entity, &mut Node)>,
) {
    if removed_nodes.is_empty() {
        return;
    }

    let removed: Vec<_> = removed_nodes.read().collect();

    for r in removed.iter() {
        println!("** {:?} removed", r);
    }

    for (entity, mut node) in nodes {
        if node.inputs.iter().any(|input| removed.contains(input)) {
            for n in removed.iter() {
                if node.deref_mut().disconnect_node(n) {
                    println!("** {:?} removed input from {:?}", entity, n);
                }
            }
        }
    }
}

pub(crate) fn update(
    mut commands: Commands,
    mut audio_graph: NonSendMut<AudioGraph>,
    mut changed_nodes: Query<(Entity, Ref<node::Node>, Option<&Name>)>,
    mut removed_nodes: RemovedComponents<node::Node>,
    output_node: Option<Single<(Entity, &OutputNode)>>,
) {
    let removed = Vec::from_iter(removed_nodes.read());

    let mut changed = Vec::default();

    for (entity, node, name) in &mut changed_nodes {
        if node.is_changed() {
            println!("{:?} ({:?}) is changed", entity, name);
            changed.push((entity, node.clone()));
        }
    }

    let output_node = output_node.map(|s| s.0);

    audio_graph.sender.send(AudioGraphMessage::UpdateGraph {
        changed,
        removed,
        output_node,
    });
}

impl AudioGraphWorker {
    fn new(receiver: Receiver<AudioGraphMessage>, state_writer: StateWriter) -> Self {
        Self {
            receiver,
            state_writer,
            graph: Default::default(),
            output: None,
            num_channels: 0,
            sample_rate: 0,
        }
    }

    pub fn configure(&mut self, channels: u16, sample_rate: u32) {
        self.num_channels = channels;
        self.sample_rate = sample_rate;
    }

    pub fn tick(&mut self, data: &mut [f32], timestamp: Duration) {
        for message in self.receiver.try_iter() {
            match message {
                AudioGraphMessage::UpdateGraph {
                    changed,
                    removed,
                    output_node,
                } => {
                    self.graph.update(changed, removed);
                    self.output = output_node;
                }
                AudioGraphMessage::SetProcessor(entity, processor) => {
                    self.graph.processors.borrow_mut().set(entity, processor);
                }
            }
        }

        self.state_writer.swap_buffers();

        let num_frames = data.len() / self.num_channels as usize;
        let mut block = AudioBlockInterleavedViewMut::from_slice(data, self.num_channels);

        if let Some(output) = self.output {
            self.graph.process(
                output,
                num_frames,
                self.sample_rate,
                &timestamp,
                &mut self.state_writer,
            );

            if let Some(output_node) = self.graph.get_node(output) {
                let output_buffers = output_node.output_audio_buffers.get();

                if output_buffers.num_channels() == block.num_channels() {
                    let output_buffers = output_buffers.frames_iter();
                    let frames_dest = block.frames_iter_mut();

                    for (output_channel, dest_channel) in output_buffers.zip(frames_dest) {
                        for (output_frame, mut dest_frame) in output_channel.zip(dest_channel) {
                            *dest_frame = *output_frame;
                        }
                    }
                } else if output_buffers.num_channels() == 1 {
                    let frames_dest = block.channels_iter_mut();
                    for dest_channel in frames_dest {
                        for (output_frame, mut dest_frame) in
                            output_buffers.channel_iter(0).zip(dest_channel)
                        {
                            *dest_frame = *output_frame;
                        }
                    }
                } else {
                    panic!(
                        "Don't know how to convert from {} channels to {} channels",
                        output_buffers.num_channels(),
                        block.num_channels()
                    );
                }
            }
        } else {
            block.fill_with(0.0);
        }
    }
}

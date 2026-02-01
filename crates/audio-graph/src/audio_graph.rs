#![allow(unused)]
use bevy_ecs::prelude::*;

use audio_blocks::{AudioBlock, AudioBlockInterleavedViewMut, AudioBlockMut, AudioBlockOpsMut};

use crate::{
    Processor, StateBufferGuard,
    node::{self, OutputNode},
    worker::{Graph, StateTracker},
};
use std::{
    cell::RefCell,
    sync::mpsc::{Receiver, Sender, channel},
    time::Duration,
};

pub struct AudioGraph {
    modified: bool,
    sender: Sender<AudioGraphMessage>,
    state_tracker: StateTracker,
}

/// This is the part of the audio graph that does audio processing, so it lives
/// on the audio thread.
pub struct AudioGraphWorker {
    receiver: Receiver<AudioGraphMessage>,
    state_tracker: StateTracker,
    num_channels: u16,
    sample_rate: u32,
    pub(crate) graph: Graph,
    output: Option<Entity>,
}

enum AudioGraphMessage {
    ChangedNodes(Vec<(Entity, node::Node)>),
    SetProcessor(Entity, Box<dyn Processor>),
    SetOutputNode(Option<Entity>),
}

impl AudioGraph {
    pub fn new() -> (AudioGraph, AudioGraphWorker) {
        let (sender, receiver) = channel();
        let state_tracker = StateTracker::default();

        let audio_graph = AudioGraph {
            modified: false,
            sender,
            state_tracker: state_tracker.clone(),
        };

        (audio_graph, AudioGraphWorker::new(receiver, state_tracker))
    }

    pub fn set_processor(&self, entity: Entity, processor: Box<dyn Processor>) {
        self.sender
            .send(AudioGraphMessage::SetProcessor(entity, processor));
    }

    pub fn get_state_buffer(&mut self) -> StateBufferGuard {
        self.state_tracker.get_buffer()
    }
}

pub(crate) fn update(
    mut commands: Commands,
    mut audio_graph: NonSendMut<AudioGraph>,
    mut changed_nodes: Query<(Entity, Ref<node::Node>)>,
    output_node: Option<Single<(Entity, &OutputNode)>>,
) {
    let mut changed = Vec::default();

    for (entity, node) in &mut changed_nodes {
        if node.is_changed() {
            changed.push((entity, node.clone()));
        }
    }

    let output_node = output_node.map(|s| s.0);

    audio_graph
        .sender
        .send(AudioGraphMessage::ChangedNodes(changed));
    audio_graph
        .sender
        .send(AudioGraphMessage::SetOutputNode(output_node));
}

impl AudioGraphWorker {
    fn new(receiver: Receiver<AudioGraphMessage>, state_tracker: StateTracker) -> Self {
        Self {
            receiver,
            state_tracker,
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
                AudioGraphMessage::ChangedNodes(nodes) => self.graph.update(nodes),
                AudioGraphMessage::SetOutputNode(output) => self.output = output,
                AudioGraphMessage::SetProcessor(entity, processor) => {
                    self.graph.processors.borrow_mut().set(entity, processor);
                }
            }
        }

        let mut buffer = self.state_tracker.get_buffer_mut();

        let num_frames = data.len() / self.num_channels as usize;
        let mut block = AudioBlockInterleavedViewMut::from_slice(data, self.num_channels);

        if let Some(output) = self.output {
            self.graph.process(
                output,
                num_frames,
                self.sample_rate,
                &timestamp,
                &mut buffer,
            );

            let output_node = self.graph.get_node(output);
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
        } else {
            block.fill_with(0.0);
        }
    }
}

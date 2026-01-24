#![allow(unused)]
use bevy_ecs::prelude::*;

use audio_blocks::{AudioBlock, AudioBlockInterleavedViewMut, AudioBlockMut, AudioBlockOps};

use crate::{
    Processor,
    desc::{self, OutputNode},
    worker::Graph,
};
use std::{
    cell::RefCell,
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
    num_channels: u16,
    sample_rate: u32,
    pub(crate) graph: Graph,
    output: Option<Entity>,
}

enum AudioGraphMessage {
    ChangedNodes(Vec<(Entity, desc::Node)>),
    SetProcessor(Entity, Box<dyn Processor>),
    SetOutputNode(Option<Entity>),
}

impl AudioGraph {
    pub fn new() -> (AudioGraph, AudioGraphWorker) {
        let (sender, receiver) = channel();

        let audio_graph = AudioGraph {
            modified: false,
            sender,
        };

        (audio_graph, AudioGraphWorker::new(receiver))
    }

    pub fn set_processor(&self, entity: Entity, processor: Box<dyn Processor>) {
        self.sender
            .send(AudioGraphMessage::SetProcessor(entity, processor));
    }
}

pub(crate) fn update(
    audio_graph: NonSendMut<AudioGraph>,
    mut changed_nodes: Query<(Entity, Ref<desc::Node>)>,
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
    fn new(receiver: Receiver<AudioGraphMessage>) -> Self {
        Self {
            receiver,
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

        let num_frames = data.len() / self.num_channels as usize;
        let mut block =
            AudioBlockInterleavedViewMut::from_slice(data, self.num_channels, num_frames);

        if let Some(output) = self.output {
            self.graph.process(output, num_frames, &timestamp);

            let output_node = self.graph.get_node(output);
            let output_buffers = output_node.output_audio_buffers.get();
            let a = &output_buffers[0];
            let b = &output_buffers[1];

            assert_eq!(1, a.num_channels());
            assert_eq!(1, b.num_channels());

            let frames_dest = block.frames_iter_mut();
            let frames_a = a.frames_iter();
            let frames_b = b.frames_iter();

            for (mut dest, (mut a, mut b)) in frames_dest.zip(frames_a.zip(frames_b)) {
                *dest.next().unwrap() = *a.next().unwrap();
                *dest.next().unwrap() = *b.next().unwrap();
                assert!(a.next().is_none());
                assert!(b.next().is_none());
                assert!(dest.next().is_none());
            }
        } else {
            block.fill_with(0.0);
        }
    }
}

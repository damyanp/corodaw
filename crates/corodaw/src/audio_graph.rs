use std::{
    cell::RefCell,
    collections::HashMap,
    sync::mpsc::{Receiver, Sender, channel},
};

use audio_blocks::{
    AudioBlock, AudioBlockInterleavedViewMut, AudioBlockMut, AudioBlockOps, AudioBlockSequential,
};
use clack_host::{prelude::*, process::PluginAudioProcessor};

use crate::plugins::ClapPlugin;

#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug)]
pub struct NodeId(usize);

enum Message {
    AddNode { id: NodeId, desc: NodeDesc },
}

pub fn audio_graph() -> (AudioGraph, AudioGraphWorker) {
    let (sender, receiver) = channel();
    (AudioGraph::new(sender), AudioGraphWorker::new(receiver))
}

pub struct AudioGraph {
    next_node_id: NodeId,
    sender: Sender<Message>,
}

impl AudioGraph {
    fn new(sender: Sender<Message>) -> Self {
        Self {
            next_node_id: NodeId(1),
            sender,
        }
    }

    pub fn add_node(&mut self, desc: NodeDesc) -> NodeId {
        let id = self.next_node_id;
        self.next_node_id.0 += 1;

        self.sender
            .send(Message::AddNode { id, desc })
            .expect("send should not fail");

        id
    }
}

pub struct AudioGraphWorker {
    receiver: Receiver<Message>,
    nodes: HashMap<NodeId, Node>,
    channels: u16,
    sample_rate: u32,
}

impl AudioGraphWorker {
    fn new(receiver: Receiver<Message>) -> Self {
        Self {
            receiver,
            nodes: HashMap::new(),
            channels: 0,
            sample_rate: 0,
        }
    }

    pub fn configure(&mut self, channels: u16, sample_rate: u32) {
        self.channels = channels;
        self.sample_rate = sample_rate;
    }

    pub fn process(&mut self, data: &mut [f32]) {
        self.process_messages();

        let num_frames = data.len() / (self.channels as usize);
        let mut block = AudioBlockInterleavedViewMut::from_slice(data, self.channels, num_frames);

        // for now: just the first node
        let node = self.nodes.iter_mut().next();
        if let Some((_, node)) = node {
            node.process(num_frames);
            let port = &node.audio_buffers.ports[0];

            block.copy_from_block(port);
        }

        //data.fill(0.0);
    }

    fn process_messages(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                Message::AddNode { id, desc } => {
                    let previous = self.nodes.insert(id, Node::new(desc));
                    assert!(previous.is_none());
                }
            }
        }
    }
}

struct Node {
    desc: NodeDesc,
    audio_buffers: AudioBuffers,
}

pub struct NodeDesc {
    pub _is_generator: bool,
    pub processor: RefCell<Box<dyn Processor>>,
    pub audio_inputs: Vec<AudioPortDesc>,
    pub audio_outputs: Vec<AudioPortDesc>,
}

impl Node {
    fn new(desc: NodeDesc) -> Self {
        assert!(
            desc.audio_inputs.len() == 0,
            "Audio inputs not yet implemented"
        );

        let audio_buffers = AudioBuffers::new(desc.audio_outputs.as_slice(), 1024);

        Node {
            desc,
            audio_buffers,
        }
    }

    fn process(&mut self, num_frames: usize) {
        self.audio_buffers.prepare_for_processing(num_frames);

        let processor = &self.desc.processor;

        processor
            .borrow_mut()
            .process(self.audio_buffers.ports.as_mut_slice());
    }
}

pub trait Processor: Send {
    fn process(&mut self, out_audio_buffers: &mut [AudioBlockSequential<f32>]);
}

pub struct AudioPortDesc {
    pub num_channels: u16,
}

struct AudioBuffers {
    ports: Vec<AudioBlockSequential<f32>>,
}

impl AudioBuffers {
    fn new(ports: &[AudioPortDesc], num_frames: usize) -> Self {
        AudioBuffers {
            ports: ports
                .iter()
                .map(|desc| AudioBlockSequential::new(desc.num_channels, num_frames))
                .collect(),
        }
    }

    fn prepare_for_processing(&mut self, num_frames: usize) {
        for port in &mut self.ports {
            if port.num_frames() < num_frames {
                println!("Allocating new audio buffers for {num_frames} frames");
                *port = AudioBlockSequential::new(port.num_channels(), num_frames);
            } else {
                port.set_active_num_frames(num_frames);
            }
        }
    }
}

impl Processor for PluginAudioProcessor<ClapPlugin> {
    fn process(&mut self, out_audio_buffers: &mut [AudioBlockSequential<f32>]) {
        let processor = if self.is_started() {
            self.as_started_mut()
        } else {
            println!("Starting processor!");
            self.start_processing()
        }
        .unwrap();

        let audio_inputs = InputAudioBuffers::empty();
        let input_events = InputEvents::empty();
        let mut output_events = OutputEvents::void();
        let steady_time = None;
        let transport = None;

        let total_channel_count = out_audio_buffers
            .iter()
            .map(|buffer| buffer.num_channels())
            .reduce(|a, b| a + b)
            .unwrap_or(0);

        let mut audio_ports =
            AudioPorts::with_capacity(total_channel_count as usize, out_audio_buffers.len());

        let mut audio_outputs =
            audio_ports.with_output_buffers(out_audio_buffers.iter_mut().map(|port| {
                AudioPortBuffer {
                    latency: 0,
                    channels: AudioPortBufferType::f32_output_only(port.channels_mut()),
                }
            }));

        processor
            .process(
                &audio_inputs,
                &mut audio_outputs,
                &input_events,
                &mut output_events,
                steady_time,
                transport,
            )
            .unwrap();
    }
}

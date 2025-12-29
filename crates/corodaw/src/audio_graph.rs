use std::{
    cell::RefCell,
    collections::HashMap,
    sync::mpsc::{Receiver, Sender, channel},
};

use audio_blocks::{
    AudioBlock, AudioBlockInterleavedViewMut, AudioBlockMut, AudioBlockOps, AudioBlockSequential,
};

pub mod clap_adapter;

#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug)]
pub struct NodeId(usize);

enum Message {
    AddNode { id: NodeId, desc: NodeDesc },
    SetOutputNode(NodeId, bool),
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

    pub fn set_output_node(&mut self, node_id: NodeId, is_output: bool) {
        self.sender
            .send(Message::SetOutputNode(node_id, is_output))
            .expect("send should not fail");
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

        // Process all the nodes (TODO: do them in the right order!)
        for node in self.nodes.values_mut() {
            node.process(num_frames);
        }

        // Sum all the output from the output nodes
        block.fill_with(0.0);

        let output_ports = self
            .nodes
            .values()
            .filter(|node| node.is_output)
            .map(|node| &node.audio_buffers.ports[0]);
        for port in output_ports {
            for (dst, src) in block.frames_iter_mut().zip(port.frames_iter()) {
                assert_eq!(port.num_channels(), port.num_channels());
                dst.zip(src).for_each(|(dst, src)| *dst += *src);
            }
        }
    }

    fn process_messages(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                Message::AddNode { id, desc } => {
                    let previous = self.nodes.insert(id, Node::new(desc));
                    assert!(previous.is_none());
                }
                Message::SetOutputNode(node_id, is_output) => {
                    self.nodes
                        .get_mut(&node_id)
                        .map(|node| node.is_output = is_output);
                }
            }
        }
    }
}

struct Node {
    desc: NodeDesc,
    audio_buffers: AudioBuffers,
    is_output: bool,
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
            desc.audio_inputs.is_empty(),
            "Audio inputs not yet implemented"
        );

        let audio_buffers = AudioBuffers::new(desc.audio_outputs.as_slice(), 1024);

        Node {
            desc,
            audio_buffers,
            is_output: false,
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

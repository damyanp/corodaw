use std::{
    cell::{Ref, RefCell, RefMut},
    collections::HashMap,
    fmt::Debug,
    sync::mpsc::{Receiver, Sender, channel},
};

use audio_blocks::{
    AudioBlock, AudioBlockInterleavedViewMut, AudioBlockMut, AudioBlockOps, AudioBlockSequential,
};

pub mod clap_adapter;

#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug)]
pub struct NodeId(usize);

enum Message {
    AddNode {
        id: NodeId,
        desc: NodeDesc,
    },
    SetOutputNode(NodeId, bool),
    Connect {
        source: NodeId,
        source_port: u32,
        dest: NodeId,
        dest_port: u32,
    },
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

    pub fn set_output_node(&self, node_id: NodeId, is_output: bool) {
        self.sender
            .send(Message::SetOutputNode(node_id, is_output))
            .expect("send should not fail");
    }

    pub fn connect(&self, source: NodeId, source_port: u32, dest: NodeId, dest_port: u32) {
        self.sender
            .send(Message::Connect {
                source,
                source_port,
                dest,
                dest_port,
            })
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

        // Prepare all the nodes
        for node in self.nodes.values_mut() {
            node.prepare_for_processing(num_frames);
        }

        // Process all the nodes (TODO: do them in the right order!)
        for node in self.nodes.values() {
            node.process(self);
        }

        // Sum all the output from the output nodes
        block.fill_with(0.0);

        let output_audio_buffers = self
            .nodes
            .values()
            .filter(|node| node.is_output)
            .map(|node| &node.audio_buffers);
        for audio_buffers in output_audio_buffers {
            let port = &audio_buffers.ports[0].borrow();
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
                    if let Some(node) = self.nodes.get_mut(&node_id) {
                        node.is_output = is_output
                    }
                }
                Message::Connect {
                    source,
                    source_port,
                    dest,
                    dest_port,
                } => {
                    let dest = self.nodes.get_mut(&dest).unwrap();
                    dest.audio_connections[dest_port as usize] = Some((source, source_port));
                }
            }
        }
    }
}

struct Node {
    desc: NodeDesc,
    audio_buffers: AudioBuffers,
    is_output: bool,
    audio_connections: Vec<Option<(NodeId, u32)>>,
}

pub struct NodeDesc {
    pub processor: RefCell<Box<dyn Processor>>,
    pub audio_inputs: Vec<AudioPortDesc>,
    pub audio_outputs: Vec<AudioPortDesc>,
}

impl Debug for NodeDesc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeDesc").finish()
    }
}

impl Node {
    fn new(desc: NodeDesc) -> Self {
        let audio_buffers = AudioBuffers::new(desc.audio_outputs.as_slice(), 1024);
        let audio_connections = desc.audio_inputs.iter().map(|_| None).collect();

        Node {
            desc,
            audio_buffers,
            is_output: false,
            audio_connections,
        }
    }

    fn process(&self, audio_graph: &AudioGraphWorker) {
        let inputs: Vec<_> = self
            .audio_connections
            .iter()
            .map(|connection| {
                connection.map(|(node_id, port_id)| {
                    audio_graph.nodes.get(&node_id).unwrap().audio_buffers.ports[port_id as usize]
                        .borrow()
                })
            })
            .collect();

        let mut borrowed_output_ports: Vec<_> = self
            .audio_buffers
            .ports
            .iter()
            .map(|port| port.borrow_mut())
            .collect();

        let processor = &self.desc.processor;
        processor
            .borrow_mut()
            .process(inputs.as_slice(), borrowed_output_ports.as_mut_slice());
    }

    fn prepare_for_processing(&mut self, num_frames: usize) {
        self.audio_buffers.prepare_for_processing(num_frames);
    }
}

pub trait Processor: Send {
    fn process(
        &mut self,
        in_audio_buffers: &[Option<Ref<'_, AudioBlockSequential<f32>>>],
        out_audio_buffers: &mut [RefMut<'_, AudioBlockSequential<f32>>],
    );
}

pub struct AudioPortDesc {
    pub num_channels: u16,
}

struct AudioBuffers {
    ports: Vec<RefCell<AudioBlockSequential<f32>>>,
}

impl AudioBuffers {
    fn new(ports: &[AudioPortDesc], num_frames: usize) -> Self {
        AudioBuffers {
            ports: ports
                .iter()
                .map(|desc| AudioBlockSequential::new(desc.num_channels, num_frames))
                .map(RefCell::new)
                .collect(),
        }
    }

    fn prepare_for_processing(&mut self, num_frames: usize) {
        for port in &mut self.ports {
            let mut port_ref = port.borrow_mut();
            if port_ref.num_frames_allocated() < num_frames {
                println!("Allocating new audio buffers for {num_frames} frames");
                let num_channels = port_ref.num_channels();
                drop(port_ref);

                port.replace(AudioBlockSequential::new(num_channels, num_frames));
            } else {
                port_ref.set_active_num_frames(num_frames);
            }
        }
    }
}

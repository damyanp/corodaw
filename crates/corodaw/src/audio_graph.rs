use std::{
    collections::HashMap,
    sync::mpsc::{Receiver, Sender, channel},
};

use clack_host::process::PluginAudioProcessor;
use cpal::SampleFormat;

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
    nodes: HashMap<NodeId, NodeDesc>,
}

impl AudioGraphWorker {
    fn new(receiver: Receiver<Message>) -> Self {
        Self {
            receiver,
            nodes: HashMap::new(),
        }
    }

    pub fn process(&mut self, data: &mut [f32]) {
        self.process_messages();
        data.fill(0.0);
    }

    fn process_messages(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                Message::AddNode { id, desc } => {
                    println!("Added node {:?}", id);
                    println!("Node inputs: ");
                    for i in desc._audio_inputs.iter().enumerate() {
                        println!("{}: {} channels", i.0, i.1._channel_count)
                    }
                    println!("Node outputs: ");
                    for i in desc._audio_outputs.iter().enumerate() {
                        println!("{}: {} channels", i.0, i.1._channel_count)
                    }

                    let previous = self.nodes.insert(id, desc);
                    assert!(previous.is_none());
                }
            }
        }
    }
}

pub struct NodeDesc {
    pub _is_generator: bool,
    pub _processor: Box<dyn Processor>,
    pub _audio_inputs: Vec<AudioPortDesc>,
    pub _audio_outputs: Vec<AudioPortDesc>,
}

pub trait Processor: Send {}

pub struct AudioPortDesc {
    pub _channel_count: u32,
    pub _sample_format: SampleFormat,
}

impl Processor for PluginAudioProcessor<ClapPlugin> {}

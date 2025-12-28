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
    AddNode(NodeDesc),
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

    pub fn add_node(
        &mut self,
        processor: Box<dyn Processor>,
        audio_inputs: Vec<AudioPortDesc>,
        audio_outputs: Vec<AudioPortDesc>,
    ) -> NodeId {
        let node_id = self.next_node_id;
        self.next_node_id.0 += 1;

        self.sender
            .send(Message::AddNode(NodeDesc {
                id: node_id,
                _processor: processor,
                _audio_inputs: audio_inputs,
                _audio_outputs: audio_outputs,
            }))
            .expect("send should not fail");

        node_id
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
                Message::AddNode(node_desc) => {
                    println!("Added node {:?}", node_desc.id);
                    let previous = self.nodes.insert(node_desc.id, node_desc);
                    assert!(previous.is_none());
                }
            }
        }
    }
}

struct NodeDesc {
    id: NodeId,
    _processor: Box<dyn Processor>,
    _audio_inputs: Vec<AudioPortDesc>,
    _audio_outputs: Vec<AudioPortDesc>,
}

pub trait Processor: Send {}

pub struct AudioPortDesc {
    _channel_count: u32,
    _sample_format: SampleFormat,
}

impl Processor for PluginAudioProcessor<ClapPlugin> {}

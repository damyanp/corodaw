use std::{
    cell::{Ref, RefCell},
    cmp::Reverse,
    collections::BinaryHeap,
    fmt::Debug,
    time::Duration,
};

use audio_blocks::{AudioBlock, AudioBlockMut, AudioBlockSequential};
use fixedbitset::FixedBitSet;

use crate::{
    Event,
    desc::{GraphDesc, NodeDesc, NodeId},
};

pub trait Processor: Send + Debug {
    fn process(
        &mut self,
        graph: &Graph,
        node: &Node,
        num_frames: usize,
        timestamp: &Duration,
        out_audio_buffers: &mut [AudioBlockSequential<f32>],
        out_event_buffers: &mut [Vec<Event>],
    );
}

pub struct Node {
    pub desc: NodeDesc,
    pub processor: RefCell<Box<dyn Processor>>,
    pub output_audio_buffers: AudioBuffers,
    pub output_event_buffers: EventBuffers,
}

impl Node {
    fn new(desc: NodeDesc, processor: Box<dyn Processor>) -> Self {
        const HARDCODED_NUM_FRAMES: usize = 1024;
        let output_audio_buffers =
            AudioBuffers::new(desc.num_audio_outputs as u16, HARDCODED_NUM_FRAMES);
        let output_event_buffers = EventBuffers::new(desc.num_event_outputs as usize);

        Self {
            desc,
            processor: RefCell::new(processor),
            output_audio_buffers,
            output_event_buffers,
        }
    }
}

pub struct Graph {
    pub(crate) nodes: Vec<Node>,
}

impl Graph {
    pub(crate) fn new(mut desc: GraphDesc, old_graph: Option<Graph>) -> Self {
        if let Some(old_graph) = old_graph {
            // Processor's can't be copied - so we take all the ones from the
            // old graph and swap them into nodes in the new graph that are
            // missing processors.
            let mut processors: Vec<_> = old_graph
                .nodes
                .into_iter()
                .map(|node| Some(node.processor.into_inner()))
                .collect();

            for (id, processor) in desc.processors.iter_mut().enumerate() {
                if processor.is_none() {
                    std::mem::swap(processor, &mut processors[id]);
                }
            }
        }

        let nodes = desc
            .nodes
            .into_iter()
            .zip(desc.processors)
            .map(|(n, p)| Node::new(n, p.unwrap()))
            .collect();

        Self { nodes }
    }

    pub fn get_node(&self, node_id: &NodeId) -> &Node {
        &self.nodes[node_id.0]
    }

    pub fn process(&mut self, node_id: NodeId, num_frames: usize, timestamp: &Duration) {
        let ordered = self.build_breadth_first_traversal(node_id);
        for node_id in ordered {
            let node = &self.nodes[node_id.0];

            node.output_audio_buffers.prepare_for_processing(num_frames);

            let mut out_audio_buffers = node.output_audio_buffers.channels.borrow_mut();
            let out_audio_buffers = out_audio_buffers.as_mut_slice();

            node.output_event_buffers.prepare_for_processing();

            let mut out_event_buffers = node.output_event_buffers.ports.borrow_mut();
            let out_event_buffers = out_event_buffers.as_mut_slice();

            node.processor.borrow_mut().process(
                self,
                node,
                num_frames,
                timestamp,
                out_audio_buffers,
                out_event_buffers,
            );
        }
    }

    fn build_breadth_first_traversal(&self, start_node: NodeId) -> Vec<NodeId> {
        let reachable = self.get_reachable_nodes(start_node);

        let mut incoming = Vec::default();
        incoming.resize(self.nodes.len(), 0);

        let mut outputs: Vec<Vec<usize>> = Vec::with_capacity(self.nodes.len());
        outputs.resize(self.nodes.len(), Vec::new());

        let mut heap: BinaryHeap<Reverse<usize>> = BinaryHeap::with_capacity(self.nodes.len());
        for id in reachable.ones() {
            let node = &self.nodes[id];
            for input in node.desc.input_nodes.iter() {
                outputs[input.0].push(id);
            }
            incoming[id] = node.desc.input_nodes.len();
            if incoming[id] == 0 {
                heap.push(Reverse(id));
            }
        }

        let mut ordered = Vec::with_capacity(self.nodes.len());

        while let Some(Reverse(node_id)) = heap.pop() {
            assert_eq!(incoming[node_id], 0);
            ordered.push(node_id);

            for input in &outputs[node_id] {
                incoming[*input] -= 1;
                if incoming[*input] == 0 {
                    heap.push(Reverse(*input));
                }
            }
        }

        ordered.into_iter().map(NodeId).collect()
    }

    pub(crate) fn get_reachable_nodes(&self, start_node: NodeId) -> FixedBitSet {
        let mut reachable = FixedBitSet::with_capacity(self.nodes.len());
        let mut stack = Vec::with_capacity(self.nodes.len());

        stack.push(start_node);
        while let Some(node) = stack.pop() {
            if !reachable.contains(node.0) {
                reachable.put(node.0);
                let node = &self.nodes[node.0];
                stack.extend_from_slice(node.desc.input_nodes.as_slice());
            }
        }

        reachable
    }
}

pub struct AudioBuffers {
    pub(crate) channels: RefCell<Vec<AudioBlockSequential<f32>>>,
}

impl AudioBuffers {
    fn new(num_channels: u16, num_frames: usize) -> Self {
        AudioBuffers {
            channels: RefCell::new(AudioBuffers::build_channels(num_channels, num_frames)),
        }
    }

    fn build_channels(num_channels: u16, num_frames: usize) -> Vec<AudioBlockSequential<f32>> {
        (0..num_channels)
            .map(|_| AudioBlockSequential::new(1, num_frames))
            .collect()
    }

    pub fn get(&self) -> Ref<'_, Vec<AudioBlockSequential<f32>>> {
        self.channels.borrow()
    }

    fn prepare_for_processing(&self, num_frames: usize) {
        let mut channels = self.channels.borrow_mut();

        if let Some(channel) = channels.first()
            && channel.num_frames_allocated() < num_frames
        {
            println!("Allocating new audio buffers for {num_frames} frames");
            *channels = AudioBuffers::build_channels(channels.len() as u16, num_frames);
        } else {
            for channel in channels.iter_mut() {
                channel.set_active_num_frames(num_frames);
            }
        }
    }
}

pub struct EventBuffers {
    pub(crate) ports: RefCell<Vec<Vec<Event>>>,
}

impl EventBuffers {
    fn new(num_ports: usize) -> Self {
        EventBuffers {
            ports: RefCell::new((0..num_ports).map(|_| Vec::new()).collect()),
        }
    }

    pub fn get(&self) -> Ref<'_, Vec<Vec<Event>>> {
        self.ports.borrow()
    }

    fn prepare_for_processing(&self) {
        for port in self.ports.borrow_mut().iter_mut() {
            port.clear();
        }
    }
}

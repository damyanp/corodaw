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

#[derive(Clone)]
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

    fn prepare_for_processing(&self, num_frames: usize) {
        for port in &self.ports {
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

#[cfg(test)]
#[allow(unused)]
mod tests {
    use std::{
        cmp::Reverse,
        collections::{BinaryHeap, VecDeque},
        pin::Pin,
        rc::Rc,
        sync::{Arc, RwLock, RwLockReadGuard},
    };

    use fixedbitset::FixedBitSet;

    use super::*;

    #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct NodeId(usize);

    #[derive(Default)]
    struct GraphDesc {
        nodes: Vec<NodeDesc>,
    }

    struct NodeDesc {
        id: NodeId,
        processor: Box<dyn Processor>,
        inputs: Vec<NodeId>,
        num_outputs: usize,
    }

    impl NodeDesc {
        fn new(id: NodeId, num_outputs: usize, processor: Box<dyn Processor>) -> Self {
            Self {
                id,
                processor,
                inputs: Vec::default(),
                num_outputs,
            }
        }
    }

    impl GraphDesc {
        fn add_node(&mut self, num_outputs: usize, processor: Box<dyn Processor>) -> NodeId {
            let id = NodeId(self.nodes.len());
            self.nodes.push(NodeDesc::new(id, num_outputs, processor));
            id
        }

        fn add_input(&mut self, node: NodeId, new_input: NodeId) {
            self.nodes[node.0].inputs.push(new_input);
        }
    }

    struct Node {
        desc: NodeDesc,
        output_buffers: AudioBuffers,
    }

    impl Node {
        fn new(desc: NodeDesc) -> Self {
            let mut ports = Vec::with_capacity(desc.num_outputs);
            ports.resize(desc.num_outputs, AudioPortDesc { num_channels: 1 });

            const HARDCODED_NUM_FRAMES: usize = 1024;
            let output_buffers = AudioBuffers::new(ports.as_slice(), HARDCODED_NUM_FRAMES);

            Self {
                desc,
                output_buffers,
            }
        }
    }

    struct Graph {
        nodes: Vec<Node>,
    }

    impl Graph {
        fn new(desc: GraphDesc) -> Self {
            let nodes = desc.nodes.into_iter().map(Node::new).collect();

            Self { nodes }
        }

        fn process(&mut self, node_id: NodeId, num_frames: usize) {
            let ordered = self.build_breadth_first_traversal(node_id);

            for node_id in ordered {
                let node = &self.nodes[node_id.0];

                node.output_buffers.prepare_for_processing(num_frames);

                let mut borrowed_output_ports: Vec<_> = node
                    .output_buffers
                    .ports
                    .iter()
                    .map(|port| port.borrow_mut())
                    .collect();

                node.desc
                    .processor
                    .process(self, node, borrowed_output_ports.as_mut_slice());
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
                for input in node.desc.inputs.iter() {
                    outputs[input.0].push(id);
                }
                incoming[id] = node.desc.inputs.len();
                if incoming[id] == 0 {
                    heap.push(Reverse(id));
                }
            }

            let mut ordered = Vec::with_capacity(self.nodes.len());

            while let Some(Reverse(node_id)) = heap.pop() {
                assert_eq!(incoming[node_id], 0);
                ordered.push(node_id);

                let node = &self.nodes[node_id];
                for input in &outputs[node_id] {
                    incoming[*input] -= 1;
                    if incoming[*input] == 0 {
                        heap.push(Reverse(*input));
                    }
                }
            }

            ordered.into_iter().map(NodeId).collect()
        }

        fn get_reachable_nodes(&self, start_node: NodeId) -> FixedBitSet {
            let mut reachable = FixedBitSet::with_capacity(self.nodes.len());
            let mut stack = Vec::with_capacity(self.nodes.len());

            stack.push(start_node);
            while let Some(node) = stack.pop() {
                if !reachable.contains(node.0) {
                    reachable.put(node.0);
                    let node = &self.nodes[node.0];
                    stack.extend_from_slice(node.desc.inputs.as_slice());
                }
            }

            reachable
        }
    }

    trait Processor: Send {
        fn process(
            &self,
            graph: &Graph,
            node: &Node,
            out_audio_buffers: &mut [RefMut<'_, AudioBlockSequential<f32>>],
        );
    }

    struct Constant(f32);
    impl Processor for Constant {
        fn process(
            &self,
            graph: &Graph,
            node: &Node,
            out_audio_buffers: &mut [RefMut<'_, AudioBlockSequential<f32>>],
        ) {
            out_audio_buffers[0].channel_mut(0)[0] = self.0;
        }
    }

    struct AddInputs;
    impl Processor for AddInputs {
        fn process(
            &self,
            graph: &Graph,
            node: &Node,
            out_audio_buffers: &mut [RefMut<'_, AudioBlockSequential<f32>>],
        ) {
            out_audio_buffers[0].channel_mut(0).fill(0.0);

            let inputs = node
                .desc
                .inputs
                .iter()
                .map(|id| &graph.nodes[id.0])
                .map(|node| node.output_buffers.ports[0].borrow());

            for input in inputs {
                for (input, mut output) in input
                    .channel(0)
                    .iter()
                    .zip(out_audio_buffers[0].channel_iter_mut(0))
                {
                    *output += *input;
                }
            }
        }
    }

    struct LogProcessor {
        log: Arc<RwLock<Vec<NodeId>>>,
    }
    impl Processor for LogProcessor {
        fn process(
            &self,
            graph: &Graph,
            node: &Node,
            out_audio_buffers: &mut [RefMut<'_, AudioBlockSequential<f32>>],
        ) {
            self.log.write().unwrap().push(node.desc.id);
        }
    }

    struct Logger {
        log: Arc<RwLock<Vec<NodeId>>>,
    }

    impl Logger {
        fn new() -> Self {
            Self {
                log: Arc::default(),
            }
        }

        fn make(&self) -> Box<LogProcessor> {
            Box::new(LogProcessor {
                log: self.log.clone(),
            })
        }

        fn get(&self) -> RwLockReadGuard<'_, Vec<NodeId>> {
            self.log.read().unwrap()
        }
    }

    #[test]
    fn graph_can_be_sent_to_thread() {
        let mut graph = GraphDesc::default();

        let node1 = graph.add_node(1, Box::new(Constant(1.0)));
        let node2 = graph.add_node(1, Box::new(Constant(2.0)));

        graph.add_input(node1, node2);

        let join = std::thread::spawn(move || graph.nodes.len());

        assert_eq!(2, join.join().unwrap());
    }

    #[test]
    fn single_node_process() {
        let logger = Logger::new();

        let mut graph = GraphDesc::default();
        let node = graph.add_node(0, logger.make());

        let mut graph = Graph::new(graph);
        graph.process(node, 1);

        assert_eq!([node], logger.get().as_slice());
    }

    #[test]
    fn reachable_nodes() {
        // 0 --> 1
        // 2 --> 3
        // 4
        let logger = Logger::new();

        let mut graph = GraphDesc::default();
        let nodes: Vec<NodeId> = (0..5).map(|_| graph.add_node(0, logger.make())).collect();
        graph.add_input(nodes[0], nodes[1]);
        graph.add_input(nodes[2], nodes[3]);

        let graph = Graph::new(graph);

        itertools::assert_equal(
            graph.get_reachable_nodes(nodes[0]).ones(),
            [0, 1].into_iter(),
        );

        itertools::assert_equal(graph.get_reachable_nodes(nodes[1]).ones(), [1].into_iter());

        itertools::assert_equal(
            graph.get_reachable_nodes(nodes[2]).ones(),
            [2, 3].into_iter(),
        );

        itertools::assert_equal(graph.get_reachable_nodes(nodes[3]).ones(), [3].into_iter());

        itertools::assert_equal(graph.get_reachable_nodes(nodes[4]).ones(), [4].into_iter());
    }

    #[test]
    fn multiple_node_process_order() {
        // d -- > a --> b
        //        \---> c

        let logger = Logger::new();

        let mut graph = GraphDesc::default();
        let a = graph.add_node(0, logger.make());
        let b = graph.add_node(0, logger.make());
        let c = graph.add_node(0, logger.make());
        let d = graph.add_node(0, logger.make());

        graph.add_input(d, a);
        graph.add_input(a, b);
        graph.add_input(a, c);

        let mut graph = Graph::new(graph);
        graph.process(d, 1);

        assert_eq!([b, c, a, d], logger.get().as_slice());
    }

    #[test]
    fn node_processing() {
        //
        // a --> b
        //   \-> c

        let mut graph = GraphDesc::default();
        let a = graph.add_node(1, Box::new(AddInputs));
        let b = graph.add_node(1, Box::new(Constant(1.0)));
        let c = graph.add_node(1, Box::new(Constant(1.0)));

        graph.add_input(a, b);
        graph.add_input(a, c);

        let mut graph = Graph::new(graph);
        graph.process(a, 1);

        assert_eq!(
            2.0,
            graph.nodes[a.0].output_buffers.ports[0].borrow().channel(0)[0]
        );
    }
}

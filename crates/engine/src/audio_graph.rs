use std::{
    cell::{Ref, RefCell},
    cmp::Reverse,
    collections::BinaryHeap,
    fmt::Debug,
    mem::swap,
    rc::Rc,
    sync::mpsc::{Receiver, Sender, channel},
};

use audio_blocks::{
    AudioBlock, AudioBlockInterleavedViewMut, AudioBlockMut, AudioBlockOps, AudioBlockSequential,
};
use fixedbitset::FixedBitSet;

pub mod clap_adapter;

/// Interface to the audio graph; cheap to clone, but must be kept on the
/// application main thread.
#[derive(Clone)]
pub struct AudioGraph {
    inner: Rc<RefCell<AudioGraphInner>>,
}

struct AudioGraphInner {
    modified: bool,
    graph_desc: GraphDesc,
    sender: Sender<AudioGraphMessage>,
}

/// This is the part of the audio graph that does audio processing, so it lives
/// on the audio thread.
pub struct AudioGraphWorker {
    receiver: Receiver<AudioGraphMessage>,
    graph: Option<Graph>,
}

enum AudioGraphMessage {
    UpdateGraph(GraphDesc),
}

impl AudioGraph {
    pub fn new() -> (AudioGraph, AudioGraphWorker) {
        let (sender, receiver) = channel();

        let audio_graph = AudioGraph {
            inner: Rc::new(RefCell::new(AudioGraphInner::new(sender))),
        };

        (audio_graph, AudioGraphWorker::new(receiver))
    }

    pub fn update(&self) {
        self.inner.borrow_mut().update();
    }

    pub fn add_node(
        &self,
        num_inputs: usize,
        num_outputs: usize,
        processor: Box<dyn Processor>,
    ) -> NodeId {
        self.inner
            .borrow_mut()
            .add_node(num_inputs, num_outputs, processor)
    }

    pub fn connect(&self, dest_node: NodeId, dest_port: usize, src_node: NodeId, src_port: usize) {
        self.inner
            .borrow_mut()
            .connect(dest_node, dest_port, src_node, src_port)
    }

    pub fn connect_grow_input(
        &self,
        dest_node: NodeId,
        dest_port: usize,
        src_node: NodeId,
        src_port: usize,
    ) {
        self.inner
            .borrow_mut()
            .connect_grow_input(dest_node, dest_port, src_node, src_port);
    }

    pub fn set_output_node(&self, node_id: NodeId) {
        self.inner.borrow_mut().set_output_node(node_id);
    }
}

impl AudioGraphInner {
    fn new(sender: Sender<AudioGraphMessage>) -> Self {
        Self {
            modified: false,
            graph_desc: GraphDesc::default(),
            sender,
        }
    }

    fn add_node(
        &mut self,
        num_inputs: usize,
        num_outputs: usize,
        processor: Box<dyn Processor>,
    ) -> NodeId {
        self.modified = true;
        self.graph_desc.add_node(num_inputs, num_outputs, processor)
    }

    fn connect(&mut self, dest_node: NodeId, dest_port: usize, src_node: NodeId, src_port: usize) {
        self.modified = true;
        self.graph_desc
            .connect(dest_node, dest_port, src_node, src_port)
    }

    fn connect_grow_input(
        &mut self,
        dest_node: NodeId,
        dest_port: usize,
        src_node: NodeId,
        src_port: usize,
    ) {
        self.modified = true;
        self.graph_desc
            .connect_grow_input(dest_node, dest_port, src_node, src_port)
    }

    pub fn set_output_node(&mut self, node_id: NodeId) {
        self.modified = true;
        self.graph_desc.set_output_node(node_id);
    }

    fn update(&mut self) {
        if self.modified {
            self.modified = false;
            self.sender
                .send(AudioGraphMessage::UpdateGraph(self.graph_desc.send()))
                .unwrap();
        }
    }
}

impl AudioGraphWorker {
    fn new(receiver: Receiver<AudioGraphMessage>) -> Self {
        Self {
            receiver,
            graph: None,
        }
    }

    pub fn tick(&mut self, channels: u16, data: &mut [f32]) {
        let mut new_graph_desc = None;

        for message in self.receiver.try_iter() {
            match message {
                AudioGraphMessage::UpdateGraph(graph_desc) => new_graph_desc = Some(graph_desc),
            }
        }

        if let Some(new_graph_desc) = new_graph_desc {
            self.graph = Some(Graph::new(new_graph_desc, self.graph.take()));
        }

        let num_frames = data.len() / channels as usize;
        let mut block = AudioBlockInterleavedViewMut::from_slice(data, channels, num_frames);

        if let Some(graph) = self.graph.as_mut() {
            let output_node_id = graph.output_node;
            graph.process(output_node_id, num_frames);

            let output_node = graph.get_node(&output_node_id);
            let output_buffers = output_node.output_buffers.get();
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

pub trait NodeCreator {
    fn create_node(&self, graph: &AudioGraph) -> NodeId;
}

#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug)]
pub struct NodeId(usize);

#[derive(Default)]
struct GraphDesc {
    nodes: Vec<NodeDesc>,
    processors: Vec<Option<Box<dyn Processor>>>,
    output_node: Option<NodeId>,
}

#[derive(Clone)]
pub struct NodeDesc {
    pub id: NodeId,
    pub input_nodes: Vec<NodeId>,
    pub input_connections: Vec<InputConnection>,
    pub num_outputs: usize,
}

#[derive(Clone, Copy, Debug)]
pub enum InputConnection {
    Disconnected,
    Connected(NodeId, usize),
}

impl NodeDesc {
    fn new(id: NodeId, num_inputs: usize, num_outputs: usize) -> Self {
        let mut input_connections = Vec::default();
        input_connections.resize(num_inputs, InputConnection::Disconnected);

        Self {
            id,
            input_nodes: Vec::default(),
            input_connections,
            num_outputs,
        }
    }
}

impl GraphDesc {
    pub fn add_node(
        &mut self,
        num_inputs: usize,
        num_outputs: usize,
        processor: Box<dyn Processor>,
    ) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(NodeDesc::new(id, num_inputs, num_outputs));
        self.processors.push(Some(processor));
        id
    }

    pub fn connect_grow_input(
        &mut self,
        dest_node: NodeId,
        dest_port: usize,
        src_node: NodeId,
        src_port: usize,
    ) {
        let dest = &mut self.nodes[dest_node.0];
        while dest.input_connections.len() <= dest_port {
            dest.input_connections.push(InputConnection::Disconnected);
        }
        self.connect(dest_node, dest_port, src_node, src_port)
    }

    pub fn connect(
        &mut self,
        dest_node: NodeId,
        dest_port: usize,
        src_node: NodeId,
        src_port: usize,
    ) {
        assert_ne!(dest_node, src_node);

        let [dest, src] = self
            .nodes
            .get_disjoint_mut([dest_node.0, src_node.0])
            .unwrap();

        assert!(dest_port < dest.input_connections.len());
        assert!(src_port < src.num_outputs);

        if !dest.input_nodes.contains(&src_node) {
            dest.input_nodes.push(src_node);
        }

        dest.input_connections[dest_port] = InputConnection::Connected(src_node, src_port);
    }

    pub fn set_output_node(&mut self, node_id: NodeId) {
        self.output_node = Some(node_id);
    }

    pub fn send(&mut self) -> Self {
        let mut processors = Vec::new();
        for _ in 0..self.processors.len() {
            processors.push(None);
        }

        swap(&mut processors, &mut self.processors);

        Self {
            nodes: self.nodes.clone(),
            processors,
            output_node: self.output_node,
        }
    }
}

pub struct Node {
    pub desc: NodeDesc,
    pub processor: Box<dyn Processor>,
    pub output_buffers: AudioBuffers,
}

impl Node {
    fn new(desc: NodeDesc, processor: Box<dyn Processor>) -> Self {
        const HARDCODED_NUM_FRAMES: usize = 1024;
        let output_buffers = AudioBuffers::new(desc.num_outputs as u16, HARDCODED_NUM_FRAMES);

        Self {
            desc,
            processor,
            output_buffers,
        }
    }
}

pub struct Graph {
    nodes: Vec<Node>,
    output_node: NodeId,
}

impl Graph {
    fn new(mut desc: GraphDesc, old_graph: Option<Graph>) -> Self {
        if let Some(old_graph) = old_graph {
            // Processor's can't be copied - so we take all the ones from the
            // old graph and swap them into nodes in the new graph that are
            // missing processors.
            let mut processors: Vec<_> = old_graph
                .nodes
                .into_iter()
                .map(|node| Some(node.processor))
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

        Self {
            nodes,
            output_node: desc.output_node.unwrap(),
        }
    }

    pub fn get_node(&self, node_id: &NodeId) -> &Node {
        &self.nodes[node_id.0]
    }

    fn process(&mut self, node_id: NodeId, num_frames: usize) {
        let ordered = self.build_breadth_first_traversal(node_id);
        for node_id in ordered {
            let node = &self.nodes[node_id.0];

            node.output_buffers.prepare_for_processing(num_frames);

            node.processor.process(
                self,
                node,
                node.output_buffers.channels.borrow_mut().as_mut_slice(),
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

    fn get_reachable_nodes(&self, start_node: NodeId) -> FixedBitSet {
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

pub trait Processor: Send + Debug {
    fn process(
        &self,
        graph: &Graph,
        node: &Node,
        out_audio_buffers: &mut [AudioBlockSequential<f32>],
    );
}

pub struct AudioBuffers {
    channels: RefCell<Vec<AudioBlockSequential<f32>>>,
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

    #[derive(Debug)]
    struct Constant(f32);
    impl Processor for Constant {
        fn process(
            &self,
            graph: &Graph,
            node: &Node,
            out_audio_buffers: &mut [AudioBlockSequential<f32>],
        ) {
            out_audio_buffers[0].channel_mut(0)[0] = self.0;
        }
    }

    #[derive(Debug)]
    struct SumInputs;

    impl Processor for SumInputs {
        fn process(
            &self,
            graph: &Graph,
            node: &Node,
            out_audio_buffers: &mut [AudioBlockSequential<f32>],
        ) {
            out_audio_buffers[0].channel_mut(0).fill(0.0);

            let inputs = node
                .desc
                .input_nodes
                .iter()
                .map(|id| &graph.nodes[id.0])
                .map(|node| node.output_buffers.channels.borrow());

            for input in inputs {
                let input = input[0].channel(0);
                for (input, mut output) in
                    input.iter().zip(out_audio_buffers[0].channel_iter_mut(0))
                {
                    *output += *input;
                }
            }
        }
    }

    #[derive(Debug)]
    struct LogProcessor {
        log: Arc<RwLock<Vec<NodeId>>>,
    }
    impl Processor for LogProcessor {
        fn process(
            &self,
            graph: &Graph,
            node: &Node,
            out_audio_buffers: &mut [AudioBlockSequential<f32>],
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

        let node1 = graph.add_node(1, 0, Box::new(Constant(1.0)));
        let node2 = graph.add_node(0, 1, Box::new(Constant(2.0)));

        graph.connect(node1, 0, node2, 0);

        let join = {
            let graph = graph.send();
            std::thread::spawn(move || graph.nodes.len())
        };

        assert_eq!(2, join.join().unwrap());
    }

    #[test]
    fn single_node_process() {
        let logger = Logger::new();

        let mut graph = GraphDesc::default();
        let node = graph.add_node(0, 0, logger.make());

        let mut graph = Graph::new(graph, None);
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
        let nodes: Vec<NodeId> = (0..5)
            .map(|_| graph.add_node(1, 1, logger.make()))
            .collect();
        graph.connect(nodes[0], 0, nodes[1], 0);
        graph.connect(nodes[2], 0, nodes[3], 0);

        let graph = Graph::new(graph, None);

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
        let a = graph.add_node(2, 1, logger.make());
        let b = graph.add_node(0, 1, logger.make());
        let c = graph.add_node(0, 1, logger.make());
        let d = graph.add_node(1, 0, logger.make());

        graph.connect(d, 0, a, 0);
        graph.connect(a, 0, b, 0);
        graph.connect(a, 1, c, 0);

        let mut graph = Graph::new(graph, None);
        graph.process(d, 1);

        assert_eq!([b, c, a, d], logger.get().as_slice());
    }

    #[test]
    fn node_processing() {
        //
        // a --> b
        //   \-> c

        let mut graph = GraphDesc::default();
        let a = graph.add_node(2, 1, Box::new(SumInputs));
        let b = graph.add_node(0, 1, Box::new(Constant(1.0)));
        let c = graph.add_node(0, 1, Box::new(Constant(1.0)));

        graph.connect(a, 0, b, 0);
        graph.connect(a, 1, c, 0);

        let mut graph = Graph::new(graph, None);
        graph.process(a, 1);

        assert_eq!(2.0, graph.nodes[a.0].output_buffers.get()[0].channel(0)[0]);
    }
}

#![allow(unused)]

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    pin::Pin,
    rc::Rc,
    sync::{Arc, RwLock, RwLockReadGuard},
    time::Duration,
};

use audio_blocks::{AudioBlockMut, AudioBlockSequential};
use fixedbitset::FixedBitSet;

use crate::desc::GraphDesc;

use super::*;

#[derive(Debug)]
struct Constant(f32);
impl Processor for Constant {
    fn process(
        &mut self,
        graph: &Graph,
        node: &Node,
        _: &Duration,
        out_audio_buffers: &mut [AudioBlockSequential<f32>],
    ) {
        out_audio_buffers[0].channel_mut(0)[0] = self.0;
    }
}

#[derive(Debug)]
struct SumInputs;

impl Processor for SumInputs {
    fn process(
        &mut self,
        graph: &Graph,
        node: &Node,
        _: &Duration,
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
            for (input, mut output) in input.iter().zip(out_audio_buffers[0].channel_iter_mut(0)) {
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
        &mut self,
        graph: &Graph,
        node: &Node,
        _: &Duration,
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
    graph.process(node, 1, &Duration::default());

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
    graph.process(d, 1, &Duration::default());

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
    graph.process(a, 1, &Duration::default());

    assert_eq!(2.0, graph.nodes[a.0].output_buffers.get()[0].channel(0)[0]);
}

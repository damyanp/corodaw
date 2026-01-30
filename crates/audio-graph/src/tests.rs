#![allow(unused)]

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    pin::Pin,
    process::Output,
    rc::Rc,
    sync::{Arc, RwLock, RwLockReadGuard},
    time::Duration,
};

use audio_blocks::{AudioBlockMut, AudioBlockSequential};
use bevy_ecs::{schedule::MultiThreadedExecutor, world::CommandQueue};
use fixedbitset::FixedBitSet;
use itertools::Itertools;
use wmidi::{Channel, MidiMessage, Note, U7};

use crate::AgEvent;

use super::*;

#[derive(Debug)]
struct Constant(f32);
impl Processor for Constant {
    fn process(
        &mut self,
        graph: &Graph,
        node: &AgNode,
        _: usize,
        _: &Duration,
        out_audio_buffers: &mut [AudioBlockSequential<f32>],
        _: &mut [Vec<AgEvent>],
    ) {
        for out in out_audio_buffers {
            out.channel_mut(0)[0] = self.0;
        }
    }
}

#[derive(Debug)]
struct SumInputs;

impl Processor for SumInputs {
    fn process(
        &mut self,
        graph: &Graph,
        node: &AgNode,
        _: usize,
        _: &Duration,
        out_audio_buffers: &mut [AudioBlockSequential<f32>],
        _: &mut [Vec<AgEvent>],
    ) {
        out_audio_buffers[0].channel_mut(0).fill(0.0);

        let inputs = node
            .desc
            .inputs
            .iter()
            .map(|id| &graph.nodes[id])
            .map(|node| node.output_audio_buffers.ports.borrow());

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
    log: Arc<RwLock<Vec<Entity>>>,
}
impl Processor for LogProcessor {
    fn process(
        &mut self,
        graph: &Graph,
        node: &AgNode,
        _: usize,
        _: &Duration,
        out_audio_buffers: &mut [AudioBlockSequential<f32>],
        _: &mut [Vec<AgEvent>],
    ) {
        self.log.write().unwrap().push(node.entity);
    }
}

struct Logger {
    log: Arc<RwLock<Vec<Entity>>>,
}

impl Logger {
    fn new() -> Self {
        Self {
            log: Arc::default(),
        }
    }

    fn make_processor(&self) -> Box<dyn Processor> {
        Box::new(LogProcessor {
            log: self.log.clone(),
        })
    }

    fn get(&self) -> RwLockReadGuard<'_, Vec<Entity>> {
        self.log.read().unwrap()
    }
}

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins(AudioGraphPlugin);
    app
}

#[test]
fn single_node_process() {
    let mut app = test_app();

    let node = app
        .world_mut()
        .spawn((node::Node::default().audio(0, 2), node::OutputNode))
        .id();

    node::set_processor(app.world_mut(), node, Box::new(Constant(1.0)));

    app.update();
    app.update();

    let mut data = [0.0, 0.0];

    let mut audio_graph_worker: AudioGraphWorker =
        app.world_mut().remove_non_send_resource().unwrap();
    audio_graph_worker.configure(2, 1);
    audio_graph_worker.tick(&mut data, Duration::default());

    assert_eq!([1.0, 1.0], data);
}

#[test]
fn reachable_nodes() {
    // 0 --> 1
    // 2 --> 3
    // 4

    let mut app = test_app();

    let nodes: Vec<Entity> = app
        .world_mut()
        .spawn_batch((0..5).map(|_| (node::Node::default().audio(1, 1),)))
        .collect();

    node::connect_audio(app.world_mut(), nodes[0], Connection::new(0, nodes[1], 0));
    node::connect_audio(app.world_mut(), nodes[2], Connection::new(0, nodes[3], 0));

    app.update();

    let mut audio_graph_worker: AudioGraphWorker =
        app.world_mut().remove_non_send_resource().unwrap();
    audio_graph_worker.configure(2, 1);
    let mut data = [0.0, 0.0];
    audio_graph_worker.tick(&mut data, Duration::default());

    let graph = audio_graph_worker.graph;

    itertools::assert_equal(
        graph.get_reachable_nodes(nodes[0]).into_iter().sorted(),
        [nodes[0], nodes[1]].into_iter().sorted(),
    );

    itertools::assert_equal(
        graph.get_reachable_nodes(nodes[1]).into_iter(),
        [nodes[1]].into_iter(),
    );

    itertools::assert_equal(
        graph.get_reachable_nodes(nodes[2]).into_iter().sorted(),
        [nodes[2], nodes[3]].into_iter().sorted(),
    );

    itertools::assert_equal(
        graph.get_reachable_nodes(nodes[3]).into_iter(),
        [nodes[3]].into_iter(),
    );

    itertools::assert_equal(
        graph.get_reachable_nodes(nodes[4]).into_iter(),
        [nodes[4]].into_iter(),
    );
}

#[test]
fn multiple_node_process_order() {
    // d -- > a --> b
    //        \---> c

    let mut app = test_app();

    let logger = Logger::new();

    let mut w = app.world_mut();
    let a = w.spawn((node::Node::default().audio(2, 1))).id();
    let b = w.spawn((node::Node::default().audio(0, 1))).id();
    let c = w.spawn((node::Node::default().audio(0, 1))).id();
    let d = w
        .spawn((node::Node::default().audio(1, 2), node::OutputNode))
        .id();

    for e in [a, b, c, d] {
        node::set_processor(w, e, logger.make_processor());
    }

    node::connect_audio(w, d, Connection::new(0, a, 0));
    node::connect_audio(w, a, Connection::new(0, b, 0));
    node::connect_audio(w, a, Connection::new(1, c, 0));

    app.update();
    let mut audio_graph_worker: AudioGraphWorker =
        app.world_mut().remove_non_send_resource().unwrap();
    audio_graph_worker.configure(2, 1);
    let mut data = [0.0, 0.0];
    audio_graph_worker.tick(&mut data, Duration::default());

    // First two can be any order
    itertools::assert_equal([b, c].iter().sorted(), logger.get()[..2].iter().sorted());

    // Rest has to be set order
    assert_eq!([a, d], logger.get()[2..]);
}

#[test]
fn always_run_nodes() {
    // Nodes marked to always run should....always run!

    // a --> b --> c
    //   \-> d
    // e
    // f
    //
    // always_run: c, d, e

    let mut app = test_app();

    let logger = Logger::new();

    let mut w = app.world_mut();
    let [a, b, f] = std::array::from_fn(|_| w.spawn((Node::default().audio(1, 2))).id());
    let [c, d, e] = std::array::from_fn(|_| w.spawn(Node::default().audio(1, 1).always_run()).id());

    w.insert_batch([(a, OutputNode)]);

    for e in [a, b, c, d, e, f] {
        set_processor(w, e, logger.make_processor());
    }

    connect_audio(w, a, Connection::new(0, b, 0));
    connect_audio(w, a, Connection::new(0, d, 0));
    connect_audio(w, b, Connection::new(0, c, 0));

    app.update();
    let mut audio_graph_worker: AudioGraphWorker =
        app.world_mut().remove_non_send_resource().unwrap();
    audio_graph_worker.configure(2, 1);
    let mut data = [0.0, 0.0];
    audio_graph_worker.tick(&mut data, Duration::default());

    let order = logger.get();

    assert!(is_before(&order, c, b));
    assert!(is_before(&order, b, a));
    assert!(is_before(&order, d, a));
    assert!(order.contains(&e));
    assert!(!order.contains(&f));
}

fn is_before(order: &Vec<Entity>, entity: Entity, is_before: Entity) -> bool {
    let entity_index = order.iter().find_position(|e| **e == entity).unwrap();
    let is_before_index = order.iter().find_position(|e| **e == is_before).unwrap();

    entity_index < is_before_index
}

#[test]
fn node_processing() {
    //
    // a --> b
    //   \-> c

    let mut app = test_app();
    let mut w = app.world_mut();

    let a = w
        .spawn((node::Node::default().audio(2, 2), node::OutputNode))
        .id();
    node::set_processor(w, a, Box::new(SumInputs));

    let [b, c] = w
        .spawn_batch((0..2).map(|_| (node::Node::default().audio(0, 1),)))
        .collect::<Vec<_>>()[..2]
        .try_into()
        .unwrap();

    for e in [b, c] {
        node::set_processor(w, e, Box::new(Constant(1.0)));
    }

    node::connect_audio(w, a, Connection::new(0, b, 0));
    node::connect_audio(w, a, Connection::new(1, c, 0));

    app.update();

    let mut audio_graph_worker: AudioGraphWorker =
        app.world_mut().remove_non_send_resource().unwrap();
    audio_graph_worker.configure(2, 1);
    let mut data = [0.0, 0.0];
    audio_graph_worker.tick(&mut data, Duration::default());

    assert_eq!(2.0, data[0]);
}

#[derive(Debug)]
struct EventSource {
    events: VecDeque<crate::AgEvent>,
}
impl Processor for EventSource {
    fn process(
        &mut self,
        graph: &Graph,
        node: &AgNode,
        _: usize,
        timestamp: &Duration,
        out_audio_buffers: &mut [AudioBlockSequential<f32>],
        out_event_buffers: &mut [Vec<AgEvent>],
    ) {
        out_event_buffers[0].extend(self.events.iter().cloned());
    }
}

impl EventSource {
    fn make_processor(events: &[crate::AgEvent]) -> Box<dyn Processor> {
        let events = VecDeque::from_iter(events.iter().cloned());

        Box::new(EventSource { events: events })
    }
}

#[derive(Debug)]
struct EventSink {
    events: Arc<RwLock<VecDeque<crate::AgEvent>>>,
}
impl Processor for EventSink {
    fn process(
        &mut self,
        graph: &Graph,
        node: &AgNode,
        _: usize,
        timestamp: &Duration,
        out_audio_buffers: &mut [AudioBlockSequential<f32>],
        _: &mut [Vec<crate::AgEvent>],
    ) {
        for input_connection in &node.desc.event_ports.connections {
            let input_node = graph.get_node(input_connection.src);
            let input_events = &input_node.output_event_buffers.get()[input_connection.src_port];

            self.events
                .write()
                .unwrap()
                .extend(input_events.iter().cloned());
        }
    }
}

impl EventSink {
    fn make_processor(events: Arc<RwLock<VecDeque<crate::AgEvent>>>) -> Box<dyn Processor> {
        Box::new(EventSink {
            events: events.clone(),
        })
    }
}

fn new_test_midi_message(n: u8) -> MidiMessage<'static> {
    MidiMessage::Reserved(n)
}

#[test]
fn events_output_to_single_input() {
    let mut app = test_app();
    let mut w = app.world_mut();

    let events = vec![
        AgEvent {
            timestamp: Duration::from_micros(1),
            midi: new_test_midi_message(1),
        },
        AgEvent {
            timestamp: Duration::from_micros(2),
            midi: new_test_midi_message(2),
        },
    ];

    let events_sink: Arc<RwLock<VecDeque<AgEvent>>> = Arc::default();

    let source = w.spawn((node::Node::default().event(0, 1),)).id();
    node::set_processor(w, source, EventSource::make_processor(&events));

    let sink = w
        .spawn((
            node::Node::default().event(1, 0).audio(0, 2),
            node::OutputNode,
        ))
        .id();
    node::set_processor(w, sink, EventSink::make_processor(events_sink.clone()));

    node::connect_event(w, sink, Connection::new(0, source, 0)).unwrap();

    app.update();

    let mut audio_graph_worker: AudioGraphWorker =
        app.world_mut().remove_non_send_resource().unwrap();
    audio_graph_worker.configure(2, 1);
    let mut data = [0.0, 0.0];
    audio_graph_worker.tick(&mut data, Duration::default());

    assert_eq!(
        events_sink
            .read()
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        events
    );
}

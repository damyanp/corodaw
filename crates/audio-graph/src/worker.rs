#![allow(unused)]

use std::{
    cell::RefCell,
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, HashSet, hash_map::Entry},
    fmt::Debug,
    time::Duration,
};

use audio_blocks::AudioBlockSequential;
use bevy_ecs::{entity::Entity, system::RunSystemOnce, world::World};
use fixedbitset::FixedBitSet;

use crate::{AgEvent, node};

mod buffers;
pub use crate::worker::buffers::{AudioBuffers, EventBuffers};

pub struct ProcessContext<'a> {
    pub graph: &'a Graph,
    pub node: &'a AgNode,
    pub num_frames: usize,
    pub timestamp: &'a Duration,
    pub out_audio_buffers: &'a mut AudioBlockSequential<f32>,
    pub out_event_buffers: &'a mut [Vec<AgEvent>],
}

pub trait Processor: Send + Debug {
    fn process(&mut self, ctx: ProcessContext);
}

#[derive(Default)]
pub(crate) struct Processors {
    processors: HashMap<Entity, Box<dyn Processor>>,
}

impl Processors {
    fn get_mut(&mut self, entity: Entity) -> &mut dyn Processor {
        self.processors.get_mut(&entity).unwrap().as_mut()
    }

    pub(crate) fn set(&mut self, entity: Entity, processor: Box<dyn Processor>) {
        let _ = self.processors.insert(entity, processor);
    }
}

pub struct AgNode {
    pub entity: Entity,
    pub desc: node::Node,
    pub output_audio_buffers: AudioBuffers,
    pub output_event_buffers: EventBuffers,
}

impl AgNode {
    fn new(entity: Entity, desc: node::Node) -> Self {
        const HARDCODED_NUM_FRAMES: usize = 1024;
        let output_audio_buffers =
            AudioBuffers::new(desc.audio_channels.num_outputs, HARDCODED_NUM_FRAMES);
        let output_event_buffers = EventBuffers::new(desc.event_channels.num_outputs as usize);

        Self {
            entity,
            desc,
            output_audio_buffers,
            output_event_buffers,
        }
    }
}

#[derive(Default)]
pub struct Graph {
    pub(crate) nodes: HashMap<Entity, AgNode>,
    pub(crate) processors: RefCell<Processors>,
}

impl Graph {
    pub(crate) fn update(&mut self, changed: Vec<(Entity, node::Node)>) {
        for (entity, node) in changed {
            match self.nodes.entry(entity) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().desc = node;
                }
                Entry::Vacant(entry) => {
                    entry.insert(AgNode::new(entity, node));
                }
            };
        }
    }

    pub fn get_node(&self, node_entity: Entity) -> &AgNode {
        &self.nodes[&node_entity]
    }

    pub fn process(&mut self, node_entity: Entity, num_frames: usize, timestamp: &Duration) {
        let ordered = self.build_breadth_first_traversal(node_entity);
        for node_entity in ordered {
            let node = self.get_node(node_entity);

            node.output_audio_buffers.prepare_for_processing(num_frames);

            let mut out_audio_buffers = node.output_audio_buffers.buffers.borrow_mut();

            node.output_event_buffers.prepare_for_processing();

            let mut out_event_buffers = node.output_event_buffers.ports.borrow_mut();
            let out_event_buffers = out_event_buffers.as_mut_slice();

            let mut processors = self.processors.borrow_mut();
            let processor = processors.get_mut(node_entity);

            processor.process(ProcessContext {
                graph: self,
                node,
                num_frames,
                timestamp,
                out_audio_buffers: &mut out_audio_buffers,
                out_event_buffers,
            });
        }
    }

    fn build_breadth_first_traversal(&self, start_node: Entity) -> Vec<Entity> {
        let reachable = self.get_reachable_nodes(start_node);

        let mut incoming: HashMap<Entity, usize> = HashMap::with_capacity(self.nodes.len());

        let mut outputs: HashMap<Entity, Vec<Entity>> = HashMap::with_capacity(self.nodes.len());

        let mut heap: BinaryHeap<Reverse<Entity>> = BinaryHeap::with_capacity(self.nodes.len());
        for node_entity in reachable.iter() {
            let node = self.get_node(*node_entity);
            for input_entity in node.desc.inputs.iter() {
                if !outputs.contains_key(input_entity) {
                    outputs.insert(*input_entity, Vec::default());
                }
                outputs.get_mut(input_entity).unwrap().push(*node_entity);
            }
            incoming.insert(*node_entity, node.desc.inputs.len());

            if *incoming.get(node_entity).unwrap_or(&0) == 0 {
                heap.push(Reverse(*node_entity));
            }
        }

        let mut ordered = Vec::with_capacity(self.nodes.len());

        while let Some(Reverse(node_entity)) = heap.pop() {
            assert_eq!(incoming[&node_entity], 0);
            ordered.push(node_entity);

            if let Some(outputs) = outputs.get(&node_entity) {
                for input in outputs {
                    *incoming.get_mut(input).unwrap() -= 1;
                    if incoming[input] == 0 {
                        heap.push(Reverse(*input));
                    }
                }
            }
        }

        ordered
    }

    pub(crate) fn get_reachable_nodes(&self, start_node: Entity) -> HashSet<Entity> {
        let mut reachable = HashSet::with_capacity(self.nodes.len());
        let mut stack = Vec::with_capacity(self.nodes.len());

        for (entity, node) in self.nodes.iter() {
            if node.desc.always_run {
                stack.push(*entity);
            }
        }

        stack.push(start_node);
        while let Some(node) = stack.pop() {
            if !reachable.contains(&node) {
                reachable.insert(node);
                let node = self.get_node(node);
                stack.extend_from_slice(node.desc.inputs.as_slice());
            }
        }

        reachable
    }
}

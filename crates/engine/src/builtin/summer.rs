use bevy_ecs::prelude::*;
use std::time::Duration;

use audio_blocks::AudioBlockSequential;

use audio_graph::{AgEvent, AgNode, Graph, Node, Processor};

pub struct Summer {
    pub entity: Entity,
}

impl Summer {
    pub fn new(world: &mut World, num_channels: u16) -> Self {
        let entity = world
            .spawn(Node::default().audio(num_channels, num_channels))
            .id();

        audio_graph::set_processor(world, entity, Box::new(SummerProcessor));

        Self { entity }
    }
}

#[derive(Debug)]
struct SummerProcessor;
impl Processor for SummerProcessor {
    fn process(
        &mut self,
        graph: &Graph,
        node: &AgNode,
        _: usize,
        _: &Duration,
        out_audio_buffers: &mut AudioBlockSequential<f32>,
        _: &mut [Vec<AgEvent>],
    ) {
        for (output_channel, output_buffer) in out_audio_buffers.channels_mut().enumerate() {
            output_buffer.fill(0.0);

            let inputs = node
                .desc
                .audio_channels
                .connections
                .iter()
                .filter(|c| c.channel == output_channel as u16);

            for input in inputs {
                let input_node = graph.get_node(input.src);
                let input_buffers = input_node.output_audio_buffers.get();
                let input_buffer = &input_buffers.channel(input.src_channel);

                for (input, output) in input_buffer.iter().zip(output_buffer.iter_mut()) {
                    *output += *input;
                }
            }
        }
    }
}

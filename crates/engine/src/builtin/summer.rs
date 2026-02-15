use bevy_ecs::prelude::*;

use audio_graph::{GraphNodeDesc, GraphProcessContext, GraphProcessor};

pub struct SummerOwner {
    pub entity: Entity,
}

impl SummerOwner {
    pub fn new(world: &mut World, num_channels: u16) -> Self {
        let entity = world
            .spawn(GraphNodeDesc::default().audio(num_channels, num_channels))
            .id();

        audio_graph::graph_set_processor(world, entity, Box::new(SummerProcessor));

        Self { entity }
    }
}

#[derive(Debug)]
struct SummerProcessor;
impl GraphProcessor for SummerProcessor {
    fn process(&mut self, ctx: GraphProcessContext) {
        for (output_channel, output_buffer) in ctx.out_audio_buffers.channels_mut().enumerate() {
            output_buffer.fill(0.0);

            let inputs = ctx
                .node
                .desc
                .audio_channels
                .connections
                .iter()
                .filter(|c| c.channel == output_channel as u16);

            for input in inputs {
                let Some(input_node) = ctx.graph.get_node(input.src) else {
                    continue;
                };
                let input_buffers = input_node.output_audio_buffers.get();
                let input_buffer = &input_buffers.channel(input.src_channel);

                for (input, output) in input_buffer.iter().zip(output_buffer.iter_mut()) {
                    *output += *input;
                }
            }
        }
    }
}

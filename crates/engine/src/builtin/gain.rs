use audio_blocks::AudioBlock;
use bevy_ecs::prelude::*;
use crossbeam::channel::{self, Receiver, Sender};

use audio_graph::{
    GraphConnection, GraphNodeDesc, GraphProcessContext, GraphProcessor, GraphStateValue,
};

use crate::builtin::peak::PeakMeter;

#[derive(Debug)]
pub struct GainNodeOwner {
    pub entity: Entity,
    sender: Sender<f32>,
}

impl GainNodeOwner {
    pub fn new(commands: &mut Commands, initial_gain: f32) -> Self {
        let (sender, receiver) = channel::unbounded();

        let entity = commands.spawn(GraphNodeDesc::default().audio(2, 2)).id();

        commands.queue(move |world: &mut World| {
            audio_graph::graph_set_processor(
                world,
                entity,
                Box::new(GainProcessor {
                    receiver,
                    gain: initial_gain,
                    vu_meters: Default::default(),
                }),
            );
        });

        GainNodeOwner { entity, sender }
    }

    pub fn set_gain(&self, gain: f32) {
        self.sender.send(gain).unwrap();
    }
}

#[derive(Debug)]
struct GainProcessor {
    receiver: Receiver<f32>,
    gain: f32,
    vu_meters: Vec<PeakMeter>,
}

impl GraphProcessor for GainProcessor {
    fn process(&mut self, ctx: GraphProcessContext) {
        self.process_messages();

        let num_channels = ctx.out_audio_buffers.num_channels();
        self.vu_meters
            .resize_with(num_channels as usize, Default::default);

        for ((output_channel, output_buffer), vu_meter) in ctx
            .out_audio_buffers
            .channels_mut()
            .enumerate()
            .zip(self.vu_meters.iter_mut())
        {
            output_buffer.fill(0.0);

            for GraphConnection {
                channel,
                src,
                src_channel,
            } in &ctx.node.desc.audio_channels.connections
            {
                if *channel == output_channel as u16 {
                    let Some(input_node) = ctx.graph.get_node(*src) else {
                        continue;
                    };
                    let input_buffers = input_node.output_audio_buffers.get();
                    let input_buffer = input_buffers.channel(*src_channel);

                    for (input, output) in input_buffer.iter().zip(output_buffer.iter_mut()) {
                        *output += *input * self.gain;
                    }
                }
            }

            vu_meter.update(ctx.sample_rate, output_buffer.iter().as_slice());
        }

        let value = match self.vu_meters.len() {
            0 => GraphStateValue::None,
            1 => GraphStateValue::Mono(self.vu_meters[0].value()),
            _ => GraphStateValue::Stereo(self.vu_meters[0].value(), self.vu_meters[1].value()),
        };
        ctx.state.insert(ctx.node.entity, value);
    }
}

impl GainProcessor {
    fn process_messages(&mut self) {
        while let Ok(gain) = self.receiver.try_recv() {
            self.gain = gain;
        }
    }
}

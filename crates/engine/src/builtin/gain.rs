use bevy_ecs::prelude::*;
use crossbeam::channel::{self, Receiver, Sender};
use std::time::Duration;

use audio_blocks::AudioBlockSequential;

use audio_graph::{AgEvent, AgNode, Connection, Graph, Node, Processor};

#[derive(Debug)]
pub struct GainControl {
    pub entity: Entity,
    sender: Sender<f32>,
}

impl GainControl {
    pub fn new(commands: &mut Commands, initial_gain: f32) -> Self {
        let (sender, receiver) = channel::unbounded();

        let entity = commands.spawn(Node::default().audio(2, 2)).id();

        commands.queue(move |world: &mut World| {
            audio_graph::set_processor(
                world,
                entity,
                Box::new(GainControlProcessor {
                    receiver,
                    gain: initial_gain,
                }),
            );
        });

        GainControl { entity, sender }
    }

    pub fn set_gain(&self, gain: f32) {
        self.sender.send(gain).unwrap();
    }
}

#[derive(Debug)]
struct GainControlProcessor {
    receiver: Receiver<f32>,
    gain: f32,
}

impl Processor for GainControlProcessor {
    fn process(
        &mut self,
        graph: &Graph,
        node: &AgNode,
        _: usize,
        _: &Duration,
        out_audio_buffers: &mut AudioBlockSequential<f32>,
        _: &mut [Vec<AgEvent>],
    ) {
        self.process_messages();

        for (output_channel, output_buffer) in out_audio_buffers.channels_mut().enumerate() {
            output_buffer.fill(0.0);

            for Connection {
                channel,
                src,
                src_channel,
            } in &node.desc.audio_channels.connections
            {
                if *channel == output_channel as u16 {
                    let input_node = graph.get_node(*src);
                    let input_buffers = input_node.output_audio_buffers.get();
                    let input_buffer = input_buffers.channel(*src_channel);

                    for (input, output) in input_buffer.iter().zip(output_buffer.iter_mut()) {
                        *output += *input * self.gain;
                    }
                }
            }
        }
    }
}

impl GainControlProcessor {
    fn process_messages(&mut self) {
        while let Ok(gain) = self.receiver.try_recv() {
            self.gain = gain;
        }
    }
}

use std::{
    cell::Cell,
    sync::mpsc::{Receiver, Sender, channel},
};

use audio_blocks::{AudioBlock, AudioBlockMut, AudioBlockOps, AudioBlockSequential};

use audio_graph::{AudioGraph, Graph, InputConnection, Node, NodeId, Processor};

pub struct GainControl {
    pub node_id: NodeId,
    sender: Sender<f32>,
}

impl GainControl {
    pub fn new(graph: &AudioGraph, initial_gain: f32) -> GainControl {
        let (sender, receiver) = channel();

        let processor = Box::new(GainControlProcessor {
            receiver,
            gain: Cell::new(initial_gain),
        });

        let node_id = graph.add_node(2, 2, processor);

        GainControl { node_id, sender }
    }

    pub fn set_gain(&self, gain: f32) {
        self.sender.send(gain).unwrap();
    }
}

#[derive(Debug)]
struct GainControlProcessor {
    receiver: Receiver<f32>,
    gain: Cell<f32>,
}

impl Processor for GainControlProcessor {
    fn process(
        &self,
        graph: &Graph,
        node: &Node,
        out_audio_buffers: &mut [AudioBlockSequential<f32>],
    ) {
        self.process_messages();

        let gain = self.gain.get();

        for (channel, output_buffer) in out_audio_buffers.iter_mut().enumerate() {
            let input_connection = node.desc.input_connections[channel];
            if let InputConnection::Connected(input_node_id, input_channel) = input_connection {
                let input_node = graph.get_node(&input_node_id);
                let input_buffers = input_node.output_buffers.get();
                let input_buffer = &input_buffers[input_channel];

                for (input, output) in input_buffer
                    .channel_iter(0)
                    .zip(output_buffer.channel_iter_mut(0))
                {
                    *output = *input * gain;
                }
            } else {
                output_buffer.fill_with(0.0);
            }
        }
    }
}

impl GainControlProcessor {
    fn process_messages(&self) {
        if let Ok(gain) = self.receiver.try_recv() {
            self.gain.set(gain);
        }
    }
}

#[derive(Debug)]
pub struct Summer;
impl Processor for Summer {
    fn process(
        &self,
        graph: &Graph,
        node: &Node,
        out_audio_buffers: &mut [AudioBlockSequential<f32>],
    ) {
        for (channel, output_buffer) in out_audio_buffers.iter_mut().enumerate() {
            output_buffer.fill_with(0.0);

            let inputs = node.desc.input_connections.iter().filter(|c| {
                if let InputConnection::Connected(_, n) = c {
                    *n == channel
                } else {
                    false
                }
            });

            for input in inputs {
                if let InputConnection::Connected(input_node, input_channel) = input {
                    let input_node = graph.get_node(input_node);
                    let input_buffers = input_node.output_buffers.get();
                    let input_buffer = &input_buffers[*input_channel];

                    for (input, output) in input_buffer
                        .channel_iter(0)
                        .zip(output_buffer.channel_iter_mut(0))
                    {
                        *output += *input;
                    }
                }
            }
        }
    }
}

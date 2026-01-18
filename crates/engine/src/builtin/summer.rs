use std::time::Duration;

use audio_blocks::{AudioBlock, AudioBlockMut, AudioBlockOps, AudioBlockSequential};

use audio_graph::{Event, Graph, InputConnection, Node, Processor};

#[derive(Debug)]
pub struct Summer;
impl Processor for Summer {
    fn process(
        &mut self,
        graph: &Graph,
        node: &Node,
        _: &Duration,
        out_audio_buffers: &mut [AudioBlockSequential<f32>],
        _: &mut [Vec<Event>],
    ) {
        for (channel, output_buffer) in out_audio_buffers.iter_mut().enumerate() {
            output_buffer.fill_with(0.0);

            let inputs = node.desc.audio_input_connections.iter().filter(|c| {
                if let InputConnection::Connected(_, n) = c {
                    *n == channel
                } else {
                    false
                }
            });

            for input in inputs {
                if let InputConnection::Connected(input_node, input_channel) = input {
                    let input_node = graph.get_node(input_node);
                    let input_buffers = input_node.output_audio_buffers.get();
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

use std::time::Duration;

use audio_blocks::{AudioBlock, AudioBlockMut, AudioBlockOps, AudioBlockSequential};

use audio_graph::{
    AudioGraph, Event, Graph, InputConnection, Node, NodeDescBuilder, NodeId, Processor,
};

pub struct Summer {
    pub node_id: NodeId,
}

impl Summer {
    pub fn new(graph: &mut AudioGraph, num_channels: usize) -> Self {
        let processor = Box::new(SummerProcessor);
        let node_id = graph.add_node(NodeDescBuilder::default().audio(0, num_channels), processor);
        Self { node_id }
    }
}

#[derive(Debug)]
struct SummerProcessor;
impl Processor for SummerProcessor {
    fn process(
        &mut self,
        graph: &Graph,
        node: &Node,
        _: usize,
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

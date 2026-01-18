use std::{
    sync::mpsc::{Receiver, Sender, channel},
    time::Duration,
};

use audio_blocks::{AudioBlock, AudioBlockMut, AudioBlockOps, AudioBlockSequential};
use derivative::Derivative;

use audio_graph::{
    AudioGraph, Event, Graph, InputConnection, Node, NodeDescBuilder, NodeId, Processor,
};

#[derive(Derivative)]
#[derivative(Debug)]
pub struct GainControl {
    pub node_id: NodeId,
    sender: Sender<f32>,
}

impl GainControl {
    pub fn new(graph: &AudioGraph, initial_gain: f32) -> Self {
        let (sender, receiver) = channel();

        let processor = Box::new(GainControlProcessor {
            receiver,
            gain: initial_gain,
        });

        let node_id = graph.add_node(NodeDescBuilder::default().audio(2, 2), processor);

        GainControl { node_id, sender }
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
        node: &Node,
        _: usize,
        _: &Duration,
        out_audio_buffers: &mut [AudioBlockSequential<f32>],
        _: &mut [Vec<Event>],
    ) {
        self.process_messages();

        for (channel, output_buffer) in out_audio_buffers.iter_mut().enumerate() {
            let input_connection = node.desc.audio_input_connections[channel];
            if let InputConnection::Connected(input_node_id, input_channel) = input_connection {
                let input_node = graph.get_node(&input_node_id);
                let input_buffers = input_node.output_audio_buffers.get();
                let input_buffer = &input_buffers[input_channel];

                for (input, output) in input_buffer
                    .channel_iter(0)
                    .zip(output_buffer.channel_iter_mut(0))
                {
                    *output = *input * self.gain;
                }
            } else {
                output_buffer.fill_with(0.0);
            }
        }
    }
}

impl GainControlProcessor {
    fn process_messages(&mut self) {
        if let Ok(gain) = self.receiver.try_recv() {
            self.gain = gain;
        }
    }
}

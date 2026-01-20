use audio_blocks::{AudioBlock, AudioBlockInterleavedViewMut, AudioBlockMut, AudioBlockOps};

use crate::{
    desc::{AudioGraphDescError, GraphDesc, NodeDescBuilder, NodeId},
    worker::{Graph, Processor},
};
use std::{
    sync::mpsc::{Receiver, Sender, channel},
    time::Duration,
};

pub struct AudioGraph {
    modified: bool,
    graph_desc: GraphDesc,
    sender: Sender<AudioGraphMessage>,
}

/// This is the part of the audio graph that does audio processing, so it lives
/// on the audio thread.
pub struct AudioGraphWorker {
    receiver: Receiver<AudioGraphMessage>,
    num_channels: u16,
    sample_rate: u32,
    graph: Option<Graph>,
    output_node_id: Option<NodeId>,
}

enum AudioGraphMessage {
    UpdateGraph(GraphDesc),
}

impl AudioGraph {
    pub fn new() -> (AudioGraph, AudioGraphWorker) {
        let (sender, receiver) = channel();

        let audio_graph = AudioGraph {
            modified: false,
            graph_desc: Default::default(),
            sender,
        };

        (audio_graph, AudioGraphWorker::new(receiver))
    }

    pub fn add_node(
        &mut self,
        node_desc_builder: NodeDescBuilder,
        processor: Box<dyn Processor>,
    ) -> NodeId {
        self.modified = true;
        self.graph_desc.add_node(node_desc_builder, processor)
    }

    pub fn connect_audio(
        &mut self,
        dest_node: NodeId,
        dest_port: usize,
        src_node: NodeId,
        src_port: usize,
    ) -> Result<(), AudioGraphDescError> {
        self.modified = true;
        self.graph_desc
            .connect_audio(dest_node, dest_port, src_node, src_port)
    }

    pub fn connect_audio_add_input(
        &mut self,
        dest_node: NodeId,
        src_node: NodeId,
        src_port: usize,
    ) -> Result<(), AudioGraphDescError> {
        self.modified = true;
        self.graph_desc
            .connect_audio_add_input(dest_node, src_node, src_port)
    }

    pub fn connect_event(
        &mut self,
        dest_node: NodeId,
        dest_port: usize,
        src_node: NodeId,
        src_port: usize,
    ) -> Result<(), AudioGraphDescError> {
        self.modified = true;
        self.graph_desc
            .connect_event(dest_node, dest_port, src_node, src_port)
    }

    pub fn disconnect_event(
        &mut self,
        dest_node: NodeId,
        dest_port: usize,
    ) -> Result<(), AudioGraphDescError> {
        self.modified = true;
        self.graph_desc.disconnect_event(dest_node, dest_port)
    }

    pub fn add_input_node(
        &mut self,
        dest_node: NodeId,
        src_node: NodeId,
    ) -> Result<(), AudioGraphDescError> {
        self.modified = true;
        self.graph_desc.add_input_node(dest_node, src_node)
    }

    pub fn set_output_node(&mut self, node_id: NodeId) -> Result<(), AudioGraphDescError> {
        self.modified = true;
        self.graph_desc.set_output_node(node_id)
    }

    pub fn update(&mut self) {
        if self.modified {
            self.modified = false;
            self.sender
                .send(AudioGraphMessage::UpdateGraph(self.graph_desc.send()))
                .unwrap();
        }
    }
}

impl AudioGraphWorker {
    fn new(receiver: Receiver<AudioGraphMessage>) -> Self {
        Self {
            receiver,
            graph: None,
            output_node_id: None,
            num_channels: 0,
            sample_rate: 0,
        }
    }

    pub fn configure(&mut self, channels: u16, sample_rate: u32) {
        self.num_channels = channels;
        self.sample_rate = sample_rate;
    }

    pub fn tick(&mut self, data: &mut [f32], timestamp: Duration) {
        let mut new_graph_desc = None;

        for message in self.receiver.try_iter() {
            match message {
                AudioGraphMessage::UpdateGraph(graph_desc) => {
                    new_graph_desc = Some(graph_desc);
                }
            }
        }

        if let Some(new_graph_desc) = new_graph_desc {
            self.output_node_id = new_graph_desc.output_node_id;
            self.graph = Some(Graph::new(new_graph_desc, self.graph.take()));
        }

        let num_frames = data.len() / self.num_channels as usize;
        let mut block =
            AudioBlockInterleavedViewMut::from_slice(data, self.num_channels, num_frames);

        if let Some(graph) = self.graph.as_mut()
            && let Some(output_node_id) = self.output_node_id
        {
            graph.process(output_node_id, num_frames, &timestamp);

            let output_node = graph.get_node(&output_node_id);
            let output_buffers = output_node.output_audio_buffers.get();
            let a = &output_buffers[0];
            let b = &output_buffers[1];

            assert_eq!(1, a.num_channels());
            assert_eq!(1, b.num_channels());

            let frames_dest = block.frames_iter_mut();
            let frames_a = a.frames_iter();
            let frames_b = b.frames_iter();

            for (mut dest, (mut a, mut b)) in frames_dest.zip(frames_a.zip(frames_b)) {
                *dest.next().unwrap() = *a.next().unwrap();
                *dest.next().unwrap() = *b.next().unwrap();
                assert!(a.next().is_none());
                assert!(b.next().is_none());
                assert!(dest.next().is_none());
            }
        } else {
            block.fill_with(0.0);
        }
    }
}

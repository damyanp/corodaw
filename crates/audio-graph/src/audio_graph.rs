use audio_blocks::{AudioBlock, AudioBlockInterleavedViewMut, AudioBlockMut, AudioBlockOps};

use crate::{
    desc::{GraphDesc, NodeDescBuilder, NodeId},
    worker::{Graph, Processor},
};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc::{Receiver, Sender, channel},
    time::Duration,
};

pub trait NodeCreator {
    fn create_node(&self, graph: &AudioGraph) -> NodeId;
}

/// Interface to the audio graph; cheap to clone, but must be kept on the
/// application main thread.
#[derive(Clone)]
pub struct AudioGraph {
    inner: Rc<RefCell<AudioGraphInner>>,
}

struct AudioGraphInner {
    modified: bool,
    graph_desc: GraphDesc,
    sender: Sender<AudioGraphMessage>,
}

/// This is the part of the audio graph that does audio processing, so it lives
/// on the audio thread.
pub struct AudioGraphWorker {
    receiver: Receiver<AudioGraphMessage>,
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
            inner: Rc::new(RefCell::new(AudioGraphInner::new(sender))),
        };

        (audio_graph, AudioGraphWorker::new(receiver))
    }

    pub fn update(&self) {
        self.inner.borrow_mut().update();
    }

    pub fn add_node(
        &self,
        node_desc_builder: NodeDescBuilder,
        processor: Box<dyn Processor>,
    ) -> NodeId {
        self.inner
            .borrow_mut()
            .add_node(node_desc_builder, processor)
    }

    pub fn connect(&self, dest_node: NodeId, dest_port: usize, src_node: NodeId, src_port: usize) {
        self.inner
            .borrow_mut()
            .connect(dest_node, dest_port, src_node, src_port)
    }

    pub fn connect_grow_inputs(
        &self,
        dest_node: NodeId,
        dest_port: usize,
        src_node: NodeId,
        src_port: usize,
    ) {
        self.inner
            .borrow_mut()
            .connect_grow_input(dest_node, dest_port, src_node, src_port);
    }

    pub fn set_output_node(&self, node_id: NodeId) {
        self.inner.borrow_mut().set_output_node(node_id);
    }

    pub fn add_input_node(&self, summer: NodeId, midi_input: NodeId) {
        self.inner.borrow_mut().add_input_node(summer, midi_input)
    }
}

impl AudioGraphInner {
    fn new(sender: Sender<AudioGraphMessage>) -> Self {
        Self {
            modified: false,
            graph_desc: GraphDesc::default(),
            sender,
        }
    }

    fn add_node(
        &mut self,
        node_desc_builder: NodeDescBuilder,
        processor: Box<dyn Processor>,
    ) -> NodeId {
        self.modified = true;
        self.graph_desc.add_node(node_desc_builder, processor)
    }

    fn connect(&mut self, dest_node: NodeId, dest_port: usize, src_node: NodeId, src_port: usize) {
        self.modified = true;
        self.graph_desc
            .connect(dest_node, dest_port, src_node, src_port)
    }

    fn connect_grow_input(
        &mut self,
        dest_node: NodeId,
        dest_port: usize,
        src_node: NodeId,
        src_port: usize,
    ) {
        self.modified = true;
        self.graph_desc
            .connect_grow_input(dest_node, dest_port, src_node, src_port)
    }

    pub fn add_input_node(&mut self, dest_node: NodeId, src_node: NodeId) {
        self.modified = true;
        self.graph_desc.add_input_node(dest_node, src_node);
    }

    pub fn set_output_node(&mut self, node_id: NodeId) {
        self.modified = true;
        self.graph_desc.set_output_node(node_id);
    }

    fn update(&mut self) {
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
        }
    }

    pub fn tick(&mut self, channels: u16, data: &mut [f32], timestamp: Duration) {
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

        let num_frames = data.len() / channels as usize;
        let mut block = AudioBlockInterleavedViewMut::from_slice(data, channels, num_frames);

        if let Some(graph) = self.graph.as_mut()
            && let Some(output_node_id) = self.output_node_id
        {
            graph.process(output_node_id, num_frames, &timestamp);

            let output_node = graph.get_node(&output_node_id);
            let output_buffers = output_node.output_buffers.get();
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

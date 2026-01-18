use crate::worker::Processor;

#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug)]
pub struct NodeId(pub(crate) usize);

#[derive(Default)]
pub struct GraphDesc {
    pub(crate) nodes: Vec<NodeDesc>,
    pub(crate) processors: Vec<Option<Box<dyn Processor>>>,
    pub(crate) output_node_id: Option<NodeId>,
}

#[derive(Clone)]
pub struct NodeDesc {
    pub id: NodeId,
    pub input_nodes: Vec<NodeId>,
    pub audio_input_connections: Vec<InputConnection>,
    pub event_input_connections: Vec<InputConnection>,
    pub num_audio_outputs: usize,
    pub num_event_outputs: usize,
}

#[derive(Clone, Copy, Debug)]
pub enum InputConnection {
    Disconnected,
    Connected(NodeId, usize),
}

impl NodeDesc {
    fn new(id: NodeId, node_desc_builder: NodeDescBuilder) -> Self {
        let mut audio_input_connections = Vec::default();
        audio_input_connections.resize(
            node_desc_builder.num_audio_inputs,
            InputConnection::Disconnected,
        );
        let num_audio_outputs = node_desc_builder.num_audio_outputs;

        let mut event_input_connections = Vec::default();
        event_input_connections.resize(
            node_desc_builder.num_event_inputs,
            InputConnection::Disconnected,
        );
        let num_event_outputs = node_desc_builder.num_event_outputs;

        Self {
            id,
            input_nodes: Vec::default(),
            audio_input_connections,
            event_input_connections,
            num_audio_outputs,
            num_event_outputs,
        }
    }

    fn add_input_node(&mut self, node: &NodeId) {
        if !self.input_nodes.contains(node) {
            self.input_nodes.push(*node);
        }
    }
}

#[derive(Default)]
pub struct NodeDescBuilder {
    num_audio_inputs: usize,
    num_audio_outputs: usize,
    num_event_inputs: usize,
    num_event_outputs: usize,
}

impl NodeDescBuilder {
    pub fn audio(self, num_audio_inputs: usize, num_audio_outputs: usize) -> Self {
        Self {
            num_audio_inputs,
            num_audio_outputs,
            ..self
        }
    }

    pub fn event(self, num_event_inputs: usize, num_event_outputs: usize) -> Self {
        Self {
            num_event_inputs,
            num_event_outputs,
            ..self
        }
    }
}

impl GraphDesc {
    pub fn add_node(
        &mut self,
        node_desc_builder: NodeDescBuilder,
        processor: Box<dyn Processor>,
    ) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(NodeDesc::new(id, node_desc_builder));
        self.processors.push(Some(processor));
        id
    }

    pub fn connect_audio_grow_inputs(
        &mut self,
        dest_node: NodeId,
        dest_port: usize,
        src_node: NodeId,
        src_port: usize,
    ) {
        let dest = &mut self.nodes[dest_node.0];
        while dest.audio_input_connections.len() <= dest_port {
            dest.audio_input_connections
                .push(InputConnection::Disconnected);
        }
        self.connect_audio(dest_node, dest_port, src_node, src_port)
    }

    pub fn connect_audio(
        &mut self,
        dest_node: NodeId,
        dest_port: usize,
        src_node: NodeId,
        src_port: usize,
    ) {
        assert_ne!(dest_node, src_node);

        let [dest, src] = self
            .nodes
            .get_disjoint_mut([dest_node.0, src_node.0])
            .unwrap();

        assert!(dest_port < dest.audio_input_connections.len());
        assert!(src_port < src.num_audio_outputs);

        dest.add_input_node(&src_node);

        dest.audio_input_connections[dest_port] = InputConnection::Connected(src_node, src_port);
    }

    pub fn add_input_node(&mut self, dest_node: NodeId, src_node: NodeId) {
        let dest = self.nodes.get_mut(dest_node.0).unwrap();
        dest.add_input_node(&src_node);
    }

    pub fn set_output_node(&mut self, node_id: NodeId) {
        self.output_node_id = Some(node_id);
    }

    pub fn send(&mut self) -> Self {
        let mut processors = Vec::new();
        for _ in 0..self.processors.len() {
            processors.push(None);
        }

        std::mem::swap(&mut processors, &mut self.processors);

        Self {
            nodes: self.nodes.clone(),
            processors,
            output_node_id: self.output_node_id,
        }
    }
}

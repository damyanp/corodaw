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
    pub input_connections: Vec<InputConnection>,
    pub num_outputs: usize,
}

#[derive(Clone, Copy, Debug)]
pub enum InputConnection {
    Disconnected,
    Connected(NodeId, usize),
}

impl NodeDesc {
    fn new(id: NodeId, num_inputs: usize, num_outputs: usize) -> Self {
        let mut input_connections = Vec::default();
        input_connections.resize(num_inputs, InputConnection::Disconnected);

        Self {
            id,
            input_nodes: Vec::default(),
            input_connections,
            num_outputs,
        }
    }
}

impl GraphDesc {
    pub fn add_node(
        &mut self,
        num_inputs: usize,
        num_outputs: usize,
        processor: Box<dyn Processor>,
    ) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(NodeDesc::new(id, num_inputs, num_outputs));
        self.processors.push(Some(processor));
        id
    }

    pub fn connect_grow_input(
        &mut self,
        dest_node: NodeId,
        dest_port: usize,
        src_node: NodeId,
        src_port: usize,
    ) {
        let dest = &mut self.nodes[dest_node.0];
        while dest.input_connections.len() <= dest_port {
            dest.input_connections.push(InputConnection::Disconnected);
        }
        self.connect(dest_node, dest_port, src_node, src_port)
    }

    pub fn connect(
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

        assert!(dest_port < dest.input_connections.len());
        assert!(src_port < src.num_outputs);

        if !dest.input_nodes.contains(&src_node) {
            dest.input_nodes.push(src_node);
        }

        dest.input_connections[dest_port] = InputConnection::Connected(src_node, src_port);
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

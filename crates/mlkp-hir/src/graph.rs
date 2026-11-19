// CEP:FILE: crates/mlkp-hir/src/graph.rs
// CEP:WHAT: The heterogeneous Sea of Nodes graph: nodes carry device placement and PGO annotations, edges carry cross-device communication costs, and no artificial ordering is imposed beyond data dependencies.
// CEP:WHY: Design section IX makes this IR the substrate everything operates on: the autotuner reasons about cross-device tradeoffs natively, passes preserve structure so the pruner can reason about legality, and PGO annotations let the cost model predict accurately.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Graph construction returns GraphError on duplicate node references; the verifier rejects dangling edges, arity mismatches, undeclared cross-device transfers, and cycles.
// CEP:ASSUMES: node ids are densely assigned by the builder; the graph is a DAG for v1 (control flow lives in kernel plans, not the dataflow graph).
// CEP:COST: cold; graph operations run during compilation.
// CEP:EVIDENCE: unit tests in crates/mlkp-hir/tests/hir_test.rs.

#![deny(missing_docs)]

use cep_hash::Hasher;
use mlkp_tensor::{DimExpr, TensorType};

/// Errors from graph construction and verification.
#[derive(Clone, Debug, PartialEq)]
pub enum GraphError {
    /// A node referenced an id that does not exist.
    UnknownNode(u32),
    /// A node id was used twice.
    DuplicateNode(u32),
    /// An operand count violated the operation's arity.
    ArityMismatch(String),
    /// A cross-device edge lacked a declared communication cost.
    MissingCommCost(u32, u32),
}

impl std::fmt::Display for GraphError {
    // CEP:WHAT: Formats the graph error.
    // CEP:WHY: Deterministic diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::UnknownNode(id) => write!(f, "graph error: unknown node {id}"),
            GraphError::DuplicateNode(id) => write!(f, "graph error: duplicate node {id}"),
            GraphError::ArityMismatch(s) => write!(f, "graph error: {s}"),
            GraphError::MissingCommCost(a, b) => {
                write!(f, "graph error: edge {a}->{b} lacks communication cost")
            }
        }
    }
}

impl std::error::Error for GraphError {}

/// Logical device placement for one node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    /// A CPU worker; the u32 is the worker index.
    Cpu(u32),
    /// A GPU device; the u32 is the device index.
    Gpu(u32),
    /// A cluster node in a distributed setting.
    Cluster(u32),
}

/// A node of the sea of nodes.
#[derive(Clone, Debug)]
pub struct Node {
    /// Stable id within the graph.
    pub id: u32,
    /// The operation.
    pub op: Op,
    /// Operand node ids in data-dependency order.
    pub inputs: Vec<u32>,
    /// Device placement.
    pub device: Device,
    /// Result type.
    pub ty: TensorType,
    /// PGO annotation.
    pub pgo: PgoAnnot,
    /// Name for diagnostics.
    pub name: String,
}

/// Heterogeneous IR operations. Semantic identity from the Tensor IR is preserved via
/// the node name; physical decisions (layout, tiling, scheduling) are made when lowering
/// out of this IR, never here.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Graph input.
    Input,
    /// Constant tensor; payload is bit-exact f32 values.
    Const(Vec<f32>),
    /// Elementwise map.
    Map(mlkp_tensor::MapFn),
    /// Reduction.
    Reduce {
        /// Combiner.
        op: mlkp_tensor::ReduceOp,
        /// Reduced axes.
        axes: Vec<u32>,
    },
    /// Generalized contraction (matmul refines here but keeps its semantic id).
    Contract,
    /// Softmax composite.
    Softmax {
        /// Normalized axis.
        axis: u32,
    },
    /// Broadcast of singleton axes.
    Broadcast {
        /// Broadcast axes.
        axes: Vec<u32>,
    },
    /// Axis permutation.
    Permute {
        /// New order.
        order: Vec<u32>,
    },
    /// Logical reshape.
    Reshape {
        /// New dims.
        dims: Vec<DimExpr>,
    },
}

/// PGO annotation attached to one node.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PgoAnnot {
    /// Observed execution count.
    pub exec_count: u64,
    /// Observed shape specializations as (dims, frequency) pairs.
    pub shape_obs: Vec<(Vec<u64>, u32)>,
    /// Measured cost in nanoseconds per execution, when known.
    pub measured_cost_ns: Option<u64>,
    /// Estimated memory traffic in bytes, when known.
    pub mem_traffic: Option<u64>,
}

/// The heterogeneous dataflow graph.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    nodes: Vec<Node>,
    /// Output node ids in return order.
    pub outputs: Vec<u32>,
    /// Input node ids in parameter order.
    pub inputs: Vec<u32>,
    /// Cross-device communication costs in nanoseconds for the declared edge.
    comm_costs: Vec<((u32, u32), u64)>,
}

impl Graph {
    /// Adds a node and returns its id.
    // CEP:WHAT: Adds one node to the graph.
    // CEP:WHY: Dense id assignment keeps operand lookups cheap.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns DuplicateNode when an id is reused.
    // CEP:ASSUMES: ids are assigned densely from zero.
    // CEP:COST: O(1) amortized.
    // CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
    pub fn add(&mut self, node: Node) -> Result<u32, GraphError> {
        if self.nodes.iter().any(|n| n.id == node.id) {
            return Err(GraphError::DuplicateNode(node.id));
        }
        let id = node.id;
        self.nodes.push(node);
        Ok(id)
    }

    /// Reads one node.
    // CEP:WHAT: Reads one node.
    // CEP:WHY: Passes and the verifier need read access.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None for unknown ids.
    // CEP:ASSUMES: none
    // CEP:COST: O(n) linear scan; graphs are small at this level.
    // CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Mutates one node.
    // CEP:WHAT: Mutates one node.
    // CEP:WHY: Passes rewrite annotations and operands in place.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None for unknown ids.
    // CEP:ASSUMES: none
    // CEP:COST: O(n).
    // CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
    pub fn node_mut(&mut self, id: u32) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// Number of nodes.
    // CEP:WHAT: Counts nodes.
    // CEP:WHY: Pass logging and limits.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True when the graph has no nodes.
    // CEP:WHAT: Emptiness check.
    // CEP:WHY: Driver fast paths.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Iterates nodes in insertion order.
    // CEP:WHAT: Iterates nodes.
    // CEP:WHY: Passes traverse the whole sea.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1) per node.
    // CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
    pub fn iter(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter()
    }

    /// Retains nodes matching the predicate, preserving order.
    // CEP:WHAT: Retains nodes matching a predicate.
    // CEP:WHY: DCE removes dead nodes in place without renumbering survivors.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: callers fix up dangling operand references after removal.
    // CEP:COST: O(nodes).
    // CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
    pub fn nodes_retain(&mut self, pred: impl Fn(&Node) -> bool) {
        self.nodes.retain(pred);
    }

    /// Mutably iterates all nodes.
    // CEP:WHAT: Mutably iterates nodes.
    // CEP:WHY: Retargeting passes rewrite operand lists in bulk.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1) per node.
    // CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
    pub fn iter_mut_nodes(&mut self) -> impl Iterator<Item = &mut Node> {
        self.nodes.iter_mut()
    }

    /// Declares a communication cost for a cross-device edge.
    // CEP:WHAT: Declares one edge cost.
    // CEP:WHY: Design section IX: cross-device edges carry communication cost metadata from measured PGO numbers, not theoretical bandwidth.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1) amortized.
    // CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
    pub fn declare_comm_cost(&mut self, from: u32, to: u32, nanos: u64) {
        self.comm_costs.push(((from, to), nanos));
    }

    /// Reads the declared cost of one edge.
    // CEP:WHAT: Reads one edge cost.
    // CEP:WHY: The verifier rejects undeclared cross-device edges and the cost model consumes the numbers.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when undeclared.
    // CEP:ASSUMES: none
    // CEP:COST: O(edges).
    // CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
    pub fn comm_cost(&self, from: u32, to: u32) -> Option<u64> {
        self.comm_costs
            .iter()
            .find(|((a, b), _)| *a == from && *b == to)
            .map(|(_, v)| *v)
    }

    /// Structural hash of the whole graph: op kinds, shapes, devices, and edge structure.
    // CEP:WHAT: Folds the graph structure into one 64-bit hash.
    // CEP:WHY: The compilation cache keys on the subgraph hash; the hash must be deterministic across processes.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: node ids are stable within one graph version.
    // CEP:COST: O(nodes + edges).
    // CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
    pub fn hash64(&self, seed: u64) -> u64 {
        let mut h = Hasher::seeded(seed);
        h.update_u64(self.nodes.len() as u64);
        for n in &self.nodes {
            hash_node(&mut h, n);
            for inp in &n.inputs {
                h.update_u64(u64::from(*inp));
            }
        }
        for ((a, b), v) in &self.comm_costs {
            h.update_u64(u64::from(*a));
            h.update_u64(u64::from(*b));
            h.update_u64(*v);
        }
        h.finish()
    }
}

// CEP:ID: JIT-785
// CEP:WHAT: Folds one node's operation, device, and type into a hasher.
// CEP:WHY: Shared by the graph hash and the CSE structural hash so their semantics cannot drift.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(node payload).
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
fn hash_node(h: &mut Hasher, n: &Node) {
    h.update_u64(u64::from(n.id));
    h.update_str(n.name.as_str());
    match &n.op {
        Op::Input => {
            h.update_u64(1);
        }
        Op::Const(v) => {
            h.update_u64(2);
            h.update_u64(cep_hash::hash_f32_slice(v, h.snapshot()));
        }
        Op::Map(f) => {
            h.update_u64(3);
            h.update_u64(map_tag(*f));
        }
        Op::Reduce { op, axes } => {
            h.update_u64(4);
            h.update_u64(reduce_tag(*op));
            for a in axes {
                h.update_u64(u64::from(*a));
            }
        }
        Op::Contract => {
            h.update_u64(5);
        }
        Op::Softmax { axis } => {
            h.update_u64(6);
            h.update_u64(u64::from(*axis));
        }
        Op::Broadcast { axes } => {
            h.update_u64(7);
            for a in axes {
                h.update_u64(u64::from(*a));
            }
        }
        Op::Permute { order } => {
            h.update_u64(8);
            for a in order {
                h.update_u64(u64::from(*a));
            }
        }
        Op::Reshape { dims } => {
            h.update_u64(9);
            for d in dims {
                d.hash_into(h);
            }
        }
    }
    // CEP:WHAT: Folds the device kind tag and index separately.
    // CEP:WHY: A single offset would collide across kinds (Cpu(256) == Gpu(0)); the cache key must distinguish placements.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
    h.update_u64(match n.device {
        Device::Cpu(_) => 0x100,
        Device::Gpu(_) => 0x200,
        Device::Cluster(_) => 0x300,
    });
    h.update_u64(match n.device {
        Device::Cpu(i) | Device::Gpu(i) | Device::Cluster(i) => u64::from(i),
    });
    let th = n.ty.hash64(h.snapshot());
    h.update_u64(th);
}

// CEP:WHAT: Stable discriminants for map functions.
// CEP:WHY: Hashing needs numeric tags without relying on enum layout.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
fn map_tag(f: mlkp_tensor::MapFn) -> u64 {
    match f {
        mlkp_tensor::MapFn::Id => 1,
        mlkp_tensor::MapFn::Relu => 2,
        mlkp_tensor::MapFn::Sigmoid => 3,
        mlkp_tensor::MapFn::Exp => 4,
        mlkp_tensor::MapFn::Neg => 5,
        mlkp_tensor::MapFn::Add => 6,
        mlkp_tensor::MapFn::Mul => 7,
        mlkp_tensor::MapFn::Sub => 8,
        mlkp_tensor::MapFn::Max => 9,
        mlkp_tensor::MapFn::Div => 10,
        mlkp_tensor::MapFn::Sub2 => 11,
    }
}

// CEP:WHAT: Stable discriminants for reduction kinds.
// CEP:WHY: Hashing needs numeric tags without relying on enum layout.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
fn reduce_tag(op: mlkp_tensor::ReduceOp) -> u64 {
    match op {
        mlkp_tensor::ReduceOp::Sum => 1,
        mlkp_tensor::ReduceOp::Max => 2,
        mlkp_tensor::ReduceOp::Min => 3,
    }
}

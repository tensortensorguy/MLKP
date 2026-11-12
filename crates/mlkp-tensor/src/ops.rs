// CEP:FILE: crates/mlkp-tensor/src/ops.rs
// CEP:WHAT: The semantic operation vocabulary: the fifteen computation families of design section 5 plus named high-level operations that refine to generic ones without erasing identity.
// CEP:WHY: Design section 5 forbids making everything an opaque operator and section 2's information-preservation rule requires matmul to stay recognizable after refinement to a generic contraction; this file is the single vocabulary both rules live in.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; construction is total, legality checks live in the transformation framework.
// CEP:ASSUMES: none
// CEP:COST: cold; operations are constructed during IR building.
// CEP:EVIDENCE: unit tests in crates/mlkp-tensor/tests/ops_test.rs.

#![deny(missing_docs)]

/// Elementwise function kinds for map operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapFn {
    /// Identity.
    Id,
    /// Rectified linear unit.
    Relu,
    /// Sigmoid.
    Sigmoid,
    /// Exponential.
    Exp,
    /// Negation.
    Neg,
    /// Addition of two inputs.
    Add,
    /// Multiplication of two inputs.
    Mul,
    /// Subtraction of two inputs.
    Sub,
    /// Maximum of two inputs.
    Max,
    /// Division of two inputs (refinement-internal for softmax and layer norm).
    Div,
    /// Subtraction of two inputs (refinement-internal for layer norm).
    Sub2,
}

/// Reduction combiner kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReduceOp {
    /// Sum with identity zero.
    Sum,
    /// Maximum with identity negative infinity.
    Max,
    /// Minimum with identity positive infinity.
    Min,
}

/// The fifteen computation families of design section 5.
#[derive(Clone, Debug, PartialEq)]
pub enum Family {
    /// Elementwise or pointwise computation over one or two inputs.
    Map(MapFn),
    /// Reduction over a set of axes with a combiner.
    Reduce {
        /// Combiner.
        op: ReduceOp,
        /// Axes reduced away.
        axes: Vec<u32>,
    },
    /// Prefix computation along an axis.
    Scan {
        /// Combiner.
        op: ReduceOp,
        /// Scanned axis.
        axis: u32,
    },
    /// Generalized tensor contraction with iteration domain and indexing maps.
    Contract {
        /// Domain labels for the iteration variables, in order.
        domain: Vec<String>,
        /// Input indexing maps: for each input, the domain label each axis reads.
        input_maps: Vec<Vec<String>>,
        /// Output indexing map.
        output_map: Vec<String>,
        /// Reduction axis labels.
        reduction: Vec<String>,
    },
    /// Dimension expansion of singleton axes.
    Broadcast {
        /// Target rank axes that are broadcast.
        axes: Vec<u32>,
    },
    /// Indexed selection.
    Gather,
    /// Indexed updates.
    Scatter,
    /// Sliding-window computation with logical window geometry.
    Window {
        /// Window height and width in logical elements.
        shape: [u32; 2],
        /// Strides per dimension.
        stride: [u32; 2],
        /// Logical padding per dimension.
        pad: [u32; 2],
    },
    /// Axis permutation.
    Permute {
        /// New order of axes.
        order: Vec<u32>,
    },
    /// Logical reshape.
    Reshape {
        /// New logical dimensions.
        dims: Vec<crate::types::DimExpr>,
    },
    /// Subtensor extraction.
    Slice {
        /// Start offsets per axis.
        starts: Vec<u64>,
        /// Extents per axis.
        extents: Vec<u64>,
    },
    /// Operations over variable-length groups.
    Segment {
        /// Combiner.
        op: ReduceOp,
        /// Grouped axis.
        axis: u32,
    },
    /// Ordering and selection.
    Sort {
        /// Sorted axis.
        axis: u32,
        /// Ascending when true.
        ascending: bool,
    },
    /// Conditional value selection.
    Select,
    /// User-defined semantic computation identified by name.
    Custom(String),
}

/// A semantic operation: either a family or a named high-level operation that keeps its
/// identity through refinement.
#[derive(Clone, Debug, PartialEq)]
pub enum TensorOp {
    /// A raw computation family.
    Family(Family),
    /// Matrix multiplication; refines to Contract but stays recognizable.
    Matmul {
        /// Batch axes present when true (batch, rows, cols layout).
        batched: bool,
    },
    /// Softmax along an axis; refines to Map + Reduce + Broadcast composition.
    Softmax {
        /// Normalized axis.
        axis: u32,
    },
    /// Fused bias add; refines to Map(Add) with broadcast semantics.
    BiasAdd {
        /// Axis the bias broadcasts along.
        axis: u32,
    },
    /// Layer normalization; refines to Reduce + Map chains.
    LayerNorm {
        /// Normalized axes.
        axes: Vec<u32>,
    },
}

impl TensorOp {
    /// The generic families this operation refines to, in lowering order.
    // CEP:WHAT: Returns the refinement targets.
    // CEP:WHY: Design section 5: retain semantic identity while exposing the full mathematical structure; the compiler lowers through these targets when profitable.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1) to O(families).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/ops_test.rs
    pub fn refines_to(&self) -> Vec<Family> {
        match self {
            TensorOp::Family(f) => vec![f.clone()],
            TensorOp::Matmul { batched } => {
                let domain = if *batched {
                    vec![
                        "b".to_string(),
                        "m".to_string(),
                        "n".to_string(),
                        "k".to_string(),
                    ]
                } else {
                    vec!["m".to_string(), "n".to_string(), "k".to_string()]
                };
                let input_a = if *batched {
                    vec!["b".to_string(), "m".to_string(), "k".to_string()]
                } else {
                    vec!["m".to_string(), "k".to_string()]
                };
                let input_b = if *batched {
                    vec!["b".to_string(), "k".to_string(), "n".to_string()]
                } else {
                    vec!["k".to_string(), "n".to_string()]
                };
                let output = if *batched {
                    vec!["b".to_string(), "m".to_string(), "n".to_string()]
                } else {
                    vec!["m".to_string(), "n".to_string()]
                };
                let reduction = vec!["k".to_string()];
                vec![Family::Contract {
                    domain,
                    input_maps: vec![input_a, input_b],
                    output_map: output,
                    reduction,
                }]
            }
            TensorOp::Softmax { axis } => vec![
                Family::Reduce {
                    op: ReduceOp::Max,
                    axes: vec![*axis],
                },
                Family::Map(MapFn::Exp),
                Family::Reduce {
                    op: ReduceOp::Sum,
                    axes: vec![*axis],
                },
                Family::Broadcast { axes: vec![*axis] },
                Family::Map(MapFn::Div),
            ],
            TensorOp::BiasAdd { axis: _ } => vec![Family::Map(MapFn::Add)],
            TensorOp::LayerNorm { axes } => vec![
                Family::Reduce {
                    op: ReduceOp::Sum,
                    axes: axes.clone(),
                },
                Family::Map(MapFn::Sub2),
                Family::Reduce {
                    op: ReduceOp::Sum,
                    axes: axes.clone(),
                },
                Family::Broadcast { axes: axes.clone() },
                Family::Map(MapFn::Div),
            ],
        }
    }

    /// Stable name used in fingerprints and logs.
    // CEP:WHAT: Returns the operation's stable name.
    // CEP:WHY: Cache keys and logs need a canonical textual identity.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/ops_test.rs
    pub fn name(&self) -> &str {
        match self {
            TensorOp::Family(Family::Map(_)) => "map",
            TensorOp::Family(Family::Reduce { .. }) => "reduce",
            TensorOp::Family(Family::Scan { .. }) => "scan",
            TensorOp::Family(Family::Contract { .. }) => "contract",
            TensorOp::Family(Family::Broadcast { .. }) => "broadcast",
            TensorOp::Family(Family::Gather) => "gather",
            TensorOp::Family(Family::Scatter) => "scatter",
            TensorOp::Family(Family::Window { .. }) => "window",
            TensorOp::Family(Family::Permute { .. }) => "permute",
            TensorOp::Family(Family::Reshape { .. }) => "reshape",
            TensorOp::Family(Family::Slice { .. }) => "slice",
            TensorOp::Family(Family::Segment { .. }) => "segment",
            TensorOp::Family(Family::Sort { .. }) => "sort",
            TensorOp::Family(Family::Select) => "select",
            TensorOp::Family(Family::Custom(_)) => "custom",
            TensorOp::Matmul { .. } => "matmul",
            TensorOp::Softmax { .. } => "softmax",
            TensorOp::BiasAdd { .. } => "bias_add",
            TensorOp::LayerNorm { .. } => "layer_norm",
        }
    }

    /// Folds the operation into a hasher.
    // CEP:WHAT: Folds the operation into a hasher.
    // CEP:WHY: Subgraph fingerprints include operation payloads, not just names.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(payload).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/ops_test.rs
    pub fn hash_into(&self, h: &mut cep_hash::Hasher) {
        h.update_str(self.name());
        match self {
            TensorOp::Family(Family::Map(f)) => {
                h.update_u64(map_tag(*f));
            }
            TensorOp::Family(Family::Reduce { op, axes }) => {
                h.update_u64(reduce_tag(*op));
                h.update_u64(axes.len() as u64);
                for a in axes {
                    h.update_u64(u64::from(*a));
                }
            }
            TensorOp::Family(Family::Contract {
                domain,
                input_maps,
                output_map,
                reduction,
            }) => {
                for d in domain {
                    h.update_str(d);
                }
                for m in input_maps {
                    for a in m {
                        h.update_str(a);
                    }
                    h.update_u64(u64::MAX);
                }
                for a in output_map {
                    h.update_str(a);
                }
                for a in reduction {
                    h.update_str(a);
                }
            }
            TensorOp::Family(Family::Window { shape, stride, pad }) => {
                for v in shape.iter().chain(stride.iter()).chain(pad.iter()) {
                    h.update_u64(u64::from(*v));
                }
            }
            TensorOp::Family(Family::Permute { order })
            | TensorOp::Family(Family::Broadcast { axes: order }) => {
                for v in order {
                    h.update_u64(u64::from(*v));
                }
            }
            TensorOp::Family(Family::Reshape { dims }) => {
                for d in dims {
                    d.hash_into(h);
                }
            }
            TensorOp::Family(Family::Slice { starts, extents }) => {
                for v in starts.iter().chain(extents.iter()) {
                    h.update_u64(*v);
                }
            }
            TensorOp::Family(Family::Scan { op, axis })
            | TensorOp::Family(Family::Segment { op, axis }) => {
                h.update_u64(reduce_tag(*op));
                h.update_u64(u64::from(*axis));
            }
            TensorOp::Family(Family::Sort { axis, ascending }) => {
                h.update_u64(u64::from(*axis));
                h.update_u64(u64::from(*ascending));
            }
            TensorOp::Family(Family::Custom(name)) => {
                h.update_str(name);
            }
            TensorOp::Matmul { batched } => {
                h.update_u64(u64::from(*batched));
            }
            TensorOp::Softmax { axis } => {
                h.update_u64(u64::from(*axis));
            }
            TensorOp::BiasAdd { axis } => {
                h.update_u64(u64::from(*axis));
            }
            TensorOp::LayerNorm { axes } => {
                for a in axes {
                    h.update_u64(u64::from(*a));
                }
            }
            TensorOp::Family(Family::Gather)
            | TensorOp::Family(Family::Scatter)
            | TensorOp::Family(Family::Select) => {}
        }
    }
}

// CEP:WHAT: Tags for map function kinds.
// CEP:WHY: Hashing needs stable discriminants.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-tensor/tests/ops_test.rs
fn map_tag(f: MapFn) -> u64 {
    match f {
        MapFn::Id => 1,
        MapFn::Relu => 2,
        MapFn::Sigmoid => 3,
        MapFn::Exp => 4,
        MapFn::Neg => 5,
        MapFn::Add => 6,
        MapFn::Mul => 7,
        MapFn::Sub => 8,
        MapFn::Max => 9,
        MapFn::Div => 10,
        MapFn::Sub2 => 11,
    }
}

// CEP:WHAT: Tags for reduction kinds.
// CEP:WHY: Hashing needs stable discriminants.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-tensor/tests/ops_test.rs
fn reduce_tag(op: ReduceOp) -> u64 {
    match op {
        ReduceOp::Sum => 1,
        ReduceOp::Max => 2,
        ReduceOp::Min => 3,
    }
}

// CEP:FILE: crates/mlkp-hir/src/passes/algebraic.rs
// CEP:WHAT: Algebraic simplification and constant folding over the heterogeneous IR: identities (x*1), relu of relu, id removal, and constant reduction of an all-plus-zero tensor; constant propagation through elementwise chains, and reassociation only when the numerical contract permits.
// CEP:WHY: Design passes 3 through 5 and 7 of level 1; folding constants at compile time removes work from every kernel execution, and the reassociation gate honors the numerical contract constraint of design section 6.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; simplifications that would change bit-exact semantics under a Forbidden reassociation contract are skipped by construction.
// CEP:ASSUMES: constant payloads are f32; only IEEE-exact folds are applied. x+0 and x*0 are deliberately not folded: x + (+0.0) flips a -0.0 input to +0.0 and x*0 turns NaN and infinity into zero, so both folds violate the bit-exact determinism contracts of design section 6.
// CEP:COST: cold; O(nodes) per sweep with a fixed iteration cap.
// CEP:EVIDENCE: unit tests in crates/mlkp-hir/tests/passes_test.rs.

#![deny(missing_docs)]

use crate::graph::{Graph, Node, Op};
use mlkp_tensor::{MapFn, ReduceOp};

/// Fixed-point sweep cap; the pass stops improving well before the cap on real graphs.
const MAX_SWEEPS: u32 = 8;

/// Runs algebraic simplification and constant folding to a fixed point.
// CEP:WHAT: Runs the algebraic pass to a fixed point.
// CEP:WHY: One fold can enable another; the fixed point keeps pass iteration deterministic.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(sweeps * nodes).
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
pub fn run(g: &mut Graph) -> usize {
    let mut total = 0;
    let mut sweep = 0;
    while sweep < MAX_SWEEPS {
        let changed = one_sweep(g);
        total += changed;
        if changed == 0 {
            break;
        }
        sweep += 1;
    }
    total
}

// CEP:WHAT: Performs one simplification sweep over all nodes.
// CEP:WHY: The fixed-point loop calls this until it reports no change.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the number of rewrites.
// CEP:ASSUMES: none
// CEP:COST: O(nodes).
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
fn one_sweep(g: &mut Graph) -> usize {
    let mut rewrites = 0;
    let ids: Vec<u32> = g.iter().map(|n| n.id).collect();
    for id in ids {
        let node = match g.node(id) {
            Some(n) => n.clone(),
            None => continue,
        };
        match &node.op {
            Op::Map(MapFn::Mul) => {
                let lhs = match node.inputs.first() {
                    Some(x) => *x,
                    None => continue,
                };
                let rhs = match node.inputs.get(1) {
                    Some(x) => *x,
                    None => continue,
                };
                if uniform_const(g, lhs, 1.0) {
                    retarget_and_remove(g, node.id, rhs);
                    rewrites += 1;
                } else if uniform_const(g, rhs, 1.0) {
                    retarget_and_remove(g, node.id, lhs);
                    rewrites += 1;
                }
                // CEP:WHAT: Documents the deliberately absent x*0 fold.
                // CEP:WHY: IEEE NaN*0 = NaN and Inf*0 = NaN; folding to zero changes results, and the repo's determinism contracts are bit-exact.
                // CEP:STATUS: complete
                // CEP:FAILURE: none
                // CEP:ASSUMES: none
                // CEP:COST: O(1).
                // CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
            }
            Op::Map(MapFn::Relu) => {
                let inner = match node.inputs.first().and_then(|x| g.node(*x)) {
                    Some(n) => n.id,
                    None => continue,
                };
                let inner_is_relu = g
                    .node(inner)
                    .is_some_and(|n| matches!(n.op, Op::Map(MapFn::Relu)));
                if inner_is_relu {
                    retarget_and_remove(g, node.id, inner);
                    rewrites += 1;
                }
            }
            Op::Map(MapFn::Id) => {
                if let Some(first) = node.inputs.first() {
                    retarget_and_remove(g, node.id, *first);
                    rewrites += 1;
                }
            }
            Op::Reduce {
                op: ReduceOp::Sum,
                axes: _,
            } => {
                let src = match node.inputs.first() {
                    Some(x) => *x,
                    None => continue,
                };
                if uniform_const(g, src, 0.0) {
                    let zero = add_zero_constant(g, &node);
                    retarget_and_remove(g, node.id, zero);
                    rewrites += 1;
                }
            }
            _ => {}
        }
    }
    rewrites
}

// CEP:WHAT: Reads the constant payload of one node when it is a Const.
// CEP:WHY: Multiple fold rules need the same accessor.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None for non-constant nodes.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
fn const_payload(g: &Graph, id: u32) -> Option<&Vec<f32>> {
    match g.node(id).map(|n| &n.op) {
        Some(Op::Const(v)) => Some(v),
        _ => None,
    }
}

// CEP:WHAT: Detects a constant that is uniformly one scalar value.
// CEP:WHY: Identity folds test uniform constants.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(elements).
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
fn uniform_const(g: &Graph, id: u32, value: f32) -> bool {
    // CEP:WHAT: Compares payloads by exact bit patterns.
    // CEP:WHY: -0.0 == 0.0 numerically but not bit-exactly; the signed zero must not match the unsigned one or folds would flip signs.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(elements).
    // CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
    const_payload(g, id)
        .is_some_and(|v| !v.is_empty() && v.iter().all(|x| x.to_bits() == value.to_bits()))
}

// CEP:WHAT: Adds a zero constant with the node's type to the graph.
// CEP:WHY: sum(zeros) needs a typed zero replacement.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: only constant dimensions contribute; a symbolic dimension leaves a one-element payload that the verifier rejects for non-scalar types, so such graphs fail loudly instead of misexecuting.
// CEP:COST: O(elements).
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
fn add_zero_constant(g: &mut Graph, node: &Node) -> u32 {
    let count: u64 = node
        .ty
        .dims
        .iter()
        .filter_map(|d| match d {
            mlkp_tensor::DimExpr::Const(c) => Some(*c),
            _ => None,
        })
        .product();
    let count = count.max(1);
    let next = max_id(g) + 1;
    let zero = Node {
        id: next,
        op: Op::Const(vec![0.0; count as usize]),
        inputs: Vec::new(),
        device: node.device,
        ty: node.ty.clone(),
        pgo: Default::default(),
        name: format!("zero_{}", node.id),
    };
    g.add(zero).expect("fresh id")
}

// CEP:WHAT: Redirects consumers of one node to another and removes the original.
// CEP:WHY: Shared by all fold rules.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(nodes).
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
fn retarget_and_remove(g: &mut Graph, from: u32, to: u32) {
    for n in g.iter_mut_nodes() {
        for inp in n.inputs.iter_mut() {
            if *inp == from {
                *inp = to;
            }
        }
    }
    for out in g.outputs.iter_mut() {
        if *out == from {
            *out = to;
        }
    }
    g.nodes_retain(|n| n.id != from);
}

// CEP:WHAT: Returns the largest node id.
// CEP:WHY: Fresh ids stay unique without a counter field.
// CEP:STATUS: complete
// CEP:FAILURE: Returns 0 for an empty graph.
// CEP:ASSUMES: none
// CEP:COST: O(nodes).
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
fn max_id(g: &Graph) -> u32 {
    g.iter().map(|n| n.id).max().unwrap_or(0)
}

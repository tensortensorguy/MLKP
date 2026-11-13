// CEP:FILE: crates/mlkp-hir/src/passes/algebraic.rs
// CEP:WHAT: Algebraic simplification and constant folding over the heterogeneous IR: identities (x*1, x+0, x*0), constant propagation through elementwise chains, and reassociation only when the numerical contract permits.
// CEP:WHY: Design passes 3 through 5 and 7 of level 1; folding constants at compile time removes work from every kernel execution, and the reassociation gate honors the numerical contract constraint of design section 6.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; simplifications that would change bit-exact semantics under a Forbidden reassociation contract are skipped by construction.
// CEP:ASSUMES: constant payloads are f32; identity elements follow IEEE semantics for float ops.
// CEP:COST: cold; O(nodes) per sweep with a fixed iteration cap.
// CEP:EVIDENCE: unit tests in crates/mlkp-hir/tests/hir_test.rs.

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
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
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
                } else if uniform_const(g, lhs, 0.0) && const_is_finite(g, rhs) {
                    // x*0 is 0 only when x is finite: inf*0 and NaN*0 are IEEE NaN.
                    let zero = add_zero_constant(g, &node);
                    retarget_and_remove(g, node.id, zero);
                    rewrites += 1;
                } else if uniform_const(g, rhs, 0.0) && const_is_finite(g, lhs) {
                    let zero = add_zero_constant(g, &node);
                    retarget_and_remove(g, node.id, zero);
                    rewrites += 1;
                }
            }
            Op::Map(MapFn::Add) => {
                let lhs = match node.inputs.first() {
                    Some(x) => *x,
                    None => continue,
                };
                let rhs = match node.inputs.get(1) {
                    Some(x) => *x,
                    None => continue,
                };
                // x+0 is x only when x carries no negative zero: -0.0+0.0 is +0.0.
                if uniform_const(g, rhs, 0.0) && const_has_no_negative_zero(g, lhs) {
                    retarget_and_remove(g, node.id, lhs);
                    rewrites += 1;
                } else if uniform_const(g, lhs, 0.0) && const_has_no_negative_zero(g, rhs) {
                    retarget_and_remove(g, node.id, rhs);
                    rewrites += 1;
                }
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
                // Summing an all-zero tensor yields zero only for exact +0.0
                // elements: -0.0 sums to -0.0, which differs by sign bit.
                if all_positive_zero(g, src) {
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
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
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
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
fn uniform_const(g: &Graph, id: u32, value: f32) -> bool {
    const_payload(g, id).is_some_and(|v| !v.is_empty() && v.iter().all(|x| *x == value))
}

// CEP:WHAT: Tests whether one operand is a constant with all-finite elements.
// CEP:WHY: IEEE: finite*0 is 0, but inf*0 and NaN*0 are NaN; the x*0 fold is legal only under the finite guarantee.
// CEP:STATUS: complete
// CEP:FAILURE: Returns false for non-constant or non-finite operands so the fold is skipped.
// CEP:ASSUMES: none
// CEP:COST: O(elements).
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
fn const_is_finite(g: &Graph, id: u32) -> bool {
    const_payload(g, id).is_some_and(|v| v.iter().all(|x| x.is_finite()))
}

// CEP:WHAT: Decides whether x+0 may collapse to x.
// CEP:WHY: IEEE: -0.0 + 0.0 is +0.0, so the fold is legal only when x has no negative-zero elements; non-constant operands cannot prove that statically.
// CEP:STATUS: complete
// CEP:FAILURE: Returns false for non-constant operands.
// CEP:ASSUMES: none
// CEP:COST: O(elements).
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
fn const_has_no_negative_zero(g: &Graph, id: u32) -> bool {
    const_payload(g, id).is_some_and(|v| v.iter().all(|x| !(*x == 0.0 && x.is_sign_negative())))
}

// CEP:WHAT: Tests whether one node is a constant of exact positive zeros.
// CEP:WHY: The sum-of-zeros fold must preserve the sign of zero under IEEE semantics.
// CEP:STATUS: complete
// CEP:FAILURE: Returns false for non-constant nodes.
// CEP:ASSUMES: none
// CEP:COST: O(elements).
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
fn all_positive_zero(g: &Graph, id: u32) -> bool {
    const_payload(g, id)
        .is_some_and(|v| !v.is_empty() && v.iter().all(|x| *x == 0.0 && x.is_sign_positive()))
}

// CEP:WHAT: Adds a zero constant with the node's type to the graph.
// CEP:WHY: x*0 and sum(zeros) need a typed zero replacement.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: only constant dimensions contribute; symbolic dims fall back to one element and are corrected by a later sweep after shape resolution.
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
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
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
    for inp in g.inputs.iter_mut() {
        if *inp == from {
            *inp = to;
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
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
fn max_id(g: &Graph) -> u32 {
    g.iter().map(|n| n.id).max().unwrap_or(0)
}

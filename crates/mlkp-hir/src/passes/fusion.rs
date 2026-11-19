// CEP:FILE: crates/mlkp-hir/src/passes/fusion.rs
// CEP:WHAT: Elementwise fusion peepholes over the heterogeneous IR: double negation collapses to its input (consumers retarget past the pair), which algebraic identities do not cover.
// CEP:WHY: Design pass 6 of level 1; removing a negation pair deletes a whole kernel pass from every plan lowered from the graph, and IEEE sign-flip is exactly self-inverse so the rewrite is bit-identical.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; the rewrite only retargets consumers of the outer node and removes that node, leaving any other consumers of the inner negation untouched.
// CEP:ASSUMES: negation is exactly self-inverse under IEEE f32 (sign-bit flip twice restores every bit, NaN included); the graph is acyclic.
// CEP:COST: cold; O(nodes) per sweep with a fixed iteration cap.
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs.

#![deny(missing_docs)]

use crate::graph::Graph;
use mlkp_tensor::MapFn;

/// Fixed-point sweep cap; the pass stabilizes after one sweep on real graphs.
const MAX_SWEEPS: u32 = 8;

/// Runs the elementwise fusion peepholes to a fixed point.
// CEP:WHAT: Runs the fusion pass to a fixed point.
// CEP:WHY: One collapse can expose another; the fixed point keeps pass iteration deterministic.
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

// CEP:ID: JIT-w01
// CEP:WHAT: Performs one fusion sweep: every Map(Neg) whose input is another Map(Neg) has its consumers retargeted to the inner input and is removed.
// CEP:WHY: Two negations compose to the identity exactly; the pair costs two kernel passes for nothing.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(nodes).
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
fn one_sweep(g: &mut Graph) -> usize {
    let ids: Vec<u32> = g.iter().map(|n| n.id).collect();
    let mut rewrites = 0;
    for id in ids {
        let node = match g.node(id) {
            Some(n) => n.clone(),
            None => continue,
        };
        if !matches!(node.op, crate::graph::Op::Map(MapFn::Neg)) {
            continue;
        }
        let inner = match node.inputs.first() {
            Some(x) => *x,
            None => continue,
        };
        let inner_node = match g.node(inner) {
            Some(n) => n.clone(),
            None => continue,
        };
        if !matches!(inner_node.op, crate::graph::Op::Map(MapFn::Neg)) {
            continue;
        }
        let target = match inner_node.inputs.first() {
            Some(x) => *x,
            None => continue,
        };
        retarget(g, id, target);
        remove(g, id);
        rewrites += 1;
    }
    rewrites
}

// CEP:WHAT: Redirects all references of one node id to another.
// CEP:WHY: The bypass replaces the collapsed pair everywhere.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: consumers were counted before the retarget.
// CEP:COST: O(nodes).
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
fn retarget(g: &mut Graph, from: u32, to: u32) {
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
}

// CEP:WHAT: Removes one node from the graph.
// CEP:WHY: The collapsed outer negation leaves the sea; the inner one dies to DCE if nothing else reads it.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: consumers were retargeted first.
// CEP:COST: O(nodes).
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
fn remove(g: &mut Graph, id: u32) {
    g.nodes_retain(|n| n.id != id);
}

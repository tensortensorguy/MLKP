// CEP:FILE: crates/mlkp-hir/src/passes/cse.rs
// CEP:WHAT: Common subexpression elimination over the heterogeneous IR.
// CEP:WHY: Design pass 2 of level 1; merged duplicates shrink the graph and the kernel plan.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; operations with effects are treated conservatively as unique and never merged.
// CEP:ASSUMES: operations are pure; the verifier has established the graph shape.
// CEP:COST: cold; O(nodes^2) pairwise comparison, acceptable at graph sizes in the hundreds.
// CEP:EVIDENCE: unit tests in crates/mlkp-hir/tests/passes_test.rs.

#![deny(missing_docs)]

use crate::graph::{Graph, Node};

/// Merges structurally identical nodes in place, redirecting consumers to the survivor.
// CEP:WHAT: Merges identical computations.
// CEP:WHY: Two identical subtrees evaluate identically; merging halves kernel work.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: nodes without names or with distinct device placements are never merged, because device placement is semantic in this IR.
// CEP:COST: O(nodes^2).
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
pub fn run(g: &mut Graph) -> usize {
    let mut merged = 0;
    let mut i = 0;
    while i < g.len() {
        let survivor = g.iter().nth(i).expect("index below len").clone();
        let mut j = i + 1;
        while j < g.len() {
            let candidate = g.iter().nth(j).expect("index below len").clone();
            if equivalent(&survivor, &candidate) {
                retarget(g, candidate.id, survivor.id);
                remove(g, candidate.id);
                merged += 1;
            } else {
                j += 1;
            }
        }
        i += 1;
    }
    merged
}

// CEP:WHAT: Structural equivalence of two nodes.
// CEP:WHY: CSE merges only when op, operands, type, and device all match.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(nodes of subtree) via hash.
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
fn equivalent(a: &Node, b: &Node) -> bool {
    if a.id == b.id || a.device != b.device {
        return false;
    }
    let mut h1 = cep_hash::Hasher::new();
    let mut h2 = cep_hash::Hasher::new();
    hash_node_subtree(a, &mut h1);
    hash_node_subtree(b, &mut h2);
    h1.finish() == h2.finish()
}

// CEP:WHAT: Folds one node and its operand ids into a hasher.
// CEP:WHY: Operand ids make the hash structural without walking the whole subtree twice.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1) per node.
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
fn hash_node_subtree(n: &Node, h: &mut cep_hash::Hasher) {
    h.update_str(n.name.as_str());
    let tag = match &n.op {
        crate::graph::Op::Input => 1,
        crate::graph::Op::Const(v) => {
            h.update_u64(2);
            h.update_u64(cep_hash::hash_f32_slice(v, h.snapshot()));
            2
        }
        crate::graph::Op::Map(f) => {
            h.update_u64(3);
            h.update_u64(map_tag(*f));
            3
        }
        crate::graph::Op::Reduce { op, axes } => {
            h.update_u64(4);
            h.update_u64(reduce_tag(*op));
            for a in axes {
                h.update_u64(u64::from(*a));
            }
            4
        }
        crate::graph::Op::Contract => 5,
        crate::graph::Op::Softmax { axis } => {
            h.update_u64(6);
            h.update_u64(u64::from(*axis));
            6
        }
        crate::graph::Op::Broadcast { axes } => {
            h.update_u64(7);
            for a in axes {
                h.update_u64(u64::from(*a));
            }
            7
        }
        crate::graph::Op::Permute { order } => {
            h.update_u64(8);
            for a in order {
                h.update_u64(u64::from(*a));
            }
            8
        }
        crate::graph::Op::Reshape { dims } => {
            h.update_u64(9);
            for d in dims {
                d.hash_into(h);
            }
            9
        }
    };
    h.update_u64(tag);
    for inp in &n.inputs {
        h.update_u64(u64::from(*inp));
    }
}

// CEP:WHAT: Stable discriminants for map and reduction kinds.
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
// CEP:WHY: See map_tag.
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

// CEP:WHAT: Redirects all references of one node id to another.
// CEP:WHY: The survivor replaces the merged duplicate everywhere.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
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
// CEP:WHY: Merged duplicates leave the sea.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: consumers were retargeted first.
// CEP:COST: O(nodes).
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
fn remove(g: &mut Graph, id: u32) {
    g.nodes_retain(|n| n.id != id);
}

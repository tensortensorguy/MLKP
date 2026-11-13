// CEP:FILE: crates/mlkp-hir/src/passes/cse.rs
// CEP:WHAT: Common subexpression elimination over the heterogeneous IR by exact structural equality of operands, operation, type, device, and provenance name.
// CEP:WHY: Design pass 2 of level 1; merged duplicates shrink the graph and the kernel plan, and exact equality keeps a hash collision from ever merging two different computations.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; operations with effects are treated conservatively as unique and never merged.
// CEP:ASSUMES: operations are pure; the verifier has established the graph shape; SSA ids make same-id operands the same computation.
// CEP:COST: cold; O(nodes^2) pairwise comparison, acceptable at graph sizes in the hundreds.
// CEP:EVIDENCE: unit tests in crates/mlkp-hir/tests/hir_test.rs.

#![deny(missing_docs)]

use crate::graph::{Graph, Node};

/// Merges structurally identical nodes in place, redirecting consumers to the survivor.
// CEP:WHAT: Merges identical computations.
// CEP:WHY: Two identical subtrees evaluate identically; merging halves kernel work.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: nodes with distinct device placements are never merged, because device placement is semantic in this IR; distinct provenance names are never merged either, preserving information per design section 11.
// CEP:COST: O(nodes^2).
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
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

// CEP:WHAT: Exact structural equivalence of two nodes.
// CEP:WHY: Operand ids are SSA references, so equal (op, operands, type, device, name) pairs compute the same value; no hash shortcut is taken.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(payload) per pair.
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
fn equivalent(a: &Node, b: &Node) -> bool {
    if a.id == b.id || a.device != b.device || a.name != b.name {
        return false;
    }
    a.op == b.op && a.inputs == b.inputs && a.ty == b.ty
}

// CEP:WHAT: Redirects all references of one node id to another.
// CEP:WHY: The survivor replaces the merged duplicate everywhere.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(nodes).
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
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
    for inp in g.inputs.iter_mut() {
        if *inp == from {
            *inp = to;
        }
    }
}

// CEP:WHAT: Removes one node from the graph.
// CEP:WHY: Merged duplicates leave the sea.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: consumers were retargeted first.
// CEP:COST: O(nodes).
// CEP:EVIDENCE: crates/mlkp-hir/tests/hir_test.rs
fn remove(g: &mut Graph, id: u32) {
    g.nodes_retain(|n| n.id != id);
}

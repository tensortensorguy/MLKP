// CEP:FILE: crates/mlkp-hir/src/passes/dce.rs
// CEP:WHAT: Dead code elimination over the heterogeneous IR.
// CEP:WHY: Design pass 1 of level 1; DCE also shrinks the search space because dead nodes waste candidate evaluation time.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; unreachable-from-output nodes are removed, everything else is preserved.
// CEP:ASSUMES: the graph is acyclic; the verifier runs before this pass in the pipeline.
// CEP:COST: cold; O(nodes + edges).
// CEP:EVIDENCE: unit tests in crates/mlkp-hir/tests/passes_test.rs.

#![deny(missing_docs)]

use crate::graph::Graph;

/// Removes nodes not reachable from the graph outputs or the declared inputs, in place.
// CEP:WHAT: Removes unreachable nodes in place.
// CEP:WHY: Dead nodes change subgraph hashes and waste search time without changing results.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: outputs and declared inputs are externally observable; parameters are externally bound and must never be removed.
// CEP:COST: O(nodes + edges).
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
pub fn run(g: &mut Graph) -> usize {
    let mut live: Vec<u32> = g.outputs.clone();
    // CEP:WHAT: Seeds the live walk with declared input nodes.
    // CEP:WHY: Parameters are bound by the caller; removing one silently changes the function signature.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: g.inputs lists the parameter node ids.
    // CEP:COST: O(inputs).
    // CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
    live.extend(g.inputs.iter().copied());
    let mut seen: Vec<u32> = Vec::new();
    while let Some(id) = live.pop() {
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        if let Some(n) = g.node(id) {
            for inp in n.inputs.clone() {
                live.push(inp);
            }
        }
    }
    let before = g.len();
    let dead: Vec<u32> = g
        .iter()
        .filter(|n| !seen.contains(&n.id))
        .map(|n| n.id)
        .collect();
    for id in dead {
        remove_node(g, id);
    }
    before - g.len()
}

// CEP:WHAT: Removes one node and renumbers nothing.
// CEP:WHY: Ids stay stable within one graph version so hashes remain comparable across passes.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(nodes).
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
fn remove_node(g: &mut Graph, id: u32) {
    g.nodes_retain(|n| n.id != id);
}

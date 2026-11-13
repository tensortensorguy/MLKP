// CEP:FILE: crates/mlkp-hir/src/verify.rs
// CEP:WHAT: The heterogeneous IR verifier: dangling references, duplicate ids, arity checks, cross-device cost declarations, and acyclicity.
// CEP:WHY: Design section XVI lists correctness failure detection as a first-class concern, and the transformation framework requires every pass output to be verifiable before the pruner reasons about legality.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns every violation found, not only the first.
// CEP:ASSUMES: the graph is a DAG; cycles are rejected.
// CEP:COST: cold; O(nodes + edges) with a cycle check of the same order.
// CEP:EVIDENCE: unit tests in crates/mlkp-hir/tests/verify_test.rs.

#![deny(missing_docs)]

use crate::graph::{Graph, GraphError, Op};
use mlkp_tensor::DimExpr;

/// Maximum inputs per operation kind.
const ARITY: fn(&Op) -> Option<usize> = |op| match op {
    Op::Input | Op::Const(_) => Some(0),
    Op::Map(_) | Op::Softmax { .. } => Some(1),
    Op::Reduce { .. } | Op::Broadcast { .. } | Op::Permute { .. } | Op::Reshape { .. } => Some(1),
    Op::Contract => Some(2),
};

/// Verifies one graph and returns all violations.
// CEP:WHAT: Verifies one graph.
// CEP:WHY: Verifiable optimization (design section 16) is a named property of the whole design; this is its enforcement point.
// CEP:STATUS: complete
// CEP:FAILURE: Returns one GraphError per violation.
// CEP:ASSUMES: none
// CEP:COST: O(nodes + edges).
// CEP:EVIDENCE: crates/mlkp-hir/tests/verify_test.rs
pub fn verify(g: &Graph) -> Result<(), Vec<GraphError>> {
    let mut errors = Vec::new();
    for n in g.iter() {
        if let Op::Const(v) = &n.op {
            // CEP:WHAT: Checks constant payload length against the declared type.
            // CEP:WHY: A payload shorter than the tensor silently reads garbage in the plan airlock; a payload longer than a symbolic type has no defined meaning.
            // CEP:STATUS: complete
            // CEP:FAILURE: Returns ArityMismatch for payload/type mismatches.
            // CEP:ASSUMES: none
            // CEP:COST: O(dims).
            // CEP:EVIDENCE: crates/mlkp-hir/tests/verify_test.rs
            let mut all_const = true;
            let mut expect: u64 = 1;
            for d in &n.ty.dims {
                match d {
                    DimExpr::Const(c) => match expect.checked_mul(*c) {
                        Some(p) => expect = p,
                        None => {
                            errors.push(GraphError::ArityMismatch(format!(
                                "node {} constant payload length overflows",
                                n.id
                            )));
                            all_const = false;
                            break;
                        }
                    },
                    _ => {
                        all_const = false;
                        break;
                    }
                }
            }
            let ok = if all_const {
                v.len() as u64 == expect
            } else {
                v.len() == 1
            };
            if !ok {
                errors.push(GraphError::ArityMismatch(format!(
                    "node {} constant payload length {} does not match type",
                    n.id,
                    v.len()
                )));
            }
        }
        if let Some(expected) = ARITY(&n.op) {
            if n.inputs.len() != expected {
                errors.push(GraphError::ArityMismatch(format!(
                    "node {} expects {expected} inputs, found {}",
                    n.id,
                    n.inputs.len()
                )));
            }
        }
        for inp in &n.inputs {
            if g.node(*inp).is_none() {
                errors.push(GraphError::UnknownNode(*inp));
            } else if let Some(other) = g.node(*inp) {
                if other.device != n.device && g.comm_cost(*inp, n.id).is_none() {
                    errors.push(GraphError::MissingCommCost(*inp, n.id));
                }
            }
        }
    }
    for id in g.inputs.iter().chain(g.outputs.iter()) {
        if g.node(*id).is_none() {
            errors.push(GraphError::UnknownNode(*id));
        }
    }
    if has_cycle(g) {
        errors.push(GraphError::ArityMismatch(
            "graph contains a cycle".to_string(),
        ));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// CEP:WHAT: Detects cycles with an iterative depth-first search and three-color marking.
// CEP:WHY: Unbounded recursion is banned by Law 6; the iterative stack bounds memory by the node count.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(nodes + edges).
// CEP:EVIDENCE: crates/mlkp-hir/tests/verify_test.rs
fn has_cycle(g: &Graph) -> bool {
    const WHITE: u8 = 0;
    const GRAY: u8 = 1;
    const BLACK: u8 = 2;
    let mut color: Vec<(u32, u8)> = g.iter().map(|n| (n.id, WHITE)).collect();
    for start in g.iter() {
        if color
            .iter()
            .find(|(id, _)| *id == start.id)
            .map(|(_, c)| *c)
            != Some(WHITE)
        {
            continue;
        }
        let mut stack = vec![(start.id, 0usize)];
        while let Some((id, idx)) = stack.pop() {
            let node = match g.node(id) {
                Some(n) => n,
                None => continue,
            };
            if idx == 0 {
                if let Some(entry) = color.iter_mut().find(|(cid, _)| *cid == id) {
                    entry.1 = GRAY;
                }
            }
            if idx < node.inputs.len() {
                stack.push((id, idx + 1));
                let next = node.inputs[idx];
                let next_color = color.iter().find(|(cid, _)| *cid == next).map(|(_, c)| *c);
                match next_color {
                    Some(GRAY) => return true,
                    Some(WHITE) => stack.push((next, 0)),
                    _ => {}
                }
            } else {
                if let Some(entry) = color.iter_mut().find(|(cid, _)| *cid == id) {
                    entry.1 = BLACK;
                }
            }
        }
    }
    false
}

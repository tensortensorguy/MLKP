// CEP:FILE: crates/mlkp-hir/src/lib.rs
// CEP:WHAT: Crate facade of the heterogeneous Sea of Nodes IR: the graph model, the verifier, and the level-1 graph passes (DCE, CSE, algebraic simplification with constant folding).
// CEP:WHY: Design.md assigns this IR ownership of the physical implementation: device placement, cross-device costs, layouts, tiling, execution distribution, scheduling, synchronization, and device-specific realization live downstream; this crate carries placement and PGO so the autotuner can reason natively about cross-device tradeoffs.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Graph construction and verification return typed errors; passes report rewrite counts.
// CEP:ASSUMES: the dataflow graph is acyclic for v1.
// CEP:COST: cold; runs during compilation.
// CEP:EVIDENCE: cargo test -p mlkp-hir.

//! Heterogeneous Sea of Nodes IR for MLKP.

#![deny(missing_docs)]

/// The graph model with devices and PGO annotations.
pub mod graph;
/// Level-1 graph passes, one file per pass.
pub mod passes;
/// The verifier.
pub mod verify;

pub use graph::{Device, Graph, GraphError, Node, Op, PgoAnnot};
pub use verify::verify;

/// Runs the standard level-1 pass pipeline in canonical order.
// CEP:WHAT: Runs the level-1 pass pipeline.
// CEP:WHY: The autotuner's pass-selection dimension composes individual passes; this canonical order is the baseline schedule.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the graph verifies before and after; the caller runs the verifier.
// CEP:COST: O(pass work).
// CEP:EVIDENCE: crates/mlkp-hir/tests/passes_test.rs
pub fn run_default_pipeline(g: &mut Graph) -> [usize; 3] {
    let folded = passes::algebraic::run(g);
    let merged = passes::cse::run(g);
    let dead = passes::dce::run(g);
    [folded, merged, dead]
}

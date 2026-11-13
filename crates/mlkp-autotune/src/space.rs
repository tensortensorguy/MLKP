// CEP:FILE: crates/mlkp-autotune/src/space.rs
// CEP:WHAT: The search space over the PL practice dimensions of design section III: pass selection, ordering, iteration counts, pass parameters, device placement, and memory layout, enumerated as concrete schedule configurations with stable hashes.
// CEP:WHY: The design's central claim is that the search space is compiler pass configurations, not raw code; this file defines the total space the pruner reduces from ~10^15 to a few hundred evaluable candidates.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; enumeration is total and deterministic.
// CEP:ASSUMES: the device dimension is filtered by the probe before enumeration; the pass registry catalog is the ground truth for dimension A.
// CEP:COST: cold; enumeration is O(space size) and bounded by the pruner's candidate caps.
// CEP:EVIDENCE: unit tests in crates/mlkp-autotune/tests/autotune_test.rs.

#![deny(missing_docs)]

use mlkp_runtime::{PassConfig, Tile};

/// Memory layout candidates of dimension F.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Row-major, the canonical layout.
    RowMajor,
    /// Transposed B operand access pattern (column-major B reads).
    TransposedB,
}

/// One point in the search space: a concrete schedule.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// Schedule configuration handed to the backend.
    pub config: PassConfig,
    /// Memory layout choice.
    pub layout: Layout,
    /// Device placement: CPU worker count for parallel evaluation.
    pub device_workers: u32,
    /// Pass selection bitmap over the implemented pass set.
    pub passes: PassSet,
    /// Pass iteration count for the algebraic fixed point.
    pub algebraic_iters: u32,
}

/// The implemented pass subset of the ~225-pass catalog that dimension A selects over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PassSet {
    /// Algebraic simplification and constant folding enabled.
    pub algebraic: bool,
    /// Common subexpression elimination enabled.
    pub cse: bool,
    /// Dead code elimination enabled.
    pub dce: bool,
}

impl PassSet {
    /// All passes on; the default schedule.
    // CEP:WHAT: Returns the full pass set.
    // CEP:WHY: The default pipeline enables every implemented pass.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
    pub const ALL: PassSet = PassSet {
        algebraic: true,
        cse: true,
        dce: true,
    };
}

/// Tile size candidates: powers of two within L1/L2 bounds (named constants per Law 7).
const TILE_CHOICES: [usize; 4] = [16, 32, 64, 128];
/// Worker count candidates (dimension E, CPU slice).
const WORKER_CHOICES: [u32; 2] = [1, 2];
/// Pass-set candidates (dimension A): which level-1 passes each schedule runs.
const PASS_SET_CHOICES: [PassSet; 4] = [
    PassSet::ALL,
    PassSet {
        algebraic: true,
        cse: true,
        dce: false,
    },
    PassSet {
        algebraic: true,
        cse: false,
        dce: true,
    },
    PassSet {
        algebraic: false,
        cse: true,
        dce: true,
    },
];
/// Pass iteration count candidates (dimension C): algebraic fixed-point sweeps.
const ALGEBRAIC_ITER_CHOICES: [u32; 2] = [1, 2];

/// Enumerates the full search space for one problem scale.
// CEP:WHAT: Enumerates the search space.
// CEP:WHY: Stage 3 of the JIT pipeline produces the candidate set the pruner reduces; every enumerated dimension reaches the backend so measured differences are schedule differences.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the caller caps the space through the pruner; the raw cross product is intentionally large.
// CEP:COST: O(space size).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
pub fn enumerate(problem_elements: u64) -> Vec<Candidate> {
    let mut out = Vec::new();
    for bm in TILE_CHOICES {
        for bn in TILE_CHOICES {
            for bk in TILE_CHOICES {
                for order in [0u32, 1] {
                    for transpose_b in [false, true] {
                        for workers in WORKER_CHOICES {
                            // Small problems do not benefit from multi-worker chunking.
                            if workers > 1 && problem_elements < 1 << 16 {
                                continue;
                            }
                            for passes in PASS_SET_CHOICES {
                                for iters in ALGEBRAIC_ITER_CHOICES {
                                    out.push(Candidate {
                                        config: PassConfig {
                                            tile: Tile { bm, bn, bk },
                                            threads: workers,
                                            loop_order: order,
                                            fused_elementwise: true,
                                            transpose_b,
                                        },
                                        layout: if transpose_b {
                                            Layout::TransposedB
                                        } else {
                                            Layout::RowMajor
                                        },
                                        device_workers: workers,
                                        passes,
                                        algebraic_iters: iters,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// Structural hash of one candidate for the historical cache.
// CEP:WHAT: Hashes one candidate.
// CEP:WHY: The historical cache layer keys on the exact config hash (design Layer 5); every field that changes the executed plan must change the hash or the cache returns wrong results.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(fields).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
pub fn candidate_hash(c: &Candidate, subgraph_hash: u64, hw_hash: u64) -> u64 {
    let mut h = cep_hash::Hasher::seeded(subgraph_hash);
    h.update_u64(hw_hash);
    h.update_u64(c.config.tile.bm as u64)
        .update_u64(c.config.tile.bn as u64)
        .update_u64(c.config.tile.bk as u64)
        .update_u64(u64::from(c.config.threads))
        .update_u64(u64::from(c.config.loop_order))
        .update_u64(u64::from(c.layout == Layout::TransposedB))
        .update_u64(u64::from(c.algebraic_iters))
        .update_u64(u64::from(c.passes.algebraic))
        .update_u64(u64::from(c.passes.cse))
        .update_u64(u64::from(c.passes.dce));
    h.finish()
}

/// Feature vector of one candidate for the cost model and clustering.
// CEP:WHAT: Builds the candidate feature vector.
// CEP:WHY: Layer 3 (cost model) and Layer 4 (similarity clustering) consume numeric features across the six schedule dimensions; pass selection acts before measurement and is hashed in the cache key instead.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: problem scale is folded in by the caller through subgraph parameters.
// CEP:COST: O(fields).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
pub fn features(c: &Candidate) -> [f64; 6] {
    [
        c.config.tile.bm as f64,
        c.config.tile.bn as f64,
        c.config.tile.bk as f64,
        f64::from(c.config.threads),
        f64::from(c.config.loop_order),
        f64::from(c.layout == Layout::TransposedB),
    ]
}

// CEP:FILE: crates/mlkp-autotune/src/lib.rs
// CEP:WHAT: Crate facade of the JIT autotuner: the PGO store, the search space over PL practice dimensions, the five-layer pruner, the four-phase search, and the JIT driver implementing the design's compilation lifecycle.
// CEP:WHY: Design.md's core loop (observe, predict, search, measure, learn, repeat) is implemented here as one orchestrated pipeline with the fallbacks of design section XII.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Compilation errors are typed; the baseline is always preserved on budget exhaustion.
// CEP:ASSUMES: none
// CEP:COST: cold compilation bounded by configured budgets.
// CEP:EVIDENCE: cargo test -p mlkp-autotune and the lifecycle integration test.

//! MLKP JIT autotuner.

#![deny(missing_docs)]

/// The JIT compilation lifecycle driver.
pub mod driver;
/// The persistent PGO store with drift detection.
pub mod pgo;
/// The five-layer pruner and the learned cost model.
pub mod pruner;
/// The four-phase search with budget management.
pub mod search;
/// The search space over the six PL practice dimensions.
pub mod space;

pub use driver::{autotune, hardware_profile_hash, recompile_warm, CacheEntry, CompileError};
pub use pgo::{Observation, PgoError, PgoRecord, PgoStore, StoredConfig};
pub use pruner::{Constraint, CostModel, Evaluated, Objectives, Pruner};
pub use search::{search, Budget, SearchContext, SearchResult};
pub use space::{candidate_hash, enumerate, features, Candidate, Layout, PassSet};

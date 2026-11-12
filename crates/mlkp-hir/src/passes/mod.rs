// CEP:FILE: crates/mlkp-hir/src/passes/mod.rs
// CEP:WHAT: Pass module registry for level-1 graph passes, one pass per file per the atomic file mandate.
// CEP:WHY: Chapter 62.1.1 bans pass landfills; the module split keeps each transformation independently reviewable.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: compile-time only.
// CEP:EVIDENCE: cargo test -p mlkp-hir.

/// Algebraic simplification and constant folding.
pub mod algebraic;
/// Common subexpression elimination.
pub mod cse;
/// Dead code elimination.
pub mod dce;

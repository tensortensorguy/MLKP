// CEP:FILE: crates/mlkp-tensor/src/lib.rs
// CEP:WHAT: Crate facade of the semantic Tensor IR: types, the shape engine, numerical contracts, value refinements, the operation vocabulary, operation contracts, and provenance.
// CEP:WHY: Design.md sections 1 through 16 define the Tensor IR as the highest-information semantic representation; this crate is the single home of that boundary and must never contain warps, lanes, registers, memory spaces, physical layouts, tiling, or hardware scheduling (section 15 prohibition list).
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Constructors and the shape engine return typed errors; no panics in any public API.
// CEP:ASSUMES: none
// CEP:COST: cold; the crate runs during IR building and pass execution, never inside kernels.
// CEP:EVIDENCE: cargo test -p mlkp-tensor.

//! Semantic Tensor IR for MLKP.
//!
//! The Tensor IR answers "what does this computation mean": mathematics, tensor
//! semantics, shapes, numerical contracts, indexing, sparsity, constraints, and
//! transformation legality. The heterogeneous Sea of Nodes IR owns "how is this
//! computation executed". The boundary is enforced by the prohibition list of
//! design.md section 15.

#![deny(missing_docs)]

/// Operation contracts with differentiation and effects.
pub mod contract;
/// First-class numerical contracts.
pub mod numeric;
/// The semantic operation vocabulary.
pub mod ops;
/// Value properties with proof status.
pub mod properties;
/// Provenance and transformation history.
pub mod provenance;
/// Symbolic shape engine with constraints and proofs.
pub mod shapes;
/// Element kinds and tensor types.
pub mod types;

pub use contract::{OperationContract, RuleAvailability};
pub use numeric::{
    Approx, Determinism, Fma, NumericContract, Overflow, Reassociation, Rounding, Underflow,
};
pub use ops::{Family, MapFn, ReduceOp, TensorOp};
pub use properties::{ProofStatus, Refinement, Refinements, ValueProperty};
pub use provenance::Provenance;
pub use shapes::{Constraint, Proof, ShapeEnv, ShapeError};
pub use types::{CompositeKind, DimExpr, ElemKind, TensorType, TypeError};

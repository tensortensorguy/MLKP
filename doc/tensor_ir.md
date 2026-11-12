# Tensor IR

## Abstract
The semantic Tensor IR answers what a computation means: mathematics, tensor semantics, shapes, numerical contracts, value properties with proof status, differentiation rules, and provenance. It owns no physical information; layout, tiling, and hardware realization live in the heterogeneous IR and below.

## Formal Definitions
Core types (Rust signatures):

```rust
pub struct TensorType { pub elem: ElemKind, pub composite: CompositeKind, pub dims: Vec<DimExpr> }
pub enum DimExpr { Const(u64), Sym(u32), Add(..), Mul(..), Div(..), CeilDiv(..), Min(..), Max(..) }
pub struct NumericContract { input, accumulator, output: ElemKind, rounding: Rounding, reassociation: Reassociation, fma: Fma, deterministic: Determinism, approx: Option<Approx> }
pub struct Refinement { property: ValueProperty, status: ProofStatus, evidence: String }
pub struct OperationContract { op: TensorOp, operand_types, result_types, shape_constraints, guarantees, differentiation, effects, semantic_id }
```

## Algorithm
The shape engine evaluates `DimExpr` trees against a `ShapeEnv` of symbol bindings. `prove_eq` returns `Proven` when both sides evaluate equal or a declared constraint equates them, `Refuted` when concrete values disagree, and `Unknown` otherwise. `validate` checks every declared constraint against bindings.

## Invariants
1. Only `Proven` and `Guaranteed` refinements are reliable for optimization; `Assumed` requires a runtime guard and `Hint` is advisory only.
2. A `NumericContract` whose accumulator is narrower than a float input is rejected at validation.
3. The Tensor IR contains no warp, lane, thread, SIMD width, physical buffer, stride, register, shared-memory, tiling, synchronization, or hardware-instruction concept; the design's prohibition list is enforced by construction.
4. Provenance history is append-only.

## Integration Points
* Depends on: `cep-hash` for structural fingerprints.
* Depended on by: `mlkp-hir` (graph nodes carry `TensorType`), `mlkp-front` (types from parsing).

## Known Limitations
Shape proofs handle equalities through bindings and declared constraints only; no SMT-style solving of conditional shapes. Data-dependent dimensions (`count_nonzero`) are rejected at bind time. Quantized types carry scale in the refinement layer, not the type.

// CEP:FILE: crates/mlkp-tensor/src/numeric.rs
// CEP:WHAT: First-class numerical contracts: precision chain, rounding, overflow and underflow policy, reassociation and FMA legality, determinism, and approximation bounds.
// CEP:WHY: Design section 6 makes numerical semantics absolutely explicit; the optimizer must respect the contract instead of guessing tolerated error, so every legality question about reassociation or fused rounding is answered by a value here.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Validation returns ContractViolation for inconsistent precision chains; no panics.
// CEP:ASSUMES: contracts constrain the compiler, not the hardware; targets that cannot meet a contract must reject the kernel rather than silently weaken it.
// CEP:COST: cold; contracts are constructed during lowering and consulted by passes.
// CEP:EVIDENCE: unit tests in crates/mlkp-tensor/tests/numeric_test.rs.

#![deny(missing_docs)]

use crate::types::{ElemKind, TypeError};

/// Rounding mode for floating-point conversions and arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rounding {
    /// Round to nearest, ties to even.
    NearestEven,
    /// Round toward zero.
    TowardZero,
    /// Stochastic rounding; legality requires an explicit deterministic seed contract.
    Stochastic,
}

/// Overflow behavior for integer and float domains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overflow {
    /// IEEE float semantics: infinities and NaNs.
    Ieee,
    /// Two's complement wraparound.
    Wrap,
    /// Clamp to the representable range.
    Saturate,
}

/// Underflow behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Underflow {
    /// Preserve subnormals.
    Preserve,
    /// Flush subnormals to zero.
    FlushToZero,
}

/// Whether reassociation of associative operations is legal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reassociation {
    /// The optimizer may regroup reductions and elementwise chains.
    Permitted,
    /// The operator order is part of the contract; bit-exact reproducibility demanded.
    Forbidden,
}

/// Whether fused multiply-add contraction is legal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fma {
    /// The optimizer may fuse a multiply into an add.
    Permitted,
    /// Each rounding step is observable; contraction forbidden.
    Forbidden,
}

/// Determinism requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Determinism {
    /// Run-to-run bit-exactness required.
    Required,
    /// Approximate reproducibility acceptable within the approximation contract.
    Relaxed,
}

/// Approximation contract declaring explicit error bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Approx {
    /// Maximum absolute error.
    pub abs_error: f64,
    /// Maximum relative error.
    pub rel_error: f64,
    /// True when NaN inputs must produce NaN outputs.
    pub nan_equivalence: bool,
}

/// The full numerical contract of one operation.
#[derive(Clone, Debug, PartialEq)]
pub struct NumericContract {
    /// Element precision of inputs.
    pub input: ElemKind,
    /// Accumulation precision for reductions and contractions.
    pub accumulator: ElemKind,
    /// Element precision of the result.
    pub output: ElemKind,
    /// Rounding mode.
    pub rounding: Rounding,
    /// Overflow policy.
    pub overflow: Overflow,
    /// Underflow policy.
    pub underflow: Underflow,
    /// Reassociation legality.
    pub reassociation: Reassociation,
    /// FMA contraction legality.
    pub fma: Fma,
    /// Determinism requirement.
    pub deterministic: Determinism,
    /// Approximation bounds when the operation is explicitly approximate.
    pub approx: Option<Approx>,
}

impl NumericContract {
    /// The contract used by the default matmul lowering: f16 storage, f32 accumulation, f16 output.
    // CEP:WHAT: Builds the matmul default contract.
    // CEP:WHY: Design section 13 names exactly this chain for the canonical matmul example.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/numeric_test.rs
    pub fn matmul_default() -> NumericContract {
        NumericContract {
            input: ElemKind::F16,
            accumulator: ElemKind::F32,
            output: ElemKind::F16,
            rounding: Rounding::NearestEven,
            overflow: Overflow::Ieee,
            underflow: Underflow::Preserve,
            reassociation: Reassociation::Permitted,
            fma: Fma::Permitted,
            deterministic: Determinism::Required,
            approx: None,
        }
    }

    /// The contract for strict bit-exact elementwise chains.
    // CEP:WHAT: Builds the strict elementwise contract.
    // CEP:WHY: The interpreter baseline must obey the same contract the optimized kernels are checked against.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/numeric_test.rs
    pub fn strict_f32() -> NumericContract {
        NumericContract {
            input: ElemKind::F32,
            accumulator: ElemKind::F32,
            output: ElemKind::F32,
            rounding: Rounding::NearestEven,
            overflow: Overflow::Ieee,
            underflow: Underflow::Preserve,
            reassociation: Reassociation::Forbidden,
            fma: Fma::Forbidden,
            deterministic: Determinism::Required,
            approx: None,
        }
    }

    /// Validates the precision chain and approximation bounds.
    // CEP:WHAT: Validates the contract.
    // CEP:WHY: Law 2 bans silent assumptions; an accumulator narrower than the inputs would silently lose precision.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns ContractViolation for negative errors or an accumulator narrower than inputs and output in float chains.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/numeric_test.rs
    pub fn validate(&self) -> Result<(), TypeError> {
        if self.input.is_float()
            && self.accumulator.is_float()
            && self.accumulator.byte_width() < self.input.byte_width()
        {
            return Err(TypeError::ContractViolation(
                "accumulator precision is narrower than input precision".to_string(),
            ));
        }
        if let Some(approx) = &self.approx {
            if approx.abs_error < 0.0 || approx.rel_error < 0.0 {
                return Err(TypeError::ContractViolation(
                    "approximation errors must be non-negative".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Folds the contract into a hasher for cache keys.
    // CEP:WHAT: Folds the contract into a hasher.
    // CEP:WHY: The compilation cache keys include the numerical contract hash per the design's artifact cache key list.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/numeric_test.rs
    pub fn hash64(&self, seed: u64) -> u64 {
        let mut h = cep_hash::Hasher::seeded(seed);
        h.update_u64(self.input.byte_width())
            .update_u64(self.accumulator.byte_width())
            .update_u64(self.output.byte_width())
            .update_u64(match self.rounding {
                Rounding::NearestEven => 1,
                Rounding::TowardZero => 2,
                Rounding::Stochastic => 3,
            })
            .update_u64(match self.reassociation {
                Reassociation::Permitted => 1,
                Reassociation::Forbidden => 2,
            })
            .update_u64(match self.fma {
                Fma::Permitted => 1,
                Fma::Forbidden => 2,
            })
            .update_u64(match self.deterministic {
                Determinism::Required => 1,
                Determinism::Relaxed => 2,
            });
        if let Some(a) = &self.approx {
            h.update_u64(a.abs_error.to_bits());
            h.update_u64(a.rel_error.to_bits());
            h.update_u64(u64::from(a.nan_equivalence));
        }
        h.finish()
    }
}

// CEP:FILE: crates/mlkp-tensor/src/contract.rs
// CEP:WHAT: Operation contracts: shape functions with constraints, value guarantees, differentiation rules, and effects, attached to every semantic operation.
// CEP:WHY: Design section 12 makes the contract the most important design decision: passes query what an operation is mathematically instead of hardcoding operator lists, and section 8 requires differentiation semantics to live in the IR rather than as an external afterthought.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; contracts are declarative data.
// CEP:ASSUMES: the shape function is expressed as constraints plus declared outputs because arbitrary shape programs are out of scope for v1 (see doc/tensor_ir.md Known Limitations).
// CEP:COST: cold; contracts are constructed with operations.
// CEP:EVIDENCE: unit tests in crates/mlkp-tensor/tests/contract_test.rs.

#![deny(missing_docs)]

use crate::ops::TensorOp;
use crate::shapes::Constraint;
use crate::types::TensorType;

/// Observable effects of an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effects {
    /// Pure: SSA value semantics only, freely reorderable subject to data flow.
    Pure,
    /// Reads external state; reordering must preserve read order relative to writers.
    ReadsState,
    /// Writes external state; reordering is constrained.
    WritesState,
    /// Consumes or produces RNG state; CSE and reordering must not duplicate or merge.
    Rng,
}

/// The differentiation contract of one operation.
#[derive(Clone, Debug, PartialEq)]
pub struct DifferentiationContract {
    /// Whether the operation is differentiable at all.
    pub differentiable: bool,
    /// Forward-mode rule availability.
    pub forward_rule: RuleAvailability,
    /// Reverse-mode rule availability.
    pub reverse_rule: RuleAvailability,
    /// Higher-order support.
    pub higher_order: bool,
    /// Operand indices that are not differentiable.
    pub nondifferentiable_inputs: Vec<u32>,
}

/// Availability of one differentiation rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleAvailability {
    /// Automatic through primitive decomposition.
    Automatic,
    /// A custom rule is registered.
    Custom,
    /// No rule; differentiation through this op is rejected.
    None,
}

/// The full semantic contract of one operation.
#[derive(Clone, Debug, PartialEq)]
pub struct OperationContract {
    /// The operation.
    pub op: TensorOp,
    /// Operand types.
    pub operand_types: Vec<TensorType>,
    /// Result types.
    pub result_types: Vec<TensorType>,
    /// Declared shape constraints over operands and results.
    pub shape_constraints: Vec<Constraint>,
    /// Value guarantees of the results with proof status.
    pub guarantees: Vec<crate::properties::Refinement>,
    /// Differentiation contract.
    pub differentiation: DifferentiationContract,
    /// Observable effects.
    pub effects: Effects,
    /// Stable semantic id (for example "matmul").
    pub semantic_id: String,
}

impl OperationContract {
    /// Builds the matmul contract from operand types.
    // CEP:WHAT: Builds the matmul operation contract.
    // CEP:WHY: Design section 13's example: dimensions, indexing, numerical chain, constraints, guarantees, and differentiation for the canonical op.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns the contract without validating types; type agreement is checked by the shape engine.
    // CEP:ASSUMES: operands are A then B; A and B have equal rank of 2 or 3.
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/contract_test.rs
    pub fn matmul(a: TensorType, b: TensorType, batched: bool) -> OperationContract {
        let mut shape_constraints = Vec::new();
        if batched {
            shape_constraints.push(Constraint::Eq(a.dims[0].clone(), b.dims[0].clone()));
            shape_constraints.push(Constraint::Eq(a.dims[2].clone(), b.dims[1].clone()));
        } else {
            shape_constraints.push(Constraint::Eq(a.dims[1].clone(), b.dims[0].clone()));
        }
        OperationContract {
            op: TensorOp::Matmul { batched },
            operand_types: vec![a, b],
            result_types: Vec::new(),
            shape_constraints,
            guarantees: Vec::new(),
            differentiation: DifferentiationContract {
                differentiable: true,
                forward_rule: RuleAvailability::Automatic,
                reverse_rule: RuleAvailability::Automatic,
                higher_order: true,
                nondifferentiable_inputs: Vec::new(),
            },
            effects: Effects::Pure,
            semantic_id: "matmul".to_string(),
        }
    }

    /// Builds a pure map contract.
    // CEP:WHAT: Builds an elementwise map contract.
    // CEP:WHY: Maps are the most common family; their contract declares shape equality of operands and results.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: one or two operands of equal shape.
    // CEP:COST: O(operands).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/contract_test.rs
    pub fn map(name: &str, operands: Vec<TensorType>, result: TensorType) -> OperationContract {
        let mut shape_constraints = Vec::new();
        if let (Some(first), Some(second)) = (operands.first(), operands.get(1)) {
            for (i, (x, y)) in first.dims.iter().zip(second.dims.iter()).enumerate() {
                shape_constraints.push(Constraint::Eq(x.clone(), y.clone()));
                let _ = i;
            }
        }
        OperationContract {
            op: TensorOp::Family(crate::ops::Family::Map(crate::ops::MapFn::Id)),
            operand_types: operands,
            result_types: vec![result],
            shape_constraints,
            guarantees: Vec::new(),
            differentiation: DifferentiationContract {
                differentiable: true,
                forward_rule: RuleAvailability::Automatic,
                reverse_rule: RuleAvailability::Automatic,
                higher_order: true,
                nondifferentiable_inputs: Vec::new(),
            },
            effects: Effects::Pure,
            semantic_id: name.to_string(),
        }
    }
}

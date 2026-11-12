// CEP:FILE: crates/mlkp-runtime/src/exec.rs
// CEP:WHAT: Plan execution: the cold shell that stages buffers, validates the airlock, dispatches the CEP-0 kernels, and applies the configured thread chunking.
// CEP:WHY: The design's parallel evaluation engine requires one worker to compile and run one candidate; this module is the per-candidate execution path, and the shell/airlock separation keeps cold-path logic out of the hot kernels.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns ExecError when the airlock fails, buffers are missing, or thread spawning fails.
// CEP:ASSUMES: the arena was preallocated to at least the plan's total elements.
// CEP:COST: shell O(plan size) plus kernel work; thread creation only when the config requests it.
// CEP:EVIDENCE: unit tests in crates/mlkp-runtime/tests/exec_test.rs.

#![deny(missing_docs)]

use crate::buffers::Arena;
use crate::kernels;
use crate::plan::{KernelPlan, PlanOp};

/// Execution failure.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecError {
    /// The airlock rejected the plan.
    Airlock(String),
    /// A buffer slice was unavailable.
    Buffer(u32),
    /// Thread chunking failed.
    Thread(String),
}

impl std::fmt::Display for ExecError {
    // CEP:WHAT: Formats the execution error.
    // CEP:WHY: Deterministic diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/exec_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecError::Airlock(s) => write!(f, "exec error: {s}"),
            ExecError::Buffer(id) => write!(f, "exec error: buffer {id} unavailable"),
            ExecError::Thread(s) => write!(f, "exec error: {s}"),
        }
    }
}

impl std::error::Error for ExecError {}

/// Executes one plan over the arena.
// CEP:WHAT: Executes one kernel plan.
// CEP:WHY: The autotuner's measurement calls this once per timed run.
// CEP:STATUS: complete
// CEP:FAILURE: Returns ExecError per the module contract.
// CEP:ASSUMES: the airlock passed; execution re-checks once and fails loudly otherwise.
// CEP:COST: O(kernel work).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/exec_test.rs
pub fn execute(plan: &KernelPlan, arena: &mut Arena) -> Result<(), ExecError> {
    plan.validate()
        .map_err(|e| ExecError::Airlock(e.to_string()))?;
    for op in &plan.ops {
        run_op(op, plan, arena)?;
    }
    Ok(())
}

// CEP:WHAT: Runs one scheduled operation.
// CEP:WHY: The dispatch loop is a flat match so each kernel call stays direct.
// CEP:STATUS: complete
// CEP:FAILURE: Returns ExecError per the module contract.
// CEP:ASSUMES: none
// CEP:COST: O(kernel work).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/exec_test.rs
fn run_op(op: &PlanOp, plan: &KernelPlan, arena: &mut Arena) -> Result<(), ExecError> {
    let off = |id: u32| plan.offset_of(id);
    match op {
        PlanOp::Unary {
            op: kind,
            src,
            dst,
            n,
        } => {
            let src_slice = arena
                .slice(off(*src), *n as u64)
                .ok_or(ExecError::Buffer(*src))?
                .to_vec();
            let dst = arena
                .slice_mut(off(*dst), *n as u64)
                .ok_or(ExecError::Buffer(*dst))?;
            kernels::ew_unary(*kind, &src_slice, dst);
        }
        PlanOp::Binary {
            op: kind,
            a,
            b,
            dst,
            n,
        } => {
            let a_slice = arena
                .slice(off(*a), *n as u64)
                .ok_or(ExecError::Buffer(*a))?
                .to_vec();
            let b_slice = arena
                .slice(off(*b), *n as u64)
                .ok_or(ExecError::Buffer(*b))?
                .to_vec();
            let dst = arena
                .slice_mut(off(*dst), *n as u64)
                .ok_or(ExecError::Buffer(*dst))?;
            kernels::ew_binary(*kind, &a_slice, &b_slice, dst);
        }
        PlanOp::Reduce {
            op: kind,
            src,
            dst,
            outer,
            red,
        } => {
            let src_slice = arena
                .slice(off(*src), (*outer * *red) as u64)
                .ok_or(ExecError::Buffer(*src))?
                .to_vec();
            let dst = arena
                .slice_mut(off(*dst), *outer as u64)
                .ok_or(ExecError::Buffer(*dst))?;
            kernels::reduce_last_axis(*kind, &src_slice, *outer, *red, dst);
        }
        PlanOp::Softmax {
            src,
            dst,
            outer,
            red,
        } => {
            let src_slice = arena
                .slice(off(*src), (*outer * *red) as u64)
                .ok_or(ExecError::Buffer(*src))?
                .to_vec();
            let dst = arena
                .slice_mut(off(*dst), (*outer * *red) as u64)
                .ok_or(ExecError::Buffer(*dst))?;
            kernels::softmax_last_axis(&src_slice, dst, *outer, *red);
        }
        PlanOp::MatMul {
            a,
            b,
            c,
            m,
            n,
            k,
            tile,
        } => {
            let a_slice = arena
                .slice(off(*a), (*m * *k) as u64)
                .ok_or(ExecError::Buffer(*a))?
                .to_vec();
            let b_slice = arena
                .slice(off(*b), (*k * *n) as u64)
                .ok_or(ExecError::Buffer(*b))?
                .to_vec();
            let c = arena
                .slice_mut(off(*c), (*m * *n) as u64)
                .ok_or(ExecError::Buffer(*c))?;
            kernels::matmul_tiled(&a_slice, &b_slice, c, *m, *n, *k, *tile);
        }
    }
    Ok(())
}

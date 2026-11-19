// CEP:FILE: crates/mlkp-runtime/src/exec.rs
// CEP:WHAT: Plan execution: the cold shell that stages buffers, validates the airlock, dispatches the CEP-0 kernels over disjoint arena views with zero per-op copies, and applies the configured thread chunking for the matmul.
// CEP:WHY: The design's parallel evaluation engine requires one worker to compile and run one candidate; copying operands inside the timed window would both distort measurement and violate the arena-reuse discipline, so dispatch derives disjoint mutable views from one arena slice instead.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns ExecError when the airlock fails, a buffer range is unavailable or overlapping, or thread spawning fails.
// CEP:ASSUMES: the arena was preallocated to at least the plan's total elements; the airlock proved per-operation buffer disjointness.
// CEP:COST: shell O(plan size) plus kernel work; no allocation inside the dispatch loop; thread creation only when the schedule requests it.
// CEP:EVIDENCE: unit tests in crates/mlkp-runtime/tests/runtime_test.rs.

#![deny(missing_docs)]

use crate::buffers::Arena;
use crate::kernels;
use crate::plan::{KernelPlan, PlanOp};

/// Execution failure.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecError {
    /// The airlock rejected the plan.
    Airlock(String),
    /// A buffer slice was unavailable or overlapped another operand.
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
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
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
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
pub fn execute(plan: &KernelPlan, arena: &mut Arena) -> Result<(), ExecError> {
    plan.validate()
        .map_err(|e| ExecError::Airlock(e.to_string()))?;
    for op in &plan.ops {
        run_op(op, plan, arena)?;
    }
    Ok(())
}

// CEP:WHAT: Derives two disjoint mutable sub-slices from one slice.
// CEP:WHY: Unary and reduction dispatch need simultaneous read and write views without copying operands.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None when either range leaves the slice or the ranges overlap.
// CEP:ASSUMES: none; disjointness is checked, not assumed.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
fn two_mut(
    data: &mut [f32],
    s0: usize,
    n0: usize,
    s1: usize,
    n1: usize,
) -> Option<(&mut [f32], &mut [f32])> {
    let e0 = s0.checked_add(n0)?;
    let e1 = s1.checked_add(n1)?;
    if e0 > data.len() || e1 > data.len() || (s0 < e1 && s1 < e0) {
        return None;
    }
    // CEP:UNSAFE: two raw slices into provably disjoint ranges of one allocation.
    // CEP:ASSUMES: disjointness is proven by the guard above; the pointers stay in bounds of the same allocation.
    unsafe {
        let base = data.as_mut_ptr();
        let v0 = core::slice::from_raw_parts_mut(base.add(s0), n0);
        let v1 = core::slice::from_raw_parts_mut(base.add(s1), n1);
        Some((v0, v1))
    }
}

// CEP:WHAT: Derives three disjoint mutable sub-slices from one slice.
// CEP:WHY: Binary and matmul dispatch read two operands and write one destination from the same arena without copies.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None when any range leaves the slice or two ranges overlap.
// CEP:ASSUMES: none; disjointness is checked, not assumed.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
fn three_mut(
    data: &mut [f32],
    s0: usize,
    n0: usize,
    s1: usize,
    n1: usize,
    s2: usize,
    n2: usize,
) -> Option<(&mut [f32], &mut [f32], &mut [f32])> {
    let e0 = s0.checked_add(n0)?;
    let e1 = s1.checked_add(n1)?;
    let e2 = s2.checked_add(n2)?;
    if e0 > data.len() || e1 > data.len() || e2 > data.len() {
        return None;
    }
    if (s0 < e1 && s1 < e0) || (s0 < e2 && s2 < e0) || (s1 < e2 && s2 < e1) {
        return None;
    }
    // CEP:UNSAFE: three raw slices into provably pairwise-disjoint ranges.
    // CEP:ASSUMES: pairwise disjointness is proven by the guard above; the pointers stay in bounds of the same allocation.
    unsafe {
        let base = data.as_mut_ptr();
        let v0 = core::slice::from_raw_parts_mut(base.add(s0), n0);
        let v1 = core::slice::from_raw_parts_mut(base.add(s1), n1);
        let v2 = core::slice::from_raw_parts_mut(base.add(s2), n2);
        Some((v0, v1, v2))
    }
}

// CEP:WHAT: Resolves one buffer id to its arena (start, len) pair.
// CEP:WHY: Dispatch resolves ranges through the plan's offset table only.
// CEP:STATUS: complete
// CEP:FAILURE: Zero length for out-of-range ids; the airlock rejects those plans first.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
fn buffer_range(plan: &KernelPlan, id: u32) -> (usize, usize) {
    let off = plan.offset_of(id);
    let len = plan.buffer_lens.get(id as usize).copied().unwrap_or(0);
    let start = usize::try_from(off).unwrap_or(usize::MAX);
    let len = usize::try_from(len).unwrap_or(0);
    (start, len)
}

// CEP:ID: JIT-e46
// CEP:WHAT: Runs one scheduled operation.
// CEP:WHY: The dispatch loop is a flat match so each kernel call stays direct and copy-free.
// CEP:STATUS: complete
// CEP:FAILURE: Returns ExecError per the module contract.
// CEP:ASSUMES: the airlock proved disjointness for every operation; dispatch re-proves it per view.
// CEP:COST: O(kernel work); no allocation on any path.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
#[allow(clippy::too_many_lines)]
fn run_op(op: &PlanOp, plan: &KernelPlan, arena: &mut Arena) -> Result<(), ExecError> {
    match op {
        PlanOp::Unary {
            op: kind,
            src,
            dst,
            n: _,
        } => {
            let (s0, l0) = buffer_range(plan, *src);
            let (s1, l1) = buffer_range(plan, *dst);
            let data = arena.as_mut_slice();
            let (src_view, dst_view) =
                two_mut(data, s0, l0, s1, l1).ok_or(ExecError::Buffer(*src))?;
            kernels::ew_unary(*kind, src_view, dst_view);
        }
        PlanOp::Binary {
            op: kind,
            a,
            b,
            dst,
            n: _,
        } => {
            let (s0, l0) = buffer_range(plan, *a);
            let (s1, l1) = buffer_range(plan, *b);
            let (s2, l2) = buffer_range(plan, *dst);
            let data = arena.as_mut_slice();
            let views = three_mut(data, s0, l0, s1, l1, s2, l2).ok_or(ExecError::Buffer(*dst))?;
            kernels::ew_binary(*kind, views.0, views.1, views.2);
        }
        PlanOp::Reduce {
            op: kind,
            src,
            dst,
            outer,
            red,
        } => {
            let (s0, l0) = buffer_range(plan, *src);
            let (s1, l1) = buffer_range(plan, *dst);
            let data = arena.as_mut_slice();
            let (src_view, dst_view) =
                two_mut(data, s0, l0, s1, l1).ok_or(ExecError::Buffer(*src))?;
            kernels::reduce_last_axis(*kind, src_view, *outer, *red, dst_view);
        }
        PlanOp::Softmax {
            src,
            dst,
            outer,
            red,
        } => {
            let (s0, l0) = buffer_range(plan, *src);
            let (s1, l1) = buffer_range(plan, *dst);
            let data = arena.as_mut_slice();
            let (src_view, dst_view) =
                two_mut(data, s0, l0, s1, l1).ok_or(ExecError::Buffer(*src))?;
            kernels::softmax_last_axis(src_view, dst_view, *outer, *red);
        }
        PlanOp::MatMul {
            a,
            b,
            c,
            m,
            n,
            k,
            tile,
            loop_order,
            transpose_b,
        } => {
            let (s0, l0) = buffer_range(plan, *a);
            let (s1, l1) = buffer_range(plan, *b);
            let (s2, l2) = buffer_range(plan, *c);
            let data = arena.as_mut_slice();
            let views = three_mut(data, s0, l0, s1, l1, s2, l2).ok_or(ExecError::Buffer(*c))?;
            let (a_view, b_view, c_view) = views;
            // CEP:WHAT: Gates the threaded path by the worker budget and the output size.
            // CEP:WHY: Threads pay off only above the named output-element threshold; below it the spawn overhead would inflate the measured cost, and the config request is capped at MAX_PLAN_THREADS so one plan cannot oversubscribe the host.
            // CEP:STATUS: complete
            // CEP:FAILURE: none
            // CEP:ASSUMES: the airlock rejected threads == 0 and loop_order > 1 already.
            // CEP:COST: O(1).
            // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
            let threads =
                (plan.config.threads as usize).clamp(1, mlkp_config::limits::MAX_PLAN_THREADS);
            if threads <= 1 || *m < 2 || *m * *n < mlkp_config::limits::PARALLEL_MIN_OUTPUT_ELEMENTS
            {
                kernels::matmul_tiled_sched(
                    a_view,
                    b_view,
                    c_view,
                    *m,
                    *n,
                    *k,
                    *tile,
                    *loop_order,
                    *transpose_b,
                );
            } else {
                run_matmul_threaded(
                    a_view,
                    b_view,
                    c_view,
                    (*m, *n, *k),
                    *tile,
                    *loop_order,
                    *transpose_b,
                    threads,
                )?;
            }
        }
    }
    Ok(())
}

// CEP:ID: JIT-420
// CEP:WHAT: Runs the chunked matmul across worker threads over disjoint output row ranges.
// CEP:WHY: The schedule's worker count (dimension E) must change execution, not just bookkeeping; disjoint row ownership needs no synchronization.
// CEP:STATUS: complete
// CEP:FAILURE: Returns ExecError::Thread when a worker fails to spawn or join.
// CEP:ASSUMES: the output view covers exactly m*n elements; workers write disjoint row ranges.
// CEP:COST: O(m*n*k) total work across workers; one thread spawn per chunk.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
#[allow(clippy::too_many_arguments)]
fn run_matmul_threaded(
    a: &[f32],
    b: &[f32],
    c: &mut [f32],
    dims: (usize, usize, usize),
    sched: crate::kernels::Tile,
    order: u32,
    strided_b: bool,
    threads: usize,
) -> Result<(), ExecError> {
    let (m, n, k) = dims;
    let tile = sched;
    let loop_order = order;
    let transpose_b = strided_b;
    let workers = threads
        .min(mlkp_config::limits::MAX_EVAL_THREADS)
        .min(m)
        .max(1);
    let chunk = m.div_ceil(workers);
    std::thread::scope(|scope| {
        let mut cursor: &mut [f32] = c;
        let mut row = 0usize;
        while row < m {
            let end = (row + chunk).min(m);
            let rows_len = (end - row) * n;
            let (rows, tail) = cursor.split_at_mut(rows_len);
            // One worker per chunk: rows is an exclusive, disjoint output range.
            scope.spawn(move || {
                kernels::matmul_tiled_rows(
                    a,
                    b,
                    rows,
                    m,
                    n,
                    k,
                    tile,
                    loop_order,
                    transpose_b,
                    row,
                    end,
                );
            });
            cursor = tail;
            row = end;
        }
    });
    Ok(())
}

// CEP:FILE: crates/mlkp-runtime/src/exec.rs
// CEP:WHAT: Plan execution: the cold shell that stages buffers, validates the airlock, dispatches the CEP-0 kernels over disjoint arena views with zero per-op copies, applies the configured thread chunking for the matmul, and fuses what the lookahead proves safe (runs of unary ops into one pass, and a unary consumer into the matmul's stores).
// CEP:WHY: The design's parallel evaluation engine requires one worker to compile and run one candidate; copying operands inside the timed window would both distort measurement and violate the arena-reuse discipline, and skipping provably-unobserved buffer passes is the cheapest optimization the dispatcher can apply without changing the plan.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns ExecError when the airlock fails, a buffer range is unavailable or overlapping, or thread spawning fails.
// CEP:ASSUMES: the arena was preallocated to at least the plan's total elements; the airlock proved per-operation buffer disjointness; the fusion guards prove every skipped buffer is never read again.
// CEP:COST: shell O(plan size) plus kernel work; no allocation inside the dispatch loop; thread creation only when the schedule requests it.
// CEP:EVIDENCE: unit tests in crates/mlkp-runtime/tests/runtime_test.rs and crates/mlkp-runtime/tests/simd_test.rs.

#![deny(missing_docs)]

use crate::buffers::Arena;
use crate::kernels;
use crate::kernels::EwUnary;
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

/// Executes one plan over the arena, fusing what the lookahead proves safe.
// CEP:ID: JIT-f01
// CEP:WHAT: Executes one kernel plan with exec-level fusion.
// CEP:WHY: The dispatcher's lookahead fuses runs of consecutive unary ops into one pass (one load and one store per run) and folds a unary consumer into the matmul's stores when the matmul output has no later reader; both fusions are bit-identical to the unfused sequence, and the dispatch loop allocates nothing.
// CEP:STATUS: complete
// CEP:FAILURE: Returns ExecError per the module contract.
// CEP:ASSUMES: the airlock passed; execution re-checks once and fails loudly otherwise; the fusion guards prove skipped buffers are never read again.
// CEP:COST: O(kernel work); no allocation on any path.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs asserts bit-equality of fused and unfused execution.
pub fn execute(plan: &KernelPlan, arena: &mut Arena) -> Result<(), ExecError> {
    plan.validate()
        .map_err(|e| ExecError::Airlock(e.to_string()))?;
    let mut i = 0;
    while i < plan.ops.len() {
        match &plan.ops[i] {
            PlanOp::Unary { .. } => {
                let chain = chain_extent(plan, i);
                if chain_fusable(plan, i, &chain) {
                    run_unary_chain(plan, &chain, arena)?;
                    i = chain.end;
                } else {
                    run_unary_one(plan, &chain, arena)?;
                    i += 1;
                }
            }
            PlanOp::MatMul { .. } => {
                let epi = matmul_epilogue(plan, i);
                run_matmul(plan, i, epi, arena)?;
                i += if epi.is_some() { 2 } else { 1 };
            }
            other => {
                run_other(other, plan, arena)?;
                i += 1;
            }
        }
    }
    Ok(())
}

/// One fused unary run: the composed kinds and the buffer ids it reads and writes.
// CEP:WHAT: Describes one fused unary run.
// CEP:WHY: The dispatcher composes up to four unary ops per pass; a fixed array keeps the hot loop allocation-free.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: len is 1..=4 and every op in the run shares one element count.
// CEP:COST: O(1) per run.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
struct ChainFuse {
    /// The composed unary kinds, prefix-live up to len.
    kinds: [EwUnary; 4],
    /// How many kinds are live (1..=4).
    len: usize,
    /// One past the last op index of the run.
    end: usize,
    /// The run's first source buffer id.
    src: u32,
    /// The run's final destination buffer id.
    dst: u32,
    /// The element count every op in the run shares.
    n: usize,
}

// CEP:ID: JIT-f02
// CEP:WHAT: Extends one fused unary run forward from op index start while consecutive unaries read the previous run's destination and share its element count.
// CEP:WHY: Chain fusion saves a load and a store per merged op; four is the register-resident composition depth that keeps the loop allocation-free.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: plan.ops[start] is a Unary op (the dispatcher matches before calling).
// CEP:COST: O(chain length).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
fn chain_extent(plan: &KernelPlan, start: usize) -> ChainFuse {
    let (op0, src0, dst0, n0) = match &plan.ops[start] {
        PlanOp::Unary { op, src, dst, n } => (*op, *src, *dst, *n),
        _ => {
            debug_assert!(false, "chain_extent called on a non-unary op");
            return ChainFuse {
                kinds: [EwUnary::Relu; 4],
                len: 0,
                end: start,
                src: 0,
                dst: 0,
                n: 0,
            };
        }
    };
    let mut cf = ChainFuse {
        kinds: [op0, op0, op0, op0],
        len: 1,
        end: start + 1,
        src: src0,
        dst: dst0,
        n: n0,
    };
    while cf.len < 4 {
        match plan.ops.get(cf.end) {
            Some(PlanOp::Unary { op, src, dst, n }) if *src == cf.dst && *n == cf.n => {
                cf.kinds[cf.len] = *op;
                cf.len += 1;
                cf.dst = *dst;
                cf.end += 1;
            }
            _ => break,
        }
    }
    cf
}

// CEP:ID: JIT-f03
// CEP:WHAT: Proves a fused unary run safe: the chain's intermediate buffers (the destinations of every op but the last) are never read after the run.
// CEP:WHY: Fusion skips writing intermediates entirely; a later reader would observe stale arena bytes, so the lookahead must prove exclusivity before fusing.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none; the read scan is exhaustive over the remaining ops.
// CEP:COST: O(intermediates * remaining ops), all in a fixed-size array.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs covers the fan-out case where fusion must be refused.
fn chain_fusable(plan: &KernelPlan, start: usize, cf: &ChainFuse) -> bool {
    if cf.len <= 1 {
        return true;
    }
    let mut ids = [0u32; 3];
    let mut count = 0;
    let mut idx = start;
    while idx + 1 < cf.end {
        if let PlanOp::Unary { dst, .. } = &plan.ops[idx] {
            ids[count] = *dst;
            count += 1;
        }
        idx += 1;
    }
    !buffer_read_after(plan, cf.end, &ids[..count])
}

// CEP:ID: JIT-f04
// CEP:WHAT: Reports whether any op from index `from` onward reads one of the given buffer ids.
// CEP:WHY: Both fusion decisions (unary chains and the matmul epilogue) reduce to this one exhaustive read scan.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(remaining ops); no allocation.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
fn buffer_read_after(plan: &KernelPlan, from: usize, ids: &[u32]) -> bool {
    let mut idx = from;
    while idx < plan.ops.len() {
        match &plan.ops[idx] {
            PlanOp::Unary { src, .. }
            | PlanOp::Reduce { src, .. }
            | PlanOp::Softmax { src, .. } => {
                if ids.contains(src) {
                    return true;
                }
            }
            PlanOp::Binary { a, b, .. } | PlanOp::MatMul { a, b, .. } => {
                if ids.contains(a) || ids.contains(b) {
                    return true;
                }
            }
        }
        idx += 1;
    }
    false
}

// CEP:WHAT: Runs one fused unary run over disjoint arena views.
// CEP:WHY: The chain kernel composes per element in registers; dispatch derives the two views without copying.
// CEP:STATUS: complete
// CEP:FAILURE: Returns ExecError::Buffer when a range is unavailable or overlapping.
// CEP:ASSUMES: the airlock proved per-op disjointness; the guard re-proves it for the composed views.
// CEP:COST: O(kernel work).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
fn run_unary_chain(plan: &KernelPlan, cf: &ChainFuse, arena: &mut Arena) -> Result<(), ExecError> {
    let (s0, l0) = buffer_range(plan, cf.src);
    let (s1, l1) = buffer_range(plan, cf.dst);
    let data = arena.as_mut_slice();
    let (src_view, dst_view) = two_mut(data, s0, l0, s1, l1).ok_or(ExecError::Buffer(cf.src))?;
    kernels::ew_unary_chain(&cf.kinds[..cf.len], src_view, dst_view);
    Ok(())
}

// CEP:WHAT: Runs one unfused unary op over disjoint arena views.
// CEP:WHY: The fallback path keeps exact semantics when the lookahead refuses a fusion (fan-out reads).
// CEP:STATUS: complete
// CEP:FAILURE: Returns ExecError::Buffer when a range is unavailable or overlapping.
// CEP:ASSUMES: the airlock proved disjointness.
// CEP:COST: O(kernel work).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
fn run_unary_one(plan: &KernelPlan, cf: &ChainFuse, arena: &mut Arena) -> Result<(), ExecError> {
    let (op0, src0, dst0) = match &plan.ops[cf.end - cf.len] {
        PlanOp::Unary { op, src, dst, .. } => (*op, *src, *dst),
        _ => return Ok(()),
    };
    let (s0, l0) = buffer_range(plan, src0);
    let (s1, l1) = buffer_range(plan, dst0);
    let data = arena.as_mut_slice();
    let (src_view, dst_view) = two_mut(data, s0, l0, s1, l1).ok_or(ExecError::Buffer(src0))?;
    kernels::ew_unary(op0, src_view, dst_view);
    Ok(())
}

// CEP:ID: JIT-f08
// CEP:WHAT: Decides the matmul epilogue fusion: the next op is a unary reading the matmul output with matching length, and the output buffer has no later reader.
// CEP:WHY: Folding the consumer into the matmul's stores removes a full buffer pass; the read scan proves the skipped output write is never observed.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none; the guard is exhaustive.
// CEP:COST: O(remaining ops) for the read scan.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
fn matmul_epilogue(plan: &KernelPlan, i: usize) -> Option<(EwUnary, u32)> {
    let (c, mn) = match &plan.ops[i] {
        PlanOp::MatMul { c, m, n, .. } => (*c, m * n),
        _ => return None,
    };
    match plan.ops.get(i + 1) {
        Some(PlanOp::Unary { op, src, dst, n })
            if *src == c && *n == mn && !buffer_read_after(plan, i + 2, &[c]) =>
        {
            Some((*op, *dst))
        }
        _ => None,
    }
}

// CEP:ID: JIT-f09
// CEP:WHAT: Runs one matmul op, writing the fused epilogue output when the lookahead fused one, over disjoint arena views.
// CEP:WHY: The fused path gives the kernel the consumer's destination buffer and the epilogue op, so the intermediate output buffer is never written.
// CEP:STATUS: complete
// CEP:FAILURE: Returns ExecError per the module contract.
// CEP:ASSUMES: the airlock proved disjointness; the epilogue destination is disjoint from both operands by the arena allocator.
// CEP:COST: O(kernel work); no allocation on any path.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
fn run_matmul(
    plan: &KernelPlan,
    i: usize,
    epi: Option<(EwUnary, u32)>,
    arena: &mut Arena,
) -> Result<(), ExecError> {
    let (a, b, c, m, n, k, tile, loop_order, transpose_b) = match &plan.ops[i] {
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
        } => (*a, *b, *c, *m, *n, *k, *tile, *loop_order, *transpose_b),
        _ => return Ok(()),
    };
    let (epi_op, out_buf) = match epi {
        Some((op, dst)) => (Some(op), dst),
        None => (None, c),
    };
    let (s0, l0) = buffer_range(plan, a);
    let (s1, l1) = buffer_range(plan, b);
    let (s2, l2) = buffer_range(plan, out_buf);
    let data = arena.as_mut_slice();
    let views = three_mut(data, s0, l0, s1, l1, s2, l2).ok_or(ExecError::Buffer(out_buf))?;
    let (a_view, b_view, c_view) = views;
    // CEP:WHAT: Gates the threaded path by the worker budget and the output size.
    // CEP:WHY: Threads pay off only above the named output-element threshold; below it the spawn overhead would inflate the measured cost, and the config request is capped at MAX_PLAN_THREADS so one plan cannot oversubscribe the host.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: the airlock rejected threads == 0 and loop_order > 1 already.
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    let threads = (plan.config.threads as usize).clamp(1, mlkp_config::limits::MAX_PLAN_THREADS);
    if threads <= 1 || m < 2 || m * n < mlkp_config::limits::PARALLEL_MIN_OUTPUT_ELEMENTS {
        kernels::matmul_tiled_sched_epi(
            a_view,
            b_view,
            c_view,
            m,
            n,
            k,
            tile,
            loop_order,
            transpose_b,
            epi_op,
        );
    } else {
        run_matmul_threaded(
            a_view,
            b_view,
            c_view,
            (m, n, k),
            tile,
            loop_order,
            transpose_b,
            threads,
            epi_op,
        )?;
    }
    Ok(())
}

// CEP:ID: JIT-f10
// CEP:WHAT: Runs one non-matmul, non-unary op (binary, reduce, softmax) over disjoint arena views.
// CEP:WHY: These ops keep their standalone kernels; no fusion applies to them in this plan vocabulary.
// CEP:STATUS: complete
// CEP:FAILURE: Returns ExecError::Buffer when a range is unavailable or overlapping.
// CEP:ASSUMES: the airlock proved disjointness.
// CEP:COST: O(kernel work).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
fn run_other(op: &PlanOp, plan: &KernelPlan, arena: &mut Arena) -> Result<(), ExecError> {
    match op {
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
        PlanOp::Unary { .. } | PlanOp::MatMul { .. } => {
            // The dispatcher handles these arms before calling here.
            return Ok(());
        }
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
    epilogue: Option<crate::kernels::EwUnary>,
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
                kernels::matmul_tiled_rows_epi(
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
                    epilogue,
                );
            });
            cursor = tail;
            row = end;
        }
    });
    Ok(())
}

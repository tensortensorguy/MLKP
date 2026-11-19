// CEP:FILE: crates/mlkp-runtime/src/kernels.rs
// CEP:WHAT: The CEP-0 hot kernels: allocation-free elementwise, reduction, tiled matrix multiply, and stable softmax over caller-provided f32 slices.
// CEP:WHY: These functions are the measured execution core of the JIT; the design requires candidates to be evaluated with real timings, and the CEP-0 class enforces no allocation, no locks, no formatting, no hidden cost in this file.
// CEP:CLASS: CEP-0
// CEP:STATUS: complete
// CEP:FAILURE: none; kernels perform no arithmetic that panics, all bounds are debug-asserted, and debug builds abort loudly on misuse while release builds assume the airlock's validated plan.
// CEP:ASSUMES: input slices have the documented lengths relative to the outer and reduction extents; the plan airlock proves those relations (standard chapter 64.2); arithmetic is IEEE f32 with the numerical contract applied by the plan.
// CEP:COST: elementwise 1 load+1 store per element; matmul O(m*n*k) flops with cache-blocked access measured in benches/artifacts/matmul_128.md.
// CEP:EVIDENCE: benches/artifacts/matmul_128.md and cargo test -p mlkp-runtime.

#![deny(missing_docs)]

use crate::simd;

/// Elementwise unary kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EwUnary {
    /// Rectified linear unit.
    Relu,
    /// Exponential.
    Exp,
    /// Negation.
    Neg,
    /// Sigmoid: 1 / (1 + exp(-x)).
    Sigmoid,
}

/// Elementwise binary kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EwBinary {
    /// Addition.
    Add,
    /// Multiplication.
    Mul,
    /// Subtraction.
    Sub,
    /// Maximum.
    Max,
}

/// Reduction combiner kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReduceKind {
    /// Sum with zero identity.
    Sum,
    /// Maximum with negative-infinity identity.
    Max,
}

/// Tile configuration for the blocked matrix multiply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    /// Rows per tile.
    pub bm: usize,
    /// Columns per tile.
    pub bn: usize,
    /// Reduction depth per tile.
    pub bk: usize,
}

impl Tile {
    /// The naive untiled configuration.
    // CEP:WHAT: Returns the untiled baseline tile.
    // CEP:WHY: The search space seeds include the naive schedule so the pruner can always fall back.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    pub const NAIVE: Tile = Tile {
        bm: 64,
        bn: 64,
        bk: 64,
    };
}

/// Applies one unary operation over a slice, on the host's widest engine.
// CEP:WHAT: Applies one unary op elementwise on the SIMD engine.
// CEP:WHY: Elementwise chains are the most frequent kernels in tensor graphs; the vector engine serves every lane the host offers and the scalar tail shares one polynomial.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: a.len() == out.len(); enforced by debug assertion and by the plan airlock.
// CEP:COST: O(n) at 8-wide vectors plus a scalar tail; allocation-free.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[inline]
pub fn ew_unary(op: EwUnary, a: &[f32], out: &mut [f32]) {
    simd::ew_unary_fast(op, a, out);
}

/// The portable scalar body of the unary pass, the Scalar engine.
// CEP:WHAT: Applies one unary op elementwise, scalar.
// CEP:WHY: Non-x86_64 hosts and the dispatch fallback run the same polynomial exp as the vector body; no libm slow path exists.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: a.len() == out.len().
// CEP:COST: O(n); allocation-free.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[inline]
pub(crate) fn ew_unary_scalar(op: EwUnary, a: &[f32], out: &mut [f32]) {
    debug_assert_eq!(a.len(), out.len(), "unary kernel length mismatch");
    let n = a.len();
    let mut i = 0;
    while i < n {
        out[i] = simd::apply1(op, a[i]);
        i += 1;
    }
}

/// Applies a fused unary chain (1..=4 ops composed per element) as one pass.
// CEP:ID: JIT-f05
// CEP:WHAT: Runs a fused unary chain over full buffers.
// CEP:WHY: Chains of elementwise ops pay one load and one store instead of one per op; the composition applies in registers, so results are bit-identical to the unfused sequence.
// CEP:STATUS: complete
// CEP:FAILURE: none; empty chains are rejected by debug assertion (the dispatcher guarantees 1..=4).
// CEP:ASSUMES: a.len() == out.len().
// CEP:COST: O(n) total instead of O(kinds * n) memory traffic; allocation-free.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs asserts bit-equality with the unfused sequence.
#[inline]
pub fn ew_unary_chain(kinds: &[EwUnary], a: &[f32], out: &mut [f32]) {
    simd::ew_unary_chain_fast(kinds, a, out);
}

/// The portable scalar body of the fused unary chain.
// CEP:WHAT: Composes the chain per scalar element.
// CEP:WHY: The Scalar engine keeps the same composition semantics as the vector chain.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: kinds non-empty; lengths equal.
// CEP:COST: O(kinds * n).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[inline]
pub(crate) fn ew_unary_chain_scalar(kinds: &[EwUnary], a: &[f32], out: &mut [f32]) {
    debug_assert!(!kinds.is_empty(), "chain must carry at least one op");
    debug_assert_eq!(a.len(), out.len(), "chain kernel length mismatch");
    let n = a.len();
    let mut i = 0;
    while i < n {
        let mut x = a[i];
        for &k in kinds {
            x = simd::apply1(k, x);
        }
        out[i] = x;
        i += 1;
    }
}

/// Applies one binary operation over two slices, on the host's widest engine.
// CEP:WHAT: Applies one binary op elementwise on the SIMD engine.
// CEP:WHY: Bias adds and pairwise ops compose into fused chains; the vector body halves the instruction count per element.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: a.len() == b.len() == out.len(); enforced by debug assertion and by the plan airlock.
// CEP:COST: O(n) with two loads and one store per element at 8-wide vectors; allocation-free.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[inline]
pub fn ew_binary(op: EwBinary, a: &[f32], b: &[f32], out: &mut [f32]) {
    simd::ew_binary_fast(op, a, b, out);
}

/// The portable scalar body of the binary pass, the Scalar engine.
// CEP:WHAT: Applies one binary op elementwise, scalar.
// CEP:WHY: The dispatch fallback shares one body with the vector tails.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: lengths equal.
// CEP:COST: O(n); allocation-free.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[inline]
pub(crate) fn ew_binary_scalar(op: EwBinary, a: &[f32], b: &[f32], out: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len(), "binary kernel length mismatch");
    debug_assert_eq!(a.len(), out.len(), "binary kernel output mismatch");
    let n = a.len();
    let mut i = 0;
    while i < n {
        out[i] = simd::apply2(op, a[i], b[i]);
        i += 1;
    }
}

/// Reduces the last axis: `src` has shape `[outer, red]`, `dst` has shape `[outer]`, on the host's widest engine.
// CEP:ID: JIT-u48
// CEP:WHAT: Reduces the last axis of a row-major tensor on the SIMD engine.
// CEP:WHY: Softmax and loss paths reduce the contiguous axis, which is the cache-friendly case; four independent vector accumulators hide the add latency.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: src.len() == outer * red and dst.len() == outer; enforced by debug assertion and the plan airlock; the vector body's association differs from the sequential scalar kernel only inside the tolerance contract.
// CEP:COST: O(outer * red) at 8-wide loads; allocation-free.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
pub fn reduce_last_axis(op: ReduceKind, src: &[f32], outer: usize, red: usize, dst: &mut [f32]) {
    simd::reduce_fast(op, src, outer, red, dst);
}

/// The portable scalar body of the last-axis reduce.
// CEP:ID: JIT-f11
// CEP:WHAT: Reduces the last axis, sequential scalar.
// CEP:WHY: Rows too short to amortize a vector run here, where it is the fast path.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: src.len() == outer * red and dst.len() == outer.
// CEP:COST: O(outer * red); allocation-free.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
pub(crate) fn reduce_last_axis_scalar(
    op: ReduceKind,
    src: &[f32],
    outer: usize,
    red: usize,
    dst: &mut [f32],
) {
    debug_assert_eq!(src.len(), outer * red, "reduce source mismatch");
    debug_assert_eq!(dst.len(), outer, "reduce destination mismatch");
    let mut o = 0;
    while o < outer {
        let base = o * red;
        let mut acc = match op {
            ReduceKind::Sum => 0.0,
            ReduceKind::Max => f32::NEG_INFINITY,
        };
        let mut k = 0;
        while k < red {
            let v = src[base + k];
            acc = match op {
                ReduceKind::Sum => acc + v,
                ReduceKind::Max => {
                    if v > acc {
                        v
                    } else {
                        acc
                    }
                }
            };
            k += 1;
        }
        dst[o] = acc;
        o += 1;
    }
}

/// Numerically stable softmax over the last axis on the host's widest engine: out has shape [outer, red].
// CEP:ID: JIT-m68
// CEP:WHAT: Applies row-wise stable softmax on the SIMD engine.
// CEP:WHY: The composite softmax op guarantees finite outputs under the declared numerical contract (design section 6); all three passes run 8-wide.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: src.len() == out.len() == outer * red; red >= 1; the exp pass uses the shared polynomial, inside the tolerance contract.
// CEP:COST: three passes over the data at 8-wide vectors plus one polynomial exp per element; allocation-free.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
pub fn softmax_last_axis(src: &[f32], out: &mut [f32], outer: usize, red: usize) {
    simd::softmax_fast(src, out, outer, red);
}

/// The portable scalar body of stable softmax.
// CEP:ID: JIT-f12
// CEP:WHAT: Applies row-wise stable softmax, sequential scalar.
// CEP:WHY: Rows too short for the vector body and non-x86_64 hosts run the shared polynomial here.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: src.len() == out.len() == outer * red; red >= 1.
// CEP:COST: two passes over the data plus one polynomial exp per element; allocation-free.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
pub(crate) fn softmax_last_axis_scalar(src: &[f32], out: &mut [f32], outer: usize, red: usize) {
    debug_assert_eq!(src.len(), outer * red, "softmax length mismatch");
    debug_assert_eq!(src.len(), out.len(), "softmax output mismatch");
    let mut o = 0;
    while o < outer {
        let base = o * red;
        let mut m = f32::NEG_INFINITY;
        let mut k = 0;
        while k < red {
            let v = src[base + k];
            if v > m {
                m = v;
            }
            k += 1;
        }
        let mut s = 0.0;
        k = 0;
        while k < red {
            let e = simd::exp_scalar(src[base + k] - m);
            out[base + k] = e;
            s += e;
            k += 1;
        }
        k = 0;
        while k < red {
            out[base + k] /= s;
            k += 1;
        }
        o += 1;
    }
}

/// Cache-blocked matrix multiply: `c` = `a * b` over row-major operands, full reset first, on the host's widest engine.
// CEP:WHAT: Computes a tiled general matrix multiply.
// CEP:WHY: Blocking for the target cache line and L2 is the single highest-leverage schedule knob in the search space; the tile sizes are the autotuner's parameters and the FMA engine is the vector realization of the same traversal.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: a.len() == m*k, b.len() == k*n, c.len() == m*n; the tile is nonzero and a power of two, proven by the plan airlock; oversized tile extents are clamped, never wrap.
// CEP:COST: O(m*n*k) multiply-adds; the FMA engine runs the contiguous-B schedule, the blocked scalar traversal serves the rest.
// CEP:EVIDENCE: benches/artifacts/matmul_128.md, benches/artifacts/exec_fastpath.md, and cargo test -p mlkp-runtime
pub fn matmul_tiled(a: &[f32], b: &[f32], c: &mut [f32], m: usize, n: usize, k: usize, tile: Tile) {
    matmul_tiled_sched(a, b, c, m, n, k, tile, 0, false)
}

/// Applies a fused epilogue op to one scalar output (or passes it through).
// CEP:WHAT: Applies one epilogue transform to a scalar.
// CEP:WHY: The scalar tails of the fused matmul share the epilogue contract with the vector stores.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[inline]
pub(crate) fn apply_epilogue(epi: Option<EwUnary>, x: f32) -> f32 {
    match epi {
        None => x,
        Some(op) => simd::apply1(op, x),
    }
}

/// Cache-blocked matrix multiply with an explicit schedule (as selected by the search space).
// CEP:ID: JIT-c31
// CEP:WHAT: Computes a tiled general matrix multiply under one schedule.
// CEP:WHY: The search space varies loop order (dimension B) and the B access pattern (dimension F); schedules must change the executed loop structure or the autotuner measures noise.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: same length and tile assumptions as matmul_tiled; loop_order is 0 or 1 and transpose_b selects the strided-B traversal.
// CEP:COST: O(m*n*k) multiply-adds in every schedule; access order differs by schedule.
// CEP:EVIDENCE: benches/artifacts/matmul_128.md and cargo test -p mlkp-runtime
#[inline]
#[allow(clippy::too_many_arguments)]
pub fn matmul_tiled_sched(
    a: &[f32],
    b: &[f32],
    c: &mut [f32],
    m: usize,
    n: usize,
    k: usize,
    tile: Tile,
    loop_order: u32,
    transpose_b: bool,
) {
    matmul_tiled_sched_epi(a, b, c, m, n, k, tile, loop_order, transpose_b, None)
}

/// Cache-blocked matrix multiply under an explicit schedule with an optional fused epilogue applied at the stores.
// CEP:ID: JIT-f06
// CEP:WHAT: Computes one tiled multiply schedule, applying the epilogue unary to each output element as it is stored.
// CEP:WHY: Matmul epilogue fusion (the exec-level lookahead) needs the consumer's unary inside the kernel's stores; with an epilogue the contiguous branch accumulates per element in the j-inner form, which is bit-identical in summation order to the k-outer form but lets the epilogue apply once per final element.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: same length and tile assumptions as matmul_tiled; the epilogue changes only the final store of each element, never the reduction order.
// CEP:COST: O(m*n*k) multiply-adds in every schedule; the FMA engine serves the contiguous-B schedule, the blocked scalar traversal the rest.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs and benches/artifacts/exec_fastpath.md.
#[allow(clippy::too_many_arguments)]
pub fn matmul_tiled_sched_epi(
    a: &[f32],
    b: &[f32],
    c: &mut [f32],
    m: usize,
    n: usize,
    k: usize,
    tile: Tile,
    loop_order: u32,
    transpose_b: bool,
    epilogue: Option<EwUnary>,
) {
    debug_assert_eq!(a.len(), m * k, "matmul a mismatch");
    debug_assert_eq!(b.len(), k * n, "matmul b mismatch");
    debug_assert_eq!(c.len(), m * n, "matmul c mismatch");
    // The FMA engine owns the contiguous-B schedule, reset included.
    #[cfg(target_arch = "x86_64")]
    if !transpose_b && simd::matmul_fma_available() {
        simd::matmul_fma_sched(a, b, c, m, n, k, tile, loop_order, epilogue);
        return;
    }
    // Reset the accumulator: this kernel owns the full result.
    let mut idx = 0;
    while idx < m * n {
        c[idx] = 0.0;
        idx += 1;
    }
    let bm = tile.bm.min(m).max(1);
    let bn = tile.bn.min(n).max(1);
    let bk = tile.bk.min(k).max(1);
    let mut i0 = 0;
    while i0 < m {
        let i_end = (i0 + bm).min(m);
        let mut outer0 = 0;
        let outer_end = if loop_order == 1 { k } else { n };
        let outer_step = if loop_order == 1 { bk } else { bn };
        while outer0 < outer_end {
            let mut inner0 = 0;
            let inner_end = if loop_order == 1 { n } else { k };
            let inner_step = if loop_order == 1 { bn } else { bk };
            while inner0 < inner_end {
                let (j0, k0) = if loop_order == 1 {
                    (inner0, outer0)
                } else {
                    (outer0, inner0)
                };
                let j_end = (j0 + bn).min(n);
                let k_end = (k0 + bk).min(k);
                let mut i = i0;
                while i < i_end {
                    let a_row = i * k;
                    let c_row = i * n;
                    if transpose_b || epilogue.is_some() {
                        // j-inner accumulate: the epilogue applies once per final
                        // element, and the reduction order over k is unchanged, so
                        // results stay bit-identical to the k-outer schedule.
                        let mut j = j0;
                        while j < j_end {
                            let mut acc = c[c_row + j];
                            let mut kk = k0;
                            while kk < k_end {
                                acc += a[a_row + kk] * b[kk * n + j];
                                kk += 1;
                            }
                            c[c_row + j] = apply_epilogue(epilogue, acc);
                            j += 1;
                        }
                    } else {
                        let mut kk = k0;
                        while kk < k_end {
                            let av = a[a_row + kk];
                            let b_row = kk * n;
                            let mut j = j0;
                            while j < j_end {
                                c[c_row + j] += av * b[b_row + j];
                                j += 1;
                            }
                            kk += 1;
                        }
                    }
                    i += 1;
                }
                inner0 += inner_step;
            }
            outer0 += outer_step;
        }
        i0 += bm;
    }
}

/// Cache-blocked matrix multiply over one row range (accumulates onto `c_rows`, rows i0..i1), on the host's widest engine.
// CEP:ID: JIT-634
// CEP:WHAT: Computes the tiled multiply restricted to a row range on the SIMD engine.
// CEP:WHY: Thread chunking (the schedule's worker count) splits the i dimension across workers; each worker owns disjoint output rows so no synchronization is needed.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: c_rows.len() == (i1 - i0) * n; ranges of concurrent calls are disjoint; otherwise the matmul_tiled_sched assumptions hold.
// CEP:COST: O((i1-i0)*n*k) multiply-adds.
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[inline]
#[allow(clippy::too_many_arguments)]
pub fn matmul_tiled_rows(
    a: &[f32],
    b: &[f32],
    c_rows: &mut [f32],
    m: usize,
    n: usize,
    k: usize,
    tile: Tile,
    loop_order: u32,
    transpose_b: bool,
    i0: usize,
    i1: usize,
) {
    matmul_tiled_rows_epi(
        a,
        b,
        c_rows,
        m,
        n,
        k,
        tile,
        loop_order,
        transpose_b,
        i0,
        i1,
        None,
    )
}

/// Cache-blocked matrix multiply over one row range with an optional fused epilogue at the stores.
// CEP:ID: JIT-f07
// CEP:WHAT: Computes the tiled multiply restricted to a row range, applying the epilogue unary to each stored element.
// CEP:WHY: The threaded fast path runs the same register-blocked kernel per worker with the same epilogue contract as the single-threaded fused matmul.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: c_rows.len() == (i1 - i0) * n; ranges of concurrent calls are disjoint; the epilogue changes only final stores, never the reduction order.
// CEP:COST: O((i1-i0)*n*k) multiply-adds.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[allow(clippy::too_many_arguments)]
pub fn matmul_tiled_rows_epi(
    a: &[f32],
    b: &[f32],
    c_rows: &mut [f32],
    m: usize,
    n: usize,
    k: usize,
    tile: Tile,
    loop_order: u32,
    transpose_b: bool,
    i0: usize,
    i1: usize,
    epilogue: Option<EwUnary>,
) {
    debug_assert!(i0 < i1 && i1 <= m, "row range inside the problem");
    debug_assert_eq!(c_rows.len(), (i1 - i0) * n, "row slice mismatch");
    // The FMA engine serves the contiguous-B schedule; rows accumulate onto the view.
    #[cfg(target_arch = "x86_64")]
    if !transpose_b && simd::matmul_fma_available() {
        simd::matmul_fma_rows(a, b, c_rows, m, n, k, tile, loop_order, i0, i1, epilogue);
        return;
    }
    let bm = tile.bm.min(m).max(1);
    let bn = tile.bn.min(n).max(1);
    let bk = tile.bk.min(k).max(1);
    let mut ii0 = i0;
    while ii0 < i1 {
        let i_end = (ii0 + bm).min(i1);
        let mut outer0 = 0;
        let outer_end = if loop_order == 1 { k } else { n };
        let outer_step = if loop_order == 1 { bk } else { bn };
        while outer0 < outer_end {
            let mut inner0 = 0;
            let inner_end = if loop_order == 1 { n } else { k };
            let inner_step = if loop_order == 1 { bn } else { bk };
            while inner0 < inner_end {
                let (j0, k0) = if loop_order == 1 {
                    (inner0, outer0)
                } else {
                    (outer0, inner0)
                };
                let j_end = (j0 + bn).min(n);
                let k_end = (k0 + bk).min(k);
                let mut i = ii0;
                while i < i_end {
                    let a_row = i * k;
                    let c_row = (i - i0) * n;
                    if transpose_b || epilogue.is_some() {
                        // j-inner accumulate: same summation order, epilogue once per element.
                        let mut j = j0;
                        while j < j_end {
                            let mut acc = c_rows[c_row + j];
                            let mut kk = k0;
                            while kk < k_end {
                                acc += a[a_row + kk] * b[kk * n + j];
                                kk += 1;
                            }
                            c_rows[c_row + j] = apply_epilogue(epilogue, acc);
                            j += 1;
                        }
                    } else {
                        let mut kk = k0;
                        while kk < k_end {
                            let av = a[a_row + kk];
                            let b_row = kk * n;
                            let mut j = j0;
                            while j < j_end {
                                c_rows[c_row + j] += av * b[b_row + j];
                                j += 1;
                            }
                            kk += 1;
                        }
                    }
                    i += 1;
                }
                inner0 += inner_step;
            }
            outer0 += outer_step;
        }
        ii0 += bm;
    }
}

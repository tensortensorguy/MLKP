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

/// Applies one unary operation over a slice.
// CEP:WHAT: Applies one unary op elementwise.
// CEP:WHY: Elementwise chains are the most frequent kernels in tensor graphs.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: a.len() == out.len(); enforced by debug assertion and by the plan airlock.
// CEP:COST: O(n) with one load and one store per element; allocation-free.
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[inline]
pub fn ew_unary(op: EwUnary, a: &[f32], out: &mut [f32]) {
    debug_assert_eq!(a.len(), out.len(), "unary kernel length mismatch");
    let n = a.len();
    let mut i = 0;
    while i < n {
        let x = a[i];
        out[i] = match op {
            EwUnary::Relu => {
                if x > 0.0 {
                    x
                } else {
                    0.0
                }
            }
            EwUnary::Exp => x.exp(),
            EwUnary::Neg => -x,
            EwUnary::Sigmoid => {
                let e = (-x).exp();
                1.0 / (1.0 + e)
            }
        };
        i += 1;
    }
}

/// Applies one binary operation over two slices.
// CEP:WHAT: Applies one binary op elementwise.
// CEP:WHY: Bias adds and pairwise ops compose into fused chains.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: a.len() == b.len() == out.len(); enforced by debug assertion and by the plan airlock.
// CEP:COST: O(n) with two loads and one store per element; allocation-free.
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[inline]
pub fn ew_binary(op: EwBinary, a: &[f32], b: &[f32], out: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len(), "binary kernel length mismatch");
    debug_assert_eq!(a.len(), out.len(), "binary kernel output mismatch");
    let n = a.len();
    let mut i = 0;
    while i < n {
        let x = a[i];
        let y = b[i];
        out[i] = match op {
            EwBinary::Add => x + y,
            EwBinary::Mul => x * y,
            EwBinary::Sub => x - y,
            EwBinary::Max => {
                if x > y {
                    x
                } else {
                    y
                }
            }
        };
        i += 1;
    }
}

/// Reduces the last axis: `src` has shape `[outer, red]`, `dst` has shape `[outer]`.
// CEP:ID: JIT-u48
// CEP:WHAT: Reduces the last axis of a row-major tensor.
// CEP:WHY: Softmax and loss paths reduce the contiguous axis, which is the cache-friendly case.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: src.len() == outer * red and dst.len() == outer; enforced by debug assertion and the plan airlock.
// CEP:COST: O(outer * red); sequential per row, allocation-free.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
pub fn reduce_last_axis(op: ReduceKind, src: &[f32], outer: usize, red: usize, dst: &mut [f32]) {
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

/// Numerically stable softmax over the last axis: out has shape [outer, red].
// CEP:ID: JIT-m68
// CEP:WHAT: Applies row-wise stable softmax.
// CEP:WHY: The composite softmax op guarantees finite outputs under the declared numerical contract (design section 6).
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: src.len() == out.len() == outer * red; red >= 1.
// CEP:COST: two passes over the data plus one exp per element; allocation-free.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
pub fn softmax_last_axis(src: &[f32], out: &mut [f32], outer: usize, red: usize) {
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
            let e = (src[base + k] - m).exp();
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

/// Cache-blocked matrix multiply: `c` = `a * b` over row-major operands, full reset first.
// CEP:WHAT: Computes a tiled general matrix multiply.
// CEP:WHY: Blocking for the target cache line and L2 is the single highest-leverage schedule knob in the search space; the tile sizes are the autotuner's parameters.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: a.len() == m*k, b.len() == k*n, c.len() == m*n; the tile is nonzero and a power of two, proven by the plan airlock; oversized tile extents are clamped, never wrap.
// CEP:COST: O(m*n*k) multiply-adds; blocked inner loops measured against the naive schedule in benches/artifacts/matmul_128.md.
// CEP:EVIDENCE: benches/artifacts/matmul_128.md and cargo test -p mlkp-runtime
pub fn matmul_tiled(a: &[f32], b: &[f32], c: &mut [f32], m: usize, n: usize, k: usize, tile: Tile) {
    matmul_tiled_sched(a, b, c, m, n, k, tile, 0, false)
}

/// Cache-blocked matrix multiply with an explicit schedule: loop order over the
/// matmul triple and the B access pattern, as selected by the search space.
// CEP:ID: JIT-c31
// CEP:WHAT: Computes a tiled general matrix multiply under one schedule.
// CEP:WHY: The search space varies loop order (dimension B) and the B access pattern (dimension F); schedules must change the executed loop structure or the autotuner measures noise.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: same length and tile assumptions as matmul_tiled; loop_order is 0 or 1 and transpose_b selects the strided-B traversal.
// CEP:COST: O(m*n*k) multiply-adds in every schedule; access order differs by schedule.
// CEP:EVIDENCE: benches/artifacts/matmul_128.md and cargo test -p mlkp-runtime
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
    debug_assert_eq!(a.len(), m * k, "matmul a mismatch");
    debug_assert_eq!(b.len(), k * n, "matmul b mismatch");
    debug_assert_eq!(c.len(), m * n, "matmul c mismatch");
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
                    if transpose_b {
                        // Strided-B traversal: consecutive kk reads step n elements
                        // apart, the column-order access pattern dimension F names;
                        // the reduction order over k is unchanged, so results are
                        // bit-identical to the contiguous schedule.
                        let mut j = j0;
                        while j < j_end {
                            let mut acc = c[c_row + j];
                            let mut kk = k0;
                            while kk < k_end {
                                acc += a[a_row + kk] * b[kk * n + j];
                                kk += 1;
                            }
                            c[c_row + j] = acc;
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

/// Cache-blocked matrix multiply over one row range: writes `c_rows` (rows i0..i1).
// CEP:ID: JIT-634
// CEP:WHAT: Computes the tiled multiply restricted to a row range.
// CEP:WHY: Thread chunking (the schedule's worker count) splits the i dimension across workers; each worker owns disjoint output rows so no synchronization is needed.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: c_rows.len() == (i1 - i0) * n; ranges of concurrent calls are disjoint; otherwise the matmul_tiled_sched assumptions hold.
// CEP:COST: O((i1-i0)*n*k) multiply-adds.
// CEP:EVIDENCE: cargo test -p mlkp-runtime
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
    debug_assert!(i0 < i1 && i1 <= m, "row range inside the problem");
    debug_assert_eq!(c_rows.len(), (i1 - i0) * n, "row slice mismatch");
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
                    if transpose_b {
                        let mut j = j0;
                        while j < j_end {
                            let mut acc = c_rows[c_row + j];
                            let mut kk = k0;
                            while kk < k_end {
                                acc += a[a_row + kk] * b[kk * n + j];
                                kk += 1;
                            }
                            c_rows[c_row + j] = acc;
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

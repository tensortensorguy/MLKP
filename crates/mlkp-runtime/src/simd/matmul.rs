// CEP:FILE: crates/mlkp-runtime/src/simd/matmul.rs
// CEP:WHAT: The matrix-multiply stage: the register-blocked AVX2+FMA kernel over the schedule's tile traversal with optional fused epilogues, plus the scalar epilogue helpers.
// CEP:WHY: The CEP-0 atomic-file limit keeps every hot-kernel file under 400 lines of code; the engine is split by stage with one shared polynomial in the module root so every path (vector body, scalar tail, non-x86_64 host) runs the same fitted algorithm.
// CEP:CLASS: CEP-0
// CEP:STATUS: complete
// CEP:FAILURE: none; no kernel arithmetic panics, bounds are debug-asserted, exp saturates to IEEE inf and zero outside its finite range and propagates NaN, and feature dispatch is cached atomically with no lock or allocation.
// CEP:ASSUMES: x86_64 hosts always provide SSE2 (the vectorized elementwise baseline); the FMA matrix multiply requires AVX2 and FMA and serves the contiguous-B schedule, while the strided-B schedule keeps the blocked scalar traversal so schedule dimension F stays real; exp outputs below 2^-126 flush to zero (absolute error below 1e-37, inside the plan's absolute-tolerance contract); the polynomial's relative error is below 1.3e-7 over the finite mid-range (fitted in scripts/fit_exp_poly.py, asserted in tests).
// CEP:COST: elementwise O(n) at 8-wide vectors; matmul O(m*n*k) with 8-wide FMAs at up to two per cycle; no allocation, no locks, no formatting on any path.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs and benches/artifacts/exec_fastpath.md.
// CEP:UNSAFE: every function here is a feature-gated intrinsic kernel; the AVX2 and FMA features are guaranteed by the engine() dispatch before the call and the #[target_feature] signatures, and all loads and stores stay in bounds by the loop conditions and the airlock-proven slice lengths.

use super::{apply1, transform_avx2};
use crate::kernels::EwUnary;
use core::arch::x86_64::*;

// CEP:UNSAFE: the feature-gated vector intrinsics require the named target features; guaranteed by the engine() dispatch and the #[target_feature] signature; all loads and stores stay in bounds by the loop conditions and the airlock-proven slice lengths.
// CEP:ID: JIT-e10
// CEP:WHAT: Applies the epilogue transform to one output vector (or passes it through).
// CEP:WHY: Matmul epilogue fusion applies the consumer's unary op inside the store, one transform per accumulator vector.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: AVX2 and FMA enabled when epi is Some.
// CEP:COST: O(1) per vector.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn epi_vec(
    epi: Option<EwUnary>,
    v: core::arch::x86_64::__m256,
) -> core::arch::x86_64::__m256 {
    // CEP:UNSAFE: the feature-gated vector intrinsics require the named target features; guaranteed by the engine() dispatch and the #[target_feature] signature; all loads and stores stay in bounds by the loop conditions and the airlock-proven slice lengths.
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    match epi {
        None => v,
        Some(op) => transform_avx2(op, v),
    }
}

// CEP:WHAT: Applies the epilogue transform to one scalar output (or passes it through).
// CEP:WHY: The scalar tail of the fused matmul shares the epilogue contract.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
fn epi_scalar(epi: Option<EwUnary>, x: f32) -> f32 {
    match epi {
        None => x,
        Some(op) => apply1(op, x),
    }
}

// CEP:ID: JIT-e11
// CEP:WHAT: Runs the register-blocked AVX2+FMA matmul over the schedule's tile traversal with an optional epilogue, writing full output rows.
// CEP:WHY: The matmul is 93 percent of measured execution; four-row register blocking with two 8-wide FMA accumulator pairs per row turns 2.6 cycles per MAC into the FMA engine's throughput while the schedule's tile and loop-order dimensions keep their meaning.
// CEP:STATUS: complete
// CEP:FAILURE: none; length mismatches are debug-asserted and proven by the plan airlock; ragged extents fall to scalar tails.
// CEP:ASSUMES: a.len() == m*k, b.len() == k*n, c.len() == m*n; c was reset to zero first (the kernel owns the full result); the contiguous-B schedule.
// CEP:COST: O(m*n*k) with 8-wide FMAs; B rows are reloaded once per four-row block, A values are broadcast per row and reduction step.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs and benches/artifacts/exec_fastpath.md.
#[allow(clippy::too_many_arguments)]
pub(crate) fn matmul_fma_sched(
    a: &[f32],
    b: &[f32],
    c: &mut [f32],
    m: usize,
    n: usize,
    k: usize,
    tile: crate::kernels::Tile,
    loop_order: u32,
    epilogue: Option<EwUnary>,
) {
    debug_assert_eq!(a.len(), m * k, "matmul a mismatch");
    debug_assert_eq!(b.len(), k * n, "matmul b mismatch");
    debug_assert_eq!(c.len(), m * n, "matmul c mismatch");
    let mut idx = 0;
    while idx < m * n {
        c[idx] = 0.0;
        idx += 1;
    }
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    unsafe {
        matmul_fma_sched_avx2(a, b, c, m, n, k, tile, loop_order, epilogue, 0, m, 0);
    }
}

// CEP:ID: JIT-e12
// CEP:WHAT: Runs the register-blocked FMA matmul restricted to a row range, accumulating onto the given rows.
// CEP:WHY: Thread chunking splits the i dimension across workers; each worker runs the same register-blocked kernel on its own rows with no synchronization.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: c_rows.len() == (i1 - i0) * n; ranges of concurrent calls are disjoint; the caller owns the zeroing discipline of c_rows exactly as in the scalar rows kernel.
// CEP:COST: O((i1-i0)*n*k) with 8-wide FMAs.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[allow(clippy::too_many_arguments)]
pub(crate) fn matmul_fma_rows(
    a: &[f32],
    b: &[f32],
    c_rows: &mut [f32],
    m: usize,
    n: usize,
    k: usize,
    tile: crate::kernels::Tile,
    loop_order: u32,
    i0: usize,
    i1: usize,
    epilogue: Option<EwUnary>,
) {
    debug_assert!(i0 < i1 && i1 <= m, "row range inside the problem");
    debug_assert_eq!(c_rows.len(), (i1 - i0) * n, "row slice mismatch");
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    unsafe {
        matmul_fma_sched_avx2(
            a, b, c_rows, m, n, k, tile, loop_order, epilogue, i0, i1, i0,
        );
    }
}

// CEP:UNSAFE: the feature-gated vector intrinsics require the named target features; guaranteed by the engine() dispatch and the #[target_feature] signature; all loads and stores stay in bounds by the loop conditions and the airlock-proven slice lengths.
// CEP:ID: JIT-e31
// CEP:ID: JIT-e13
// CEP:WHAT: The schedule traversal of the FMA engine: i blocks, then the schedule's outer/inner tile order over (j, k), then the register-blocked tile kernel.
// CEP:WHY: The autotuner's loop-order and tile dimensions must change real loop structure in the vector engine too, so candidates measure schedules, not one fixed order.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: AVX2 and FMA enabled; extents already clamped by the caller; c_base is the row-index origin of the c view (0 for full outputs, i0 for row slices).
// CEP:COST: O(m*n*k).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[allow(clippy::too_many_arguments)]
unsafe fn matmul_fma_sched_avx2(
    a: &[f32],
    b: &[f32],
    c: &mut [f32],
    m: usize,
    n: usize,
    k: usize,
    tile: crate::kernels::Tile,
    loop_order: u32,
    epilogue: Option<EwUnary>,
    i_lo: usize,
    i_hi: usize,
    c_base: usize,
) {
    // CEP:UNSAFE: the feature-gated vector intrinsics require the named target features; guaranteed by the engine() dispatch and the #[target_feature] signature; all loads and stores stay in bounds by the loop conditions and the airlock-proven slice lengths.
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    let bm = tile.bm.min(m).max(1);
    let bn = tile.bn.min(n).max(1);
    let bk = tile.bk.min(k).max(1);
    let mut ii = i_lo;
    while ii < i_hi {
        let i_end = (ii + bm).min(i_hi);
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
                fma_tile(
                    a, b, c, n, k, ii, i_end, j0, j_end, k0, k_end, epilogue, c_base,
                );
                inner0 += inner_step;
            }
            outer0 += outer_step;
        }
        ii += bm;
    }
}

// CEP:UNSAFE: the feature-gated vector intrinsics require the named target features; guaranteed by the engine() dispatch and the #[target_feature] signature; all loads and stores stay in bounds by the loop conditions and the airlock-proven slice lengths.
// CEP:ID: JIT-e32
// CEP:WHAT: Runs the register-blocked kernel over one tile: four-row blocks with two 8-wide FMA accumulator pairs, then single rows, then scalar tails per ragged extent.
// CEP:WHY: Four rows amortize the B loads across four FMA chains; accumulators start from c so multi-k-tile schedules accumulate correctly.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: AVX2 and FMA enabled; extents within the problem; c row base offset shifts the store index for row-slice views.
// CEP:COST: 8 FMAs per reduction step per four-row block plus one B pair load.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[allow(clippy::too_many_arguments)]
unsafe fn fma_tile(
    a: &[f32],
    b: &[f32],
    c: &mut [f32],
    n: usize,
    k: usize,
    i_lo: usize,
    i_hi: usize,
    j0: usize,
    j_end: usize,
    k0: usize,
    k_end: usize,
    epilogue: Option<EwUnary>,
    c_base: usize,
) {
    // CEP:UNSAFE: the feature-gated vector intrinsics require the named target features; guaranteed by the engine() dispatch and the #[target_feature] signature; all loads and stores stay in bounds by the loop conditions and the airlock-proven slice lengths.
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    let mut i = i_lo;
    while i + 4 <= i_hi {
        fma_rows4(a, b, c, n, k, i, j0, j_end, k0, k_end, epilogue, c_base);
        i += 4;
    }
    while i < i_hi {
        fma_row1(a, b, c, n, k, i, j0, j_end, k0, k_end, epilogue, c_base);
        i += 1;
    }
}

// CEP:UNSAFE: the feature-gated vector intrinsics require the named target features; guaranteed by the engine() dispatch and the #[target_feature] signature; all loads and stores stay in bounds by the loop conditions and the airlock-proven slice lengths.
// CEP:ID: JIT-e33
// CEP:ID: JIT-e14
// CEP:WHAT: Multiplies one four-row block against one tile: two 8-wide FMA accumulator pairs per row, 16 columns per iteration, then one 8-wide chunk, then a scalar column tail.
// CEP:WHY: Four rows share each B load pair across four broadcast FMAs, which is the register-blocking shape that reaches FMA throughput at these tile sizes.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: AVX2 and FMA enabled; i + 4 <= the row bound; column and depth extents within the tile.
// CEP:COST: 8 FMAs and 2 B loads per reduction step, plus per-row broadcasts.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[allow(clippy::too_many_arguments)]
unsafe fn fma_rows4(
    a: &[f32],
    b: &[f32],
    c: &mut [f32],
    n: usize,
    k: usize,
    i: usize,
    j0: usize,
    j_end: usize,
    k0: usize,
    k_end: usize,
    epilogue: Option<EwUnary>,
    c_base: usize,
) {
    // CEP:UNSAFE: the feature-gated vector intrinsics require the named target features; guaranteed by the engine() dispatch and the #[target_feature] signature; all loads and stores stay in bounds by the loop conditions and the airlock-proven slice lengths.
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    let mut j = j0;
    while j + 16 <= j_end {
        let mut a0 = _mm256_loadu_ps(c.as_ptr().add((i - c_base) * n + j));
        let mut a1 = _mm256_loadu_ps(c.as_ptr().add((i - c_base) * n + j + 8));
        let mut b0 = _mm256_loadu_ps(c.as_ptr().add((i + 1 - c_base) * n + j));
        let mut b1 = _mm256_loadu_ps(c.as_ptr().add((i + 1 - c_base) * n + j + 8));
        let mut cc0 = _mm256_loadu_ps(c.as_ptr().add((i + 2 - c_base) * n + j));
        let mut cc1 = _mm256_loadu_ps(c.as_ptr().add((i + 2 - c_base) * n + j + 8));
        let mut d0 = _mm256_loadu_ps(c.as_ptr().add((i + 3 - c_base) * n + j));
        let mut d1 = _mm256_loadu_ps(c.as_ptr().add((i + 3 - c_base) * n + j + 8));
        let mut kk = k0;
        while kk < k_end {
            let bb0 = _mm256_loadu_ps(b.as_ptr().add(kk * n + j));
            let bb1 = _mm256_loadu_ps(b.as_ptr().add(kk * n + j + 8));
            a0 = _mm256_fmadd_ps(_mm256_set1_ps(*a.as_ptr().add(i * k + kk)), bb0, a0);
            a1 = _mm256_fmadd_ps(_mm256_set1_ps(*a.as_ptr().add(i * k + kk)), bb1, a1);
            b0 = _mm256_fmadd_ps(_mm256_set1_ps(*a.as_ptr().add((i + 1) * k + kk)), bb0, b0);
            b1 = _mm256_fmadd_ps(_mm256_set1_ps(*a.as_ptr().add((i + 1) * k + kk)), bb1, b1);
            cc0 = _mm256_fmadd_ps(_mm256_set1_ps(*a.as_ptr().add((i + 2) * k + kk)), bb0, cc0);
            cc1 = _mm256_fmadd_ps(_mm256_set1_ps(*a.as_ptr().add((i + 2) * k + kk)), bb1, cc1);
            d0 = _mm256_fmadd_ps(_mm256_set1_ps(*a.as_ptr().add((i + 3) * k + kk)), bb0, d0);
            d1 = _mm256_fmadd_ps(_mm256_set1_ps(*a.as_ptr().add((i + 3) * k + kk)), bb1, d1);
            kk += 1;
        }
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i - c_base) * n + j),
            epi_vec(epilogue, a0),
        );
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i - c_base) * n + j + 8),
            epi_vec(epilogue, a1),
        );
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i + 1 - c_base) * n + j),
            epi_vec(epilogue, b0),
        );
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i + 1 - c_base) * n + j + 8),
            epi_vec(epilogue, b1),
        );
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i + 2 - c_base) * n + j),
            epi_vec(epilogue, cc0),
        );
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i + 2 - c_base) * n + j + 8),
            epi_vec(epilogue, cc1),
        );
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i + 3 - c_base) * n + j),
            epi_vec(epilogue, d0),
        );
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i + 3 - c_base) * n + j + 8),
            epi_vec(epilogue, d1),
        );
        j += 16;
    }
    while j + 8 <= j_end {
        let mut a0 = _mm256_loadu_ps(c.as_ptr().add((i - c_base) * n + j));
        let mut b0 = _mm256_loadu_ps(c.as_ptr().add((i + 1 - c_base) * n + j));
        let mut cc0 = _mm256_loadu_ps(c.as_ptr().add((i + 2 - c_base) * n + j));
        let mut d0 = _mm256_loadu_ps(c.as_ptr().add((i + 3 - c_base) * n + j));
        let mut kk = k0;
        while kk < k_end {
            let bb = _mm256_loadu_ps(b.as_ptr().add(kk * n + j));
            a0 = _mm256_fmadd_ps(_mm256_set1_ps(*a.as_ptr().add(i * k + kk)), bb, a0);
            b0 = _mm256_fmadd_ps(_mm256_set1_ps(*a.as_ptr().add((i + 1) * k + kk)), bb, b0);
            cc0 = _mm256_fmadd_ps(_mm256_set1_ps(*a.as_ptr().add((i + 2) * k + kk)), bb, cc0);
            d0 = _mm256_fmadd_ps(_mm256_set1_ps(*a.as_ptr().add((i + 3) * k + kk)), bb, d0);
            kk += 1;
        }
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i - c_base) * n + j),
            epi_vec(epilogue, a0),
        );
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i + 1 - c_base) * n + j),
            epi_vec(epilogue, b0),
        );
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i + 2 - c_base) * n + j),
            epi_vec(epilogue, cc0),
        );
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i + 3 - c_base) * n + j),
            epi_vec(epilogue, d0),
        );
        j += 8;
    }
    while j < j_end {
        let mut r = 0;
        while r < 4 {
            let mut acc = c[(i + r - c_base) * n + j];
            let mut kk = k0;
            while kk < k_end {
                acc += a[(i + r) * k + kk] * b[kk * n + j];
                kk += 1;
            }
            c[(i + r - c_base) * n + j] = epi_scalar(epilogue, acc);
            r += 1;
        }
        j += 1;
    }
}

// CEP:UNSAFE: the feature-gated vector intrinsics require the named target features; guaranteed by the engine() dispatch and the #[target_feature] signature; all loads and stores stay in bounds by the loop conditions and the airlock-proven slice lengths.
// CEP:ID: JIT-e34
// CEP:ID: JIT-e15
// CEP:WHAT: Multiplies one single row against one tile with the same accumulator structure reduced to one row.
// CEP:WHY: Ragged row bounds (m not a multiple of four) must keep the vector engine, not fall to a scalar loop.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: AVX2 and FMA enabled; i within the row bound.
// CEP:COST: 2 FMAs and 2 B loads per reduction step in the 16-column body.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[allow(clippy::too_many_arguments)]
unsafe fn fma_row1(
    a: &[f32],
    b: &[f32],
    c: &mut [f32],
    n: usize,
    k: usize,
    i: usize,
    j0: usize,
    j_end: usize,
    k0: usize,
    k_end: usize,
    epilogue: Option<EwUnary>,
    c_base: usize,
) {
    // CEP:UNSAFE: the feature-gated vector intrinsics require the named target features; guaranteed by the engine() dispatch and the #[target_feature] signature; all loads and stores stay in bounds by the loop conditions and the airlock-proven slice lengths.
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    let mut j = j0;
    while j + 16 <= j_end {
        let mut a0 = _mm256_loadu_ps(c.as_ptr().add((i - c_base) * n + j));
        let mut a1 = _mm256_loadu_ps(c.as_ptr().add((i - c_base) * n + j + 8));
        let mut kk = k0;
        while kk < k_end {
            let bb0 = _mm256_loadu_ps(b.as_ptr().add(kk * n + j));
            let bb1 = _mm256_loadu_ps(b.as_ptr().add(kk * n + j + 8));
            let ra = _mm256_set1_ps(*a.as_ptr().add(i * k + kk));
            a0 = _mm256_fmadd_ps(ra, bb0, a0);
            a1 = _mm256_fmadd_ps(ra, bb1, a1);
            kk += 1;
        }
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i - c_base) * n + j),
            epi_vec(epilogue, a0),
        );
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i - c_base) * n + j + 8),
            epi_vec(epilogue, a1),
        );
        j += 16;
    }
    while j + 8 <= j_end {
        let mut a0 = _mm256_loadu_ps(c.as_ptr().add((i - c_base) * n + j));
        let mut kk = k0;
        while kk < k_end {
            let bb = _mm256_loadu_ps(b.as_ptr().add(kk * n + j));
            a0 = _mm256_fmadd_ps(_mm256_set1_ps(*a.as_ptr().add(i * k + kk)), bb, a0);
            kk += 1;
        }
        _mm256_storeu_ps(
            c.as_mut_ptr().add((i - c_base) * n + j),
            epi_vec(epilogue, a0),
        );
        j += 8;
    }
    while j < j_end {
        let mut acc = c[(i - c_base) * n + j];
        let mut kk = k0;
        while kk < k_end {
            acc += a[i * k + kk] * b[kk * n + j];
            kk += 1;
        }
        c[(i - c_base) * n + j] = epi_scalar(epilogue, acc);
        j += 1;
    }
}

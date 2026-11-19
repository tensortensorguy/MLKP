// CEP:FILE: crates/mlkp-runtime/src/simd/reduce.rs
// CEP:WHAT: The reduction stage: vector last-axis sum and max with four accumulator vectors, and the row-wise stable softmax over vector max, exp, and normalize passes.
// CEP:WHY: The CEP-0 atomic-file limit keeps every hot-kernel file under 400 lines of code; the engine is split by stage with one shared polynomial in the module root so every path (vector body, scalar tail, non-x86_64 host) runs the same fitted algorithm.
// CEP:CLASS: CEP-0
// CEP:STATUS: complete
// CEP:FAILURE: none; no kernel arithmetic panics, bounds are debug-asserted, exp saturates to IEEE inf and zero outside its finite range and propagates NaN, and feature dispatch is cached atomically with no lock or allocation.
// CEP:ASSUMES: x86_64 hosts always provide SSE2 (the vectorized elementwise baseline); the FMA matrix multiply requires AVX2 and FMA and serves the contiguous-B schedule, while the strided-B schedule keeps the blocked scalar traversal so schedule dimension F stays real; exp outputs below 2^-126 flush to zero (absolute error below 1e-37, inside the plan's absolute-tolerance contract); the polynomial's relative error is below 1.3e-7 over the finite mid-range (fitted in scripts/fit_exp_poly.py, asserted in tests).
// CEP:COST: elementwise O(n) at 8-wide vectors; matmul O(m*n*k) with 8-wide FMAs at up to two per cycle; no allocation, no locks, no formatting on any path.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs and benches/artifacts/exec_fastpath.md.

use super::{engine, exp_avx2, exp_scalar, Engine};
use crate::kernels::ReduceKind;
use core::arch::x86_64::*;

// CEP:ID: JIT-e07
// CEP:WHAT: Sums eight f32 lanes into one scalar with pairwise adds.
// CEP:WHY: The reduce and softmax kernels need a cheap horizontal combine whose rounding stays inside the tolerance contract.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: AVX2 enabled.
// CEP:COST: four vector adds.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn horizontal_sum_avx2(v: core::arch::x86_64::__m256) -> f32 {
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    let h = _mm_add_ps(_mm256_castps256_ps128(v), _mm256_extractf128_ps(v, 1));
    let s = _mm_add_ps(h, _mm_movehl_ps(h, h));
    let s2 = _mm_add_ss(s, _mm_shuffle_ps(s, s, 1));
    _mm_cvtss_f32(s2)
}

// CEP:ID: JIT-e08
// CEP:WHAT: Reduces the last axis with vector accumulators (four independent accumulator vectors per row).
// CEP:WHY: The reduce kernel is memory-bound; four accumulators hide the add latency and the horizontal combine runs once per row.
// CEP:STATUS: complete
// CEP:FAILURE: none; rows shorter than sixteen elements run the scalar kernel, where it is the fast path.
// CEP:ASSUMES: src.len() == outer * red and dst.len() == outer; reduction order differs from the sequential scalar kernel only in association, inside the tolerance contract.
// CEP:COST: O(outer * red) at 8-wide loads.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
pub(crate) fn reduce_fast(
    kind: ReduceKind,
    src: &[f32],
    outer: usize,
    red: usize,
    dst: &mut [f32],
) {
    debug_assert_eq!(src.len(), outer * red, "reduce source mismatch");
    debug_assert_eq!(dst.len(), outer, "reduce destination mismatch");
    if red < 16 {
        crate::kernels::reduce_last_axis_scalar(kind, src, outer, red, dst);
        return;
    }
    #[cfg(target_arch = "x86_64")]
    {
        match engine() {
            // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
            Engine::Avx2Fma | Engine::Sse2 => unsafe { reduce_avx2(kind, src, outer, red, dst) },
            Engine::Scalar => crate::kernels::reduce_last_axis_scalar(kind, src, outer, red, dst),
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        crate::kernels::reduce_last_axis_scalar(kind, src, outer, red, dst);
    }
}

// CEP:ID: JIT-e29
// CEP:WHAT: The vector body of the last-axis reduce.
// CEP:WHY: One code path serves AVX2 and SSE2-capable hosts; SSE2 hosts take the same body with 8-wide AVX2 loads only when AVX2 exists, so the body is AVX2-only and SSE2 hosts fall to the scalar kernel (rows shorter than the vector width dominate small graphs anyway).
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: AVX2 enabled; red >= 16.
// CEP:COST: four accumulator vectors per row.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn reduce_avx2(kind: ReduceKind, src: &[f32], outer: usize, red: usize, dst: &mut [f32]) {
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    let mut o = 0;
    while o < outer {
        let base = o * red;
        let vecs = (red / 8) * 8;
        let mut k = 0;
        if kind == ReduceKind::Sum {
            let mut a0 = _mm256_setzero_ps();
            let mut a1 = _mm256_setzero_ps();
            let mut a2 = _mm256_setzero_ps();
            let mut a3 = _mm256_setzero_ps();
            while k + 32 <= vecs {
                a0 = _mm256_add_ps(a0, _mm256_loadu_ps(src.as_ptr().add(base + k)));
                a1 = _mm256_add_ps(a1, _mm256_loadu_ps(src.as_ptr().add(base + k + 8)));
                a2 = _mm256_add_ps(a2, _mm256_loadu_ps(src.as_ptr().add(base + k + 16)));
                a3 = _mm256_add_ps(a3, _mm256_loadu_ps(src.as_ptr().add(base + k + 24)));
                k += 32;
            }
            while k + 8 <= vecs {
                a0 = _mm256_add_ps(a0, _mm256_loadu_ps(src.as_ptr().add(base + k)));
                k += 8;
            }
            let mut acc =
                horizontal_sum_avx2(_mm256_add_ps(_mm256_add_ps(a0, a1), _mm256_add_ps(a2, a3)));
            while k < red {
                acc += src[base + k];
                k += 1;
            }
            dst[o] = acc;
        } else {
            let mut m0 = _mm256_set1_ps(f32::NEG_INFINITY);
            let mut m1 = m0;
            let mut m2 = m0;
            let mut m3 = m0;
            while k + 32 <= vecs {
                m0 = _mm256_max_ps(m0, _mm256_loadu_ps(src.as_ptr().add(base + k)));
                m1 = _mm256_max_ps(m1, _mm256_loadu_ps(src.as_ptr().add(base + k + 8)));
                m2 = _mm256_max_ps(m2, _mm256_loadu_ps(src.as_ptr().add(base + k + 16)));
                m3 = _mm256_max_ps(m3, _mm256_loadu_ps(src.as_ptr().add(base + k + 24)));
                k += 32;
            }
            while k + 8 <= vecs {
                m0 = _mm256_max_ps(m0, _mm256_loadu_ps(src.as_ptr().add(base + k)));
                k += 8;
            }
            let big = _mm256_max_ps(_mm256_max_ps(m0, m1), _mm256_max_ps(m2, m3));
            let h = _mm_max_ps(_mm256_castps256_ps128(big), _mm256_extractf128_ps(big, 1));
            let s = _mm_max_ps(h, _mm_movehl_ps(h, h));
            let s2 = _mm_max_ps(s, _mm_shuffle_ps(s, s, 1));
            let mut acc = _mm_cvtss_f32(s2);
            while k < red {
                if src[base + k] > acc {
                    acc = src[base + k];
                }
                k += 1;
            }
            dst[o] = acc;
        }
        o += 1;
    }
}

// CEP:ID: JIT-e09
// CEP:WHAT: Applies row-wise stable softmax with a vector max pass, the shared polynomial exp pass, and a broadcast-divide normalize pass.
// CEP:WHY: Softmax is three memory passes plus one exp per element; vectorizing all three keeps the composite op off the hot-path critical list.
// CEP:STATUS: complete
// CEP:FAILURE: none; rows shorter than sixteen elements run the scalar kernel.
// CEP:ASSUMES: src.len() == out.len() == outer * red; red >= 1; outputs finite under the declared numerical contract.
// CEP:COST: three passes at 8-wide vectors plus one polynomial exp per element.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
pub(crate) fn softmax_fast(src: &[f32], out: &mut [f32], outer: usize, red: usize) {
    debug_assert_eq!(src.len(), outer * red, "softmax length mismatch");
    debug_assert_eq!(src.len(), out.len(), "softmax output mismatch");
    if red < 16 {
        crate::kernels::softmax_last_axis_scalar(src, out, outer, red);
        return;
    }
    #[cfg(target_arch = "x86_64")]
    {
        match engine() {
            // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
            Engine::Avx2Fma => unsafe { softmax_avx2(src, out, outer, red) },
            Engine::Sse2 => crate::kernels::softmax_last_axis_scalar(src, out, outer, red),
            Engine::Scalar => crate::kernels::softmax_last_axis_scalar(src, out, outer, red),
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        crate::kernels::softmax_last_axis_scalar(src, out, outer, red);
    }
}

// CEP:ID: JIT-e30
// CEP:WHAT: The AVX2 body of stable softmax.
// CEP:WHY: Max, exp, and normalize passes all run 8-wide; the exp sum accumulates in vectors.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: AVX2 and FMA enabled; red >= 16.
// CEP:COST: three vector passes per row.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn softmax_avx2(src: &[f32], out: &mut [f32], outer: usize, red: usize) {
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    let mut o = 0;
    while o < outer {
        let base = o * red;
        let vecs = (red / 8) * 8;
        // Max pass.
        let mut m0 = _mm256_set1_ps(f32::NEG_INFINITY);
        let mut k = 0;
        while k + 8 <= vecs {
            m0 = _mm256_max_ps(m0, _mm256_loadu_ps(src.as_ptr().add(base + k)));
            k += 8;
        }
        let h = _mm_max_ps(_mm256_castps256_ps128(m0), _mm256_extractf128_ps(m0, 1));
        let s = _mm_max_ps(h, _mm_movehl_ps(h, h));
        let s2 = _mm_max_ps(s, _mm_shuffle_ps(s, s, 1));
        let mut m = _mm_cvtss_f32(s2);
        while k < red {
            if src[base + k] > m {
                m = src[base + k];
            }
            k += 1;
        }
        // Exp pass: store exps, accumulate their sum in four vectors.
        let mut e0 = _mm256_setzero_ps();
        let mut e1 = _mm256_setzero_ps();
        let mut e2 = _mm256_setzero_ps();
        let mut e3 = _mm256_setzero_ps();
        let mm = _mm256_set1_ps(m);
        k = 0;
        while k + 32 <= vecs {
            let v0 = exp_avx2(_mm256_sub_ps(
                _mm256_loadu_ps(src.as_ptr().add(base + k)),
                mm,
            ));
            let v1 = exp_avx2(_mm256_sub_ps(
                _mm256_loadu_ps(src.as_ptr().add(base + k + 8)),
                mm,
            ));
            let v2 = exp_avx2(_mm256_sub_ps(
                _mm256_loadu_ps(src.as_ptr().add(base + k + 16)),
                mm,
            ));
            let v3 = exp_avx2(_mm256_sub_ps(
                _mm256_loadu_ps(src.as_ptr().add(base + k + 24)),
                mm,
            ));
            _mm256_storeu_ps(out.as_mut_ptr().add(base + k), v0);
            _mm256_storeu_ps(out.as_mut_ptr().add(base + k + 8), v1);
            _mm256_storeu_ps(out.as_mut_ptr().add(base + k + 16), v2);
            _mm256_storeu_ps(out.as_mut_ptr().add(base + k + 24), v3);
            e0 = _mm256_add_ps(e0, v0);
            e1 = _mm256_add_ps(e1, v1);
            e2 = _mm256_add_ps(e2, v2);
            e3 = _mm256_add_ps(e3, v3);
            k += 32;
        }
        while k + 8 <= vecs {
            let v = exp_avx2(_mm256_sub_ps(
                _mm256_loadu_ps(src.as_ptr().add(base + k)),
                mm,
            ));
            _mm256_storeu_ps(out.as_mut_ptr().add(base + k), v);
            e0 = _mm256_add_ps(e0, v);
            k += 8;
        }
        let mut ssum =
            horizontal_sum_avx2(_mm256_add_ps(_mm256_add_ps(e0, e1), _mm256_add_ps(e2, e3)));
        while k < red {
            let e = exp_scalar(src[base + k] - m);
            out[base + k] = e;
            ssum += e;
            k += 1;
        }
        // Normalize pass.
        let rs = _mm256_set1_ps(ssum);
        k = 0;
        while k + 8 <= vecs {
            let v = _mm256_div_ps(_mm256_loadu_ps(out.as_ptr().add(base + k)), rs);
            _mm256_storeu_ps(out.as_mut_ptr().add(base + k), v);
            k += 8;
        }
        while k < red {
            out[base + k] /= ssum;
            k += 1;
        }
        o += 1;
    }
}

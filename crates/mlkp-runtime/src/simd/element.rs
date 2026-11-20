// CEP:FILE: crates/mlkp-runtime/src/simd/element.rs
// CEP:WHAT: The elementwise stage: unary, fused-chain, and binary passes with AVX2 bodies, SSE2 bodies, and shared polynomial scalar tails.
// CEP:WHY: The CEP-0 atomic-file limit keeps every hot-kernel file under 400 lines of code; the engine is split by stage with one shared polynomial in the module root so every path (vector body, scalar tail, non-x86_64 host) runs the same fitted algorithm.
// CEP:CLASS: CEP-0
// CEP:STATUS: complete
// CEP:FAILURE: none; no kernel arithmetic panics, bounds are debug-asserted, exp saturates to IEEE inf and zero outside its finite range and propagates NaN, and feature dispatch is cached atomically with no lock or allocation.
// CEP:ASSUMES: x86_64 hosts always provide SSE2 (the vectorized elementwise baseline); the FMA matrix multiply requires AVX2 and FMA and serves the contiguous-B schedule, while the strided-B schedule keeps the blocked scalar traversal so schedule dimension F stays real; exp outputs below 2^-126 flush to zero (absolute error below 1e-37, inside the plan's absolute-tolerance contract); the polynomial's relative error is below 1.3e-7 over the finite mid-range (fitted in scripts/fit_exp_poly.py, asserted in tests).
// CEP:COST: elementwise O(n) at 8-wide vectors; matmul O(m*n*k) with 8-wide FMAs at up to two per cycle; no allocation, no locks, no formatting on any path.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs and benches/artifacts/exec_fastpath.md.

use super::{apply1, apply2, engine, exp_sse2, transform_avx2, Engine};
use crate::kernels::{EwBinary, EwUnary};
use core::arch::x86_64::*;

// CEP:ID: JIT-e04
// CEP:WHAT: Runs one unary pass over full buffers with the host's widest engine and the shared scalar tail.
// CEP:WHY: The elementwise hot path must run vector code on every x86_64 host and the same polynomial everywhere.
// CEP:STATUS: complete
// CEP:FAILURE: none; length mismatches are debug-asserted and proven by the plan airlock.
// CEP:ASSUMES: a.len() == out.len().
// CEP:COST: O(n) at 8 (AVX2) or 4 (SSE2) lanes plus a scalar tail.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
pub(crate) fn ew_unary_fast(op: EwUnary, a: &[f32], out: &mut [f32]) {
    debug_assert_eq!(a.len(), out.len(), "unary kernel length mismatch");
    #[cfg(target_arch = "x86_64")]
    {
        match engine() {
            // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
            Engine::Avx2Fma => unsafe { ew_unary_avx2(op, a, out) },
            // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
            Engine::Sse2 => unsafe { ew_unary_sse2(op, a, out) },
            Engine::Scalar => crate::kernels::ew_unary_scalar(op, a, out),
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        crate::kernels::ew_unary_scalar(op, a, out);
    }
}

// CEP:ID: JIT-e94
// CEP:WHAT: The AVX2 body of the unary pass: two vectors in flight per iteration.
// CEP:WHY: Eight lanes per iteration with one scalar tail loop; the interleave hides the exp latency the same way the chain pass does.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: AVX2 and FMA enabled; lengths equal.
// CEP:COST: O(n/8) vector iterations.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn ew_unary_avx2(op: EwUnary, a: &[f32], out: &mut [f32]) {
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    let n = a.len();
    let mut i = 0;
    // Two vectors per iteration for the same latency-hiding reason as the
    // chain pass; per-element arithmetic is unchanged.
    while i + 32 <= n {
        let v0 = _mm256_loadu_ps(a.as_ptr().add(i));
        let v1 = _mm256_loadu_ps(a.as_ptr().add(i + 8));
        let v2 = _mm256_loadu_ps(a.as_ptr().add(i + 16));
        let v3 = _mm256_loadu_ps(a.as_ptr().add(i + 24));
        _mm256_storeu_ps(out.as_mut_ptr().add(i), transform_avx2(op, v0));
        _mm256_storeu_ps(out.as_mut_ptr().add(i + 8), transform_avx2(op, v1));
        _mm256_storeu_ps(out.as_mut_ptr().add(i + 16), transform_avx2(op, v2));
        _mm256_storeu_ps(out.as_mut_ptr().add(i + 24), transform_avx2(op, v3));
        i += 32;
    }
    while i + 16 <= n {
        let v0 = _mm256_loadu_ps(a.as_ptr().add(i));
        let v1 = _mm256_loadu_ps(a.as_ptr().add(i + 8));
        _mm256_storeu_ps(out.as_mut_ptr().add(i), transform_avx2(op, v0));
        _mm256_storeu_ps(out.as_mut_ptr().add(i + 8), transform_avx2(op, v1));
        i += 16;
    }
    while i + 8 <= n {
        let v = _mm256_loadu_ps(a.as_ptr().add(i));
        _mm256_storeu_ps(out.as_mut_ptr().add(i), transform_avx2(op, v));
        i += 8;
    }
    while i < n {
        out[i] = apply1(op, a[i]);
        i += 1;
    }
}

// CEP:WHAT: The SSE2 body of the unary pass.
// CEP:WHY: Non-AVX2 hosts run four lanes per iteration with the same polynomial.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: SSE2 enabled; lengths equal.
// CEP:COST: O(n/4) vector iterations.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
unsafe fn ew_unary_sse2(op: EwUnary, a: &[f32], out: &mut [f32]) {
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    use core::arch::x86_64::{
        _mm_add_ps, _mm_div_ps, _mm_loadu_ps, _mm_max_ps, _mm_set1_ps, _mm_setzero_ps,
        _mm_storeu_ps, _mm_xor_ps,
    };
    let n = a.len();
    let mut i = 0;
    while i + 4 <= n {
        let v = _mm_loadu_ps(a.as_ptr().add(i));
        let r = match op {
            EwUnary::Relu => _mm_max_ps(v, _mm_setzero_ps()),
            EwUnary::Exp => exp_sse2(v),
            EwUnary::Neg => _mm_xor_ps(v, _mm_set1_ps(-0.0)),
            EwUnary::Sigmoid => {
                let e = exp_sse2(_mm_xor_ps(v, _mm_set1_ps(-0.0)));
                _mm_div_ps(_mm_set1_ps(1.0), _mm_add_ps(_mm_set1_ps(1.0), e))
            }
        };
        _mm_storeu_ps(out.as_mut_ptr().add(i), r);
        i += 4;
    }
    while i < n {
        out[i] = apply1(op, a[i]);
        i += 1;
    }
}

// CEP:ID: JIT-e05
// CEP:WHAT: Runs a fused unary chain (up to four ops composed per element) as one pass over the buffers.
// CEP:WHY: Chains of elementwise ops pay one load and one store instead of one per op; the composition applies in registers, so results are bit-identical to the unfused sequence.
// CEP:STATUS: complete
// CEP:FAILURE: none; empty chains are rejected by debug assertion.
// CEP:ASSUMES: a.len() == out.len(); kinds is non-empty (the dispatcher guarantees 1..=4).
// CEP:COST: O(n) total instead of O(kinds * n) memory traffic.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs asserts bit-equality with the unfused sequence.
pub(crate) fn ew_unary_chain_fast(kinds: &[EwUnary], a: &[f32], out: &mut [f32]) {
    debug_assert!(!kinds.is_empty(), "chain must carry at least one op");
    debug_assert_eq!(a.len(), out.len(), "chain kernel length mismatch");
    #[cfg(target_arch = "x86_64")]
    {
        match engine() {
            // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
            Engine::Avx2Fma => unsafe { ew_chain_avx2(kinds, a, out) },
            // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
            Engine::Sse2 => unsafe { ew_chain_sse2(kinds, a, out) },
            Engine::Scalar => crate::kernels::ew_unary_chain_scalar(kinds, a, out),
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        crate::kernels::ew_unary_chain_scalar(kinds, a, out);
    }
}

// CEP:ID: JIT-e95
// CEP:WHAT: The AVX2 body of the fused chain pass: two vectors in flight per iteration.
// CEP:WHY: The composition applies per vector, one load and one store for the whole chain; the two-vector interleave hides the exp polynomial's serial latency, which the 64-op profile showed dominates the elementwise passes.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: AVX2 and FMA enabled; lengths equal; kinds non-empty.
// CEP:COST: O(n/8) iterations, each applying kinds.len() transforms.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn ew_chain_avx2(kinds: &[EwUnary], a: &[f32], out: &mut [f32]) {
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    let n = a.len();
    let mut i = 0;
    // Two vectors per iteration: the transform chains of independent vectors
    // interleave, hiding the exp Horner's serial latency that a one-vector
    // loop exposes on this engine. Per-element arithmetic is unchanged.
    while i + 32 <= n {
        let mut v0 = _mm256_loadu_ps(a.as_ptr().add(i));
        let mut v1 = _mm256_loadu_ps(a.as_ptr().add(i + 8));
        let mut v2 = _mm256_loadu_ps(a.as_ptr().add(i + 16));
        let mut v3 = _mm256_loadu_ps(a.as_ptr().add(i + 24));
        for &k in kinds {
            v0 = transform_avx2(k, v0);
            v1 = transform_avx2(k, v1);
            v2 = transform_avx2(k, v2);
            v3 = transform_avx2(k, v3);
        }
        _mm256_storeu_ps(out.as_mut_ptr().add(i), v0);
        _mm256_storeu_ps(out.as_mut_ptr().add(i + 8), v1);
        _mm256_storeu_ps(out.as_mut_ptr().add(i + 16), v2);
        _mm256_storeu_ps(out.as_mut_ptr().add(i + 24), v3);
        i += 32;
    }
    while i + 16 <= n {
        let mut v0 = _mm256_loadu_ps(a.as_ptr().add(i));
        let mut v1 = _mm256_loadu_ps(a.as_ptr().add(i + 8));
        for &k in kinds {
            v0 = transform_avx2(k, v0);
            v1 = transform_avx2(k, v1);
        }
        _mm256_storeu_ps(out.as_mut_ptr().add(i), v0);
        _mm256_storeu_ps(out.as_mut_ptr().add(i + 8), v1);
        i += 16;
    }
    while i + 8 <= n {
        let mut v = _mm256_loadu_ps(a.as_ptr().add(i));
        for &k in kinds {
            v = transform_avx2(k, v);
        }
        _mm256_storeu_ps(out.as_mut_ptr().add(i), v);
        i += 8;
    }
    while i < n {
        let mut x = a[i];
        for &k in kinds {
            x = apply1(k, x);
        }
        out[i] = x;
        i += 1;
    }
}

// CEP:ID: JIT-e28
// CEP:WHAT: The SSE2 body of the fused chain pass.
// CEP:WHY: Non-AVX2 hosts compose per four lanes with the same polynomial.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: SSE2 enabled; lengths equal; kinds non-empty.
// CEP:COST: O(n/4) iterations.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
unsafe fn ew_chain_sse2(kinds: &[EwUnary], a: &[f32], out: &mut [f32]) {
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    use core::arch::x86_64::{
        _mm_add_ps, _mm_div_ps, _mm_loadu_ps, _mm_max_ps, _mm_set1_ps, _mm_setzero_ps,
        _mm_storeu_ps, _mm_xor_ps,
    };
    let n = a.len();
    let mut i = 0;
    while i + 4 <= n {
        let mut v = _mm_loadu_ps(a.as_ptr().add(i));
        for &k in kinds {
            v = match k {
                EwUnary::Relu => _mm_max_ps(v, _mm_setzero_ps()),
                EwUnary::Exp => exp_sse2(v),
                EwUnary::Neg => _mm_xor_ps(v, _mm_set1_ps(-0.0)),
                EwUnary::Sigmoid => {
                    let e = exp_sse2(_mm_xor_ps(v, _mm_set1_ps(-0.0)));
                    _mm_div_ps(_mm_set1_ps(1.0), _mm_add_ps(_mm_set1_ps(1.0), e))
                }
            };
        }
        _mm_storeu_ps(out.as_mut_ptr().add(i), v);
        i += 4;
    }
    while i < n {
        let mut x = a[i];
        for &k in kinds {
            x = apply1(k, x);
        }
        out[i] = x;
        i += 1;
    }
}

// CEP:ID: JIT-e06
// CEP:WHAT: Runs one binary pass over three full buffers with the host's widest engine and the shared scalar tail.
// CEP:WHY: Bias adds and pairwise ops are memory-bound; the vector body halves the instruction count per element.
// CEP:STATUS: complete
// CEP:FAILURE: none; length mismatches are debug-asserted and proven by the plan airlock.
// CEP:ASSUMES: a.len() == b.len() == out.len().
// CEP:COST: O(n) with two loads and one store per element.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
pub(crate) fn ew_binary_fast(op: EwBinary, a: &[f32], b: &[f32], out: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len(), "binary kernel length mismatch");
    debug_assert_eq!(a.len(), out.len(), "binary kernel output mismatch");
    #[cfg(target_arch = "x86_64")]
    {
        match engine() {
            // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
            Engine::Avx2Fma => unsafe { ew_binary_avx2(op, a, b, out) },
            // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
            Engine::Sse2 => unsafe { ew_binary_sse2(op, a, b, out) },
            Engine::Scalar => crate::kernels::ew_binary_scalar(op, a, b, out),
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        crate::kernels::ew_binary_scalar(op, a, b, out);
    }
}

// CEP:WHAT: The AVX2 body of the binary pass.
// CEP:WHY: Eight lanes per iteration.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: AVX2 and FMA enabled; lengths equal.
// CEP:COST: O(n/8) iterations.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn ew_binary_avx2(op: EwBinary, a: &[f32], b: &[f32], out: &mut [f32]) {
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    let n = a.len();
    let mut i = 0;
    while i + 8 <= n {
        let x = _mm256_loadu_ps(a.as_ptr().add(i));
        let y = _mm256_loadu_ps(b.as_ptr().add(i));
        let r = match op {
            EwBinary::Add => _mm256_add_ps(x, y),
            EwBinary::Mul => _mm256_mul_ps(x, y),
            EwBinary::Sub => _mm256_sub_ps(x, y),
            EwBinary::Max => _mm256_max_ps(x, y),
        };
        _mm256_storeu_ps(out.as_mut_ptr().add(i), r);
        i += 8;
    }
    while i < n {
        out[i] = apply2(op, a[i], b[i]);
        i += 1;
    }
}

// CEP:WHAT: The SSE2 body of the binary pass.
// CEP:WHY: Four lanes per iteration on non-AVX2 hosts.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: SSE2 enabled; lengths equal.
// CEP:COST: O(n/4) iterations.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
unsafe fn ew_binary_sse2(op: EwBinary, a: &[f32], b: &[f32], out: &mut [f32]) {
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    use core::arch::x86_64::{
        _mm_add_ps, _mm_loadu_ps, _mm_max_ps, _mm_mul_ps, _mm_storeu_ps, _mm_sub_ps,
    };
    let n = a.len();
    let mut i = 0;
    while i + 4 <= n {
        let x = _mm_loadu_ps(a.as_ptr().add(i));
        let y = _mm_loadu_ps(b.as_ptr().add(i));
        let r = match op {
            EwBinary::Add => _mm_add_ps(x, y),
            EwBinary::Mul => _mm_mul_ps(x, y),
            EwBinary::Sub => _mm_sub_ps(x, y),
            EwBinary::Max => _mm_max_ps(x, y),
        };
        _mm_storeu_ps(out.as_mut_ptr().add(i), r);
        i += 4;
    }
    while i < n {
        out[i] = apply2(op, a[i], b[i]);
        i += 1;
    }
}

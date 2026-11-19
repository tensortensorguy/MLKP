// CEP:FILE: crates/mlkp-runtime/src/simd/mod.rs
// CEP:WHAT: The SIMD engine module root: host engine detection, the shared minimax exp polynomial (scalar, AVX2, SSE2), and the scalar apply helpers; the elementwise, reduction, and matmul stages live in their own CEP-0 files.
// CEP:WHY: The CEP-0 atomic-file limit keeps every hot-kernel file under 400 lines of code; the engine is split by stage with one shared polynomial in the module root so every path (vector body, scalar tail, non-x86_64 host) runs the same fitted algorithm.
// CEP:CLASS: CEP-0
// CEP:STATUS: complete
// CEP:FAILURE: none; no kernel arithmetic panics, bounds are debug-asserted, exp saturates to IEEE inf and zero outside its finite range and propagates NaN, and feature dispatch is cached atomically with no lock or allocation.
// CEP:ASSUMES: x86_64 hosts always provide SSE2 (the vectorized elementwise baseline); the FMA matrix multiply requires AVX2 and FMA and serves the contiguous-B schedule, while the strided-B schedule keeps the blocked scalar traversal so schedule dimension F stays real; exp outputs below 2^-126 flush to zero (absolute error below 1e-37, inside the plan's absolute-tolerance contract); the polynomial's relative error is below 1.3e-7 over the finite mid-range (fitted in scripts/fit_exp_poly.py, asserted in tests).
// CEP:COST: elementwise O(n) at 8-wide vectors; matmul O(m*n*k) with 8-wide FMAs at up to two per cycle; no allocation, no locks, no formatting on any path.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs and benches/artifacts/exec_fastpath.md.
//! SIMD engine root: engine detection, the shared exp polynomial, scalar helpers.

#![deny(missing_docs)]

use crate::kernels::{EwBinary, EwUnary};
use core::arch::x86_64::*;
use std::sync::atomic::{AtomicU8, Ordering};

/// The exp polynomial, degree 6 minimax on [-0.5, 0.5], fitted and verified in
/// scripts/fit_exp_poly.py; coefficients are stored as bit patterns so the
/// constants are exactly the f32 values the fit produced.
const EXP_C: [f32; 7] = [
    f32::from_bits(0x3f80_0000), // 1.0 exactly: exp(0) == 1.0
    f32::from_bits(0x3f80_0003), // 1.000000358
    f32::from_bits(0x3f00_0002), // 0.5000001192
    f32::from_bits(0x3e2a_a7b9), // 0.1666554362
    f32::from_bits(0x3d2a_a804), // 0.04166413844
    f32::from_bits(0x3c09_f97b), // 0.008421297185
    f32::from_bits(0x3ab7_ec9d), // 0.001403230825
];

/// The reduction constant log2(e) for the exponent split (exactly the fit's f32 value).
const EXP_LOG2E: f32 = f32::from_bits(0x3fb8aa3b);

/// The high half of ln(2), exactly representable in f32.
const EXP_LN2_HI: f32 = f32::from_bits(0x3f318000);

/// The low half of ln(2) that reconstructs full precision after the split.
const EXP_LN2_LO: f32 = f32::from_bits(0xb95e8083);

/// The vector engine this host runs: AVX2+FMA, SSE2, or the portable scalar kernels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Engine {
    /// AVX2 plus FMA: the full engine including the register-blocked matmul.
    Avx2Fma,
    /// SSE2 baseline: vectorized elementwise, scalar blocked matmul.
    Sse2,
    /// Portable scalar kernels with the same polynomial exp.
    Scalar,
}

static ENGINE: AtomicU8 = AtomicU8::new(0xff);

// CEP:WHAT: Detects the host engine once.
// CEP:WHY: The dispatch decision must reflect real hardware features.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; runs once per process.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
fn detect() -> Engine {
    if std::arch::is_x86_feature_detected!("avx2") && std::arch::is_x86_feature_detected!("fma") {
        Engine::Avx2Fma
    } else {
        Engine::Sse2
    }
}

// CEP:ID: JIT-e01
// CEP:WHAT: Returns the cached engine kind for this host.
// CEP:WHY: Feature detection must run once per process, not per kernel call; a relaxed atomic load is one mov and no lock, inside the CEP-0 budget.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the engine set is fixed for the life of the process.
// CEP:COST: O(1), one atomic load after the first call.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
pub(crate) fn engine() -> Engine {
    match ENGINE.load(Ordering::Relaxed) {
        1 => Engine::Avx2Fma,
        2 => Engine::Sse2,
        3 => Engine::Scalar,
        _ => {
            let e = detect();
            let code = match e {
                Engine::Avx2Fma => 1u8,
                Engine::Sse2 => 2,
                Engine::Scalar => 3,
            };
            ENGINE.store(code, Ordering::Relaxed);
            e
        }
    }
}

// CEP:WHAT: Detects the host engine once on non-x86_64 hosts.
// CEP:WHY: Portable builds run the scalar kernels with the same polynomial.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(not(target_arch = "x86_64"))]
fn detect() -> Engine {
    Engine::Scalar
}

// CEP:WHAT: Reports whether the FMA matmul engine is active.
// CEP:WHY: The matmul dispatch in kernels.rs asks once per invocation.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
pub(crate) fn matmul_fma_available() -> bool {
    engine() == Engine::Avx2Fma
}

// CEP:ID: JIT-e02
// CEP:WHAT: Computes exp via the shared minimax polynomial: exponent split, two-part ln2 correction, degree-6 Horner, and an exponent-field scale.
// CEP:WHY: Every exp on every path (vector body, scalar tail, non-x86_64 hosts) uses this one algorithm so the whole stack shares one numerical contract; the polynomial's relative error is below 1.3e-7, far inside the plan's absolute tolerance.
// CEP:STATUS: complete
// CEP:FAILURE: none; saturates to inf above the overflow threshold and to zero below the underflow threshold, propagates NaN.
// CEP:ASSUMES: outputs below 2^-126 flush to zero (absolute error below 1e-37); the magic-number rounding matches the vector path's round-to-nearest-even.
// CEP:COST: about 12 f32 operations per element, no calls.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs asserts accuracy against libm expf.
pub(crate) fn exp_scalar(x: f32) -> f32 {
    if x.is_nan() {
        return f32::NAN;
    }
    let z = x * EXP_LOG2E;
    // Round z to nearest-even with the 1.5*2^23 magic constant; the cast is
    // saturating, so infinities land beyond the clamp thresholds below.
    let kf = (z + 12_582_912.0) - 12_582_912.0;
    let k = kf as i32;
    if k > 128 {
        return f32::INFINITY;
    }
    if k < -126 {
        return 0.0;
    }
    let r1 = x - kf * EXP_LN2_HI;
    let r2 = r1 - kf * EXP_LN2_LO;
    let mut p = 0.0f32;
    let mut ci = 7;
    while ci > 0 {
        ci -= 1;
        p = EXP_C[ci] + p * r2;
    }
    // k == 128 is still representable: scale by 2^127 and double once, which
    // overflows to inf exactly where libm does.
    let kc = if k == 128 { 127 } else { k };
    let s = f32::from_bits(((kc + 127) as u32) << 23);
    let out = p * s;
    if k == 128 {
        out + out
    } else {
        out
    }
}

// CEP:WHAT: Applies one unary op to one scalar element, the shared scalar-tail body.
// CEP:WHY: Scalar tails must run the same polynomial as the vector body; no libm slow path exists.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
pub(crate) fn apply1(op: EwUnary, x: f32) -> f32 {
    match op {
        EwUnary::Relu => {
            if x > 0.0 {
                x
            } else {
                0.0
            }
        }
        EwUnary::Exp => exp_scalar(x),
        EwUnary::Neg => -x,
        EwUnary::Sigmoid => {
            let e = exp_scalar(-x);
            1.0 / (1.0 + e)
        }
    }
}

// CEP:WHAT: Applies one binary op to one scalar element pair.
// CEP:WHY: The scalar tail of the binary kernels shares one body with the vector lanes.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
pub(crate) fn apply2(op: EwBinary, x: f32, y: f32) -> f32 {
    match op {
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
    }
}

// CEP:ID: JIT-e03
// CEP:WHAT: Computes exp for eight lanes with the AVX2 engine: FMA Horner over the shared polynomial and an exponent-field scale, with saturation and NaN masks.
// CEP:WHY: The vector body must be the same algorithm as the scalar path so fused epilogues and tails agree bit-for-bit within rounding.
// CEP:STATUS: complete
// CEP:FAILURE: none; k beyond the exponent range saturates through the comparison masks, NaN input propagates.
// CEP:ASSUMES: called only with AVX2 and FMA enabled; round-to-nearest-even conversion is the default MXCSR mode.
// CEP:COST: about 16 vector operations per 8 lanes.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn exp_avx2(x: core::arch::x86_64::__m256) -> core::arch::x86_64::__m256 {
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    let nan = _mm256_cmp_ps::<_CMP_UNORD_Q>(x, x);
    let z = _mm256_mul_ps(x, _mm256_set1_ps(EXP_LOG2E));
    let k = _mm256_cvtps_epi32(z);
    let kf = _mm256_cvtepi32_ps(k);
    let mut r = _mm256_fnmadd_ps(kf, _mm256_set1_ps(EXP_LN2_HI), x);
    r = _mm256_fnmadd_ps(kf, _mm256_set1_ps(EXP_LN2_LO), r);
    let mut p = _mm256_set1_ps(0.0);
    let mut ci = 7;
    while ci > 0 {
        ci -= 1;
        p = _mm256_fmadd_ps(p, r, _mm256_set1_ps(EXP_C[ci]));
    }
    // Scale from the k clamped at 127; lanes with k == 128 double once, which
    // overflows to inf exactly where libm does.
    let kc = _mm256_min_epi32(k, _mm256_set1_epi32(127));
    let scale = _mm256_castsi256_ps(_mm256_slli_epi32(
        _mm256_add_epi32(kc, _mm256_set1_epi32(127)),
        23,
    ));
    let mut out = _mm256_mul_ps(p, scale);
    let mid = _mm256_castsi256_ps(_mm256_cmpeq_epi32(k, _mm256_set1_epi32(128)));
    out = _mm256_add_ps(out, _mm256_and_ps(mid, out));
    let over = _mm256_castsi256_ps(_mm256_cmpgt_epi32(k, _mm256_set1_epi32(128)));
    out = _mm256_or_ps(
        _mm256_and_ps(over, _mm256_set1_ps(f32::INFINITY)),
        _mm256_andnot_ps(over, out),
    );
    let under = _mm256_castsi256_ps(_mm256_cmpgt_epi32(_mm256_set1_epi32(-126), k));
    out = _mm256_or_ps(
        _mm256_and_ps(under, _mm256_setzero_ps()),
        _mm256_andnot_ps(under, out),
    );
    _mm256_or_ps(_mm256_and_ps(nan, x), _mm256_andnot_ps(nan, out))
}

// CEP:ID: JIT-e27
// CEP:WHAT: Computes exp for four lanes with the SSE2 engine, the same polynomial without fused multiply-add.
// CEP:WHY: Non-AVX2 x86_64 hosts must run vector code too; the algorithm matches the AVX2 path operation for operation.
// CEP:STATUS: complete
// CEP:FAILURE: none; same saturation and NaN behavior.
// CEP:ASSUMES: SSE2 is present on every x86_64 host.
// CEP:COST: about 16 vector operations per 4 lanes.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
unsafe fn exp_sse2(x: core::arch::x86_64::__m128) -> core::arch::x86_64::__m128 {
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    use core::arch::x86_64::{
        _mm_add_epi32, _mm_add_ps, _mm_and_ps, _mm_andnot_ps, _mm_castsi128_ps, _mm_cmpgt_epi32,
        _mm_cvtepi32_ps, _mm_cvtps_epi32, _mm_mul_ps, _mm_or_ps, _mm_set1_epi32, _mm_set1_ps,
        _mm_setzero_ps, _mm_slli_epi32, _mm_sub_ps,
    };
    let nan = _mm_cmpunord_ps(x, x);
    let z = _mm_mul_ps(x, _mm_set1_ps(EXP_LOG2E));
    let k = _mm_cvtps_epi32(z);
    let kf = _mm_cvtepi32_ps(k);
    let mut r = _mm_sub_ps(x, _mm_mul_ps(kf, _mm_set1_ps(EXP_LN2_HI)));
    r = _mm_sub_ps(r, _mm_mul_ps(kf, _mm_set1_ps(EXP_LN2_LO)));
    let mut p = _mm_setzero_ps();
    let mut ci = 7;
    while ci > 0 {
        ci -= 1;
        p = _mm_add_ps(_mm_mul_ps(p, r), _mm_set1_ps(EXP_C[ci]));
    }
    let kc = _mm_min_epi32(k, _mm_set1_epi32(127));
    let scale = _mm_castsi128_ps(_mm_slli_epi32(_mm_add_epi32(kc, _mm_set1_epi32(127)), 23));
    let mut out = _mm_mul_ps(p, scale);
    let mid = _mm_castsi128_ps(_mm_cmpeq_epi32(k, _mm_set1_epi32(128)));
    out = _mm_add_ps(out, _mm_and_ps(mid, out));
    let over = _mm_castsi128_ps(_mm_cmpgt_epi32(k, _mm_set1_epi32(128)));
    out = _mm_or_ps(
        _mm_and_ps(over, _mm_set1_ps(f32::INFINITY)),
        _mm_andnot_ps(over, out),
    );
    let under = _mm_castsi128_ps(_mm_cmpgt_epi32(_mm_set1_epi32(-126), k));
    out = _mm_or_ps(
        _mm_and_ps(under, _mm_setzero_ps()),
        _mm_andnot_ps(under, out),
    );
    _mm_or_ps(_mm_and_ps(nan, x), _mm_andnot_ps(nan, out))
}

// CEP:WHAT: Computes sigmoid for eight lanes from the shared exp.
// CEP:WHY: Sigmoid is one exp, one add, and one divide; the IEEE division keeps full accuracy.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: AVX2 and FMA enabled.
// CEP:COST: one exp plus two vector ops per 8 lanes.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn sigmoid_avx2(x: core::arch::x86_64::__m256) -> core::arch::x86_64::__m256 {
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    let e = exp_avx2(_mm256_xor_ps(x, _mm256_set1_ps(-0.0)));
    _mm256_div_ps(_mm256_set1_ps(1.0), _mm256_add_ps(_mm256_set1_ps(1.0), e))
}

// CEP:WHAT: Applies one unary transform to eight lanes.
// CEP:WHY: The elementwise, chain, and epilogue kernels share one transform body.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: AVX2 and FMA enabled.
// CEP:COST: O(1) per vector.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/simd_test.rs
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn transform_avx2(op: EwUnary, v: core::arch::x86_64::__m256) -> core::arch::x86_64::__m256 {
    // CEP:UNSAFE: the intrinsics read and write fixed-size lanes; every load and store stays in bounds by the loop conditions and the airlock-proven slice lengths.
    match op {
        EwUnary::Relu => _mm256_max_ps(v, _mm256_setzero_ps()),
        EwUnary::Exp => exp_avx2(v),
        EwUnary::Neg => _mm256_xor_ps(v, _mm256_set1_ps(-0.0)),
        EwUnary::Sigmoid => sigmoid_avx2(v),
    }
}

mod element;
mod matmul;
mod reduce;

pub(crate) use element::{ew_binary_fast, ew_unary_chain_fast, ew_unary_fast};
pub(crate) use matmul::{matmul_fma_rows, matmul_fma_sched};
pub(crate) use reduce::{reduce_fast, softmax_fast};

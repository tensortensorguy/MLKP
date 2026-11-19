// CEP:FILE: tools/cep_lint/src/ed25519.rs
// CEP:WHAT: Ed25519 signing and verification over curve25519 (RFC 8032 construction) implemented in-tree with 2^51 field limbs and extended twisted Edwards coordinates.
// CEP:WHY: The Hunt and HUNTER receipts of chapters 65 and 67 are signed by the CI runner key and verified by the linter; the repository bans third-party dependencies, so the signature scheme exists in-tree with round-trip and tamper tests as the correctness evidence.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: verify returns false on any malformed point, non-canonical scalar, or invalid signature; sign panics only if the seed is not exactly 32 bytes, which the type system cannot express, so the length is checked.
// CEP:ASSUMES: the base point is the point with y = 4/5 and even x; signatures produced and consumed by this implementation are internally consistent, and cross-implementation interop is not asserted (see doc/cep_lint.md Known Limitations).
// CEP:COST: cold; sign and verify are O(256) group operations on field elements.
// CEP:EVIDENCE: unit tests in tools/cep_lint/tests/ed25519_test.rs.

use crate::sha512::sha512;

/// Field element in radix 2^51 over p = 2^255 - 19.
#[derive(Clone, Copy, Debug)]
pub struct Fe([u64; 5]);

/// Group order L = 2^252 + 27742317777372353535851937790883648493, little-endian 64-bit limbs.
const L: [u64; 4] = [
    0x5812631a5cf5d3ed,
    0x14def9dea2f79cd6,
    0x0000000000000000,
    0x1000000000000000,
];

const MASK51: u64 = (1 << 51) - 1;

// CEP:WHAT: Constructs a field element from limbs without validation.
// CEP:WHY: Internal constructor used by arithmetic helpers.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: limbs are already reduced; callers reduce before comparison.
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe(limbs: [u64; 5]) -> Fe {
    Fe(limbs)
}

// CEP:WHAT: Returns the zero field element.
// CEP:WHY: Identity for addition.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_zero() -> Fe {
    Fe([0, 0, 0, 0, 0])
}

// CEP:WHAT: Returns the one field element.
// CEP:WHY: Multiplicative identity.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_one() -> Fe {
    Fe([1, 0, 0, 0, 0])
}

// CEP:WHAT: Constructs a small integer field element.
// CEP:WHY: Curve constants 4, 5, 121665, and 121666 are needed to derive d and the base point.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: v < 2^51.
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_small(v: u64) -> Fe {
    Fe([v, 0, 0, 0, 0])
}

// CEP:WHAT: Adds two field elements without carrying.
// CEP:WHY: Addition is limbwise; mul performs the reduction.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: inputs are below 2^54 per limb after prior reductions.
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_add(a: Fe, b: Fe) -> Fe {
    let mut r = [0u64; 5];
    for i in 0..5 {
        r[i] = a.0[i] + b.0[i];
    }
    fe(r)
}

// CEP:WHAT: Subtracts b from a as a + 2p - b so every limb stays non-negative.
// CEP:WHY: Limbwise subtraction would underflow; 2p in 51-bit limbs keeps the value congruent and small enough for the next reduction.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: inputs are below 2^54 per limb.
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_sub(a: Fe, b: Fe) -> Fe {
    // 2p = 2^256 - 38; limbs: [2b-38, 2b-2, 2b-2, 2b-2, 2b-2] with b = 2^51.
    let two_b = (1u64 << 52) - 2;
    let mut r = [0u64; 5];
    r[0] = a.0[0] + ((1u64 << 52) - 38) - b.0[0];
    for i in 1..5 {
        r[i] = a.0[i] + two_b - b.0[i];
    }
    fe(r)
}

// CEP:ID: CS-e32
// CEP:WHAT: Multiplies two field elements and reduces modulo p.
// CEP:WHY: All curve arithmetic reduces to field multiplication.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: input limbs are below 2^54; output limbs are below 2^52.
// CEP:COST: cold; O(1) with u128 intermediates.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_mul(a: Fe, b: Fe) -> Fe {
    let a = &a.0;
    let b = &b.0;
    let b1_19 = b[1] * 19;
    let b2_19 = b[2] * 19;
    let b3_19 = b[3] * 19;
    let b4_19 = b[4] * 19;
    let c0 = (a[0] as u128) * (b[0] as u128)
        + (a[4] as u128) * b1_19 as u128
        + (a[3] as u128) * b2_19 as u128
        + (a[2] as u128) * b3_19 as u128
        + (a[1] as u128) * b4_19 as u128;
    let c1 = (a[1] as u128) * (b[0] as u128)
        + (a[0] as u128) * (b[1] as u128)
        + (a[4] as u128) * b2_19 as u128
        + (a[3] as u128) * b3_19 as u128
        + (a[2] as u128) * b4_19 as u128;
    let c2 = (a[2] as u128) * (b[0] as u128)
        + (a[1] as u128) * (b[1] as u128)
        + (a[0] as u128) * (b[2] as u128)
        + (a[4] as u128) * b3_19 as u128
        + (a[3] as u128) * b4_19 as u128;
    let c3 = (a[3] as u128) * (b[0] as u128)
        + (a[2] as u128) * (b[1] as u128)
        + (a[1] as u128) * (b[2] as u128)
        + (a[0] as u128) * (b[3] as u128)
        + (a[4] as u128) * b4_19 as u128;
    let c4 = (a[4] as u128) * (b[0] as u128)
        + (a[3] as u128) * (b[1] as u128)
        + (a[2] as u128) * (b[2] as u128)
        + (a[1] as u128) * (b[3] as u128)
        + (a[0] as u128) * (b[4] as u128);
    carry_reduce([c0, c1, c2, c3, c4])
}

// CEP:ID: CS-v73
// CEP:WHAT: Folds five 128-bit accumulators into five 51-bit limbs modulo p.
// CEP:WHY: Shared by mul and square; the top carry wraps with factor 19 because 2^255 is congruent to 19.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: inputs are the exact accumulator layout of fe_mul.
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn carry_reduce(mut c: [u128; 5]) -> Fe {
    const M: u128 = (1u128 << 51) - 1;
    let mut r = [0u64; 5];
    let mut carry = c[0] >> 51;
    r[0] = (c[0] & M) as u64;
    for idx in 1..5 {
        c[idx] += carry;
        carry = c[idx] >> 51;
        r[idx] = (c[idx] & M) as u64;
    }
    r[0] += (carry as u64) * 19;
    let wrap = r[0] >> 51;
    r[0] &= MASK51;
    r[1] += wrap;
    fe(r)
}

// CEP:WHAT: Squares a field element.
// CEP:WHY: Point doubling and inversion exponentiation square repeatedly.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_sq(a: Fe) -> Fe {
    fe_mul(a, a)
}

// CEP:WHAT: Raises a field element to an exponent given as little-endian 255-bit limbs.
// CEP:WHY: Inversion uses the exponent p-2; square roots use (p+3)/8 and the fourth-root constant.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: exponent bits are indexed 0..255.
// CEP:COST: cold; O(255) squarings plus multiplies.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_pow(a: Fe, exp: &[u64; 4]) -> Fe {
    let mut r = fe_one();
    for i in (0..255).rev() {
        r = fe_sq(r);
        let bit = (exp[i / 64] >> (i % 64)) & 1;
        if bit == 1 {
            r = fe_mul(r, a);
        }
    }
    r
}

// CEP:WHAT: Inverts a field element as a^(p-2).
// CEP:WHY: Point encoding and sqrt computation need inverses.
// CEP:STATUS: complete
// CEP:FAILURE: Inverting zero yields zero by convention and is rejected by callers through point checks.
// CEP:ASSUMES: none
// CEP:COST: cold; O(255) squarings.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_invert(a: Fe) -> Fe {
    // p - 2 = 2^255 - 21.
    let exp: [u64; 4] = [u64::MAX - 20, u64::MAX, u64::MAX, 0x7FFFFFFFFFFFFFFF];
    fe_pow(a, &exp)
}

// CEP:ID: CS-n50
// CEP:WHAT: Fully carries the limb chain so each limb is below 2^51.
// CEP:WHY: Canonical encoding and comparisons need reduced limbs.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_carry(a: Fe) -> Fe {
    let mut h = a.0;
    let mut carry = h[0] >> 51;
    h[0] &= MASK51;
    for limb in h.iter_mut().skip(1) {
        *limb += carry;
        carry = *limb >> 51;
        *limb &= MASK51;
    }
    h[0] += carry * 19;
    let wrap = h[0] >> 51;
    h[0] &= MASK51;
    h[1] += wrap;
    fe(h)
}

// CEP:ID: CS-k29
// CEP:WHAT: Encodes a field element canonically as 32 little-endian bytes.
// CEP:WHY: Point and scalar encodings are byte strings.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_tobytes(a: Fe) -> [u8; 32] {
    let mut h = fe_carry(a).0;
    // Conditional subtract of p when the value is >= p: q = (h0+19)>>51 chain.
    let mut q = (h[0] + 19) >> 51;
    for limb in h.iter().skip(1) {
        q = (*limb + q) >> 51;
    }
    h[0] += 19 * q;
    let mut carry = h[0] >> 51;
    h[0] &= MASK51;
    for limb in h.iter_mut().skip(1) {
        *limb += carry;
        carry = *limb >> 51;
        *limb &= MASK51;
    }
    let l0 = h[0] | (h[1] << 51);
    let l1 = (h[1] >> 13) | (h[2] << 38);
    let l2 = (h[2] >> 26) | (h[3] << 25);
    let l3 = (h[3] >> 39) | (h[4] << 12);
    let mut out = [0u8; 32];
    out[0..8].copy_from_slice(&l0.to_le_bytes());
    out[8..16].copy_from_slice(&l1.to_le_bytes());
    out[16..24].copy_from_slice(&l2.to_le_bytes());
    out[24..32].copy_from_slice(&l3.to_le_bytes());
    out
}

// CEP:ID: CS-f64
// CEP:WHAT: Decodes 32 little-endian bytes into a field element, masking the high bit.
// CEP:WHY: Point decoding splits the sign bit before interpreting the y coordinate.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_frombytes(b: &[u8; 32]) -> Fe {
    let l0 = u64::from_le_bytes(b[0..8].try_into().expect("slice len checked"));
    let l1 = u64::from_le_bytes(b[8..16].try_into().expect("slice len checked"));
    let l2 = u64::from_le_bytes(b[16..24].try_into().expect("slice len checked"));
    let l3 = u64::from_le_bytes(b[24..32].try_into().expect("slice len checked"));
    let h0 = l0 & MASK51;
    let h1 = ((l0 >> 51) | (l1 << 13)) & MASK51;
    let h2 = ((l1 >> 38) | (l2 << 26)) & MASK51;
    let h3 = ((l2 >> 25) | (l3 << 39)) & MASK51;
    let h4 = (l3 >> 12) & MASK51;
    fe([h0, h1, h2, h3, h4])
}

// CEP:WHAT: Reports whether the low bit of the canonical encoding is set.
// CEP:WHY: Point encoding stores the parity of x in bit 255.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_is_negative(a: Fe) -> bool {
    fe_tobytes(a)[31] & 1 == 1
}

// CEP:WHAT: Reports whether the element is canonically zero.
// CEP:WHY: Equality checks on decoded points use x and z comparisons.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_is_zero(a: Fe) -> bool {
    let b = fe_tobytes(a);
    b.iter().all(|x| *x == 0)
}

/// Extended twisted Edwards point (X, Y, Z, T) with a = -1.
#[derive(Clone, Copy, Debug)]
struct Point {
    /// X projective coordinate.
    x: Fe,
    /// Y projective coordinate.
    y: Fe,
    /// Z projective coordinate.
    z: Fe,
    /// T = XY/Z scaled coordinate.
    t: Fe,
}

// CEP:WHAT: Returns the neutral point (0, 1, 1, 0).
// CEP:WHY: Scalar multiplication starts from the identity.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn point_identity() -> Point {
    Point {
        x: fe_zero(),
        y: fe_one(),
        z: fe_one(),
        t: fe_zero(),
    }
}

// CEP:WHAT: Returns the curve constant d = -121665/121666.
// CEP:WHY: The Edwards addition formulas need d; deriving it from integers avoids a transcribed constant.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; computed once per process with one inversion.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn curve_d() -> Fe {
    static CELL: std::sync::OnceLock<[u64; 5]> = std::sync::OnceLock::new();
    let limbs =
        CELL.get_or_init(|| fe_mul(fe_neg(fe_small(121665)), fe_invert(fe_small(121666))).0);
    Fe(*limbs)
}

// CEP:WHAT: Returns 2d, the k constant of the hwcd-3 addition formulas.
// CEP:WHY: Point addition uses k = 2d; caching avoids recomputation per operation.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; computed once per process.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn curve_d2() -> Fe {
    static CELL: std::sync::OnceLock<[u64; 5]> = std::sync::OnceLock::new();
    let limbs = CELL.get_or_init(|| fe_add(curve_d(), curve_d()).0);
    Fe(*limbs)
}

// CEP:WHAT: Negates a field element as p - a.
// CEP:WHY: Curve constant d is negative and doubling uses a = -1.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: a is reduced.
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_neg(a: Fe) -> Fe {
    fe_sub(fe_zero(), a)
}

// CEP:WHAT: Adds two points with the hwcd-3 formulas for a = -1.
// CEP:WHY: Scalar multiplication accumulates with point addition.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: points are on the curve; the identity is representable.
// CEP:COST: cold; O(1) with field multiplications.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn point_add(p: Point, q: Point) -> Point {
    let d2 = curve_d2();
    let a = fe_mul(fe_sub(p.y, p.x), fe_sub(q.y, q.x));
    let b = fe_mul(fe_add(p.y, p.x), fe_add(q.y, q.x));
    let c = fe_mul(fe_mul(d2, p.t), q.t);
    let dd = fe_mul(fe_add(p.z, p.z), q.z);
    let e = fe_sub(b, a);
    let f = fe_sub(dd, c);
    let g = fe_add(dd, c);
    let h = fe_add(b, a);
    Point {
        x: fe_mul(e, f),
        y: fe_mul(g, h),
        t: fe_mul(e, h),
        z: fe_mul(f, g),
    }
}

// CEP:WHAT: Doubles a point with the hwcd formulas for a = -1.
// CEP:WHY: Scalar multiplication doubles once per bit.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: points are on the curve.
// CEP:COST: cold; O(1) with field multiplications.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn point_double(p: Point) -> Point {
    let a = fe_sq(p.x);
    let b = fe_sq(p.y);
    let c = fe_add(fe_sq(p.z), fe_sq(p.z));
    let d = fe_neg(a);
    let e = fe_sub(fe_sub(fe_sq(fe_add(p.x, p.y)), a), b);
    let g = fe_add(d, b);
    let f = fe_sub(g, c);
    let h = fe_sub(d, b);
    Point {
        x: fe_mul(e, f),
        y: fe_mul(g, h),
        t: fe_mul(e, h),
        z: fe_mul(f, g),
    }
}

// CEP:WHAT: Multiplies the base point by a scalar (little-endian limbs).
// CEP:WHY: Public keys, nonces, and verification products are scalar multiples of B.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(256) group operations.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn scalar_mul_base(s: [u64; 4]) -> Point {
    let base = base_point();
    scalar_mul_point(s, base)
}

// CEP:WHAT: Multiplies one point by a scalar (little-endian limbs).
// CEP:WHY: Verification needs [k]A for arbitrary A.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(256) group operations.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn scalar_mul_point(s: [u64; 4], p: Point) -> Point {
    let mut r = point_identity();
    for i in (0..256).rev() {
        r = point_double(r);
        let bit = (s[i / 64] >> (i % 64)) & 1;
        if bit == 1 {
            r = point_add(r, p);
        }
    }
    r
}

// CEP:WHAT: Encodes a point as 32 bytes with the sign of x in bit 255.
// CEP:WHY: Keys and signature halves are point encodings.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1) with one inversion.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn point_encode(p: Point) -> [u8; 32] {
    let z_inv = fe_invert(p.z);
    let x = fe_mul(p.x, z_inv);
    let y = fe_mul(p.y, z_inv);
    let mut out = fe_tobytes(y);
    if fe_is_negative(x) {
        out[31] |= 0x80;
    }
    out
}

// CEP:ID: CS-v37
// CEP:WHAT: Decodes a point from 32 bytes, recovering x by square root.
// CEP:WHY: Verification consumes encoded keys and signature halves.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None for non-canonical y, x2 not a square, or zero with a sign bit set.
// CEP:ASSUMES: none
// CEP:COST: cold; O(255) squarings for the root and inversion.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn point_decode(bytes: &[u8; 32]) -> Option<Point> {
    let mut yb = *bytes;
    let sign = yb[31] >> 7;
    yb[31] &= 0x7F;
    let y = fe_frombytes(&yb);
    // Reject non-canonical encodings: decoded value must round trip.
    if fe_tobytes(y) != yb {
        return None;
    }
    let y2 = fe_sq(y);
    let u = fe_sub(y2, fe_one());
    let v = fe_add(fe_mul(curve_d(), y2), fe_one());
    // x2 = u / v.
    let x2 = fe_mul(u, fe_invert(v));
    // sqrt via exponent (p+3)/8 = 2^252 - 2, adjusted by sqrt(-1) when needed.
    let exp: [u64; 4] = [u64::MAX - 1, u64::MAX, u64::MAX, 0x0FFFFFFFFFFFFFFF];
    let mut x = fe_pow(x2, &exp);
    if !fe_eq(fe_sq(x), x2) {
        x = fe_mul(x, fe_sqrt_m1());
        if !fe_eq(fe_sq(x), x2) {
            return None;
        }
    }
    if fe_is_zero(x) && sign == 1 {
        return None;
    }
    if fe_is_negative(x) != (sign == 1) {
        x = fe_neg(x);
    }
    Some(Point {
        x,
        y,
        z: fe_one(),
        t: fe_mul(x, y),
    })
}

// CEP:WHAT: Compares two field elements canonically.
// CEP:WHY: Equality on decoded square roots drives the sqrt(-1) adjustment.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_eq(a: Fe, b: Fe) -> bool {
    fe_tobytes(a) == fe_tobytes(b)
}

// CEP:WHAT: Returns sqrt(-1) = 2^((p-1)/4) mod p.
// CEP:WHY: The p = 5 mod 8 square root needs the adjustment constant.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(255) squarings, computed once per process.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn fe_sqrt_m1() -> Fe {
    static CELL: std::sync::OnceLock<[u64; 5]> = std::sync::OnceLock::new();
    let limbs = CELL.get_or_init(|| {
        // (p-1)/4 = 2^253 - 5.
        let exp: [u64; 4] = [u64::MAX - 4, u64::MAX, u64::MAX, 0x1FFFFFFFFFFFFFFF];
        fe_pow(fe_small(2), &exp).0
    });
    Fe(*limbs)
}

// CEP:WHAT: Returns the standard base point B (y = 4/5, even x).
// CEP:WHY: All keys and signature halves are multiples of B.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the derived encoding is the same across runs of this build.
// CEP:COST: cold; computed once per process.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn base_point() -> Point {
    static CELL: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();
    let bytes = CELL.get_or_init(|| {
        let four_fifths = fe_mul(fe_small(4), fe_invert(fe_small(5)));
        fe_tobytes(four_fifths)
    });
    let mut b = *bytes;
    // The base point uses the even square root: sign bit 0.
    b[31] &= 0x7F;
    point_decode(&b).expect("base point is on the curve by construction")
}

// CEP:ID: CS-m54
// CEP:WHAT: Reduces a 512-bit little-endian value modulo L by bit-wise division.
// CEP:WHY: Nonces and challenge scalars are SHA-512 outputs that must land in [0, L); the doubling carry propagates from low limbs to high limbs because bits shift upward.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(512) conditional subtractions.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn scalar_reduce_wide(wide: &[u8; 64]) -> [u64; 4] {
    let mut r = [0u64; 4];
    for i in (0..512).rev() {
        // r = 2r + bit; carry flows from low limbs to high limbs.
        let mut carry = 0u64;
        for limb in r.iter_mut() {
            let nc = *limb >> 63;
            *limb = (*limb << 1) | carry;
            carry = nc;
        }
        debug_assert_eq!(
            carry, 0,
            "reducer invariant violated: r was not below 2^255"
        );
        let bit = ((wide[i / 8] >> (i % 8)) & 1) as u64;
        r[0] |= bit;
        if geq_l(&r) {
            sub_l(&mut r);
        }
    }
    r
}

// CEP:WHAT: Reports whether a scalar is greater than or equal to L.
// CEP:WHY: The modular reduction and canonical-S check compare against L.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: limbs are little-endian.
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn geq_l(r: &[u64; 4]) -> bool {
    for i in (0..4).rev() {
        if r[i] != L[i] {
            return r[i] > L[i];
        }
    }
    true
}

// CEP:WHAT: Subtracts L from a scalar in place.
// CEP:WHY: Bit-wise modular reduction subtracts L at most once per bit.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: r >= L; enforced by the caller.
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn sub_l(r: &mut [u64; 4]) {
    let mut borrow = 0u64;
    for i in 0..4 {
        let (v1, b1) = r[i].overflowing_sub(L[i]);
        let (v2, b2) = v1.overflowing_sub(borrow);
        r[i] = v2;
        borrow = (b1 as u64) | (b2 as u64);
    }
}

// CEP:ID: CS-t29
// CEP:WHAT: Adds two scalars modulo L by adding and conditional subtracting.
// CEP:WHY: The signature scalar S = r + k*s must be canonical.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: inputs are below L.
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn scalar_add_mod_l(a: &[u64; 4], b: &[u64; 4]) -> [u64; 4] {
    let mut r = [0u64; 4];
    let mut carry = 0u128;
    for i in 0..4 {
        let s = (a[i] as u128) + (b[i] as u128) + carry;
        r[i] = s as u64;
        carry = s >> 64;
    }
    // carry beyond limb 3 is impossible for two values below L (< 2^253): the sum fits in 254 bits.
    if geq_l(&r) {
        sub_l(&mut r);
    }
    r
}

// CEP:ID: CS-x19
// CEP:WHAT: Multiplies two scalars modulo L.
// CEP:WHY: The signing equation needs k*s mod L.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: a may use the full 255 bits (the clamped signing scalar is not reduced) and is normalized modulo L on entry; b may use the full 255 bits; the accumulator stays below 2^254 between conditional subtractions.
// CEP:COST: cold; O(256) iterations of double, conditional subtract, add, conditional subtract.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn scalar_mul_mod_l(a: &[u64; 4], b: &[u64; 4]) -> [u64; 4] {
    let mut a_r = *a;
    while geq_l(&a_r) {
        sub_l(&mut a_r);
    }
    let a = &a_r;
    let mut r = [0u64; 4];
    for i in (0..256).rev() {
        // r = 2r; carry flows from low limbs to high limbs.
        let mut carry = 0u64;
        for limb in r.iter_mut() {
            let nc = *limb >> 63;
            *limb = (*limb << 1) | carry;
            carry = nc;
        }
        debug_assert_eq!(
            carry, 0,
            "accumulator invariant violated: r was not below 2^255"
        );
        if geq_l(&r) {
            sub_l(&mut r);
        }
        let bit = (b[i / 64] >> (i % 64)) & 1;
        if bit == 1 {
            let mut sum_carry = 0u64;
            for j in 0..4 {
                let (v, o1) = r[j].overflowing_add(a[j]);
                let (v2, o2) = v.overflowing_add(sum_carry);
                r[j] = v2;
                sum_carry = (o1 as u64) | (o2 as u64);
            }
            if geq_l(&r) {
                sub_l(&mut r);
            }
        }
    }
    r
}

// CEP:ID: CS-716
// CEP:WHAT: Clamps the SHA-512 digest half into the signing scalar.
// CEP:WHY: RFC 8032 clamps bits 0..2 and 255 to define the scalar range.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn clamp(digest_half: &[u8]) -> [u64; 4] {
    let mut h = [0u8; 32];
    h.copy_from_slice(&digest_half[..32]);
    h[0] &= 248;
    h[31] &= 63;
    h[31] |= 64;
    let mut s = [0u64; 4];
    for i in 0..4 {
        let mut v = 0u64;
        for b in 0..8 {
            v |= u64::from(h[i * 8 + b]) << (8 * b);
        }
        s[i] = v;
    }
    s
}

/// Signs a message with a 32-byte seed; returns the 64-byte signature R || S.
// CEP:ID: CS-n25
// CEP:WHAT: Signs one message with a 32-byte seed.
// CEP:WHY: The CI runner signs hunt receipts so the linter can verify them without trusting the JSON text.
// CEP:STATUS: complete
// CEP:FAILURE: Panics if the seed is not exactly 32 bytes; the type system cannot carry the length, so the check is explicit.
// CEP:ASSUMES: the seed is a high-entropy secret held by the CI runner.
// CEP:COST: cold; two scalar multiplications and three hashes.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
pub fn sign(seed: &[u8], msg: &[u8]) -> [u8; 64] {
    assert_eq!(seed.len(), 32, "ed25519 seed must be 32 bytes");
    let h = sha512(seed);
    let s = clamp(&h[0..32]);
    let public = public_from_scalar(s);
    let mut prefix = [0u8; 32];
    prefix.copy_from_slice(&h[32..64]);
    let mut nonce_input = Vec::with_capacity(32 + msg.len());
    nonce_input.extend_from_slice(&prefix);
    nonce_input.extend_from_slice(msg);
    let r_digest = sha512(&nonce_input);
    let r = scalar_reduce_wide(&r_digest);
    let r_point = scalar_mul_base(r);
    let r_bytes = point_encode(r_point);
    let mut challenge_input = Vec::with_capacity(64 + msg.len());
    challenge_input.extend_from_slice(&r_bytes);
    challenge_input.extend_from_slice(&public);
    challenge_input.extend_from_slice(msg);
    let k_digest = sha512(&challenge_input);
    let k = scalar_reduce_wide(&k_digest);
    let k_s = scalar_mul_mod_l(&k, &s);
    let s_out = scalar_add_mod_l(&r, &k_s);
    let mut sig = [0u8; 64];
    sig[0..32].copy_from_slice(&r_bytes);
    for i in 0..4 {
        sig[32 + i * 8..32 + (i + 1) * 8].copy_from_slice(&s_out[i].to_le_bytes());
    }
    sig
}

// CEP:WHAT: Derives the 32-byte public key from the signing scalar.
// CEP:WHY: The public key is [s]B in encoded form.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; one scalar multiplication.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn public_from_scalar(s: [u64; 4]) -> [u8; 32] {
    point_encode(scalar_mul_base(s))
}

/// Derives the 32-byte public key from a 32-byte seed.
// CEP:WHAT: Derives the public key from a seed.
// CEP:WHY: Key generation produces the pair (seed, public) once per CI runner.
// CEP:STATUS: complete
// CEP:FAILURE: Panics if the seed is not exactly 32 bytes.
// CEP:ASSUMES: none
// CEP:COST: cold; one hash and one scalar multiplication.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
pub fn public_from_seed(seed: &[u8]) -> [u8; 32] {
    assert_eq!(seed.len(), 32, "ed25519 seed must be 32 bytes");
    let h = sha512(seed);
    public_from_scalar(clamp(&h[0..32]))
}

/// Verifies a 64-byte signature over a message with a 32-byte public key.
// CEP:ID: CS-b20
// CEP:WHAT: Verifies an Ed25519 signature.
// CEP:WHY: The linter must reject tampered or missing hunt receipts.
// CEP:STATUS: complete
// CEP:FAILURE: Returns false on any malformed input or failed check; never panics.
// CEP:ASSUMES: the equation checked is [S]B == R + [k]A with canonical S, consistent with this implementation's sign().
// CEP:COST: cold; three scalar multiplications and one hash.
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
pub fn verify(public: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
    let Some(a_point) = point_decode(public) else {
        return false;
    };
    let mut r_bytes = [0u8; 32];
    r_bytes.copy_from_slice(&sig[0..32]);
    let Some(r_point) = point_decode(&r_bytes) else {
        return false;
    };
    let mut s_le = [0u8; 32];
    s_le.copy_from_slice(&sig[32..64]);
    let mut s_scalar = [0u64; 4];
    for i in 0..4 {
        let mut v = 0u64;
        for b in 0..8 {
            v |= u64::from(s_le[i * 8 + b]) << (8 * b);
        }
        s_scalar[i] = v;
    }
    if geq_l(&s_scalar) {
        return false;
    }
    let mut challenge = Vec::with_capacity(64 + msg.len());
    challenge.extend_from_slice(&r_bytes);
    challenge.extend_from_slice(public);
    challenge.extend_from_slice(msg);
    let k_digest = sha512(&challenge);
    let k = scalar_reduce_wide(&k_digest);
    let p1 = scalar_mul_base(s_scalar);
    let k_a = scalar_mul_point(k, a_point);
    let p2 = point_add(r_point, k_a);
    point_eq(&p1, &p2)
}

// CEP:WHAT: Projective equality of two points by cross-multiplication.
// CEP:WHY: Verification compares [S]B with R + [k]A without inverting Z.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/ed25519_test.rs
fn point_eq(p: &Point, q: &Point) -> bool {
    let x1z2 = fe_mul(p.x, q.z);
    let x2z1 = fe_mul(q.x, p.z);
    let y1z2 = fe_mul(p.y, q.z);
    let y2z1 = fe_mul(q.y, p.z);
    fe_eq(x1z2, x2z1) && fe_eq(y1z2, y2z1)
}

#[cfg(test)]
mod internal_tests {
    use super::*;

    // CEP:WHAT: Verifies byte round trips through the field codec.
    // CEP:WHY: Encoding bugs corrupt keys before any arithmetic runs.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics via assert on mismatch.
    // CEP:ASSUMES: none
    // CEP:COST: cold
    // CEP:EVIDENCE: cargo test -p cep_lint
    #[test]
    fn fe_bytes_round_trip() {
        let vals = [
            fe_zero(),
            fe_one(),
            fe_small(12345),
            fe_sub(fe_zero(), fe_one()),
            Fe([MASK51, MASK51, MASK51, MASK51, MASK51]),
        ];
        for v in vals {
            let bytes = fe_tobytes(v);
            let back = fe_frombytes(&bytes);
            assert!(fe_eq(v, back), "round trip failed for {:?}", bytes);
        }
    }

    // CEP:ID: CS-w55
    // CEP:WHAT: Verifies field distributivity and inversion.
    // CEP:WHY: Multiplication and inversion bugs break all group formulas.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics via assert on mismatch.
    // CEP:ASSUMES: none
    // CEP:COST: cold
    // CEP:EVIDENCE: cargo test -p cep_lint
    #[test]
    fn field_arithmetic_identities() {
        let a = fe_small(1234577);
        let b = fe_small(7654321);
        let c = fe_small(99001);
        let left = fe_mul(fe_add(a, b), c);
        let right = fe_add(fe_mul(a, c), fe_mul(b, c));
        assert!(fe_eq(left, right), "distributivity failed");
        let five = fe_small(5);
        let inv = fe_invert(five);
        assert!(fe_eq(fe_mul(five, inv), fe_one()), "5 * inv(5) != 1");
        let four_fifths = fe_mul(fe_small(4), inv);
        assert!(
            fe_eq(fe_mul(four_fifths, five), fe_small(4)),
            "4/5 * 5 != 4"
        );
        let p_minus_1 = fe_sub(fe_zero(), fe_one());
        assert!(fe_eq(fe_mul(p_minus_1, p_minus_1), fe_one()), "(-1)^2 != 1");
        // Small exponent: fe_pow(2, 10) = 1024.
        let ten = fe_pow(fe_small(2), &[10, 0, 0, 0]);
        assert!(fe_eq(ten, fe_small(1024)), "2^10 != 1024");
        // Large exponent: 2^(2^64) equals 64 squarings of 2.
        let big_exp: [u64; 4] = [0, 1, 0, 0];
        let via_pow = fe_pow(fe_small(2), &big_exp);
        let mut via_sq = fe_small(2);
        for _ in 0..64 {
            via_sq = fe_sq(via_sq);
        }
        assert!(fe_eq(via_pow, via_sq), "2^(2^64) mismatch");
    }

    // CEP:WHAT: Verifies that the derived base point satisfies the curve equation.
    // CEP:WHY: A wrong d or wrong base point breaks signatures globally.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics via assert on mismatch.
    // CEP:ASSUMES: none
    // CEP:COST: cold
    // CEP:EVIDENCE: cargo test -p cep_lint
    #[test]
    fn base_point_on_curve() {
        let b = base_point();
        // -x^2 + y^2 == 1 + d x^2 y^2 in projective form:
        // (y^2 - x^2) * z^2 == z^4 + d * x^2 * y^2
        let x2 = fe_sq(b.x);
        let y2 = fe_sq(b.y);
        let z2 = fe_sq(b.z);
        let z4 = fe_sq(z2);
        let lhs = fe_mul(fe_sub(y2, x2), z2);
        let rhs = fe_add(z4, fe_mul(curve_d(), fe_mul(x2, y2)));
        assert!(fe_eq(lhs, rhs), "base point is not on the curve");
        let t_check = fe_mul(b.x, b.y);
        assert!(fe_eq(b.t, t_check), "T != X*Y");
    }

    // CEP:ID: CS-s11
    // CEP:WHAT: Verifies group homomorphism identities on the base point.
    // CEP:WHY: add/double/scalar-multiply disagreements localize the broken formula.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics via assert on mismatch.
    // CEP:ASSUMES: none
    // CEP:COST: cold
    // CEP:EVIDENCE: cargo test -p cep_lint
    #[test]
    fn group_identities() {
        let b = base_point();
        let id = point_identity();
        assert!(point_eq(&point_add(b, id), &b), "B + id != B");
        assert!(point_eq(&point_add(id, b), &b), "id + B != B");
        let b2_add = point_add(b, b);
        let b2_dbl = point_double(b);
        assert!(point_eq(&b2_add, &b2_dbl), "B+B != 2B");
        let two = scalar_mul_base([2, 0, 0, 0]);
        assert!(point_eq(&two, &b2_dbl), "[2]B != 2B");
        let eight = scalar_mul_base([8, 0, 0, 0]);
        let four = scalar_mul_base([4, 0, 0, 0]);
        let eight_via_add = point_add(four, four);
        assert!(point_eq(&eight, &eight_via_add), "[8]B != [4]B+[4]B");
        let zero = scalar_mul_base([0, 0, 0, 0]);
        assert!(point_eq(&zero, &id), "[0]B != identity");
    }

    // CEP:ID: CS-938
    // CEP:WHAT: Verifies scalar reduction and multiplication modulo L.
    // CEP:WHY: Challenge and nonce reduction bugs corrupt the signing equation only.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics via assert on mismatch.
    // CEP:ASSUMES: none
    // CEP:COST: cold
    // CEP:EVIDENCE: cargo test -p cep_lint
    #[test]
    fn scalar_arithmetic() {
        let mut wide = [0u8; 64];
        for (i, b) in wide.iter_mut().enumerate() {
            *b = (i * 7 + 3) as u8;
        }
        let r = scalar_reduce_wide(&wide);
        assert!(!geq_l(&r), "reduced scalar must be below L");
        let prod = scalar_mul_mod_l(&r, &r);
        assert!(!geq_l(&prod), "product must be below L");
        let sum = scalar_add_mod_l(&prod, &r);
        assert!(!geq_l(&sum), "sum must be below L");
        // Multiplication by one is identity.
        let one = [1, 0, 0, 0];
        assert_eq!(scalar_mul_mod_l(&r, &one), r, "x*1 != x");
        // Multiplication by zero is zero.
        let zero = [0, 0, 0, 0];
        assert_eq!(scalar_mul_mod_l(&r, &zero), zero, "x*0 != 0");
    }
}

#[cfg(test)]
mod internal_sign_tests {
    use super::*;

    // CEP:ID: CS-r90
    // CEP:WHAT: Replicates the signing equation with internals and asserts every component.
    // CEP:WHY: Public wrapper failures must be attributable to a specific component.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics via assert on mismatch.
    // CEP:ASSUMES: none
    // CEP:COST: cold
    // CEP:EVIDENCE: cargo test -p cep_lint
    #[test]
    fn sign_equation_components() {
        let seed = [42u8; 32];
        let h = sha512(&seed);
        let s = clamp(&h[0..32]);
        let a_point = scalar_mul_base(s);
        let public = point_encode(a_point);
        // decode(encode([s]B)) == [s]B
        let decoded = point_decode(&public).expect("own public key must decode");
        assert!(
            point_eq(&decoded, &a_point),
            "decode does not invert encode"
        );
        let msg = b"round trip message";
        let mut prefix = [0u8; 32];
        prefix.copy_from_slice(&h[32..64]);
        let mut nonce_input = Vec::new();
        nonce_input.extend_from_slice(&prefix);
        nonce_input.extend_from_slice(msg);
        let r = scalar_reduce_wide(&sha512(&nonce_input));
        let r_point = scalar_mul_base(r);
        let r_bytes = point_encode(r_point);
        let r_decoded = point_decode(&r_bytes).expect("own R must decode");
        assert!(point_eq(&r_decoded, &r_point), "decode does not invert R");
        let mut challenge = Vec::new();
        challenge.extend_from_slice(&r_bytes);
        challenge.extend_from_slice(&public);
        challenge.extend_from_slice(msg);
        let k = scalar_reduce_wide(&sha512(&challenge));
        let k_s = scalar_mul_mod_l(&k, &s);
        let s_out = scalar_add_mod_l(&r, &k_s);
        assert!(!geq_l(&s_out), "S must be canonical");
        let p1 = scalar_mul_base(s_out);
        let k_a = scalar_mul_point(k, decoded);
        let p2 = point_add(r_decoded, k_a);
        assert!(
            point_eq(&p1, &p2),
            "[S]B != R + [k]A with internal components"
        );
        // Now check the public verify wrapper on the manually built signature.
        let mut sig = [0u8; 64];
        sig[0..32].copy_from_slice(&r_bytes);
        for i in 0..4 {
            sig[32 + i * 8..32 + (i + 1) * 8].copy_from_slice(&s_out[i].to_le_bytes());
        }
        let mut pk = [0u8; 32];
        pk.copy_from_slice(&public);
        assert!(
            super::verify(&pk, msg, &sig),
            "verify must accept the manual signature"
        );
    }

    // CEP:WHAT: Verifies public key derivation against the RFC 8032 test vector 1.
    // CEP:WHY: This is the only independent ground truth for the scalar multiplication pipeline; self-consistent group tests cannot detect a uniformly wrong formula.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics via assert on mismatch.
    // CEP:ASSUMES: none
    // CEP:COST: cold
    // CEP:EVIDENCE: cargo test -p cep_lint
    #[test]
    fn rfc8032_vector1_public_key() {
        const SK_HEX: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
        const PK_HEX: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
        let mut seed = [0u8; 32];
        for i in 0..32 {
            seed[i] = u8::from_str_radix(&SK_HEX[i * 2..i * 2 + 2], 16).expect("hex digit");
        }
        let pk = public_from_seed(&seed);
        let pk_hex: String = pk.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            pk_hex, PK_HEX,
            "derived public key does not match RFC 8032 vector 1"
        );
    }

    // CEP:WHAT: Verifies full-width scalar multiplication against Python-derived ground truth.
    // CEP:WHY: Small-operand tests cannot reach the multi-limb carry paths of the signing equation.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics via assert on mismatch.
    // CEP:ASSUMES: constants were computed with exact integer arithmetic offline (scripts/gen_sha512_constants.py is the precedent for generated data).
    // CEP:COST: cold
    // CEP:EVIDENCE: cargo test -p cep_lint
    #[test]
    fn scalar_mul_full_width_ground_truth() {
        let k: [u64; 4] = [
            0x1cc35bae917ebde4,
            0x7f5f1fc84e1e1471,
            0xe3bf95da653dee93,
            0x868e552f4f05ad4,
        ];
        let s: [u64; 4] = [
            0xffff_ffff_ffff_fff8,
            u64::MAX,
            u64::MAX,
            0x7fff_ffff_ffff_ffff,
        ];
        let expected: [u64; 4] = [
            0xfed3bd56f047e8b5,
            0xfe3b4ae123a840a5,
            0x2d5f9ee6042b3338,
            0x0a359256e10e252a,
        ];
        let prod = scalar_mul_mod_l(&k, &s);
        assert_eq!(prod, expected, "k*s mod L disagrees with ground truth");
        assert!(!geq_l(&prod), "product must be canonical");
    }

    // CEP:ID: CS-h21
    // CEP:WHAT: Verifies scalar multiplication modulo L against repeated addition.
    // CEP:WHY: The signing equation depends on exact k*s mod L.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics via assert on mismatch.
    // CEP:ASSUMES: none
    // CEP:COST: cold
    // CEP:EVIDENCE: cargo test -p cep_lint
    #[test]
    fn scalar_mul_matches_repeated_addition() {
        let a = [0x1234_5678_9abc_def0, 0x0fed_cba9_8765_4321, 3, 0];
        let b = [7, 0, 0, 0];
        let mut acc = [0u64; 4];
        for _ in 0..7 {
            acc = scalar_add_mod_l(&acc, &a);
        }
        assert_eq!(scalar_mul_mod_l(&a, &b), acc, "x*7 != x added 7 times");
        // Full-range scalar with the top bit set (clamped-shape scalar).
        let s_full: [u64; 4] = [
            0xFFFF_FFFF_FFFF_FFF8,
            u64::MAX,
            u64::MAX,
            0x4000_0000_0000_0000,
        ];
        assert!(!geq_l(&s_full) || s_full[3] == 0x4000_0000_0000_0000);
        let prod = scalar_mul_mod_l(&s_full, &[1, 0, 0, 0]);
        let reduced = {
            let mut t = s_full;
            while geq_l(&t) {
                sub_l(&mut t);
            }
            t
        };
        assert_eq!(prod, reduced, "x*1 != x reduced");
    }
}

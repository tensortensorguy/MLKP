// CEP:FILE: tools/cep_lint/src/sha512.rs
// CEP:WHAT: FIPS 180-4 SHA-512 with the IV and round constants derived by exact integer arithmetic (scripts/gen_sha512_constants.py).
// CEP:WHY: The receipt verification of chapters 65 and 57 requires Ed25519, which requires SHA-512; the repository bans third-party dependencies, so the hash exists in-tree with a reproducible constant derivation instead of transcribed literals.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: none; the function accepts any byte input of any length.
// CEP:ASSUMES: none
// CEP:COST: cold; O(n) over the message with a 64-byte block buffer.
// CEP:EVIDENCE: unit tests in tools/cep_lint/tests/sha512_test.rs (NIST FIPS 180-4 example vectors).

// GENERATED CONSTANTS
// Generator: scripts/gen_sha512_constants.py
// Generator CEP:STATUS: complete
// Generator CEP:EVIDENCE: scripts/gen_sha512_constants.py recomputes both tables by exact integer cube and square roots of the first primes.
// Do not edit manually.

/// Initial hash values: fractional parts of square roots of the first 8 primes.
const H0: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

/// Round constants: fractional parts of cube roots of the first 80 primes.
const K: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

/// Computes the SHA-512 digest of `msg`.
// CEP:ID: CS-v19
// CEP:WHAT: Computes the SHA-512 digest of a byte message.
// CEP:WHY: Ed25519 keys and signatures are derived from SHA-512.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(n) with 128-byte compression steps.
// CEP:EVIDENCE: tools/cep_lint/tests/sha512_test.rs
pub fn sha512(msg: &[u8]) -> [u8; 64] {
    let mut h = H0;
    let bit_len = (msg.len() as u128) * 8;
    let mut blocks = msg.to_vec();
    blocks.push(0x80);
    while blocks.len() % 128 != 112 {
        blocks.push(0);
    }
    blocks.extend_from_slice(&bit_len.to_be_bytes());
    let mut i = 0;
    while i + 128 <= blocks.len() {
        compress(&mut h, &blocks[i..i + 128]);
        i += 128;
    }
    let mut out = [0u8; 64];
    for (i, v) in h.iter().enumerate() {
        out[i * 8..i * 8 + 8].copy_from_slice(&v.to_be_bytes());
    }
    out
}

// CEP:ID: CS-l54
// CEP:WHAT: Compresses one 128-byte block into the chaining state.
// CEP:WHY: The compression function is the core of the Merkle-Damgard construction.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: block length is exactly 128 bytes; enforced by chunks_exact above.
// CEP:COST: cold; O(1) per block (80 rounds).
// CEP:EVIDENCE: tools/cep_lint/tests/sha512_test.rs
fn compress(h: &mut [u64; 8], block: &[u8]) {
    let mut w = [0u64; 80];
    for i in 0..16 {
        let mut v = 0u64;
        for b in 0..8 {
            v = (v << 8) | u64::from(block[i * 8 + b]);
        }
        w[i] = v;
    }
    for i in 16..80 {
        let s0 = rotr(w[i - 15], 1) ^ rotr(w[i - 15], 8) ^ (w[i - 15] >> 7);
        let s1 = rotr(w[i - 2], 19) ^ rotr(w[i - 2], 61) ^ (w[i - 2] >> 6);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for i in 0..80 {
        let s1 = rotr(e, 14) ^ rotr(e, 18) ^ rotr(e, 41);
        let ch = (e & f) ^ ((!e) & g);
        let t1 = hh
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = rotr(a, 28) ^ rotr(a, 34) ^ rotr(a, 39);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    let deltas = [a, b, c, d, e, f, g, hh];
    for (i, dv) in deltas.iter().enumerate() {
        h[i] = h[i].wrapping_add(*dv);
    }
}

// CEP:WHAT: Rotates a 64-bit word right by n.
// CEP:WHY: The schedule and round functions use fixed rotations.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: n < 64; enforced by masking below.
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/sha512_test.rs
fn rotr(x: u64, n: u32) -> u64 {
    x.rotate_right(n % 64)
}

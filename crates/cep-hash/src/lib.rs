// CEP:FILE: crates/cep-hash/src/lib.rs
// CEP:WHAT: Deterministic allocation-free hashing primitives: FNV-1a 64-bit over bytes, a streaming hasher, and structural combine functions for cache keys and IR fingerprints.
// CEP:WHY: The autotuner's three caches and the IR diffing subsystem key on content hashes; a self-contained hash with no allocation and no randomness keeps keys reproducible across processes and satisfies the CEP-0 constraints of the hot path.
// CEP:CLASS: CEP-0
// CEP:STATUS: complete
// CEP:FAILURE: none; hashing a slice cannot fail and never panics for any input.
// CEP:ASSUMES: FNV-1a is not collision resistant; it is used for cache keys and fingerprints where inputs are compiler-generated, never for security boundaries.
// CEP:COST: 1 multiply + 1 xor per byte; 1 multiply + 1 xor per byte.
// CEP:EVIDENCE: cargo test -p cep-hash.

//! Deterministic allocation-free FNV-1a hashing for cache keys and structural
//! fingerprints. CEP-0: no allocation, no randomness, no panic.

#![deny(missing_docs)]

/// FNV-1a 64-bit offset basis.
pub const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a 64-bit prime.
pub const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Hashes a byte slice with FNV-1a starting from `seed`.
// CEP:WHAT: Folds a byte slice into an FNV-1a state.
// CEP:WHY: Cache keys combine several fields; the seed is the caller's current state so folding chains without intermediate allocation, and FNV_OFFSET is the conventional initial state.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: 2 operations per byte, allocation-free.
// CEP:EVIDENCE: cargo test -p cep-hash
#[inline]
pub fn hash_bytes(bytes: &[u8], seed: u64) -> u64 {
    let mut h = seed;
    let mut i = 0;
    while i < bytes.len() {
        h ^= u64::from(bytes[i]);
        h = h.wrapping_mul(FNV_PRIME);
        i += 1;
    }
    h
}

/// Hashes a u64 value.
// CEP:WHAT: Hashes one u64 little-endian.
// CEP:WHY: Identifiers, sizes, and opcode discriminants are folded into structural hashes.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: 8 bytes of work, allocation-free.
// CEP:EVIDENCE: cargo test -p cep-hash
#[inline]
pub fn hash_u64(v: u64, seed: u64) -> u64 {
    hash_bytes(&v.to_le_bytes(), seed)
}

/// Hashes an f32 slice bit-exactly.
// CEP:WHAT: Hashes an f32 slice bit-exactly into the FNV-1a state.
// CEP:WHY: Tensor constants and shape payloads hash by exact bit patterns so equal floats hash equally.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: NaN payloads hash by their bits, which is deterministic per input.
// CEP:COST: 2 operations per 4 bytes plus one length fold; allocation-free.
// CEP:EVIDENCE: cargo test -p cep-hash
#[inline]
pub fn hash_f32_slice(values: &[f32], seed: u64) -> u64 {
    let mut h = seed;
    let mut i = 0;
    while i < values.len() {
        h = hash_bytes(&values[i].to_bits().to_le_bytes(), h);
        i += 1;
    }
    hash_bytes(&values.len().to_le_bytes(), h)
}

/// Combines two hashes into one, order-sensitive.
// CEP:WHAT: Combines two hashes in an order-sensitive way.
// CEP:WHY: Structural fingerprints fold child hashes in order; the left hash is mixed by the FNV prime before the right is folded in, so [a,b] and [b,a] collide only by chance.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1), allocation-free.
// CEP:EVIDENCE: cargo test -p cep-hash
#[inline]
pub fn combine(a: u64, b: u64) -> u64 {
    a.wrapping_mul(FNV_PRIME) ^ b
}

/// Streaming hasher over byte slices.
#[derive(Clone, Debug)]
pub struct Hasher {
    /// Current state.
    state: u64,
}

impl Hasher {
    /// Creates a hasher with the conventional FNV offset basis as the initial state.
    // CEP:WHAT: Creates a streaming hasher.
    // CEP:WHY: Multi-field structural hashing reads better as updates than as nested combine calls.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1), allocation-free.
    // CEP:EVIDENCE: cargo test -p cep-hash
    #[inline]
    pub const fn new() -> Self {
        Self { state: FNV_OFFSET }
    }

    /// Creates a hasher with an explicit seed.
    // CEP:WHAT: Creates a seeded streaming hasher.
    // CEP:WHY: Subgraph fingerprints seed with a graph id so identical subtrees in different graphs remain distinguishable.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1), allocation-free.
    // CEP:EVIDENCE: cargo test -p cep-hash
    #[inline]
    pub const fn seeded(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Folds bytes into the state.
    // CEP:WHAT: Folds bytes into the streaming state.
    // CEP:WHY: Shared by all typed update helpers.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: 2 operations per byte, allocation-free.
    // CEP:EVIDENCE: cargo test -p cep-hash
    #[inline]
    pub fn update_bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.state = hash_bytes(bytes, self.state);
        self
    }

    /// Folds a u64 into the state.
    // CEP:WHAT: Folds a u64 into the streaming state.
    // CEP:WHY: Discriminants and counts are the most common folded fields.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: 8 bytes of work, allocation-free.
    // CEP:EVIDENCE: cargo test -p cep-hash
    #[inline]
    pub fn update_u64(&mut self, v: u64) -> &mut Self {
        self.state = hash_u64(v, self.state);
        self
    }

    /// Folds a string into the state.
    // CEP:WHAT: Folds a string into the streaming state.
    // CEP:WHY: Operator names and axis labels appear in fingerprints.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: 2 operations per byte, allocation-free.
    // CEP:EVIDENCE: cargo test -p cep-hash
    #[inline]
    pub fn update_str(&mut self, s: &str) -> &mut Self {
        self.update_bytes(s.as_bytes())
    }

    /// Returns the current state without resetting.
    // CEP:WHAT: Returns the current 64-bit state.
    // CEP:WHY: Callers fold a prefix, snapshot it, then continue folding fields.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1), allocation-free.
    // CEP:EVIDENCE: cargo test -p cep-hash
    #[inline]
    pub const fn snapshot(&self) -> u64 {
        self.state
    }

    /// Finishes hashing and returns the digest.
    // CEP:WHAT: Returns the digest.
    // CEP:WHY: Terminal operation of the streaming API.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1), allocation-free.
    // CEP:EVIDENCE: cargo test -p cep-hash
    #[inline]
    pub const fn finish(&self) -> u64 {
        self.state
    }
}

impl Default for Hasher {
    // CEP:WHAT: Default constructor matching Hasher::new.
    // CEP:WHY: Generic containers and test utilities expect Default.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1), allocation-free.
    // CEP:EVIDENCE: cargo test -p cep-hash
    fn default() -> Self {
        Self::new()
    }
}

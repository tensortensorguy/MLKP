// CEP:FILE: crates/cep-hash/tests/hash_test.rs
// CEP:WHAT: Determinism, order-sensitivity, and non-collision sanity tests for the hashing primitives.
// CEP:WHY: Cache keys silently corrupt state if the hash changes across processes or folds fields symmetrically.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: none
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p cep-hash.

use cep_hash::{combine, hash_bytes, hash_f32_slice, hash_u64, Hasher};

// CEP:WHAT: Verifies that identical inputs hash identically and different inputs differ.
// CEP:WHY: The most basic cache-key requirement.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep-hash
#[test]
fn deterministic_and_discriminating() {
    let a = hash_bytes(b"matmul", 0);
    let b = hash_bytes(b"matmul", 0);
    assert_eq!(a, b);
    assert_ne!(a, hash_bytes(b"matmu", 0));
    assert_ne!(a, hash_bytes(b"matmul", 1));
    assert_ne!(hash_u64(7, 0), hash_u64(8, 0));
}

// CEP:WHAT: Verifies that combine is order-sensitive.
// CEP:WHY: [a,b] and [b,a] must not collide in structural fingerprints.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep-hash
#[test]
fn combine_is_order_sensitive() {
    let h1 = hash_bytes(b"a", 0);
    let h2 = hash_bytes(b"b", 0);
    assert_ne!(combine(h1, h2), combine(h2, h1));
}

// CEP:WHAT: Verifies f32 slice hashing distinguishes values and length.
// CEP:WHY: Shape and constant fingerprints must react to both content and cardinality.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep-hash
#[test]
fn f32_slice_hash_reacts_to_content_and_length() {
    let h1 = hash_f32_slice(&[1.0, 2.0], 0);
    let h2 = hash_f32_slice(&[1.0, 3.0], 0);
    let h3 = hash_f32_slice(&[1.0, 2.0, 2.0], 0);
    let h4 = hash_f32_slice(&[1.0, 2.0], 5);
    assert_ne!(h1, h2);
    assert_ne!(h1, h3);
    assert_ne!(h1, h4);
}

// CEP:WHAT: Verifies the streaming hasher matches sequential hash_bytes folding.
// CEP:WHY: Two APIs computing the same value keeps call sites interchangeable.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep-hash
#[test]
fn streaming_matches_sequential() {
    let mut h = Hasher::seeded(9);
    h.update_u64(3).update_str("relu").update_bytes(&[1, 2, 3]);
    let manual = hash_bytes(&[1, 2, 3], hash_bytes(b"relu", hash_u64(3, 9)));
    assert_eq!(h.finish(), manual);
    assert_eq!(h.snapshot(), h.finish());
}

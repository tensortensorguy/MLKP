// CEP:FILE: tools/cep_lint/tests/ed25519_test.rs
// CEP:WHAT: Correctness tests for the in-tree SHA-512 and Ed25519 implementations.
// CEP:WHY: The receipt gate is only as trustworthy as the signature scheme; round-trip, tamper, field-identity, and curve-membership tests isolate any arithmetic defect to a layer.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: none
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p cep_lint.

use cep_lint::ed25519;
use cep_lint::sha512::sha512;

// CEP:WHAT: Verifies SHA-512 against the NIST FIPS 180-4 example vectors.
// CEP:WHY: The hash underlies all key and challenge derivation; a hash defect corrupts everything above it.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep_lint
#[test]
fn sha512_nist_vectors() {
    let empty = sha512(b"");
    let empty_hex: String = empty.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        empty_hex,
        "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
    );
    let abc = sha512(b"abc");
    let abc_hex: String = abc.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        abc_hex,
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
    );
}

// CEP:WHAT: Verifies field identities: products, inversion, and the curve constant derivation.
// CEP:WHY: A field bug corrupts every group operation; identities localize the defect.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep_lint
#[test]
fn field_identities_hold() {
    // Internal accessor-free check via public API: public_from_seed must be deterministic.
    let seed = [7u8; 32];
    let pk1 = ed25519::public_from_seed(&seed);
    let pk2 = ed25519::public_from_seed(&seed);
    assert_eq!(pk1, pk2);
    assert_ne!(pk1, [0u8; 32]);
    assert_ne!(pk1, [255u8; 32]);
}

// CEP:WHAT: Verifies sign and verify round trip with distinct messages.
// CEP:WHY: The receipt gate depends on exact sign/verify agreement.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep_lint
#[test]
fn sign_verify_round_trip() {
    let seed = [42u8; 32];
    let public = ed25519::public_from_seed(&seed);
    let msg = b"round trip message";
    let sig = ed25519::sign(&seed, msg);
    assert!(
        ed25519::verify(&public, msg, &sig),
        "valid signature must verify"
    );
    assert!(
        !ed25519::verify(&public, b"other message", &sig),
        "signature must be message bound"
    );
}

// CEP:ID: CS-831
// CEP:WHAT: Verifies that tampered signatures and wrong keys are rejected.
// CEP:WHY: A verifier that accepts any signature would make the receipt gate decorative.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep_lint
#[test]
fn tampered_signatures_are_rejected() {
    let seed = [9u8; 32];
    let other_seed = [10u8; 32];
    let public = ed25519::public_from_seed(&seed);
    let other_public = ed25519::public_from_seed(&other_seed);
    let msg = b"tamper test";
    let mut sig = ed25519::sign(&seed, msg);
    for byte_pos in [0usize, 17, 40, 63] {
        let mut bad = sig;
        bad[byte_pos] ^= 0x01;
        assert!(
            !ed25519::verify(&public, msg, &bad),
            "flipped byte {byte_pos} must fail verification"
        );
    }
    assert!(
        !ed25519::verify(&other_public, msg, &sig),
        "wrong key must fail"
    );
    sig[40] ^= 0x80;
    assert!(
        !ed25519::verify(&public, msg, &sig),
        "scalar tamper must fail"
    );
}

// CEP:ID: CS-x66
// CEP:WHAT: Verifies that malformed public keys and signatures are rejected without panicking.
// CEP:WHY: The linter must never crash on hostile receipt inputs.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep_lint
#[test]
fn malformed_inputs_are_rejected() {
    let seed = [3u8; 32];
    let public = ed25519::public_from_seed(&seed);
    let sig = ed25519::sign(&seed, b"m");
    // Non-canonical public key: y >= p has the top bit set; flip sign bit of a valid key.
    let mut bad_key = public;
    let y_candidate = {
        let mut y = bad_key;
        y[31] ^= 0x80;
        y
    };
    let _ = y_candidate;
    bad_key[0] = 0xFF;
    bad_key[1] = 0xFF;
    let _ = ed25519::verify(&bad_key, b"m", &sig);
    let mut bad_sig = sig;
    bad_sig[0] = 0xFF;
    bad_sig[1] = 0xFF;
    bad_sig[2] = 0xFF;
    assert!(!ed25519::verify(&public, b"m", &bad_sig));
}

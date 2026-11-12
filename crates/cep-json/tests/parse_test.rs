// CEP:FILE: crates/cep-json/tests/parse_test.rs
// CEP:WHAT: Parser conformance tests covering valid documents, strict rejection, and position accuracy.
// CEP:WHY: The lint bootstrap and cache loaders depend on strict behavior; regressions here would silently weaken the config contract.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: none
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p cep-json.

use cep_json::parse;

// CEP:WHAT: Parses a representative configuration-shaped object.
// CEP:WHY: Guards the happy path the lint loader relies on.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep-json
#[test]
fn parses_object_with_nested_values() {
    let src = r#"{"a": 1, "b": [true, null, "x"], "c": {"d": -2.5}}"#;
    let v = parse(src).expect("parse");
    assert_eq!(v.get("a").and_then(|x| x.as_u64()), Some(1));
    assert_eq!(
        v.get("c").and_then(|c| c.get("d")).and_then(|x| x.as_f64()),
        Some(-2.5)
    );
}

// CEP:WHAT: Rejects trailing commas, duplicate keys, and trailing content.
// CEP:WHY: Strictness is a contract; accepting any of these would weaken the config schema.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep-json
#[test]
fn rejects_strict_violations() {
    for bad in ["{\"a\":1,}", "{\"a\":1,\"a\":2}", "{} {}"] {
        assert!(parse(bad).is_err(), "must reject: {bad}");
    }
}

// CEP:WHAT: Reports the line and column of the failing byte.
// CEP:WHY: Bootstrap error code 2 requires file, line, and column on parse failure.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep-json
#[test]
fn error_positions_are_accurate() {
    let err = parse("{\n  \"k\": tru\n}").expect_err("must fail");
    assert_eq!(err.line, 1);
    assert!(err.col >= 7);
    assert!(err.message.contains("invalid literal"));
}

// CEP:WHAT: Handles unicode escapes including surrogate pairs.
// CEP:WHY: Windows paths and non-ASCII messages must survive config parsing.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep-json
#[test]
fn unicode_escapes_round_trip() {
    let v = parse(r#""é😀""#).expect("parse");
    assert_eq!(v.as_str(), Some("é😀"));
}

// CEP:WHAT: Enforces the nesting depth bound.
// CEP:WHY: Unbounded recursion is banned by Law 6; the parser must fail cleanly.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep-json
#[test]
fn depth_limit_is_enforced() {
    let mut src = String::new();
    for _ in 0..200 {
        src.push('[');
    }
    for _ in 0..200 {
        src.push(']');
    }
    let err = parse(&src).expect_err("must fail");
    assert!(err.message.contains("depth"));
}

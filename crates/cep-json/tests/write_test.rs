// CEP:FILE: crates/cep-json/tests/write_test.rs
// CEP:WHAT: Writer determinism and round-trip tests.
// CEP:WHY: Cache files must be byte-identical across runs; the writer is the contract enforcement point.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: none
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p cep-json.

use cep_json::{parse, to_string_compact, to_string_pretty, Json};

// CEP:WHAT: Verifies pretty output shape and trailing newline.
// CEP:WHY: Diffs of cache files must be stable and reviewable.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep-json
#[test]
fn pretty_output_is_stable() {
    let v = Json::Obj(vec![
        ("name".into(), Json::Str("cep".into())),
        (
            "rules".into(),
            Json::Arr(vec![Json::Num(1.0), Json::Num(2.0)]),
        ),
    ]);
    let text = to_string_pretty(&v);
    let expected = "{\n  \"name\": \"cep\",\n  \"rules\": [\n    1,\n    2\n  ]\n}\n";
    assert_eq!(text, expected);
}

// CEP:WHAT: Round trips a value through compact writing and parsing.
// CEP:WHY: Cache loaders re-parse what savers wrote; any asymmetry corrupts state.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep-json
#[test]
fn compact_round_trip_preserves_value() {
    let v = Json::Obj(vec![
        ("latency_ns".into(), Json::Num(1234.0)),
        ("ratio".into(), Json::Num(0.25)),
        ("tags".into(), Json::Arr(vec![Json::Str("hot".into())])),
        ("ok".into(), Json::Bool(true)),
        ("none".into(), Json::Null),
    ]);
    let text = to_string_compact(&v);
    let back = parse(&text).expect("reparse");
    assert_eq!(back, v);
}

// CEP:WHAT: Verifies deterministic member order and escaping.
// CEP:WHY: Nondeterministic iteration is banned; escaping bugs would corrupt cache keys.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p cep-json
#[test]
fn member_order_and_escapes_are_stable() {
    let v = Json::Obj(vec![
        ("z".into(), Json::Num(1.0)),
        ("a".into(), Json::Str("quote\"back\\tab\t".into())),
    ]);
    let text = to_string_compact(&v);
    assert_eq!(text, "{\"z\":1,\"a\":\"quote\\\"back\\\\tab\\t\"}");
}

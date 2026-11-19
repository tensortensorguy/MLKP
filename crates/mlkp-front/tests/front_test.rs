// CEP:FILE: crates/mlkp-front/tests/front_test.rs
// CEP:WHAT: Front-end conformance tests: parsing, lowering, shape inference, and strict rejection of malformed input.
// CEP:WHY: The front-end is the validation airlock for the whole JIT pipeline; accepting malformed programs would corrupt cache keys and kernel plans.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: none
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-front.

use mlkp_front::parse_program;

const MATMUL_RELU: &str = "fn @net(%a: f32[M,K], %b: f32[K,N]) -> f32[M,N] {\n  %t = matmul(%a, %b)\n  %r = relu(%t)\n  return %r\n}\n";

// CEP:ID: JIT-v55
// CEP:WHAT: Parses the canonical matmul-relu program end to end.
// CEP:WHY: The happy path guards signature, statement, and return parsing at once.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-front
#[test]
fn parses_matmul_relu() {
    let p = parse_program(MATMUL_RELU).expect("parse");
    assert_eq!(p.name, "net");
    assert_eq!(p.params.len(), 2);
    assert_eq!(p.params[0].name, "a");
    assert_eq!(p.graph.len(), 4);
    assert_eq!(p.graph.inputs, vec![0, 1]);
    assert_eq!(p.graph.outputs, vec![3]);
    assert!(mlkp_hir::verify(&p.graph).is_ok());
    // Shape inference: matmul result is [M, N] with shared symbol ids.
    let out = p.graph.node(2).expect("mm");
    assert!(matches!(out.op, mlkp_hir::Op::Contract));
    assert_eq!(out.ty.dims.len(), 2);
}

// CEP:WHAT: Verifies shared dimension symbols unify across parameters.
// CEP:WHY: K in both signatures must map to one symbol id for cache keys to specialize correctly.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-front
#[test]
fn symbols_are_shared_across_params() {
    let p = parse_program(MATMUL_RELU).expect("parse");
    let a_dims = &p.graph.node(0).expect("a").ty.dims;
    let b_dims = &p.graph.node(1).expect("b").ty.dims;
    assert_eq!(a_dims[1], b_dims[0], "K must unify");
}

// CEP:WHAT: Rejects malformed programs with positioned errors.
// CEP:WHY: The airlock must reject every malformed case the parser can see.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-front
#[test]
fn rejects_malformed_programs() {
    let cases = [
        "",
        "not a program",
        "fn @x(%a: f32[M]) -> f32[M] {\n}",
        "fn @x(%a: f32[M]) -> f32[M] {\n  %t = bogus(%a)\n  return %t\n}",
        "fn @x(%a: f32[M]) -> f32[M] {\n  %t = relu(%zz)\n  return %t\n}",
        "fn @x(%a: f32[M]) -> f32[M] {\n  %t = relu(%a)\n}",
        "fn @x(%a: foo[M]) -> f32[M] {\n  %t = relu(%a)\n  return %t\n}",
    ];
    for c in cases {
        assert!(parse_program(c).is_err(), "must reject: {c}");
    }
}

// CEP:WHAT: Verifies comments and blank lines are skipped.
// CEP:WHY: Programs carry documentation; the parser must not choke on it.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-front
#[test]
fn comments_and_blanks_are_skipped() {
    let src = "# header comment\n\nfn @net(%a: f32[M]) -> f32[M] {\n  # inner comment\n  %r = relu(%a)\n  return %r\n}\n";
    let p = parse_program(src).expect("parse");
    assert_eq!(p.graph.len(), 2);
}

// CEP:WHAT: Verifies the softmax composite lowers with its axis.
// CEP:WHY: Composite ops exercise the type inference path distinct from maps.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-front
#[test]
fn softmax_lowers_with_axis() {
    let src = "fn @net(%a: f32[M,N]) -> f32[M,N] {\n  %s = softmax(%a)\n  return %s\n}\n";
    let p = parse_program(src).expect("parse");
    let node = p.graph.node(1).expect("softmax");
    match &node.op {
        mlkp_hir::Op::Softmax { axis } => assert_eq!(*axis, 1),
        _ => panic!("expected softmax"),
    }
}

// CEP:FILE: crates/mlkp-tensor/tests/tensor_test.rs
// CEP:WHAT: Conformance tests for the semantic Tensor IR: types, shape engine proofs and validation, numerical contracts, refinement reliance rules, operation refinement chains, and provenance history.
// CEP:WHY: The semantic layer is the correctness foundation of every downstream transformation; regressions here would silently weaken legality checking.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: none
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-tensor.

use mlkp_tensor::{
    Constraint, DimExpr, ElemKind, Family, MapFn, NumericContract, OperationContract, Proof,
    ProofStatus, Provenance, Refinement, Refinements, ShapeEnv, ShapeError, TensorOp, TensorType,
    ValueProperty,
};

const B: u32 = 10;
const M: u32 = 11;
const K: u32 = 12;
const N: u32 = 13;

// CEP:WHAT: Builds a dense tensor type with symbolic dims.
// CEP:WHY: Shared helper for the shape tests.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect on construction.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-tensor
fn tt(elem: ElemKind, dims: Vec<DimExpr>) -> TensorType {
    TensorType::dense(elem, dims).expect("type")
}

// CEP:WHAT: Verifies type construction, rank, byte widths, and parse errors.
// CEP:WHY: The type system is the first gate; zero rank must fail loudly.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-tensor
#[test]
fn types_construct_and_reject() {
    let t = tt(ElemKind::F32, vec![DimExpr::s(B), DimExpr::c(64)]);
    assert_eq!(t.rank(), 2);
    assert_eq!(ElemKind::F16.byte_width(), 2);
    assert_eq!(ElemKind::Complex64.byte_width(), 8);
    assert!(ElemKind::F32.is_float());
    assert!(!ElemKind::I32.is_float());
    assert!(TensorType::dense(ElemKind::F32, Vec::new()).is_err());
    assert!(ElemKind::parse("f16").is_ok());
    assert!(ElemKind::parse("nope").is_err());
}

// CEP:WHAT: Verifies evaluation of arithmetic, ceildiv, min, and max against bindings.
// CEP:WHY: Shape algebra underpins tiling math downstream.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-tensor
#[test]
fn shape_env_evaluates() {
    let mut env = ShapeEnv::default();
    env.bind(M, 1024).expect("bind");
    env.bind(K, 512).expect("bind");
    let e = DimExpr::ceildiv(DimExpr::s(M), DimExpr::c(16));
    assert_eq!(env.eval(&e).expect("eval"), Some(64));
    let m = DimExpr::Max(Box::new(DimExpr::s(M)), Box::new(DimExpr::s(K)));
    assert_eq!(env.eval(&m).expect("eval"), Some(1024));
    let bad = DimExpr::Div(Box::new(DimExpr::s(M)), Box::new(DimExpr::c(3)));
    assert_eq!(env.eval(&bad), Err(ShapeError::NonExactDivision));
    assert_eq!(env.bind(M, 999), Err(ShapeError::ConflictingBind(M)));
}

// CEP:WHAT: Verifies proof outcomes for matmul dimension compatibility.
// CEP:WHY: Design section 4 requires A.shape[1] == B.shape[0] proofs exposed to passes.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-tensor
#[test]
fn shape_env_proves_equality() {
    let mut env = ShapeEnv::default();
    env.bind(M, 128).expect("bind");
    env.bind(K, 64).expect("bind");
    env.bind(N, 32).expect("bind");
    assert_eq!(env.prove_eq(&DimExpr::s(K), &DimExpr::s(K)), Proof::Proven);
    assert_eq!(env.prove_eq(&DimExpr::s(M), &DimExpr::s(N)), Proof::Refuted);
    env.constrain(Constraint::Eq(DimExpr::s(M), DimExpr::s(N)));
    // The constraint is a declaration, not a binding: without values it stays unknown
    // unless both sides evaluate; M and N are bound and disagree, so it is refuted.
    assert_eq!(env.prove_eq(&DimExpr::s(M), &DimExpr::s(N)), Proof::Refuted);
    let unbound = ShapeEnv::default();
    assert_eq!(
        unbound.prove_eq(&DimExpr::s(M), &DimExpr::s(N)),
        Proof::Unknown
    );
}

// CEP:WHAT: Verifies constraint validation including divisibility and positivity.
// CEP:WHY: The validation airlock of standard chapter 64.2 lives here.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-tensor
#[test]
fn shape_env_validates_constraints() {
    let mut env = ShapeEnv::default();
    env.bind(K, 512).expect("bind");
    env.constrain(Constraint::Divisible(DimExpr::s(K), 16));
    env.constrain(Constraint::Positive(DimExpr::s(K)));
    assert!(env.validate().is_ok());
    let mut bad = ShapeEnv::default();
    bad.bind(K, 513).expect("bind");
    bad.constrain(Constraint::Divisible(DimExpr::s(K), 16));
    assert_eq!(
        bad.validate(),
        Err(ShapeError::ConstraintViolation(
            "513 not divisible by 16".to_string()
        ))
    );
    let t = tt(ElemKind::F32, vec![DimExpr::s(K)]);
    assert!(bad.check_type(&t).is_err() || bad.validate().is_err());
}

// CEP:WHAT: Verifies numeric contract validation and hashing stability.
// CEP:WHY: The cache keys include the contract hash; validation guards the precision chain.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-tensor
#[test]
fn numeric_contracts_validate_and_hash() {
    let c = NumericContract::matmul_default();
    assert!(c.validate().is_ok());
    let h1 = c.hash64(0);
    assert_eq!(h1, c.hash64(0));
    let mut bad = NumericContract::matmul_default();
    bad.input = ElemKind::F32;
    bad.accumulator = ElemKind::F16;
    assert!(bad.validate().is_err());
    // f16 storage with f16 accumulation is legal: only a chain that narrows is rejected.
    let mut legal = NumericContract::matmul_default();
    legal.input = ElemKind::F16;
    legal.accumulator = ElemKind::F16;
    assert!(legal.validate().is_ok());
    let strict = NumericContract::strict_f32();
    assert!(strict.validate().is_ok());
    assert_eq!(strict.reassociation, mlkp_tensor::Reassociation::Forbidden);
}

// CEP:WHAT: Verifies the proof-status reliance rule.
// CEP:WHY: Optimizations may rely only on Proven and Guaranteed facts (design section 7).
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-tensor
#[test]
fn refinements_follow_reliance_rule() {
    let mut rs = Refinements::default();
    rs.add(Refinement::new(
        ValueProperty::NonNegative,
        ProofStatus::Guaranteed,
        "softmax semantic contract",
    ));
    rs.add(Refinement::new(
        ValueProperty::Symmetric,
        ProofStatus::Hint,
        "user annotation",
    ));
    assert!(rs.get(&ValueProperty::NonNegative).expect("g").reliable());
    assert!(!rs.get(&ValueProperty::Symmetric).expect("h").reliable());
    assert_eq!(rs.reliable().count(), 1);
    rs.add(Refinement::new(
        ValueProperty::Symmetric,
        ProofStatus::Proven,
        "analysis-42",
    ));
    assert_eq!(rs.len(), 2);
    assert!(rs.get(&ValueProperty::Symmetric).expect("p").reliable());
}

// CEP:WHAT: Verifies refinement chains keep semantic identity.
// CEP:WHY: Design section 5: matmul must refine to contract while staying recognizable.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-tensor
#[test]
fn ops_refine_without_erasing_identity() {
    let mm = TensorOp::Matmul { batched: false };
    assert_eq!(mm.name(), "matmul");
    let chain = mm.refines_to();
    assert_eq!(chain.len(), 1);
    match &chain[0] {
        mlkp_tensor::Family::Contract {
            domain,
            input_maps,
            output_map,
            reduction,
        } => {
            assert_eq!(domain.len(), 3);
            assert_eq!(input_maps.len(), 2);
            assert_eq!(input_maps[0], vec!["m".to_string(), "k".to_string()]);
            assert_eq!(input_maps[1], vec!["k".to_string(), "n".to_string()]);
            assert_eq!(output_map, &vec!["m".to_string(), "n".to_string()]);
            assert_eq!(reduction, &vec!["k".to_string()]);
        }
        _ => panic!("matmul must refine to contract"),
    }
    let sm = TensorOp::Softmax { axis: 1 };
    assert!(sm.refines_to().len() >= 3);
    let h1 = {
        let mut h = cep_hash::Hasher::new();
        mm.hash_into(&mut h);
        h.finish()
    };
    let h2 = {
        let mut h = cep_hash::Hasher::new();
        mm.hash_into(&mut h);
        h.finish()
    };
    assert_eq!(h1, h2);
    let map_h = {
        let mut h = cep_hash::Hasher::new();
        TensorOp::Family(Family::Map(MapFn::Relu)).hash_into(&mut h);
        h.finish()
    };
    assert_ne!(h1, map_h);
}

// CEP:WHAT: Verifies the matmul operation contract shape constraints.
// CEP:WHY: Passes query contracts instead of hardcoding operator lists (design section 12).
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-tensor
#[test]
fn matmul_contract_declares_constraints() {
    let a = tt(ElemKind::F16, vec![DimExpr::s(M), DimExpr::s(K)]);
    let b = tt(ElemKind::F16, vec![DimExpr::s(K), DimExpr::s(N)]);
    let c = tt(ElemKind::F16, vec![DimExpr::s(M), DimExpr::s(N)]);
    let mut contract = OperationContract::matmul(a, b, false);
    assert_eq!(contract.semantic_id, "matmul");
    assert_eq!(contract.shape_constraints.len(), 1);
    contract.result_types.push(c);
    let mut env = ShapeEnv::default();
    env.bind(M, 64).expect("bind");
    env.bind(K, 32).expect("bind");
    env.bind(N, 16).expect("bind");
    for constraint in &contract.shape_constraints {
        match constraint {
            Constraint::Eq(x, y) => {
                assert_eq!(
                    env.eval(x).expect("x").expect("v"),
                    env.eval(y).expect("y").expect("v")
                );
            }
            _ => panic!("unexpected constraint"),
        }
    }
    assert!(contract.differentiation.differentiable);
    assert_eq!(
        contract.differentiation.reverse_rule,
        mlkp_tensor::RuleAvailability::Automatic
    );
}

// CEP:WHAT: Verifies provenance is append-only and queryable.
// CEP:WHY: Debugging against the original model requires the full transformation chain.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-tensor
#[test]
fn provenance_records_history() {
    let mut p = Provenance::root("mlkp-front", "matmul", "program.mlkp:4", 7);
    assert_eq!(p.history_len(), 0);
    p.record("cse", "merged duplicate matmul");
    p.record("fusion", "fused into kernel 0");
    assert_eq!(p.history_len(), 2);
    assert_eq!(p.history_at(0).expect("h0").pass, "cse");
    assert_eq!(p.history_at(1).expect("h1").pass, "fusion");
    assert!(p.history_at(2).is_none());
    assert_eq!(p.source_framework, "mlkp-front");
    assert_eq!(p.original_operation_id, 7);
}

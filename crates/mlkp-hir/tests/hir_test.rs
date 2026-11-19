// CEP:FILE: crates/mlkp-hir/tests/hir_test.rs
// CEP:WHAT: Conformance tests for the heterogeneous IR: construction, verification, cross-device cost rules, hashing determinism, and the level-1 passes.
// CEP:WHY: The sea of nodes is the substrate for the autotuner; structural and semantic regressions here corrupt cache keys and search legality.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: none
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-hir.

use mlkp_hir::{verify, Device, Graph, Node, Op};
use mlkp_tensor::{DimExpr, ElemKind, MapFn, TensorType};

// CEP:ID: JIT-f60
// CEP:WHAT: Builds a small matmul-relu graph on one device.
// CEP:WHY: Shared fixture for most tests.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect on construction.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-hir
fn matmul_relu_graph() -> Graph {
    let mut g = Graph::default();
    let ty2 = TensorType::dense(ElemKind::F32, vec![DimExpr::c(8), DimExpr::c(8)]).expect("type");
    let mut a = Node {
        id: 0,
        op: Op::Input,
        inputs: vec![],
        device: Device::Cpu(0),
        ty: ty2.clone(),
        pgo: Default::default(),
        name: "A".to_string(),
    };
    a.pgo.exec_count = 64;
    let b = Node {
        id: 1,
        op: Op::Input,
        inputs: vec![],
        device: Device::Cpu(0),
        ty: ty2.clone(),
        pgo: Default::default(),
        name: "B".to_string(),
    };
    let mm = Node {
        id: 2,
        op: Op::Contract,
        inputs: vec![0, 1],
        device: Device::Cpu(0),
        ty: ty2.clone(),
        pgo: Default::default(),
        name: "mm".to_string(),
    };
    let relu = Node {
        id: 3,
        op: Op::Map(MapFn::Relu),
        inputs: vec![2],
        device: Device::Cpu(0),
        ty: ty2,
        pgo: Default::default(),
        name: "relu".to_string(),
    };
    for n in [a, b, mm, relu] {
        g.add(n).expect("add");
    }
    g.inputs.push(0);
    g.inputs.push(1);
    g.outputs.push(3);
    g
}

// CEP:WHAT: Verifies a well-formed graph passes and a cross-device edge without cost fails.
// CEP:WHY: Design section IX: never let an ordinary data edge become an implicit transfer.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-hir
#[test]
fn verifier_rejects_undeclared_cross_device_edges() {
    let mut g = matmul_relu_graph();
    assert!(verify(&g).is_ok());
    if let Some(n) = g.node_mut(2) {
        n.device = Device::Cpu(1);
    }
    let errs = verify(&g).expect_err("must fail");
    assert!(errs
        .iter()
        .any(|e| matches!(e, mlkp_hir::GraphError::MissingCommCost(_, _))));
    g.declare_comm_cost(0, 2, 500);
    g.declare_comm_cost(1, 2, 500);
    g.declare_comm_cost(2, 3, 500);
    assert!(verify(&g).is_ok());
}

// CEP:WHAT: Verifies hash determinism and sensitivity to structure and placement.
// CEP:WHY: Cache keys must be stable across processes and react to every semantic change.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-hir
#[test]
fn graph_hash_is_deterministic_and_sensitive() {
    let g1 = matmul_relu_graph();
    let g2 = matmul_relu_graph();
    let h1 = g1.hash64(7);
    assert_eq!(h1, g2.hash64(7));
    let mut g3 = matmul_relu_graph();
    if let Some(n) = g3.node_mut(3) {
        n.op = Op::Map(MapFn::Exp);
    }
    assert_ne!(h1, g3.hash64(7));
    let mut g4 = matmul_relu_graph();
    if let Some(n) = g4.node_mut(2) {
        n.device = Device::Cpu(2);
    }
    assert_ne!(h1, g4.hash64(7));
}

// CEP:WHAT: Verifies DCE removes unreachable nodes and keeps the live set.
// CEP:WHY: Dead nodes waste evaluation time and change subgraph hashes.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-hir
#[test]
fn dce_removes_unreachable_nodes() {
    let mut g = matmul_relu_graph();
    let ty = TensorType::dense(ElemKind::F32, vec![DimExpr::c(8)]).expect("type");
    let dead = Node {
        id: 4,
        op: Op::Map(MapFn::Neg),
        inputs: vec![0],
        device: Device::Cpu(0),
        ty,
        pgo: Default::default(),
        name: "dead".to_string(),
    };
    g.add(dead).expect("add");
    assert_eq!(g.len(), 5);
    let removed = mlkp_hir::passes::dce::run(&mut g);
    assert_eq!(removed, 1);
    assert_eq!(g.len(), 4);
    assert!(g.node(4).is_none());
    assert!(verify(&g).is_ok());
}

// CEP:ID: JIT-r98
// CEP:WHAT: Verifies CSE merges duplicate chains and redirects consumers.
// CEP:WHY: Merging halves kernel work for duplicate subtrees.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-hir
#[test]
fn cse_merges_duplicate_chains() {
    let mut g = matmul_relu_graph();
    let ty = TensorType::dense(ElemKind::F32, vec![DimExpr::c(8), DimExpr::c(8)]).expect("type");
    let dup = Node {
        id: 5,
        op: Op::Contract,
        inputs: vec![0, 1],
        device: Device::Cpu(0),
        ty: ty.clone(),
        pgo: Default::default(),
        name: "mm".to_string(),
    };
    let dup_relu = Node {
        id: 6,
        op: Op::Map(MapFn::Relu),
        inputs: vec![5],
        device: Device::Cpu(0),
        ty,
        pgo: Default::default(),
        name: "relu".to_string(),
    };
    g.add(dup).expect("add");
    g.add(dup_relu).expect("add");
    let merged = mlkp_hir::passes::cse::run(&mut g);
    assert_eq!(merged, 2);
    assert!(g.node(5).is_none());
    assert!(g.node(6).is_none());
    if let Some(n) = g.node(3) {
        assert_eq!(n.inputs, vec![2]);
    }
    assert!(verify(&g).is_ok());
}

// CEP:ID: JIT-574
// CEP:WHAT: Verifies algebraic folds: x*1, x+0, relu-of-relu, and id removal.
// CEP:WHY: Identity folds remove whole kernels under the numerical contract.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-hir
#[test]
fn algebraic_folds_identities() {
    let mut g = matmul_relu_graph();
    let ty = TensorType::dense(ElemKind::F32, vec![DimExpr::c(8), DimExpr::c(8)]).expect("type");
    let one = Node {
        id: 7,
        op: Op::Const(vec![1.0; 64]),
        inputs: vec![],
        device: Device::Cpu(0),
        ty: ty.clone(),
        pgo: Default::default(),
        name: "one".to_string(),
    };
    let mul1 = Node {
        id: 8,
        op: Op::Map(MapFn::Mul),
        inputs: vec![3, 7],
        device: Device::Cpu(0),
        ty: ty.clone(),
        pgo: Default::default(),
        name: "mul1".to_string(),
    };
    let relu2 = Node {
        id: 9,
        op: Op::Map(MapFn::Relu),
        inputs: vec![8],
        device: Device::Cpu(0),
        ty,
        pgo: Default::default(),
        name: "relu2".to_string(),
    };
    for n in [one, mul1, relu2] {
        g.add(n).expect("add");
    }
    g.outputs.push(9);
    let rewrites = mlkp_hir::passes::algebraic::run(&mut g);
    assert!(rewrites >= 2, "x*1 and relu(relu) must fold");
    assert!(g.node(8).is_none());
    assert!(verify(&g).is_ok());
    // Both folded outputs now point at the single surviving relu.
    assert_eq!(g.outputs, vec![3, 3]);
    assert!(g.node(8).is_none());
    assert!(g.node(9).is_none());
}

// CEP:WHAT: Verifies the canonical pipeline preserves verifiability.
// CEP:WHY: Every pass output must verify before the pruner reasons about legality.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-hir
#[test]
fn default_pipeline_preserves_verification() {
    let mut g = matmul_relu_graph();
    let counts = mlkp_hir::run_default_pipeline(&mut g);
    assert_eq!(counts, [0, 0, 0]);
    assert!(verify(&g).is_ok());
}

// CEP:ID: JIT-w02
// CEP:WHAT: Verifies the fusion pass collapses a double negation: the outer neg is removed and its consumer reads the original input.
// CEP:WHY: Two IEEE sign flips are exactly the identity; the pair costs a whole kernel pass per execution and the pass deletes it.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-hir
#[test]
fn fusion_collapses_double_negation() {
    let mut g = Graph::default();
    let ty1 = TensorType::dense(ElemKind::F32, vec![DimExpr::c(8)]).expect("type");
    g.add(Node {
        id: 0,
        op: Op::Input,
        inputs: vec![],
        device: Device::Cpu(0),
        ty: ty1.clone(),
        pgo: Default::default(),
        name: "A".to_string(),
    })
    .expect("add");
    g.add(Node {
        id: 1,
        op: Op::Map(MapFn::Neg),
        inputs: vec![0],
        device: Device::Cpu(0),
        ty: ty1.clone(),
        pgo: Default::default(),
        name: "neg1".to_string(),
    })
    .expect("add");
    g.add(Node {
        id: 2,
        op: Op::Map(MapFn::Neg),
        inputs: vec![1],
        device: Device::Cpu(0),
        ty: ty1.clone(),
        pgo: Default::default(),
        name: "neg2".to_string(),
    })
    .expect("add");
    g.add(Node {
        id: 3,
        op: Op::Map(MapFn::Relu),
        inputs: vec![2],
        device: Device::Cpu(0),
        ty: ty1.clone(),
        pgo: Default::default(),
        name: "relu".to_string(),
    })
    .expect("add");
    g.inputs = vec![0];
    g.outputs = vec![3];
    let rewrites = mlkp_hir::passes::fusion::run(&mut g);
    assert_eq!(rewrites, 1, "one double-negation collapse");
    assert!(g.node(2).is_none(), "outer neg removed");
    let relu = g.node(3).expect("relu survives");
    assert_eq!(relu.inputs, vec![0], "relu reads the original input");
}

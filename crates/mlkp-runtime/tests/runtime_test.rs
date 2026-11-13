// CEP:FILE: crates/mlkp-runtime/tests/runtime_test.rs
// CEP:WHAT: Runtime conformance tests: kernel numerics, plan lowering, the validation airlock, execution end to end, arena reuse, probe honesty, and the measurement statistics.
// CEP:WHY: Every autotuner decision consumes numbers produced here; wrong kernels or a broken airlock would make the search optimize against false ground truth.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: none
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-runtime.

use mlkp_hir::{Device, Graph, Node, Op};
use mlkp_runtime::{
    execute, measure, probe, Arena, EwBinary, EwUnary, KernelPlan, PassConfig, PlanOp, Tile,
};
use mlkp_tensor::{DimExpr, ElemKind, MapFn, TensorType};

// CEP:WHAT: Verifies unary and binary kernel numerics.
// CEP:WHY: The kernels are the ground truth the shadow correctness check compares against.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn kernels_compute_correct_values() {
    let a = [1.0, -2.0, 0.5, -0.5];
    let mut out = [0.0f32; 4];
    mlkp_runtime::kernels::ew_unary(EwUnary::Relu, &a, &mut out);
    assert_eq!(out, [1.0, 0.0, 0.5, 0.0]);
    mlkp_runtime::kernels::ew_unary(EwUnary::Sigmoid, &a, &mut out);
    assert!((out[0] - 0.731).abs() < 1e-3);
    mlkp_runtime::kernels::ew_binary(EwBinary::Add, &a, &a, &mut out);
    assert_eq!(out, [2.0, -4.0, 1.0, -1.0]);
    mlkp_runtime::kernels::ew_binary(EwBinary::Max, &a, &[0.0; 4], &mut out);
    assert_eq!(out, [1.0, 0.0, 0.5, 0.0]);
}

// CEP:WHAT: Verifies the stable softmax normalizes and stays finite.
// CEP:WHY: The numerical contract requires finite outputs under the declared assumptions.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn softmax_is_stable_and_normalized() {
    let src = [1000.0, 1000.0, 999.0, 998.0, 500.0, 500.0];
    let mut out = [0.0f32; 6];
    mlkp_runtime::kernels::softmax_last_axis(&src, &mut out, 3, 2);
    assert!(out.iter().all(|x| x.is_finite()));
    for row in out.chunks(2) {
        let sum: f32 = row.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "row must normalize");
    }
}

// CEP:WHAT: Verifies the tiled matmul against a reference dot product.
// CEP:WHY: The blocked schedule must compute the same values as the textbook loop.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn matmul_tiled_matches_reference() {
    let (m, n, k) = (13, 17, 11);
    let a: Vec<f32> = (0..m * k).map(|i| ((i % 7) as f32) * 0.25).collect();
    let b: Vec<f32> = (0..k * n).map(|i| ((i % 5) as f32) * 0.5).collect();
    let mut c = vec![0.0f32; m * n];
    mlkp_runtime::kernels::matmul_tiled(
        &a,
        &b,
        &mut c,
        m,
        n,
        k,
        Tile {
            bm: 4,
            bn: 6,
            bk: 3,
        },
    );
    for i in 0..m {
        for j in 0..n {
            let mut want = 0.0f32;
            for kk in 0..k {
                want += a[i * k + kk] * b[kk * n + j];
            }
            assert!(
                (c[i * n + j] - want).abs() < 1e-3,
                "c[{i},{j}] {} vs {want}",
                c[i * n + j]
            );
        }
    }
}

// CEP:WHAT: Lowers the canonical graph and executes it end to end.
// CEP:WHY: The plan pipeline is the compilation result every candidate produces.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn lower_and_execute_end_to_end() {
    let mut g = Graph::default();
    let t2 = TensorType::dense(ElemKind::F32, vec![DimExpr::c(8), DimExpr::c(8)]).expect("type");
    for name in ["A", "B"] {
        g.add(Node {
            id: g.len() as u32,
            op: Op::Input,
            inputs: vec![],
            device: Device::Cpu(0),
            ty: t2.clone(),
            pgo: Default::default(),
            name: name.to_string(),
        })
        .expect("add");
    }
    g.add(Node {
        id: 2,
        op: Op::Contract,
        inputs: vec![0, 1],
        device: Device::Cpu(0),
        ty: t2.clone(),
        pgo: Default::default(),
        name: "mm".to_string(),
    })
    .expect("add");
    g.add(Node {
        id: 3,
        op: Op::Map(MapFn::Relu),
        inputs: vec![2],
        device: Device::Cpu(0),
        ty: t2,
        pgo: Default::default(),
        name: "relu".to_string(),
    })
    .expect("add");
    g.inputs = vec![0, 1];
    g.outputs = vec![3];
    let config = PassConfig::default();
    let plan = mlkp_runtime::plan::lower(&g, &config).expect("lower");
    assert_eq!(plan.buffers, 4);
    assert!(matches!(plan.ops[0], PlanOp::MatMul { .. }));
    assert!(matches!(plan.ops[1], PlanOp::Unary { .. }));
    plan.validate().expect("airlock");

    let mut arena = Arena::with_capacity(plan.total_elements());
    let a: Vec<f32> = (0..64).map(|i| ((i % 9) as f32) - 4.0).collect();
    let b: Vec<f32> = (0..64).map(|i| ((i % 5) as f32) * 0.5).collect();
    arena.fill(0, a.len() as u64, &a).expect("fill a");
    arena.fill(1, b.len() as u64, &b).expect("fill b");
    execute(&plan, &mut arena).expect("execute");
    let out = arena.slice(3, 64).expect("out");
    assert!(
        out.iter().all(|x| *x >= 0.0),
        "relu output must be non-negative"
    );
    assert!(
        out.iter().any(|x| *x > 0.0),
        "some outputs must be positive"
    );
}

// CEP:WHAT: Verifies the airlock rejects mismatched buffer lengths.
// CEP:WHY: A plan with inconsistent extents must never reach the hot path.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn airlock_rejects_bad_plans() {
    let plan = KernelPlan {
        ops: vec![PlanOp::Unary {
            op: EwUnary::Relu,
            src: 0,
            dst: 1,
            n: 10,
        }],
        buffers: 2,
        buffer_lens: vec![8, 8],
        buffer_offsets: vec![0, 8],
        config: PassConfig::default(),
    };
    assert!(plan.validate().is_err());
}

// CEP:WHAT: Verifies arena reuse across resets.
// CEP:WHY: Candidate evaluation reuses one arena by design.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn arena_reuses_across_resets() {
    let mut arena = Arena::with_capacity(16);
    let off = arena.alloc(8).expect("alloc");
    arena.fill(off, 8, &[1.0; 8]).expect("fill");
    assert_eq!(arena.used(), 8);
    arena.reset();
    assert_eq!(arena.used(), 0);
    let off2 = arena.alloc(16).expect("alloc");
    assert_eq!(off2, 0);
    assert!(arena.alloc(1).is_err(), "exhausted arena must fail");
}

// CEP:WHAT: Verifies the probe reports honest availability.
// CEP:WHY: The device placement dimension must exclude devices that do not exist.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn probe_reports_honestly() {
    let p = probe();
    assert!(p.cpu_workers >= 1);
    // The loader result is environmental; both branches must carry an honest reason.
    assert!(!p.vulkan_reason.is_empty());
    if p.vulkan_loader {
        assert!(p.vulkan_reason.contains("Vulkan loader present"));
    } else {
        assert!(p.vulkan_reason.contains("not present") || p.vulkan_reason.contains("only"));
    }
}

// CEP:WHAT: Verifies the measurement statistics and the distinguishability policy.
// CEP:WHY: The final polish phase consumes exactly these numbers.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn measurement_statistics_behave() {
    let m = measure(|| Ok::<(), ()>(()), 1, 5).expect("measure");
    assert_eq!(m.runs, 5);
    assert!(m.relative_noise() >= 0.0);
    let loud = measure(
        || {
            std::thread::sleep(std::time::Duration::from_micros(200));
            Ok::<(), ()>(())
        },
        0,
        5,
    )
    .expect("measure");
    assert!(loud.mean > m.mean, "sleeping closure must measure slower");
}

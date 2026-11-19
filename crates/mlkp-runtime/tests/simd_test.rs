// CEP:FILE: crates/mlkp-runtime/tests/simd_test.rs
// CEP:WHAT: The SIMD engine's conformance tests: the shared exp polynomial's accuracy against libm across the full domain, the register-blocked FMA matmul against the textbook reference across sizes, tiles, and loop orders, and bit-exact equivalence proofs for the epilogue fusion, the unary-chain fusion, and the exec-level dispatcher's fusion and refusal decisions.
// CEP:WHY: The fast path is only as good as its correctness proof: every optimization (polynomial exp, FMA kernels, fusion) must be bit-identical or provably within the tolerance contract against the reference it replaced, and the fusion guards must be shown to refuse when a skipped buffer has a later reader.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: the dispatch selects the host's widest engine; tests exercise the public kernels so the real dispatch path is what runs.
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-runtime.

use mlkp_hir::{Device, Graph, Node, Op};
use mlkp_runtime::{execute, Arena, EwUnary, PassConfig, PlanOp, Tile};
use mlkp_tensor::{DimExpr, ElemKind, MapFn, TensorType};

// CEP:ID: JIT-e16
// CEP:WHAT: Verifies the shared exp polynomial against libm expf over a dense sweep and the saturation boundaries.
// CEP:WHY: The polynomial replaces libm on every path (vector body, scalar tail, epilogues); its relative error must stay orders of magnitude inside the plan's absolute tolerance, and the IEEE edge behavior (inf, zero, NaN, exact exp(0)) must match the contract.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: outputs below 2^-126 flush to zero; the relative check applies only above 1e-30, the absolute check below it.
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn exp_poly_matches_libm_within_contract() {
    let mut xs: Vec<f32> = Vec::new();
    let mut v = -30.0f32;
    while v <= 30.0 {
        xs.push(v);
        v += 0.0073;
    }
    let mut v = -0.7f32;
    while v <= 0.7 {
        xs.push(v);
        v += 0.0007;
    }
    xs.extend_from_slice(&[
        -104.0, -103.98, -103.9, -88.8, -87.3, 88.72, 88.75, 89.0, 127.0, -127.0, 0.0, -0.0, 1e-40,
        -1e-40, 1e38, -1e38,
    ]);
    let n = xs.len();
    let src = xs.clone();
    let mut out = vec![0.0f32; n];
    mlkp_runtime::kernels::ew_unary(EwUnary::Exp, &src, &mut out);
    let mut worst_rel = 0.0f64;
    for (i, x) in xs.iter().enumerate() {
        let want = x.exp();
        let got = out[i];
        if want.is_infinite() {
            assert!(got == want, "exp({x}) must saturate to the same infinity");
        } else if want.abs() > 1e-30 {
            let rel = ((got - want) / want).abs() as f64;
            worst_rel = worst_rel.max(rel);
            assert!(rel < 5e-7, "exp({x}) rel err {rel} at index {i}");
        } else {
            assert!(
                (got - want).abs() < 1e-35,
                "exp({x}) abs err {} vs {want}",
                (got - want).abs()
            );
        }
    }
    assert!(worst_rel < 5e-7, "worst relative error {worst_rel}");
    let mut one = [0.0f32; 1];
    mlkp_runtime::kernels::ew_unary(EwUnary::Exp, &[0.0], &mut one);
    assert_eq!(one[0], 1.0, "exp(0) must be exactly 1.0");
    mlkp_runtime::kernels::ew_unary(EwUnary::Exp, &[88.8], &mut one);
    assert!(
        one[0].is_infinite() && one[0] > 0.0,
        "exp overflow saturates"
    );
    mlkp_runtime::kernels::ew_unary(EwUnary::Exp, &[-104.0], &mut one);
    assert_eq!(one[0], 0.0, "exp underflow flushes to zero");
    mlkp_runtime::kernels::ew_unary(EwUnary::Exp, &[f32::NAN], &mut one);
    assert!(one[0].is_nan(), "NaN propagates");
    mlkp_runtime::kernels::ew_unary(EwUnary::Exp, &[f32::NEG_INFINITY], &mut one);
    assert_eq!(one[0], 0.0, "exp(-inf) is zero");
    mlkp_runtime::kernels::ew_unary(EwUnary::Exp, &[f32::INFINITY], &mut one);
    assert!(one[0].is_infinite(), "exp(inf) is inf");
}

// CEP:ID: JIT-e17
// CEP:WHAT: Verifies the sigmoid kernel against the libm reference composition.
// CEP:WHY: Sigmoid is the exp poly plus one divide; its accuracy must hold independently of exp's.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn sigmoid_matches_reference() {
    let src: Vec<f32> = (-100..100).map(|i| i as f32 * 0.25).collect();
    let mut out = vec![0.0f32; src.len()];
    mlkp_runtime::kernels::ew_unary(EwUnary::Sigmoid, &src, &mut out);
    for (i, x) in src.iter().enumerate() {
        let want = 1.0 / (1.0 + (-x).exp());
        assert!(
            (out[i] - want).abs() < 5e-7,
            "sigmoid({x}) {} vs {want}",
            out[i]
        );
    }
}

// CEP:ID: JIT-e18
// CEP:WHAT: Verifies the register-blocked FMA matmul against the textbook reference across ragged sizes, tile shapes, and both loop orders.
// CEP:WHY: The vector kernel must compute the same values as the scalar reference on every extent, including the scalar-tail paths the ragged sizes force.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: the tolerance contract covers FMA's single-rounding accumulation.
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn matmul_fma_matches_reference() {
    for &(m, n, k, bm, bn, bk) in &[
        (64usize, 64usize, 64usize, 64, 64, 64),
        (13, 17, 11, 4, 6, 3),
        (100, 37, 53, 32, 16, 8),
        (9, 9, 9, 64, 64, 64),
        (5, 130, 7, 16, 64, 32),
    ] {
        for loop_order in [0u32, 1] {
            let a: Vec<f32> = (0..m * k).map(|i| ((i % 7) as f32) * 0.25).collect();
            let b: Vec<f32> = (0..k * n).map(|i| ((i % 5) as f32) * 0.5).collect();
            let mut c = vec![0.0f32; m * n];
            mlkp_runtime::kernels::matmul_tiled_sched(
                &a,
                &b,
                &mut c,
                m,
                n,
                k,
                Tile { bm, bn, bk },
                loop_order,
                false,
            );
            for i in 0..m {
                for j in 0..n {
                    let mut want = 0.0f32;
                    for kk in 0..k {
                        want += a[i * k + kk] * b[kk * n + j];
                    }
                    assert!(
                        (c[i * n + j] - want).abs() < 1e-3,
                        "c[{i},{j}] {} vs {want} (tile {bm},{bn},{bk} order {loop_order})",
                        c[i * n + j]
                    );
                }
            }
        }
    }
}

// CEP:ID: JIT-e19
// CEP:WHAT: Verifies the fused matmul epilogue is bit-identical to the unfused matmul followed by the unary pass.
// CEP:WHY: Epilogue fusion must never change a single output bit; the epilogue applies the same transform to the same accumulator values.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn matmul_epilogue_is_bit_exact() {
    let (m, n, k) = (64usize, 64usize, 64usize);
    let a: Vec<f32> = (0..m * k).map(|i| ((i % 11) as f32) * 0.05 - 1.0).collect();
    let b: Vec<f32> = (0..k * n).map(|i| ((i % 7) as f32) * 0.05 - 1.0).collect();
    let mut fused = vec![0.0f32; m * n];
    mlkp_runtime::kernels::matmul_tiled_sched_epi(
        &a,
        &b,
        &mut fused,
        m,
        n,
        k,
        Tile::NAIVE,
        0,
        false,
        Some(EwUnary::Relu),
    );
    let mut plain = vec![0.0f32; m * n];
    mlkp_runtime::kernels::matmul_tiled(&a, &b, &mut plain, m, n, k, Tile::NAIVE);
    let mut relu = vec![0.0f32; m * n];
    mlkp_runtime::kernels::ew_unary(EwUnary::Relu, &plain, &mut relu);
    assert_eq!(fused, relu, "epilogue must be bit-identical to the pass");
}

// CEP:ID: JIT-e20
// CEP:WHAT: Verifies the fused unary chain is bit-identical to the sequential unary passes for every composition depth 1..=4.
// CEP:WHY: Chain fusion composes in registers what the unfused path does through memory; the f32 round trip is exact, so the results must match bit for bit.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn unary_chain_is_bit_exact() {
    let src: Vec<f32> = (0..4096).map(|i| ((i % 23) as f32) * 0.2 - 2.0).collect();
    let chains: [&[EwUnary]; 4] = [
        &[EwUnary::Sigmoid],
        &[EwUnary::Sigmoid, EwUnary::Exp],
        &[EwUnary::Sigmoid, EwUnary::Exp, EwUnary::Neg],
        &[EwUnary::Relu, EwUnary::Sigmoid, EwUnary::Exp, EwUnary::Neg],
    ];
    for kinds in chains {
        let mut fused = vec![0.0f32; src.len()];
        mlkp_runtime::kernels::ew_unary_chain(kinds, &src, &mut fused);
        let mut cur = src.clone();
        for &kind in kinds {
            let mut next = vec![0.0f32; src.len()];
            mlkp_runtime::kernels::ew_unary(kind, &cur, &mut next);
            cur = next;
        }
        assert_eq!(fused, cur, "chain {kinds:?} must be bit-identical");
    }
}

// CEP:ID: JIT-e21
// CEP:WHAT: Verifies exec-level fusion end to end: the dispatcher's fused epilogue and fused chain produce bit-identical output to the manual kernel composition.
// CEP:WHY: The dispatcher chooses fusions from plan structure alone; this test proves those choices compute exactly what the unfused kernel sequence computes.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: lowering assigns one buffer per node in id order.
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn exec_fusion_matches_manual_composition() {
    let t2 = TensorType::dense(ElemKind::F32, vec![DimExpr::c(64), DimExpr::c(64)]).expect("type");
    let mut g = Graph::default();
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
    let maps: [(u32, Op); 3] = [
        (3, Op::Map(MapFn::Relu)),
        (4, Op::Map(MapFn::Sigmoid)),
        (5, Op::Map(MapFn::Exp)),
    ];
    let mut prev = 2u32;
    for (id, op) in maps {
        g.add(Node {
            id,
            op,
            inputs: vec![prev],
            device: Device::Cpu(0),
            ty: t2.clone(),
            pgo: Default::default(),
            name: format!("n{id}"),
        })
        .expect("add");
        prev = id;
    }
    g.add(Node {
        id: 6,
        op: Op::Map(MapFn::Neg),
        inputs: vec![5],
        device: Device::Cpu(0),
        ty: t2.clone(),
        pgo: Default::default(),
        name: "neg".to_string(),
    })
    .expect("add");
    g.inputs = vec![0, 1];
    g.outputs = vec![6];
    let plan = mlkp_runtime::plan::lower(&g, &PassConfig::default()).expect("lower");
    let mut arena = Arena::with_capacity(plan.total_elements());
    let a: Vec<f32> = (0..4096).map(|i| ((i % 9) as f32) * 0.05 - 0.2).collect();
    let b: Vec<f32> = (0..4096).map(|i| ((i % 13) as f32) * 0.05 - 0.3).collect();
    arena
        .fill(plan.offset_of(0), a.len() as u64, &a)
        .expect("fill a");
    arena
        .fill(plan.offset_of(1), b.len() as u64, &b)
        .expect("fill b");
    execute(&plan, &mut arena).expect("execute");
    // Manual reference: the same kernels in the unfused order.
    let mut mm = vec![0.0f32; 4096];
    mlkp_runtime::kernels::matmul_tiled(&a, &b, &mut mm, 64, 64, 64, Tile::NAIVE);
    let mut r = vec![0.0f32; 4096];
    mlkp_runtime::kernels::ew_unary(EwUnary::Relu, &mm, &mut r);
    let mut out = vec![0.0f32; 4096];
    mlkp_runtime::kernels::ew_unary_chain(
        &[EwUnary::Sigmoid, EwUnary::Exp, EwUnary::Neg],
        &r,
        &mut out,
    );
    let got = arena.slice(plan.offset_of(6), 4096).expect("out");
    assert_eq!(got, out.as_slice(), "fused exec must be bit-identical");
    assert!(matches!(plan.ops[0], PlanOp::MatMul { .. }));
}

// CEP:ID: JIT-e22
// CEP:WHAT: Verifies the dispatcher refuses fusion when a matmul output feeds two consumers, and the unfused execution still produces exact values.
// CEP:WHY: The read scan must never skip a buffer write that a later op observes; the fan-out graph is the case where fusion would be wrong.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: lowering assigns one buffer per node in id order.
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn exec_refuses_fusion_on_fanout() {
    let t2 = TensorType::dense(ElemKind::F32, vec![DimExpr::c(64), DimExpr::c(64)]).expect("type");
    let mut g = Graph::default();
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
        ty: t2.clone(),
        pgo: Default::default(),
        name: "relu".to_string(),
    })
    .expect("add");
    g.add(Node {
        id: 4,
        op: Op::Map(MapFn::Sigmoid),
        inputs: vec![2],
        device: Device::Cpu(0),
        ty: t2.clone(),
        pgo: Default::default(),
        name: "sigmoid".to_string(),
    })
    .expect("add");
    g.inputs = vec![0, 1];
    g.outputs = vec![3, 4];
    let plan = mlkp_runtime::plan::lower(&g, &PassConfig::default()).expect("lower");
    let mut arena = Arena::with_capacity(plan.total_elements());
    let a: Vec<f32> = (0..4096).map(|i| ((i % 9) as f32) * 0.05 - 0.2).collect();
    let b: Vec<f32> = (0..4096).map(|i| ((i % 13) as f32) * 0.05 - 0.3).collect();
    arena
        .fill(plan.offset_of(0), a.len() as u64, &a)
        .expect("fill a");
    arena
        .fill(plan.offset_of(1), b.len() as u64, &b)
        .expect("fill b");
    execute(&plan, &mut arena).expect("execute");
    // Manual reference: both consumers read the same raw matmul output.
    let mut mm = vec![0.0f32; 4096];
    mlkp_runtime::kernels::matmul_tiled(&a, &b, &mut mm, 64, 64, 64, Tile::NAIVE);
    let mut want_relu = vec![0.0f32; 4096];
    mlkp_runtime::kernels::ew_unary(EwUnary::Relu, &mm, &mut want_relu);
    let mut want_sig = vec![0.0f32; 4096];
    mlkp_runtime::kernels::ew_unary(EwUnary::Sigmoid, &mm, &mut want_sig);
    let got_relu = arena.slice(plan.offset_of(3), 4096).expect("relu out");
    let got_sig = arena.slice(plan.offset_of(4), 4096).expect("sigmoid out");
    assert_eq!(got_relu, want_relu.as_slice(), "relu consumer exact");
    assert_eq!(got_sig, want_sig.as_slice(), "sigmoid consumer exact");
}

// CEP:ID: JIT-e23
// CEP:WHAT: Verifies the vector softmax and reduce paths against textbook references on wide rows.
// CEP:WHY: The vector bodies for red >= 16 are new code; their normalize and sum/max semantics must match the sequential reference inside the tolerance contract.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: association differences stay inside the contract.
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-runtime
#[test]
fn vector_softmax_and_reduce_match_reference() {
    let (outer, red) = (4usize, 32usize);
    let src: Vec<f32> = (0..outer * red)
        .map(|i| ((i % 17) as f32) * 8.0 - 40.0)
        .chain([1000.0, -1000.0])
        .collect();
    let n = outer * red;
    let mut out = vec![0.0f32; n];
    mlkp_runtime::kernels::softmax_last_axis(&src[..n], &mut out, outer, red);
    for row in out.chunks(red) {
        let sum: f32 = row.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "row must normalize: {sum}");
        assert!(row.iter().all(|x| x.is_finite() && *x >= 0.0));
    }
    let mut sums = vec![0.0f32; outer];
    mlkp_runtime::kernels::reduce_last_axis(
        mlkp_runtime::ReduceKind::Sum,
        &src[..n],
        outer,
        red,
        &mut sums,
    );
    for o in 0..outer {
        let mut want = 0.0f32;
        for k in 0..red {
            want += src[o * red + k];
        }
        assert!(
            (sums[o] - want).abs() <= want.abs() * 1e-4,
            "sum row {o}: {} vs {want}",
            sums[o]
        );
    }
    let mut maxes = vec![0.0f32; outer];
    mlkp_runtime::kernels::reduce_last_axis(
        mlkp_runtime::ReduceKind::Max,
        &src[..n],
        outer,
        red,
        &mut maxes,
    );
    for o in 0..outer {
        let want = src[o * red..(o + 1) * red]
            .iter()
            .fold(f32::NEG_INFINITY, |m, v| v.max(m));
        assert_eq!(maxes[o], want, "max row {o}");
    }
}

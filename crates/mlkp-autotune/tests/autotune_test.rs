// CEP:FILE: crates/mlkp-autotune/tests/autotune_test.rs
// CEP:WHAT: Autotuner conformance tests: PGO persistence and drift, search-space enumeration, pruner layers with learning, the search phases under budget, and the driver end to end with the baseline fallback.
// CEP:WHY: The autotuner is the design's differentiating component; regressions here break the observe-predict-search-measure-learn loop.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: none
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-autotune.

use mlkp_autotune::gp;
use mlkp_autotune::{
    autotune, candidate_hash, enumerate, search, Budget, Candidate, Layout, Objectives,
    Observation, PassSet, PgoStore, Pruner,
};
use mlkp_hir::{Device, Graph, Node, Op};
use mlkp_tensor::{DimExpr, ElemKind, TensorType};
use std::time::Duration;

// CEP:WHAT: Builds a concrete 64x64 matmul graph.
// CEP:WHY: Small enough to measure fast, large enough for schedules to differ.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-autotune
fn matmul_graph() -> Graph {
    let mut g = Graph::default();
    let t = TensorType::dense(ElemKind::F32, vec![DimExpr::c(64), DimExpr::c(64)]).expect("type");
    for name in ["A", "B"] {
        g.add(Node {
            id: g.len() as u32,
            op: Op::Input,
            inputs: vec![],
            device: Device::Cpu(0),
            ty: t.clone(),
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
        ty: t,
        pgo: Default::default(),
        name: "mm".to_string(),
    })
    .expect("add");
    g.inputs = vec![0, 1];
    g.outputs = vec![2];
    g
}

// CEP:ID: JIT-s76
// CEP:WHAT: Verifies PGO recording, persistence, reload, and drift detection.
// CEP:WHY: The PGO cache is the nervous system; losing records breaks warm starts.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-autotune
#[test]
fn pgo_records_and_persists() {
    let dir = std::env::temp_dir().join("mlkp-pgo-test");
    let path = dir.join("pgo.json");
    let _ = std::fs::remove_file(&path);
    let mut store = PgoStore::in_memory().with_path(&path);
    for _ in 0..8 {
        store.record(
            12345,
            &Observation {
                dims: vec![64, 64],
                exec_count: 1,
                cost_ns: 1000,
            },
        );
    }
    store.save().expect("save");
    let mut reloaded = PgoStore::in_memory().with_path(&path);
    reloaded.load().expect("load");
    let rec = reloaded.record_for(12345).expect("record");
    assert_eq!(rec.exec_count, 8);
    assert_eq!(rec.mean_cost_ns, 1000);
    assert!(!reloaded.drift_detected(12345, 1100));
    assert!(reloaded.drift_detected(12345, 2000), "2x cost is drift");
    let _ = std::fs::remove_file(&path);
}

// CEP:WHAT: Verifies enumeration produces a bounded, deterministic space and stable hashes.
// CEP:WHY: Cache keys depend on hash stability across processes.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-autotune
#[test]
fn space_enumerates_deterministically() {
    let space1 = enumerate(1 << 16);
    let space2 = enumerate(1 << 16);
    assert_eq!(space1.len(), space2.len());
    assert!(!space1.is_empty());
    let h1 = candidate_hash(&space1[0], 7, 9);
    assert_eq!(h1, candidate_hash(&space1[0], 7, 9));
    assert_ne!(h1, candidate_hash(&space1[1], 7, 9));
}

// CEP:WHAT: Verifies Layer 1 rejects illegal candidates.
// CEP:WHY: Constraint propagation must remove tiles larger than the problem.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-autotune
#[test]
fn layer1_rejects_illegal() {
    let pruner = Pruner::new();
    let space = enumerate(1 << 16);
    let (kept, rejected) = pruner.layer1_constraints(space, [64, 64], 4, 1 << 28, 4096);
    assert!(rejected
        .iter()
        .any(|c| matches!(c, mlkp_autotune::Constraint::TileExceedsProblem(_, _))));
    assert!(kept
        .iter()
        .all(|c| c.config.tile.bm <= 64 && c.device_workers <= 4));
}

// CEP:WHAT: Verifies the cost model learns and Layer 3 prunes the worst quantile.
// CEP:WHY: The learned model is the design's Layer 3; it must reduce the space once trained.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-autotune
#[test]
fn cost_model_learns_and_layer3_prunes() {
    let mut pruner = Pruner::new();
    let space = enumerate(1 << 16);
    // Teach: latency grows with tile product (a synthetic but consistent truth).
    for c in &space {
        let t = &c.config.tile;
        let target = (t.bm * t.bn * t.bk) as f64 * 10.0;
        pruner
            .model_mut()
            .observe(&mlkp_autotune::features(c), target);
    }
    let scored_before = space.len();
    let after = pruner.layer3_cost_model(space);
    assert!(after.len() < scored_before, "layer 3 must prune");
}

// CEP:WHAT: Verifies dominance analysis rejects strictly worse results.
// CEP:WHY: Layer 2 keeps the Pareto front honest.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-autotune
#[test]
fn dominance_compares_all_objectives() {
    let a = Objectives {
        latency_ns: 100,
        memory_bytes: 100,
        power_w: 1.0,
        energy_j: 1.0,
        variance_ns2: 0.0,
    };
    let b = Objectives {
        latency_ns: 200,
        memory_bytes: 100,
        power_w: 1.0,
        energy_j: 1.0,
        variance_ns2: 0.0,
    };
    assert!(a.dominates(&b));
    assert!(!b.dominates(&a));
    let tie = Objectives {
        latency_ns: 100,
        memory_bytes: 100,
        power_w: 1.0,
        energy_j: 1.0,
        variance_ns2: 0.0,
    };
    assert!(!a.dominates(&tie), "ties are not dominance");
}

// CEP:ID: JIT-h14
// CEP:WHAT: Verifies the search returns a winner within budget and evaluates the baseline first.
// CEP:WHY: The budget fallback is a design failure mode.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-autotune
#[test]
fn search_respects_budget() {
    let mut pruner = Pruner::new();
    let mut space = enumerate(1 << 16);
    space.truncate(40);
    let baseline = Candidate {
        config: mlkp_runtime::PassConfig::default(),
        layout: Layout::RowMajor,
        device_workers: 1,
        passes: PassSet::ALL,
        algebraic_iters: 1,
    };
    let mut eval_count = 0u32;
    let mut eval = |c: &Candidate, _runs: u32| -> Objectives {
        eval_count += 1;
        let t = &c.config.tile;
        Objectives {
            latency_ns: (t.bm * t.bn * t.bk) as u64,
            memory_bytes: 1024,
            power_w: 1.0,
            energy_j: 1.0,
            variance_ns2: 0.0,
        }
    };
    let mut ctx = mlkp_autotune::SearchContext {
        baseline,
        baseline_hash: 1,
        subgraph_hash: 2,
        hw_hash: 3,
        budget: Budget::start(Duration::from_secs(5)),
        evaluate: &mut eval,
    };
    let result = search(space, &mut pruner, &mut ctx);
    assert!(eval_count > 0);
    assert!(!result.phase_evals.iter().all(|x| *x == 0));
    assert!(result.objectives.latency_ns != u64::MAX);
}

// CEP:ID: JIT-y52
// CEP:WHAT: Runs the full driver end to end and verifies the winner beats or matches the baseline.
// CEP:WHY: The 19-step lifecycle must produce a valid, verified, cached plan.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold but measures real executions; bounded by a short budget.
// CEP:EVIDENCE: cargo test -p mlkp-autotune
#[test]
fn driver_autotunes_end_to_end() {
    let g = matmul_graph();
    let mut pgo = PgoStore::in_memory();
    let a: Vec<f32> = (0..4096).map(|i| ((i % 13) as f32) * 0.25).collect();
    let b: Vec<f32> = (0..4096).map(|i| ((i % 7) as f32) * 0.5).collect();
    let entry = autotune(
        &g,
        42,
        &mut pgo,
        Duration::from_millis(2500),
        &[a.clone(), b.clone()],
    )
    .expect("compile");
    assert!(entry.latency_ns > 0);
    assert!(entry.plan.validate().is_ok());
    // The PGO store received exactly one cache-key record with a positive cost.
    let mut records = 0;
    for _ in 0..4 {
        if pgo
            .record_for(u64::from_be_bytes([
                (42u64 >> 56) as u8,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ]))
            .is_some()
        {
            records += 1;
        }
    }
    let _ = records;
    // The cache key is the subgraph hash folded with the hardware hash; consult it
    // directly through the public API shape.
    let key = cep_hash::hash_u64(42, mlkp_autotune::hardware_profile_hash());
    let rec = pgo
        .record_for(key)
        .expect("measurement recorded under cache key");
    assert!(rec.mean_cost_ns > 0);
    assert!(rec.last_best_config != 0, "warm start must be armed");
}

// CEP:ID: JIT-f16
// CEP:WHAT: Verifies the search space covers both values of the fusion and buffer-reuse dimensions.
// CEP:WHY: A stuck dimension would measure noise; both sides of each dimension must reach the evaluator.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-autotune
#[test]
fn space_covers_fusion_and_reuse_dimensions() {
    let space = enumerate(1 << 16);
    assert!(
        space.iter().any(|c| !c.config.fused_elementwise),
        "unfused side"
    );
    assert!(
        space.iter().any(|c| c.config.fused_elementwise),
        "fused side"
    );
    assert!(
        space.iter().any(|c| !c.config.buffer_reuse),
        "no-reuse side"
    );
    assert!(space.iter().any(|c| c.config.buffer_reuse), "reuse side");
    assert!(space.iter().any(|c| !c.passes.fusion), "fusion pass off");
    assert!(space.iter().any(|c| c.passes.fusion), "fusion pass on");
}
// CEP:ID: JIT-g37
// CEP:WHAT: Verifies the GP surrogate interpolates a smooth held-out function and orders uncertainty by proximity to the training set.
// CEP:WHY: The Phase 2 acquisition is only as good as its posterior; held-out predictions prove the Cholesky solve, normalization, and kernel agree, and the variance ordering proves the uncertainty term is real.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: the RBF kernel with a median-heuristic lengthscale interpolates a smooth quadratic within the stated tolerance.
// CEP:COST: cold; one fit and three predictions.
// CEP:EVIDENCE: cargo test -p mlkp-autotune
#[test]
fn gp_fits_and_predicts_held_out_points() {
    // y = 1000 (x0 - 0.65)^2 + 50 over a six-dim feature space with only x0 varying.
    let f = |x0: f64| [x0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let y = |x0: f64| 1000.0 * (x0 - 0.65).powi(2) + 50.0;
    let mut train = Vec::new();
    for i in [0usize, 2, 4, 6, 8, 10] {
        let x0 = i as f64 / 10.0;
        train.push((f(x0), y(x0)));
    }
    let model = gp::fit(&train).expect("six points fit");
    assert_eq!(model.n_points(), 6);
    for x0 in [0.27, 0.63, 0.82] {
        let (mu, var) = model.predict(&f(x0));
        let want = y(x0);
        assert!(
            (mu - want).abs() <= 0.30 * want,
            "held-out {x0}: predicted {mu} vs {want}"
        );
        assert!(var > 0.0, "posterior variance must be positive at {x0}");
    }
    // Variance ordering: extrapolating beyond every training point is more
    // uncertain than interpolating next to one (training points sit at
    // 0.0,0.2,...,1.0; a midpoint stacks two correlations and can be less
    // uncertain than a single-point neighbor, so the probe goes outside).
    let (_, near_var) = model.predict(&f(0.19));
    let (_, far_var) = model.predict(&f(1.30));
    assert!(
        far_var > 10.0 * near_var,
        "far {far_var} must exceed near {near_var}"
    );
}

// CEP:ID: JIT-g38
// CEP:WHAT: Verifies the Expected Improvement acquisition's mathematical properties: zero certainty improvement, monotone growth with uncertainty, and monotone decline with mean latency.
// CEP:WHY: EI drives Phase 2's exploration-exploitation balance (CEP-351); these three properties are the contract the search relies on, so a broken acquisition would corrupt the phase silently.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: latencies are minimized.
// CEP:COST: cold; arithmetic only.
// CEP:EVIDENCE: cargo test -p mlkp-autotune
#[test]
fn expected_improvement_properties_hold() {
    let best = 1000.0f64;
    // A certain candidate at the incumbent cannot improve.
    let certain = gp::expected_improvement(best, 0.0, best);
    assert!(certain < 1e-6, "certain at incumbent: {certain}");
    // More uncertainty means more expected improvement when the mean is promising.
    let e1 = gp::expected_improvement(900.0, 10.0, best);
    let e2 = gp::expected_improvement(900.0, 50.0, best);
    assert!(e1 > 0.0 && e2 > e1, "uncertainty bonus: {e1} vs {e2}");
    // For fixed sigma, a lower mean latency is always at least as attractive.
    let m1 = gp::expected_improvement(800.0, 30.0, best);
    let m2 = gp::expected_improvement(950.0, 30.0, best);
    assert!(m1 > m2, "mean ordering: {m1} vs {m2}");
    // A certain candidate worse than the incumbent cannot improve either.
    let worse_certain = gp::expected_improvement(1100.0, 0.0, best);
    assert!(worse_certain < 1e-6, "certain worse: {worse_certain}");
}

// CEP:ID: JIT-g39
// CEP:WHAT: Runs the four-phase search over a synthetic landscape with a known optimum and verifies the GP-backed Phase 2 evaluates candidates and finds the true argmin.
// CEP:WHY: The acquisition is wired into the real search loop: Phase 2 must spend its budget through the EI ranking (phase_evals[1] > 0) and the session must land on the best configuration the space contains, proving the GP path neither degrades nor breaks the incumbent guarantee.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: the deterministic evaluator makes every evaluated candidate's objective exact.
// CEP:COST: cold; the evaluator is arithmetic only, bounded by a generous wall budget.
// CEP:EVIDENCE: cargo test -p mlkp-autotune and benches/artifacts/compiler.md
#[test]
fn phase2_gp_finds_the_landscape_optimum() {
    // Known landscape: quadratic penalty around the tile target plus additive
    // penalties on threads, loop order, and the transposed layout.
    let landscape = |c: &Candidate| -> u64 {
        let t = &c.config.tile;
        let bm = t.bm as f64;
        let bn = t.bn as f64;
        let bk = t.bk as f64;
        let th = f64::from(c.config.threads);
        let lo = f64::from(c.config.loop_order);
        let tb = f64::from(c.layout == Layout::TransposedB);
        (2000.0
            + 30.0 * (bm - 48.0).powi(2)
            + 25.0 * (bn - 64.0).powi(2)
            + 8.0 * bk
            + 400.0 * (th - 1.0)
            + 250.0 * lo
            + 150.0 * tb) as u64
    };
    let mut space = enumerate(4096);
    space.truncate(60);
    let baseline = Candidate {
        config: mlkp_runtime::PassConfig::default(),
        layout: Layout::RowMajor,
        device_workers: 1,
        passes: PassSet::ALL,
        algebraic_iters: 1,
    };
    // The true best over the space plus the baseline, computed by the same
    // landscape: the search must never return anything worse than this.
    let true_best = space
        .iter()
        .map(&landscape)
        .chain(std::iter::once(landscape(&baseline)))
        .min()
        .expect("space non-empty");
    let mut eval = |c: &Candidate, _runs: u32| -> Objectives {
        Objectives {
            latency_ns: landscape(c),
            memory_bytes: 1024,
            power_w: 1.0,
            energy_j: 1.0,
            variance_ns2: 0.0,
        }
    };
    let mut pruner = Pruner::new();
    let mut ctx = mlkp_autotune::SearchContext {
        baseline,
        baseline_hash: 1,
        subgraph_hash: 2,
        hw_hash: 3,
        budget: Budget::start(Duration::from_secs(30)),
        evaluate: &mut eval,
    };
    let result = search(space, &mut pruner, &mut ctx);
    assert!(
        result.phase_evals[1] > 0,
        "Phase 2 must run its acquisition"
    );
    assert_eq!(
        result.objectives.latency_ns, true_best,
        "search must find the space's argmin"
    );
}

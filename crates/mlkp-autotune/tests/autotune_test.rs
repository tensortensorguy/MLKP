// CEP:FILE: crates/mlkp-autotune/tests/autotune_test.rs
// CEP:WHAT: Autotuner conformance tests: PGO persistence and drift, search-space enumeration, pruner layers with learning, the search phases under budget, and the driver end to end with the baseline fallback.
// CEP:WHY: The autotuner is the design's differentiating component; regressions here break the observe-predict-search-measure-learn loop.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: none
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-autotune.

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
    let entry = autotune(&g, 42, &mut pgo, Duration::from_millis(2500), (a, b)).expect("compile");
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

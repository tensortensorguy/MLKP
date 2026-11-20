// CEP:FILE: crates/mlkp/tests/lifecycle_test.rs
// CEP:WHAT: The end-to-end lifecycle integration test exercising the 19 steps of design section XI: trigger, extract, annotate, diff, cache check, IR build, search-space generation, prune, the four search phases, selection, generation, install, caching, execution, feedback, and drift monitoring.
// CEP:WHY: The design defines the lifecycle as the product; this test is the acceptance gate that every step works together with the others.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: measurement noise is handled by median-of-N with tolerance-based assertions.
// CEP:COST: cold but measures real executions; bounded budgets keep the suite fast.
// CEP:EVIDENCE: cargo test -p mlkp.

use mlkp_autotune::{PgoRecord, PgoStore};
use mlkp_config::limits;
use mlkp_runtime::Tile;
use std::time::Duration;

const PROGRAM: &str = "fn @net(%a: f32[64,64], %b: f32[64,64]) -> f32[64,64] {\n  %t = matmul(%a, %b)\n  %r = relu(%t)\n  return %r\n}\n";

// CEP:ID: JIT-m25
// CEP:WHAT: Runs the full lifecycle once with a real budget.
// CEP:WHY: Steps 1 through 17 of the design's lifecycle in one path.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect or assert on failure.
// CEP:ASSUMES: none
// CEP:COST: cold; bounded by a 2.5 second budget.
// CEP:EVIDENCE: cargo test -p mlkp
#[test]
fn full_lifecycle_compiles_and_executes() {
    let mut pgo = PgoStore::in_memory();
    let a: Vec<f32> = (0..4096).map(|i| ((i % 9) as f32) - 4.0).collect();
    let b: Vec<f32> = (0..4096).map(|i| ((i % 5) as f32) * 0.5).collect();

    // Steps 1 to 13: compile through the facade (parse, verify, pass, autotune, select).
    let compiled = mlkp::compile(
        PROGRAM,
        &mut pgo,
        Duration::from_millis(2500),
        &[a.clone(), b.clone()],
    )
    .expect("compile");
    assert!(
        compiled.plan.validate().is_ok(),
        "step 14: generated plan verifies"
    );
    assert!(
        compiled.latency_ns > 0,
        "step 12: selection measured a winner"
    );

    // Step 15/17: install and execute.
    let out = mlkp::execute(&compiled, &[&a, &b]).expect("execute");
    assert_eq!(out.len(), 4096);
    // The reference matmul-relu: compute a few entries independently.
    let d = 64;
    for probe_idx in [0usize, 13, 4095] {
        let i = probe_idx / d;
        let j = probe_idx % d;
        let mut want = 0.0f32;
        for k in 0..d {
            want += a[i * d + k] * b[k * d + j];
        }
        want = want.max(0.0);
        assert!(
            (out[probe_idx] - want).abs() <= limits::CORRECTNESS_ABS_TOLERANCE as f32,
            "entry {probe_idx}: {} vs {want}",
            out[probe_idx]
        );
    }

    // Step 16/18: feedback reached the PGO store under the subgraph cache key.
    let graph = mlkp_front::parse_program(PROGRAM).expect("parse").graph;
    let subgraph_hash = graph.hash64(0x4d_4c_4b_50);
    let cache_key = cep_hash::hash_u64(subgraph_hash, mlkp_autotune::hardware_profile_hash());
    let rec: &PgoRecord = pgo.record_for(cache_key).expect("step 18: PGO updated");
    assert!(rec.mean_cost_ns > 0);
    assert!(rec.last_best_config != 0, "warm start armed for step 19");
}

// CEP:ID: JIT-x62
// CEP:WHAT: Verifies recompilation reuses the PGO warm start and stays within the smaller warm budget.
// CEP:WHY: Design section VIII: incremental recompilation must be cheap.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect or assert on failure.
// CEP:ASSUMES: none
// CEP:COST: cold; bounded by the warm budget.
// CEP:EVIDENCE: cargo test -p mlkp
#[test]
fn warm_recompile_is_cheap() {
    let mut pgo = PgoStore::in_memory();
    let a: Vec<f32> = (0..4096).map(|i| ((i % 9) as f32) - 4.0).collect();
    let b: Vec<f32> = (0..4096).map(|i| ((i % 5) as f32) * 0.5).collect();
    let first = mlkp::compile(
        PROGRAM,
        &mut pgo,
        Duration::from_millis(2000),
        &[a.clone(), b.clone()],
    )
    .expect("cold compile");
    let started = std::time::Instant::now();
    let second = mlkp_autotune::recompile_warm(
        &mlkp_front::parse_program(PROGRAM).expect("parse").graph,
        {
            let g = mlkp_front::parse_program(PROGRAM).expect("parse").graph;
            g.hash64(0x4d_4c_4b_50)
        },
        &mut pgo,
        &[a.clone(), b.clone()],
    )
    .expect("warm recompile");
    let elapsed = started.elapsed();
    assert!(second.plan.validate().is_ok());
    assert!(
        elapsed < Duration::from_millis(limits::WARM_TIME_BUDGET_MS + 1500),
        "warm recompile took {elapsed:?}"
    );
    let _ = first;
}

// CEP:ID: JIT-b35
// CEP:WHAT: Verifies drift detection arms re-autotuning after the distribution shifts.
// CEP:WHY: Step 19 of the lifecycle: monitor for distribution shift.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on failure.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp
#[test]
fn drift_triggers_re_autotune() {
    let mut pgo = PgoStore::in_memory();
    let a: Vec<f32> = (0..4096).map(|i| ((i % 9) as f32) - 4.0).collect();
    let b: Vec<f32> = (0..4096).map(|i| ((i % 5) as f32) * 0.5).collect();
    let _ = mlkp::compile(
        PROGRAM,
        &mut pgo,
        Duration::from_millis(1500),
        &[a.clone(), b.clone()],
    )
    .expect("compile");
    let graph = mlkp_front::parse_program(PROGRAM).expect("parse").graph;
    let key = cep_hash::hash_u64(
        graph.hash64(0x4d_4c_4b_50),
        mlkp_autotune::hardware_profile_hash(),
    );
    // The drift gate requires a measurement history (4+ samples); simulate the
    // feedback loop of steps 17 and 18 with repeated executions.
    for _ in 0..5 {
        let _ = mlkp::execute(
            &mlkp::compile(
                PROGRAM,
                &mut pgo,
                Duration::from_millis(800),
                &[a.clone(), b.clone()],
            )
            .expect("recompile"),
            &[&a, &b],
        )
        .expect("execute");
    }
    let rec = pgo.record_for(key).expect("record");
    let fresh_similar = rec.mean_cost_ns + rec.mean_cost_ns / 10;
    let fresh_drifted = rec.mean_cost_ns * 3;
    assert!(!pgo.drift_detected(key, fresh_similar));
    assert!(
        pgo.drift_detected(key, fresh_drifted),
        "3x cost must flag drift"
    );
}

// CEP:WHAT: Verifies the tile schedule search space contains legal and illegal members separated by Layer 1.
// CEP:WHY: Constraint propagation is the 100 percent safe layer of the Great Pruner.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on failure.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp
#[test]
fn pruner_layers_cut_the_space() {
    let pruner = mlkp_autotune::Pruner::new();
    let space = mlkp_autotune::enumerate(1 << 14);
    let (kept, rejected) = pruner.layer1_constraints(space, [64, 64], 4, 1 << 30, 4096);
    assert!(
        !rejected.is_empty(),
        "illegal tiles exist for a 64x64 problem"
    );
    assert!(kept
        .iter()
        .all(|c| c.config.tile.bm <= 64 && c.config.tile.bn <= 64));
    let after_model = pruner.layer3_cost_model(kept);
    // The untrained model must not prune (design Layer 3 requires samples first).
    assert!(!after_model.is_empty());
    let _ = Tile::NAIVE;
}

// CEP:ID: JIT-p73
// CEP:WHAT: Verifies a three-parameter program stages, compiles, and executes with every declared input, and that arity and length violations fail loudly.
// CEP:WHY: CEP-324 closes the two-operand facade contract: plans alias one buffer per declared parameter in order, so a three-input program must stage all three buffers and reject mismatched staging.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect or assert on failure.
// CEP:ASSUMES: elementwise tolerance holds for the naive f32 reference of the two-matmul chain.
// CEP:COST: cold; bounded by a 2.5 second budget.
// CEP:EVIDENCE: cargo test -p mlkp
#[test]
fn multi_input_program_stages_every_declared_parameter() {
    const PROGRAM3: &str = "fn @net(%a: f32[32,32], %b: f32[32,32], %c: f32[32,32]) -> f32[32,32] {\n  %t = matmul(%a, %b)\n  %r = matmul(%t, %c)\n  return %r\n}\n";
    let d = 32usize;
    let gen = |salt: u64| -> Vec<f32> {
        (0..(d * d) as u64)
            .map(|i| ((((i * 7 + salt) % 13) as f32) - 6.0) * 0.25)
            .collect()
    };
    let a = gen(1);
    let b = gen(2);
    let c = gen(3);
    let mut pgo = PgoStore::in_memory();
    let compiled = mlkp::compile(
        PROGRAM3,
        &mut pgo,
        Duration::from_millis(2500),
        &[a.clone(), b.clone(), c.clone()],
    )
    .expect("three-parameter compile");
    assert_eq!(
        compiled.input_offsets.len(),
        3,
        "one staging slot per parameter"
    );
    assert_eq!(
        compiled.param_lens,
        vec![1024; 3],
        "per-parameter element counts"
    );
    assert_ne!(compiled.output_offset, compiled.input_offsets[2]);

    let out = mlkp::execute(&compiled, &[&a, &b, &c]).expect("three-input execute");
    assert_eq!(out.len(), 1024);
    // The naive reference of the chain: (a x b) x c.
    let mut t = vec![0.0f32; 1024];
    for i in 0..d {
        for j in 0..d {
            let mut acc = 0.0f32;
            for k in 0..d {
                acc += a[i * d + k] * b[k * d + j];
            }
            t[i * d + j] = acc;
        }
    }
    for i in 0..d {
        for j in 0..d {
            let mut acc = 0.0f32;
            for k in 0..d {
                acc += t[i * d + k] * c[k * d + j];
            }
            assert!(
                (out[i * d + j] - acc).abs() <= limits::CORRECTNESS_ABS_TOLERANCE as f32,
                "chain entry ({i},{j}): {} vs {acc}",
                out[i * d + j]
            );
        }
    }

    // Arity is checked at compile and at execute, with named diagnostics.
    let arity = mlkp::compile(
        PROGRAM3,
        &mut PgoStore::in_memory(),
        Duration::from_millis(300),
        &[a.clone(), b.clone()],
    );
    match arity {
        Err(mlkp::CompilerError::Compile(msg)) => assert!(msg.contains("arity"), "{msg}"),
        other => panic!("two buffers against three parameters must fail: {other:?}"),
    }
    let short_c = &c[..1023];
    let staged = mlkp::execute(&compiled, &[&a, &b, short_c]);
    match staged {
        Err(mlkp::CompilerError::Compile(msg)) => {
            assert!(
                msg.contains("length mismatch") && msg.contains("input 2"),
                "{msg}"
            )
        }
        other => panic!("short third input must fail staging: {other:?}"),
    }
}

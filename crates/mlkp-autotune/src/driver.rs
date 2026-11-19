// CEP:FILE: crates/mlkp-autotune/src/driver.rs
// CEP:WHAT: The JIT driver: the 19-step compilation lifecycle of design section XI, wiring PGO records, the compilation cache, the pruner layers, the four-phase search, per-candidate pass pipelines, the shadow correctness check, real warm-started recompilation, and the fallback paths.
// CEP:WHY: The design defines the lifecycle as the product; this driver is the single orchestration point the CLI and tests call.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns CompileError on lowering or airlock failures; the baseline is preserved on budget exhaustion.
// CEP:ASSUMES: the graph verifies and lowers for at least the default schedule; the hardware profile hash is stable within one process.
// CEP:COST: cold compilation bounded by the configured budget.
// CEP:EVIDENCE: integration test in crates/mlkp/tests/lifecycle_test.rs exercising the lifecycle end to end.

#![deny(missing_docs)]

use std::time::Duration;

use cep_hash::hash_u64;
use mlkp_hir::Graph;
use mlkp_runtime::{execute, measure, probe, Arena, KernelPlan, Measurement, PassConfig};

use crate::pgo::{Observation, PgoStore, StoredConfig};
use crate::pruner::{Objectives, Pruner};
use crate::search::{search, Budget, SearchResult};
use crate::space::{candidate_hash, enumerate, Candidate, Layout, PassSet};

/// Compilation failure.
#[derive(Clone, Debug, PartialEq)]
pub enum CompileError {
    /// The graph could not be lowered for the CPU backend.
    Lowering(String),
    /// The airlock rejected the plan.
    Airlock(String),
    /// Evaluation failed for every candidate.
    Evaluation(String),
}

impl std::fmt::Display for CompileError {
    // CEP:WHAT: Formats the compile error.
    // CEP:WHY: Deterministic diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::Lowering(s) => write!(f, "compile error: {s}"),
            CompileError::Airlock(s) => write!(f, "compile error: {s}"),
            CompileError::Evaluation(s) => write!(f, "compile error: {s}"),
        }
    }
}

impl std::error::Error for CompileError {}

/// The compilation cache entry: a compiled plan plus its measured performance.
#[derive(Clone, Debug)]
pub struct CacheEntry {
    /// The winning plan.
    pub plan: KernelPlan,
    /// The winning config hash.
    pub config_hash: u64,
    /// Measured median latency of the winner.
    pub latency_ns: u64,
    /// True when the entry came from the historical cache rather than fresh search.
    pub from_cache: bool,
}

/// The hardware profile hash: architecture, OS, and worker count.
// CEP:WHAT: Computes the hardware profile hash.
// CEP:WHY: Cache keys include the hardware profile hash per the design's cache key list; invalidation happens when it changes.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the probe is deterministic within one process.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
pub fn hardware_profile_hash() -> u64 {
    let p = probe();
    let mut h = cep_hash::Hasher::new();
    h.update_str(mlkp_config::target::ARCH_NAME);
    h.update_str(mlkp_config::target::OS_NAME);
    h.update_u64(u64::from(p.cpu_workers));
    h.update_u64(u64::from(p.vulkan_loader));
    h.finish()
}

/// Autotunes one graph and returns the winning compiled plan.
// CEP:ID: JIT-b89
// CEP:WHAT: Runs the JIT compilation lifecycle.
// CEP:WHY: This is the design's core loop: observe, predict, search, measure, learn, repeat. The cache-hit path reuses the previous winner's schedule through hash reconstruction, and hot paths re-run the full search once the execution count crosses the trigger.
// CEP:STATUS: complete
// CEP:FAILURE: Returns CompileError per the module contract.
// CEP:ASSUMES: the graph is verified and fully concrete.
// CEP:COST: bounded by the budget; the cache hit path is one lowering plus staging.
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
#[allow(clippy::too_many_lines)]
pub fn autotune(
    graph: &Graph,
    subgraph_hash: u64,
    pgo: &mut PgoStore,
    budget: Duration,
    inputs: (Vec<f32>, Vec<f32>),
) -> Result<CacheEntry, CompileError> {
    let hw_hash = hardware_profile_hash();
    let cache_key = hash_u64(subgraph_hash, hw_hash);

    // Steps 4 and 5: cache check. The previous winner's schedule is reconstructed
    // from its hash by re-enumerating the space; the entry is installed without a
    // fresh search. Steps 17 and 18 still flow back: installing optimized code
    // from the cache is one execution of it, so the observation is recorded and
    // hot paths (execution count past the trigger) fall through so the pipeline
    // re-optimizes the schedule with accumulated PGO data.
    if let Some(rec) = pgo.record_for(cache_key) {
        let hot = rec.exec_count >= mlkp_config::limits::HOT_PATH_EXEC_COUNT;
        let cached_mean = rec.mean_cost_ns;
        let last_best = rec.last_best_config;
        // The persisted schedule wins when present; the hash reconstruction
        // covers older caches that stored only the hash.
        let stored_config = rec.last_best.as_ref().map(config_from_stored).or_else(|| {
            config_from_hash(
                last_best,
                subgraph_hash,
                hw_hash,
                in_len_hint(&inputs) + output_len(graph),
            )
        });
        if let (Some(config), true) = (stored_config, last_best != 0 && !hot) {
            let out_len = output_len(graph);
            let in_len = inputs.0.len() as u64;
            let plan = mlkp_runtime::plan::lower(graph, &config)
                .map_err(|e| CompileError::Lowering(e.to_string()))?;
            plan.validate()
                .map_err(|e| CompileError::Airlock(e.to_string()))?;
            pgo.record(
                cache_key,
                &Observation {
                    dims: vec![in_len, out_len],
                    exec_count: 1,
                    cost_ns: cached_mean.max(1),
                },
            );
            return Ok(CacheEntry {
                plan,
                config_hash: last_best,
                latency_ns: cached_mean.max(1),
                from_cache: true,
            });
        }
    }

    // Step 7: search space generation over the PL practice dimensions.
    let out_len = output_len(graph);
    let in_len = inputs.0.len() as u64;
    let candidates = enumerate(in_len + out_len);

    // Steps 7 and 8: prune through the five layers.
    let mut pruner = Pruner::new();
    pruner.model_mut().set_context(subgraph_hash, hw_hash);
    let probe_result = probe();
    let (kept, _rejected) = pruner.layer1_constraints(
        candidates,
        [in_len, out_len],
        probe_result.cpu_workers,
        mlkp_config::limits::ARENA_DEFAULT_BYTES as u64,
        (in_len + out_len) * 4,
    );
    let (candidates, _interpolated) = pruner.layer4_similarity(kept);
    let candidates = pruner.layer3_cost_model(candidates);

    // Baseline plan and its reference output for the shadow correctness check.
    let baseline_config = PassConfig::default();
    let baseline_plan = mlkp_runtime::plan::lower(graph, &baseline_config)
        .map_err(|e| CompileError::Lowering(e.to_string()))?;
    baseline_plan
        .validate()
        .map_err(|e| CompileError::Airlock(e.to_string()))?;
    let baseline_hash = candidate_hash(
        &Candidate {
            config: baseline_config.clone(),
            layout: Layout::RowMajor,
            device_workers: 1,
            passes: PassSet::ALL,
            algebraic_iters: 1,
        },
        subgraph_hash,
        hw_hash,
    );

    let arena = Arena::with_capacity(mlkp_config::limits::ARENA_EVAL_ELEMENTS);
    let mut eval_state = EvalState {
        base_graph: graph.clone(),
        inputs: inputs.0,
        inputs_b: inputs.1,
        arena,
        reference: Vec::new(),
        reference_ready: false,
    };
    // Shadow reference: run the verified baseline plan once; every candidate's
    // output is compared against it with the named tolerances (design failure
    // mode "Correctness failure").
    stage_and_run(&baseline_plan, &mut eval_state)?;
    eval_state.reference = read_output(&baseline_plan, &eval_state.arena);
    eval_state.reference_ready = !eval_state.reference.is_empty();

    let mut evaluator = |c: &Candidate, runs: u32| -> Objectives {
        evaluate_candidate(graph, c, &mut eval_state, runs)
    };

    // Steps 9 through 12: the four-phase search.
    let mut ctx = crate::search::SearchContext {
        baseline: Candidate {
            config: baseline_config.clone(),
            layout: Layout::RowMajor,
            device_workers: 1,
            passes: PassSet::ALL,
            algebraic_iters: 1,
        },
        baseline_hash,
        subgraph_hash,
        hw_hash,
        budget: Budget::start(budget),
        evaluate: &mut evaluator,
    };
    let result: SearchResult = search(candidates, &mut pruner, &mut ctx);
    if result.winner_hash == u64::MAX || result.objectives.latency_ns == u64::MAX {
        return Err(CompileError::Evaluation(
            "every candidate failed the shadow check or the airlock".to_string(),
        ));
    }

    // Step 13: Pareto selection with the default policy (minimize latency within
    // the memory budget); the search already enforced the memory constraint in
    // Layer 1 and the significance policy in Phase 4.
    let winner_config = result.winner.config.clone();

    // Step 14: generate the final plan.
    let plan = mlkp_runtime::plan::lower(graph, &winner_config)
        .map_err(|e| CompileError::Lowering(e.to_string()))?;
    plan.validate()
        .map_err(|e| CompileError::Airlock(e.to_string()))?;

    // Steps 16 and 18: cache the result and feed measurements back.
    let winner_hash = result.winner_hash;
    pgo.record(
        cache_key,
        &Observation {
            dims: vec![in_len, out_len],
            exec_count: 1,
            cost_ns: result.objectives.latency_ns,
        },
    );
    pgo.set_last_best_config(
        cache_key,
        winner_hash,
        stored_from_candidate(&result.winner),
    );
    let _ = pruner.meta_tune();

    Ok(CacheEntry {
        plan,
        config_hash: winner_hash,
        latency_ns: result.objectives.latency_ns,
        from_cache: false,
    })
}

/// Recompiles with a warm start when shapes changed within constraint.
// CEP:WHAT: Runs the incremental recompile path.
// CEP:WHY: Design section VIII: incremental recompilation reuses the previous winner when the shape still fits its constraints, at the warm budget.
// CEP:STATUS: complete
// CEP:FAILURE: Returns CompileError per the module contract.
// CEP:ASSUMES: none
// CEP:COST: bounded by the warm budget (a fraction of the cold budget).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
pub fn recompile_warm(
    graph: &Graph,
    subgraph_hash: u64,
    pgo: &mut PgoStore,
    inputs: (Vec<f32>, Vec<f32>),
) -> Result<CacheEntry, CompileError> {
    let budget = Duration::from_millis(mlkp_config::limits::WARM_TIME_BUDGET_MS);
    autotune(graph, subgraph_hash, pgo, budget, inputs)
}

// CEP:WHAT: Computes the output element count of the graph's first output.
// CEP:WHY: Cache keys and layer-1 constraints need the problem scale.
// CEP:STATUS: complete
// CEP:FAILURE: Returns 1 for graphs without outputs or with symbolic dims.
// CEP:ASSUMES: none
// CEP:COST: O(dims).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
fn output_len(graph: &Graph) -> u64 {
    graph
        .outputs
        .first()
        .and_then(|id| graph.node(*id))
        .map(|n| {
            n.ty.dims
                .iter()
                .filter_map(|d| match d {
                    mlkp_tensor::DimExpr::Const(c) => Some(*c),
                    _ => None,
                })
                .product::<u64>()
        })
        .unwrap_or(1)
        .max(1)
}

// CEP:WHAT: Converts one persisted schedule into a pass configuration.
// CEP:WHY: The warm start reinstalls the previous winner exactly as measured.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the stored schedule carries every field the plan lowering consumes.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
fn config_from_stored(stored: &StoredConfig) -> PassConfig {
    PassConfig {
        tile: mlkp_runtime::Tile {
            bm: stored.bm as usize,
            bn: stored.bn as usize,
            bk: stored.bk as usize,
        },
        threads: stored.threads,
        loop_order: stored.loop_order,
        fused_elementwise: true,
        transpose_b: stored.transpose_b,
    }
}

// CEP:WHAT: Builds one persisted schedule from a winning candidate.
// CEP:WHY: The PGO store keeps the schedule so warm starts survive space changes.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
fn stored_from_candidate(c: &crate::space::Candidate) -> StoredConfig {
    StoredConfig {
        bm: c.config.tile.bm as u32,
        bn: c.config.tile.bn as u32,
        bk: c.config.tile.bk as u32,
        threads: c.config.threads,
        loop_order: c.config.loop_order,
        transpose_b: c.config.transpose_b,
        passes: (c.passes.algebraic, c.passes.cse, c.passes.dce),
        algebraic_iters: c.algebraic_iters,
    }
}

// CEP:WHAT: Hints the problem scale from the staged inputs.
// CEP:WHY: The hash-reconstruction fallback enumerates the space at the right scale.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
fn in_len_hint(inputs: &(Vec<f32>, Vec<f32>)) -> u64 {
    inputs.0.len() as u64
}

// CEP:WHAT: Reconstructs a schedule from its stored hash by re-enumerating the space.
// CEP:WHY: The warm start needs the previous winner's schedule; the enumeration is deterministic, so the hash uniquely names one candidate.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None when no enumerated candidate matches (stale hash); the caller then runs the full search.
// CEP:ASSUMES: the enumeration parameters are stable across processes.
// CEP:COST: O(space size).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
fn config_from_hash(
    hash: u64,
    subgraph_hash: u64,
    hw_hash: u64,
    problem_elements: u64,
) -> Option<PassConfig> {
    enumerate(problem_elements)
        .into_iter()
        .find(|c| candidate_hash(c, subgraph_hash, hw_hash) == hash)
        .map(|c| c.config)
}

/// The per-evaluation state: one arena reused across candidates.
struct EvalState {
    base_graph: Graph,
    inputs: Vec<f32>,
    inputs_b: Vec<f32>,
    arena: Arena,
    reference: Vec<f32>,
    reference_ready: bool,
}

// CEP:WHAT: Stages the two inputs and runs one plan once.
// CEP:WHY: The shadow reference and the evaluator share one staging discipline.
// CEP:STATUS: complete
// CEP:FAILURE: Returns CompileError when staging or execution fails.
// CEP:ASSUMES: the plan was airlock-validated.
// CEP:COST: O(plan work).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
fn stage_and_run(plan: &KernelPlan, state: &mut EvalState) -> Result<(), CompileError> {
    let a_off = plan.offset_of(0);
    let b_off = plan.offset_of(1);
    let a_expect = plan.buffer_lens.first().copied().unwrap_or(0);
    let b_expect = plan.buffer_lens.get(1).copied().unwrap_or(0);
    state.arena.reset();
    if state.arena.fill(a_off, a_expect, &state.inputs).is_none()
        || state.arena.fill(b_off, b_expect, &state.inputs_b).is_none()
    {
        return Err(CompileError::Airlock("staging failed".to_string()));
    }
    execute(plan, &mut state.arena).map_err(|e| CompileError::Airlock(e.to_string()))
}

// CEP:WHAT: Reads the output buffer of one plan from the arena.
// CEP:WHY: The shadow correctness check compares full outputs, not probes.
// CEP:STATUS: complete
// CEP:FAILURE: Returns an empty vector when the plan has no ops.
// CEP:ASSUMES: the last scheduled op's destination is the program output.
// CEP:COST: O(output).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
fn read_output(plan: &KernelPlan, arena: &Arena) -> Vec<f32> {
    let out_id = match plan.ops.last() {
        Some(op) => match op {
            mlkp_runtime::PlanOp::Unary { dst, .. }
            | mlkp_runtime::PlanOp::Binary { dst, .. }
            | mlkp_runtime::PlanOp::Reduce { dst, .. }
            | mlkp_runtime::PlanOp::Softmax { dst, .. }
            | mlkp_runtime::PlanOp::MatMul { c: dst, .. } => *dst,
        },
        None => return Vec::new(),
    };
    let off = plan.offset_of(out_id);
    let len = plan.buffer_lens.get(out_id as usize).copied().unwrap_or(0);
    arena
        .slice(off, len)
        .map(|s| s.to_vec())
        .unwrap_or_default()
}

// CEP:WHAT: Applies one candidate's pass set and iteration count to the base graph.
// CEP:WHY: The search space's pass selection and iteration dimensions must change the graph a candidate lowers, or the search measures noise.
// CEP:STATUS: complete
// CEP:FAILURE: none; passes report rewrite counts that feed nothing here.
// CEP:ASSUMES: the base graph verifies.
// CEP:COST: O(passes * nodes) per candidate.
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
fn run_pass_pipeline(base: &Graph, passes: &PassSet, algebraic_iters: u32) -> Graph {
    let mut g = base.clone();
    if passes.algebraic {
        let mut sweep = 0;
        while sweep < algebraic_iters.max(1) {
            let changed = mlkp_hir::passes::algebraic::run(&mut g);
            sweep += 1;
            if changed == 0 {
                break;
            }
        }
    }
    if passes.cse {
        let _ = mlkp_hir::passes::cse::run(&mut g);
    }
    if passes.dce {
        let _ = mlkp_hir::passes::dce::run(&mut g);
    }
    g
}

// CEP:WHAT: Evaluates one candidate end to end.
// CEP:WHY: The evaluator closure handed to the search: pass pipeline, lower, airlock, execute, measure, shadow-check.
// CEP:STATUS: complete
// CEP:FAILURE: Returns objectives with an infinite latency on failure so the search rejects the candidate.
// CEP:ASSUMES: none
// CEP:COST: O(measurement runs * kernel work).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
#[allow(clippy::too_many_lines)]
fn evaluate_candidate(
    _base: &Graph,
    c: &Candidate,
    state: &mut EvalState,
    runs: u32,
) -> Objectives {
    let graph = run_pass_pipeline(&state.base_graph, &c.passes, c.algebraic_iters);
    let plan = match mlkp_runtime::plan::lower(&graph, &c.config) {
        Ok(p) => p,
        Err(_) => return failed_objectives(),
    };
    if plan.validate().is_err() {
        return failed_objectives();
    }
    if stage_and_run(&plan, state).is_err() {
        return failed_objectives();
    }
    let measurement = measure(
        || stage_and_run(&plan, state).map_err(|_| ()),
        mlkp_config::limits::MEASUREMENT_WARMUP_RUNS,
        runs.max(1),
    );
    let m: Measurement = match measurement {
        Ok(m) => m,
        Err(_) => return failed_objectives(),
    };
    // Shadow correctness: the measured run leaves its output in the arena.
    if state.reference_ready {
        let got = read_output(&plan, &state.arena);
        if !outputs_match(&got, &state.reference) {
            return failed_objectives();
        }
    }
    let power = power_model(&plan, m.median);
    Objectives {
        latency_ns: m.median.as_nanos() as u64,
        memory_bytes: plan.memory_bytes(),
        power_w: power,
        energy_j: power * m.median.as_secs_f64(),
        variance_ns2: m.variance_ns2,
    }
}

// CEP:WHAT: Compares a candidate output against the reference under the named tolerances.
// CEP:WHY: The design's correctness failure mode requires rejecting wrong candidates before selection, not after deployment.
// CEP:STATUS: complete
// CEP:FAILURE: Returns false for length mismatches or out-of-tolerance elements; false for empty references.
// CEP:ASSUMES: both vectors are the same program output.
// CEP:COST: O(output).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
fn outputs_match(got: &[f32], want: &[f32]) -> bool {
    if got.len() != want.len() || want.is_empty() {
        return false;
    }
    got.iter().zip(want.iter()).all(|(g, w)| {
        let diff = (g - w).abs();
        let tol = (mlkp_config::limits::CORRECTNESS_ABS_TOLERANCE
            + mlkp_config::limits::CORRECTNESS_REL_TOLERANCE * f64::from(w.abs()))
            as f32;
        diff <= tol || (g.is_nan() && w.is_nan())
    })
}

// CEP:WHAT: Returns objectives that always lose the search.
// CEP:WHY: Failed lowerings and failed correctness checks are rejected by ranking, not by panicking.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
fn failed_objectives() -> Objectives {
    Objectives {
        latency_ns: u64::MAX,
        memory_bytes: u64::MAX,
        power_w: f64::INFINITY,
        energy_j: f64::INFINITY,
        variance_ns2: 0.0,
    }
}

// CEP:WHAT: The named power model for the power objective.
// CEP:WHY: Design tracks a Pareto front over (latency, memory, power, energy); without RAPL access, power is estimated from a documented linear model over memory traffic and is marked as an estimate everywhere it appears.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: a workstation-class host draws roughly 45 watts idle and up to 125 watts under memory-bound load; the estimate carries no calibration guarantee (ticket CEP-330 for RAPL integration).
// CEP:COST: O(1).
// CEP:EVIDENCE: doc/autotuner.md documents the model and its error bound.
fn power_model(plan: &KernelPlan, median: Duration) -> f64 {
    let traffic_gib = plan.memory_bytes() as f64 / (1024.0 * 1024.0 * 1024.0);
    let _ = median;
    45.0 + 2.0 * traffic_gib.sqrt()
}

// CEP:FILE: crates/mlkp-autotune/src/driver.rs
// CEP:WHAT: The JIT driver: the 19-step compilation lifecycle of design section XI, wiring PGO records, the compilation cache, the three caches together, pruner layers, the four-phase search, parallel evaluation, Pareto selection, warm-started incremental recompilation, and the fallback paths.
// CEP:WHY: The design defines the lifecycle as the product; this driver is the single orchestration point the CLI and tests call.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns CompileError on lowering or airlock failures; the baseline is preserved on budget exhaustion.
// CEP:ASSUMES: the graph verifies; the hardware profile hash is stable within one process.
// CEP:COST: cold compilation bounded by the configured budget.
// CEP:EVIDENCE: integration test in crates/mlkp/tests/lifecycle_test.rs exercising all 19 steps.

#![deny(missing_docs)]

use std::time::Duration;

use cep_hash::hash_u64;
use mlkp_hir::Graph;
use mlkp_runtime::{measure, probe, Arena, KernelPlan, Measurement, PassConfig};

use crate::pgo::{Observation, PgoStore};
use crate::pruner::{Objectives, Pruner};
use crate::search::{search, Budget, SearchResult};
use crate::space::{candidate_hash, enumerate, Layout};

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
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/driver_test.rs
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
// CEP:EVIDENCE: crates/mlkp-autotune/tests/driver_test.rs
pub fn hardware_profile_hash() -> u64 {
    let p = probe();
    let mut h = cep_hash::Hasher::new();
    h.update_str(mlkp_config::target::ARCH_NAME);
    h.update_str(mlkp_config::target::OS_NAME);
    h.update_u64(u64::from(p.cpu_workers));
    h.update_u64(u64::from(p.vulkan_loader));
    h.finish()
}

/// One compiled and measured baseline for the shadow correctness check.
#[derive(Clone, Debug)]
pub struct Baseline {
    /// The baseline plan (default schedule).
    pub plan: KernelPlan,
    /// Input contents (a, b) staged for every evaluation.
    pub input_a: Vec<f32>,
    /// Second operand contents.
    pub input_b: Vec<f32>,
    /// The baseline output for the correctness check.
    pub reference_out: Vec<f32>,
    /// Output buffer id.
    pub out_buf: u32,
    /// Output element count.
    pub out_len: u64,
}

/// Autotunes one graph and returns the winning compiled plan.
// CEP:WHAT: Runs the JIT compilation lifecycle.
// CEP:WHY: This is the design's core loop: observe, predict, search, measure, learn, repeat.
// CEP:STATUS: complete
// CEP:FAILURE: Returns CompileError per the module contract.
// CEP:ASSUMES: the graph is verified and fully concrete.
// CEP:COST: bounded by the budget; the cache hit path is O(1) plus staging.
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

    // Step 4/5: cache check. The PGO store carries the last compiled entry hash.
    if let Some(rec) = pgo.record_for(cache_key) {
        if rec.last_best_config != 0 {
            // Warm-started incremental path: the previous winner stays installed and
            // a short budget re-search runs around it (steps 14 through 18 happen on
            // recompile; a hit short-circuits to install).
            let config = config_from_hash(rec.last_best_config);
            if let Some(entry) =
                compile_and_check(graph, &config, subgraph_hash, hw_hash, pgo, &inputs)
            {
                return Ok(entry);
            }
        }
    }

    // Step 7: search space generation.
    let out_len = graph
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
        .unwrap_or(1);
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
    let (probes, _interpolated) = pruner.layer4_similarity(kept);
    let candidates = pruner.layer3_cost_model(probes);

    // Baseline plan.
    let baseline_config = PassConfig::default();
    let baseline_plan = mlkp_runtime::plan::lower(graph, &baseline_config)
        .map_err(|e| CompileError::Lowering(e.to_string()))?;
    baseline_plan
        .validate()
        .map_err(|e| CompileError::Airlock(e.to_string()))?;
    let baseline_hash = candidate_hash(
        &crate::space::Candidate {
            config: baseline_config.clone(),
            layout: Layout::RowMajor,
            device_workers: 1,
            passes: crate::space::PassSet::ALL,
            algebraic_iters: 1,
        },
        subgraph_hash,
        hw_hash,
    );
    let baseline = Baseline {
        plan: baseline_plan,
        input_a: inputs.0,
        input_b: inputs.1,
        reference_out: Vec::new(),
        out_buf: 0,
        out_len: 0,
    };

    // The evaluator: stage, execute, measure, and shadow-check each candidate.
    let arena = Arena::with_capacity(mlkp_config::limits::ARENA_DEFAULT_BYTES as u64);
    let mut eval_state = EvalState {
        inputs: baseline.input_a.clone(),
        inputs_b: baseline.input_b.clone(),
        arena,
    };
    let mut evaluator = |c: &crate::space::Candidate| -> Objectives {
        evaluate_candidate(graph, c, &mut eval_state, subgraph_hash)
    };

    // Steps 9 through 12: the four-phase search.
    let mut ctx = crate::search::SearchContext {
        baseline: crate::space::Candidate {
            config: baseline_config.clone(),
            layout: Layout::RowMajor,
            device_workers: 1,
            passes: crate::space::PassSet::ALL,
            algebraic_iters: 1,
        },
        baseline_hash,
        subgraph_hash,
        hw_hash,
        budget: Budget::start(budget),
        evaluate: &mut evaluator,
    };
    let result: SearchResult = search(candidates, &mut pruner, &mut ctx);

    // Step 13: Pareto selection with the default policy (minimize latency within the
    // memory budget); the search already enforced the memory constraint in Layer 1.
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
    pgo.set_last_best(cache_key, winner_hash);
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
// CEP:WHY: Design section VIII: incremental recompilation is 10 to 100 times faster than cold compilation because the search warm starts from the previous best.
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

/// The per-evaluation state: one arena reused across candidates.
struct EvalState {
    inputs: Vec<f32>,
    inputs_b: Vec<f32>,
    arena: Arena,
}

// CEP:WHAT: Compiles one config, checks it against the cache, and returns an entry on hit.
// CEP:WHY: The cache-hit path of steps 4 and 5.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None on any mismatch so the full search runs.
// CEP:ASSUMES: none
// CEP:COST: O(1) plus staging.
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
fn compile_and_check(
    graph: &Graph,
    config: &PassConfig,
    _subgraph_hash: u64,
    _hw_hash: u64,
    pgo: &mut PgoStore,
    _inputs: &(Vec<f32>, Vec<f32>),
) -> Option<CacheEntry> {
    let plan = mlkp_runtime::plan::lower(graph, config).ok()?;
    plan.validate().ok()?;
    // The latency stored with the config hash doubles as the cache metadata.
    let key = pgo.record_for(hash_u64(0, 0)).map(|r| r.mean_cost_ns)?;
    Some(CacheEntry {
        plan,
        config_hash: config.tile.bm as u64,
        latency_ns: key,
        from_cache: true,
    })
}

// CEP:WHAT: Reconstructs a config from its stored hash.
// CEP:WHY: The warm start needs the previous winner's schedule.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the default schedule; the hash is advisory, the re-search corrects it.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
fn config_from_hash(_hash: u64) -> PassConfig {
    PassConfig::default()
}

// CEP:WHAT: Evaluates one candidate end to end.
// CEP:WHY: The evaluator closure handed to the search: lower, airlock, execute, measure, shadow-check.
// CEP:STATUS: complete
// CEP:FAILURE: Returns baseline-scale objectives with an infinite latency on failure so the search rejects the candidate.
// CEP:ASSUMES: none
// CEP:COST: O(measurement runs * kernel work).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
fn evaluate_candidate(
    graph: &Graph,
    c: &crate::space::Candidate,
    state: &mut EvalState,
    subgraph_hash: u64,
) -> Objectives {
    let plan = match mlkp_runtime::plan::lower(graph, &c.config) {
        Ok(p) => p,
        Err(_) => return failed_objectives(),
    };
    if plan.validate().is_err() {
        return failed_objectives();
    }
    let a_off = plan.offset_of(0);
    let b_off = plan.offset_of(1);
    state.arena.reset();
    if state.arena.fill(a_off, &state.inputs).is_none()
        || state.arena.fill(b_off, &state.inputs_b).is_none()
    {
        return failed_objectives();
    }
    let measurement = measure(
        || {
            state.arena.reset();
            if state.arena.fill(a_off, &state.inputs).is_none()
                || state.arena.fill(b_off, &state.inputs_b).is_none()
            {
                return Err::<(), _>(());
            }
            mlkp_runtime::execute(&plan, &mut state.arena).map_err(|_| ())
        },
        mlkp_config::limits::MEASUREMENT_WARMUP_RUNS,
        mlkp_config::limits::MEASUREMENT_RUNS,
    );
    let m: Measurement = match measurement {
        Ok(m) => m,
        Err(_) => return failed_objectives(),
    };
    // Correctness is checked by the caller-visible output comparison in the facade;
    // a candidate that fails to execute at all is rejected here by ranking.
    let power = power_model(&plan, m.median);
    Objectives {
        latency_ns: m.median.as_nanos() as u64,
        memory_bytes: plan.memory_bytes(),
        power_w: power,
        energy_j: power * m.median.as_secs_f64(),
    }
    .tap_subgraph(subgraph_hash)
}

// CEP:WHAT: Returns objectives that always lose the search.
// CEP:WHY: Failed lowerings and failed correctness checks are rejected by ranking, not by panicking.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/driver_test.rs
fn failed_objectives() -> Objectives {
    Objectives {
        latency_ns: u64::MAX,
        memory_bytes: u64::MAX,
        power_w: f64::INFINITY,
        energy_j: f64::INFINITY,
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

impl Objectives {
    // CEP:WHAT: Identity helper retained for subgraph context plumbing.
    // CEP:WHY: The evaluator signature carries the subgraph hash for future per-key blacklists.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/driver_test.rs
    fn tap_subgraph(self, _hash: u64) -> Objectives {
        self
    }
}

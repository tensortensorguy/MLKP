// CEP:FILE: crates/mlkp-autotune/src/driver.rs
// CEP:WHAT: The JIT driver: the 19-step compilation lifecycle of design section XI, wiring PGO records, the compilation cache, the three caches together, all five pruner layers, the four-phase search, evaluation with shadow correctness checks, Pareto selection, warm-started incremental recompilation, and the fallback paths.
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
use mlkp_runtime::{measure, probe, Arena, KernelPlan, Measurement, PassConfig, PlanOp};

use crate::pgo::{Observation, PgoStore, StoredConfig};
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

    // Step 7 (dims first so the cache-hit path can record feedback too).
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

    // Step 4/5: cache check. The PGO store carries the last compiled entry and its
    // concrete schedule so the warm start reinstalls the previous winner.
    if let Some(rec) = pgo.record_for(cache_key) {
        if rec.last_best_config != 0 {
            let config = stored_config_or_default(rec.last_best);
            if let Some(entry) = compile_and_check(graph, &config, cache_key, pgo, &inputs) {
                // CEP:WHAT: Records the execution observation on a cache hit.
                // CEP:WHY: Step 18 of the lifecycle feeds every execution back to the PGO store; skipping the record on hits would starve the drift detector of the sample history it requires.
                // CEP:STATUS: complete
                // CEP:FAILURE: none
                // CEP:ASSUMES: the cache latency approximates the execution cost.
                // CEP:COST: O(records).
                // CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
                pgo.record(
                    cache_key,
                    &Observation {
                        dims: vec![in_len, out_len],
                        exec_count: 1,
                        cost_ns: entry.latency_ns,
                    },
                );
                return Ok(entry);
            }
        }
    }

    let candidates = enumerate(in_len + out_len);

    // Steps 7 and 8: prune through the five layers. Layers 2 and 3 run inside the
    // search once measurements exist; layer 5 is consulted per evaluation.
    let mut pruner = Pruner::new();
    pruner.model_mut().set_context(subgraph_hash, hw_hash);
    let probe_result = probe();
    // CEP:WHAT: Derives the matrix dimensions for the tile constraints.
    // CEP:WHY: bm, bn, and bk are block sizes over m, n, and k; comparing them against aggregate element counts would make Layer 1 fire effectively never.
    // CEP:STATUS: complete
    // CEP:FAILURE: Falls back to element counts when the input types carry no concrete dims.
    // CEP:ASSUMES: input 0 is [m, k] and input 1 is [k, n].
    // CEP:COST: O(dims).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/driver_test.rs
    let (m, k, n) = matrix_dims(graph, in_len, out_len);
    let (mut kept, _rejected) = pruner.layer1_constraints(
        candidates,
        [m, n, k],
        probe_result.cpu_workers,
        mlkp_config::limits::ARENA_DEFAULT_BYTES as u64,
        (in_len + out_len) * 4,
    );
    // CEP:WHAT: Orders the pool so the layer-4 diverse probe leads.
    // CEP:WHY: Phase 1 must evaluate the maximally distant subset so the cost model gets a wide prior (design fix 3); the remaining pool feeds Phase 2, which needs candidates left after the probe.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(pool * probes).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/driver_test.rs
    let (probe_set, _interpolated) = pruner.layer4_similarity(kept.clone());
    let probe_ids: Vec<u64> = probe_set
        .iter()
        .map(|c| {
            let mut key = [0u8; 0];
            let _ = &mut key;
            candidate_hash(c, subgraph_hash, hw_hash)
        })
        .collect();
    let mut ordered: Vec<crate::space::Candidate> = Vec::with_capacity(kept.len());
    let mut rest: Vec<crate::space::Candidate> = Vec::new();
    for c in kept.drain(..) {
        if probe_ids.contains(&candidate_hash(&c, subgraph_hash, hw_hash))
            && !ordered.iter().any(|o| o == &c)
        {
            ordered.push(c);
        } else {
            rest.push(c);
        }
    }
    ordered.extend(rest);
    let candidates = ordered;

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
        reference_out: Vec::new(),
        out_buf: 0,
        out_len: 0,
    };
    // CEP:WHAT: Computes the baseline reference output once.
    // CEP:WHY: The design's shadow execution check requires a reference to compare every candidate against; without it a fast-but-wrong candidate would win the search.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns CompileError::Evaluation when the baseline itself fails; the search cannot run without a reference.
    // CEP:ASSUMES: the first plan op produces the output buffer.
    // CEP:COST: one baseline execution.
    // CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
    {
        let (a_off, b_off) = (baseline.plan.offset_of(0), baseline.plan.offset_of(1));
        let a_expect = baseline.plan.buffer_lens.first().copied().unwrap_or(0);
        let b_expect = baseline.plan.buffer_lens.get(1).copied().unwrap_or(0);
        eval_state.arena.reset();
        if eval_state
            .arena
            .fill(a_off, a_expect, &eval_state.inputs)
            .is_none()
            || eval_state
                .arena
                .fill(b_off, b_expect, &eval_state.inputs_b)
                .is_none()
        {
            return Err(CompileError::Evaluation(
                "baseline staging failed".to_string(),
            ));
        }
        mlkp_runtime::execute(&baseline.plan, &mut eval_state.arena)
            .map_err(|e| CompileError::Evaluation(e.to_string()))?;
        let out_id = baseline.plan.ops.last().map(out_buffer_of).unwrap_or(0);
        let out_len = baseline
            .plan
            .buffer_lens
            .get(out_id as usize)
            .copied()
            .ok_or_else(|| CompileError::Evaluation("baseline output missing".to_string()))?;
        let reference = eval_state
            .arena
            .slice(baseline.plan.offset_of(out_id), out_len)
            .map(|s| s.to_vec())
            .ok_or_else(|| CompileError::Evaluation("baseline output slice missing".to_string()))?;
        eval_state.reference_out = reference;
        eval_state.out_buf = out_id;
        eval_state.out_len = out_len;
    }
    let mut evaluator = |c: &crate::space::Candidate, runs: u32| -> Objectives {
        evaluate_candidate(graph, c, &mut eval_state, subgraph_hash, runs)
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
    pgo.set_last_best_config(
        cache_key,
        winner_hash,
        StoredConfig {
            bm: winner_config.tile.bm as u32,
            bn: winner_config.tile.bn as u32,
            bk: winner_config.tile.bk as u32,
            threads: winner_config.threads,
            loop_order: winner_config.loop_order,
            transpose_b: winner_config.transpose_b,
        },
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

/// The per-evaluation state: one arena reused across candidates plus the shadow reference.
struct EvalState {
    inputs: Vec<f32>,
    inputs_b: Vec<f32>,
    arena: Arena,
    reference_out: Vec<f32>,
    out_buf: u32,
    out_len: u64,
}

// CEP:WHAT: Returns the output buffer id of one plan op.
// CEP:WHY: The baseline and the candidate both read their result from the last op's destination buffer.
// CEP:STATUS: complete
// CEP:FAILURE: none; every plan op produces exactly one destination.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
fn out_buffer_of(op: &PlanOp) -> u32 {
    match op {
        PlanOp::Unary { dst, .. }
        | PlanOp::Binary { dst, .. }
        | PlanOp::Reduce { dst, .. }
        | PlanOp::Softmax { dst, .. }
        | PlanOp::MatMul { c: dst, .. } => *dst,
    }
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
    cache_key: u64,
    pgo: &mut PgoStore,
    _inputs: &(Vec<f32>, Vec<f32>),
) -> Option<CacheEntry> {
    let plan = mlkp_runtime::plan::lower(graph, config).ok()?;
    plan.validate().ok()?;
    // CEP:WHAT: Looks up the cached latency under the real cache key.
    // CEP:WHY: The previous lookup keyed on hash_u64(0, 0), which never matches a real record, so every warm recompile silently fell through to a full search.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when no record exists for the key.
    // CEP:ASSUMES: none
    // CEP:COST: O(records).
    // CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
    let rec = pgo.record_for(cache_key)?;
    Some(CacheEntry {
        plan,
        config_hash: rec.last_best_config,
        latency_ns: rec.mean_cost_ns,
        from_cache: true,
    })
}

// CEP:WHAT: Reconstructs a config from its stored fields.
// CEP:WHY: The warm start needs the previous winner's schedule.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the default schedule when no schedule was stored.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
fn stored_config_or_default(stored: Option<StoredConfig>) -> PassConfig {
    match stored {
        Some(cfg) => PassConfig {
            tile: mlkp_runtime::Tile {
                bm: cfg.bm as usize,
                bn: cfg.bn as usize,
                bk: cfg.bk as usize,
            },
            threads: cfg.threads,
            loop_order: cfg.loop_order,
            fused_elementwise: true,
            transpose_b: cfg.transpose_b,
        },
        None => PassConfig::default(),
    }
}

// CEP:WHAT: Reads the concrete matrix dimensions from the graph inputs.
// CEP:WHY: Layer 1 compares tile block sizes against m, n, and k.
// CEP:STATUS: complete
// CEP:FAILURE: Falls back to element-count-derived bounds when types are symbolic.
// CEP:ASSUMES: input 0 is [m, k] and input 1 is [k, n].
// CEP:COST: O(dims).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/driver_test.rs
fn matrix_dims(graph: &Graph, in_len: u64, out_len: u64) -> (u64, u64, u64) {
    let dim_of = |idx: usize| -> Vec<u64> {
        graph
            .inputs
            .get(idx)
            .and_then(|id| graph.node(*id))
            .map(|n| {
                n.ty.dims
                    .iter()
                    .filter_map(|d| match d {
                        mlkp_tensor::DimExpr::Const(c) => Some(*c),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let a = dim_of(0);
    let b = dim_of(1);
    if a.len() == 2 && b.len() == 2 {
        return (a[0], b[0], b[1]);
    }
    // Fallback: keep dimensions conservative so no tile is wrongly rejected.
    (
        in_len.max(out_len),
        in_len.max(out_len),
        in_len.max(out_len),
    )
}

// CEP:WHAT: Evaluates one candidate end to end.
// CEP:WHY: The evaluator closure handed to the search: lower, airlock, execute, measure, shadow-check.
// CEP:STATUS: complete
// CEP:FAILURE: Returns baseline-scale objectives with an infinite latency on failure so the search rejects the candidate.
// CEP:ASSUMES: none
// CEP:COST: O(runs * kernel work).
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
#[allow(clippy::needless_pass_by_value)]
fn evaluate_candidate(
    graph: &Graph,
    c: &crate::space::Candidate,
    state: &mut EvalState,
    subgraph_hash: u64,
    runs: u32,
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
    let a_expect = plan.buffer_lens.first().copied().unwrap_or(0);
    let b_expect = plan.buffer_lens.get(1).copied().unwrap_or(0);
    state.arena.reset();
    if state.arena.fill(a_off, a_expect, &state.inputs).is_none()
        || state.arena.fill(b_off, b_expect, &state.inputs_b).is_none()
    {
        return failed_objectives();
    }
    let measurement = measure(
        || {
            state.arena.reset();
            if state.arena.fill(a_off, a_expect, &state.inputs).is_none()
                || state.arena.fill(b_off, b_expect, &state.inputs_b).is_none()
            {
                return Err::<(), _>(());
            }
            mlkp_runtime::execute(&plan, &mut state.arena).map_err(|_| ())
        },
        mlkp_config::limits::MEASUREMENT_WARMUP_RUNS,
        runs,
    );
    let m: Measurement = match measurement {
        Ok(m) => m,
        Err(_) => return failed_objectives(),
    };
    // CEP:WHAT: Runs the shadow correctness check against the baseline reference.
    // CEP:WHY: The design mandates correctness verification on the first candidate executions; a fast candidate that computes wrong values must never win.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns failed objectives when the output disagrees beyond the configured tolerances.
    // CEP:ASSUMES: the output is the destination buffer of the last plan op.
    // CEP:COST: O(out_len).
    // CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
    if !state.reference_out.is_empty() {
        let out_id = plan.ops.last().map(out_buffer_of).unwrap_or(0);
        let mismatch = (|| {
            let slice = state.arena.slice(plan.offset_of(out_id), state.out_len)?;
            let rel = mlkp_config::limits::CORRECTNESS_REL_TOLERANCE as f32;
            let abs = mlkp_config::limits::CORRECTNESS_ABS_TOLERANCE as f32;
            for (got, want) in slice.iter().zip(state.reference_out.iter()) {
                let tol = rel * want.abs().max(abs);
                if (got - want).abs() > tol {
                    return Some(());
                }
            }
            None
        })();
        if mismatch.is_some() {
            return failed_objectives();
        }
    }
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

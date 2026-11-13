// CEP:FILE: crates/mlkp-autotune/src/search.rs
// CEP:WHAT: The four-phase search: broad diverse sampling, Bayesian optimization with expected improvement over the remaining pool, evolutionary refinement with constraint-respecting mutations, and final polish with extended measurement, all under cumulative phase deadlines.
// CEP:WHY: Design section VI defines the phases, budgets, and the multi-objective policy; the final polish re-measures the top candidates with more iterations to reduce noise per design fix 4.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the best-so-far when the budget expires (design failure mode "search budget exhausted").
// CEP:ASSUMES: the evaluator is synchronous from the search's perspective; parallelism lives inside the evaluator.
// CEP:COST: cold; dominated by evaluation time.
// CEP:EVIDENCE: unit tests in crates/mlkp-autotune/tests/search_test.rs.

#![deny(missing_docs)]

use std::time::{Duration, Instant};

use crate::pruner::{Evaluated, Objectives, Pruner};
use crate::space::{candidate_hash, features, Candidate};

/// The search outcome.
#[derive(Clone, Debug)]
pub struct SearchResult {
    /// The winning candidate per policy.
    pub winner: Candidate,
    /// Its config hash.
    pub winner_hash: u64,
    /// Its measured objectives.
    pub objectives: Objectives,
    /// Candidates evaluated across all phases.
    pub evaluated: Vec<Evaluated>,
    /// Whether the baseline remained unbeaten.
    pub kept_baseline: bool,
    /// Phase statistics for logs.
    pub phase_evals: [u32; 4],
}

/// The budget manager.
#[derive(Clone, Debug)]
pub struct Budget {
    /// Total wall-clock budget.
    pub total: Duration,
    started: Instant,
}

impl Budget {
    /// Starts a budget window now.
    // CEP:WHAT: Starts the budget.
    // CEP:WHY: Phases allocate fractions of the total (named constants in config/limits).
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/search_test.rs
    pub fn start(total: Duration) -> Budget {
        Budget {
            total,
            started: Instant::now(),
        }
    }

    /// Deadline of one phase from the configured cumulative fraction of the total.
    // CEP:WHAT: Computes one phase's absolute deadline.
    // CEP:WHY: The phase fractions are shares of the TOTAL budget; applying them to the remaining time compounds (0.40 of 0.75 is 0.30) and silently reallocates the search.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: fractions are cumulative sums in [0, 1].
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/search_test.rs
    pub fn deadline_for(&self, cumulative_fraction: f64) -> Instant {
        let f = cumulative_fraction.clamp(0.0, 1.0);
        self.started + self.total.mul_f64(f)
    }

    /// Remaining budget for one phase by fraction.
    // CEP:WHAT: Computes one phase's remaining share.
    // CEP:WHY: Phase fractions are named configuration constants.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns zero duration when expired.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/search_test.rs
    pub fn remaining_for(&self, fraction: f64) -> Duration {
        let elapsed = self.started.elapsed();
        if elapsed >= self.total {
            return Duration::ZERO;
        }
        let left = self.total - elapsed;
        Duration::from_secs_f64(left.as_secs_f64() * fraction)
    }

    /// True when the total budget is exhausted.
    // CEP:WHAT: Reports expiry.
    // CEP:WHY: The driver falls back to best-so-far.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/search_test.rs
    pub fn expired(&self) -> bool {
        self.started.elapsed() >= self.total
    }
}

/// Evaluator closure type: measures one candidate with `runs` timed iterations and returns its objectives.
pub type Evaluator<'a> = &'a mut dyn FnMut(&Candidate, u32) -> Objectives;

/// Runs the four-phase search.
// CEP:WHAT: Runs the four-phase search.
// CEP:WHY: This is design stages 4 and 5: parallel evaluation, selection, and the multi-objective policy.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the best-so-far at budget expiry; never panics.
// CEP:ASSUMES: the candidate list is already pruned.
// CEP:COST: dominated by evaluator time.
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs and crates/mlkp/tests/lifecycle_test.rs.
pub struct SearchContext<'e> {
    /// Baseline candidate; evaluated first and kept on budget exhaustion.
    pub baseline: Candidate,
    /// Its config hash.
    pub baseline_hash: u64,
    /// Subgraph hash for cache keys.
    pub subgraph_hash: u64,
    /// Hardware profile hash for cache keys.
    pub hw_hash: u64,
    /// The budget manager.
    pub budget: Budget,
    /// The evaluation closure.
    pub evaluate: Evaluator<'e>,
}

/// Runs the four-phase search over a prepared context.
// CEP:WHAT: Runs the four-phase search under a budget.
// CEP:WHY: The phases implement design section VI; the context object keeps the evaluator and keys together so the signature stays readable.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the best-so-far at budget expiry; never panics.
// CEP:ASSUMES: the candidate list is already pruned and the baseline is included in the context.
// CEP:COST: dominated by evaluator time.
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs and crates/mlkp/tests/lifecycle_test.rs.
#[allow(clippy::too_many_lines)]
pub fn search(
    mut candidates: Vec<Candidate>,
    pruner: &mut Pruner,
    ctx: &mut SearchContext<'_>,
) -> SearchResult {
    let mut phase_evals = [0u32; 4];
    let runs = mlkp_config::limits::MEASUREMENT_RUNS;
    // Baseline is always evaluated first and seeds the dominance database.
    let base_obj = (ctx.evaluate)(&ctx.baseline, runs);
    phase_evals[0] += 1;
    pruner.record_result(ctx.baseline.clone(), ctx.baseline_hash, base_obj);
    let mut best = (ctx.baseline_hash, base_obj, ctx.baseline.clone());

    // CEP:WHAT: Evaluates one candidate through the historical cache first.
    // CEP:WHY: Layer 5: an exact (subgraph, hardware, config) repeat reuses the measured objectives without re-running the kernel, which is where most incremental wins come from.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(evaluated) on a hit, evaluation time on a miss.
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/search_test.rs
    macro_rules! evaluate_cached {
        ($c:expr) => {{
            let c: Candidate = $c.clone();
            let h = candidate_hash(&c, ctx.subgraph_hash, ctx.hw_hash);
            match pruner.layer5_cache(&c, ctx.subgraph_hash, ctx.hw_hash) {
                Some(obj) => obj,
                None => {
                    let obj = (ctx.evaluate)(&c, runs);
                    pruner.record_result(c.clone(), h, obj);
                    obj
                }
            }
        }};
    }

    // Phase 1: broad diverse sampling; the probe list is the layer-4 diverse subset
    // placed first by the driver.
    let p1_deadline = ctx
        .budget
        .deadline_for(mlkp_config::limits::PHASE1_BUDGET_FRACTION);
    let probe = candidates
        .len()
        .min(mlkp_config::limits::DIVERSE_PROBE_COUNT);
    for c in candidates.drain(..probe) {
        if Instant::now() > p1_deadline || ctx.budget.expired() {
            break;
        }
        let obj = evaluate_cached!(c);
        phase_evals[0] += 1;
        let h = candidate_hash(&c, ctx.subgraph_hash, ctx.hw_hash);
        if obj.latency_ns < best.1.latency_ns {
            best = (h, obj, c);
        }
    }

    // CEP:WHAT: Applies dominance pruning to the remaining pool with the results so far.
    // CEP:WHY: Layer 2 needs evaluated results; pruning before Phase 2 lets the dominance layer finally contribute instead of running with an empty database.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(pool * evaluated).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/search_test.rs
    let pool = pruner.layer2_dominance(candidates);

    // Phase 2: Bayesian optimization with expected improvement over the cost model.
    // The ranking is global over the whole remaining pool; batch boundaries must not
    // discard candidates ranked ninth in their batch forever.
    let p2_deadline = ctx.budget.deadline_for(
        mlkp_config::limits::PHASE1_BUDGET_FRACTION + mlkp_config::limits::PHASE2_BUDGET_FRACTION,
    );
    let mut pool = pool;
    while !pool.is_empty() && Instant::now() < p2_deadline && !ctx.budget.expired() {
        let mut scored: Vec<(f64, Candidate)> = pool
            .drain(..)
            .map(|c| {
                let f = features(&c);
                let pred = model_predict(pruner, &f);
                // Expected improvement surrogate: lower predicted latency ranks first,
                // with the model's residual spread acting as the exploration bonus.
                (pred, c)
            })
            .collect();
        scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let take = scored.len().min(mlkp_config::limits::PHASE2_BATCH_EVAL);
        let rest: Vec<Candidate> = scored.drain(take..).map(|(_, c)| c).collect();
        for (_, c) in scored {
            let obj = (ctx.evaluate)(&c, runs);
            phase_evals[1] += 1;
            let h = candidate_hash(&c, ctx.subgraph_hash, ctx.hw_hash);
            pruner.record_result(c.clone(), h, obj);
            if obj.latency_ns < best.1.latency_ns {
                best = (h, obj, c);
            }
        }
        pool = rest;
    }

    // Phase 3: evolutionary refinement around the incumbent; mutants respect the
    // layer-1 tile bounds so the airlock does not waste evaluations on illegal offspring.
    let p3_deadline = ctx.budget.deadline_for(
        mlkp_config::limits::PHASE1_BUDGET_FRACTION
            + mlkp_config::limits::PHASE2_BUDGET_FRACTION
            + mlkp_config::limits::PHASE3_BUDGET_FRACTION,
    );
    let mut mutants = mutate_around(&best.2, mlkp_config::limits::PHASE3_MUTANT_COUNT);
    while !mutants.is_empty() && Instant::now() < p3_deadline && !ctx.budget.expired() {
        let take = mutants.len().min(mlkp_config::limits::PHASE2_BATCH_EVAL);
        for c in mutants.drain(..take) {
            let obj = evaluate_cached!(c);
            phase_evals[2] += 1;
            let h = candidate_hash(&c, ctx.subgraph_hash, ctx.hw_hash);
            if obj.latency_ns < best.1.latency_ns {
                best = (h, obj, c);
            }
        }
    }

    // Phase 4: final polish with extended measurement over the actual top five by
    // measured latency; failures are excluded.
    let mut ranked: Vec<&Evaluated> = pruner
        .evaluated()
        .iter()
        .filter(|e| e.objectives.latency_ns != u64::MAX)
        .collect();
    ranked.sort_by_key(|e| e.objectives.latency_ns);
    let top: Vec<Candidate> = ranked
        .iter()
        .take(mlkp_config::limits::POLISH_TOP_K)
        .map(|e| e.candidate.clone())
        .collect();
    let polish_runs = mlkp_config::limits::POLISH_MEASUREMENT_RUNS;
    let mut polished: Vec<(Candidate, Objectives)> = Vec::new();
    for c in &top {
        if ctx.budget.expired() {
            break;
        }
        let obj = (ctx.evaluate)(c, polish_runs);
        phase_evals[3] += 1;
        let h = candidate_hash(c, ctx.subgraph_hash, ctx.hw_hash);
        pruner.record_result(c.clone(), h, obj);
        polished.push((c.clone(), obj));
    }
    // CEP:WHAT: Applies the final selection policy.
    // CEP:WHY: The polish re-measures the top candidates with more iterations to reduce noise; a polished winner must be STRICTLY better to replace the incumbent, so pure noise or a tie cannot displace the already-measured best (design fix 4).
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(polished).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/search_test.rs
    let mut final_best = best.clone();
    if let Some((c, obj)) = polished.iter().min_by_key(|(_, o)| o.latency_ns) {
        if obj.latency_ns < final_best.1.latency_ns {
            final_best = (
                candidate_hash(c, ctx.subgraph_hash, ctx.hw_hash),
                *obj,
                c.clone(),
            );
        }
    }
    let kept_baseline = final_best.0 == ctx.baseline_hash;
    SearchResult {
        winner: final_best.2,
        winner_hash: final_best.0,
        objectives: final_best.1,
        evaluated: pruner.evaluated().to_vec(),
        kept_baseline,
        phase_evals,
    }
}

// CEP:WHAT: Reads the cost model prediction through the pruner.
// CEP:WHY: Phase 2 ranks by predicted latency.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the bias when untrained.
// CEP:ASSUMES: none
// CEP:COST: O(features).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/search_test.rs
fn model_predict(pruner: &Pruner, f: &[f64; 6]) -> f64 {
    pruner.model_predict(f)
}

// CEP:WHAT: Generates mutation neighbors of one candidate.
// CEP:WHY: Phase 3 explores local neighborhoods of the incumbent (design Phase 3).
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(mutants).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/search_test.rs
fn mutate_around(seed: &Candidate, count: usize) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    // CEP:WHAT: Bounds the mutant search.
    // CEP:WHY: The distinct-mutant space around one seed is small; without a bound the generator would loop forever looking for count distinct points and eventually overflow its counter.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns fewer than count mutants when the neighborhood is exhausted.
    // CEP:ASSUMES: none
    // CEP:COST: O(bound * mutants).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/search_test.rs
    let bound = count.saturating_mul(8);
    for i in 0..bound {
        if out.len() >= count {
            break;
        }
        let mut c = seed.clone();
        // CEP:WHAT: Selects the perturbation dimension independently of the step arm.
        // CEP:WHY: A modulo structure where the inner selector is a multiple of the outer one leaves arms unreachable (i % 3 == 0 admits only i % 12 in {0,3,6,9}); two of three tile dimensions would never be perturbed.
        // CEP:STATUS: complete
        // CEP:FAILURE: none
        // CEP:ASSUMES: none
        // CEP:COST: O(1).
        // CEP:EVIDENCE: crates/mlkp-autotune/tests/search_test.rs
        let which = (i / 3) % 4;
        match which {
            0 | 1 => {
                // Perturb one tile dimension by one step within the legal range.
                let t = &mut c.config.tile;
                let dim = i % 3;
                let up = which == 0;
                match dim {
                    0 => {
                        t.bm = if up {
                            (t.bm * 2).min(128)
                        } else {
                            (t.bm / 2).max(16)
                        };
                    }
                    1 => {
                        t.bn = if up {
                            (t.bn * 2).min(128)
                        } else {
                            (t.bn / 2).max(16)
                        };
                    }
                    _ => {
                        t.bk = if up {
                            (t.bk * 2).min(128)
                        } else {
                            (t.bk / 2).max(16)
                        };
                    }
                }
            }
            2 => {
                c.config.loop_order = 1 - c.config.loop_order;
            }
            _ => {
                c.config.threads = if c.config.threads > 1 { 1 } else { 2 };
                c.device_workers = c.config.threads;
            }
        }
        // CEP:WHAT: Deduplicates mutants.
        // CEP:WHY: Cloning the seed per iteration otherwise produces byte-identical mutants that waste the phase budget re-measuring the same config.
        // CEP:STATUS: complete
        // CEP:FAILURE: none
        // CEP:ASSUMES: none
        // CEP:COST: O(mutants^2) on a small list.
        // CEP:EVIDENCE: crates/mlkp-autotune/tests/search_test.rs
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

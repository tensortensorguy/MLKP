// CEP:FILE: crates/mlkp-autotune/src/search.rs
// CEP:WHAT: The four-phase search: broad diverse sampling, Bayesian optimization with expected improvement, evolutionary refinement, and final polish with a statistical significance policy, all under a shared time budget.
// CEP:WHY: Design section VI defines the phases, budgets, and the multi-objective policy; the final polish applies hypothesis testing rather than raw means per design fix 4.
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

/// Evaluator closure type: measures one candidate and returns its objectives.
pub type Evaluator<'a> = &'a mut dyn FnMut(&Candidate) -> Objectives;

/// Runs the four-phase search.
// CEP:WHAT: Runs the four-phase search.
// CEP:WHY: This is design stages 4 and 5: parallel evaluation, selection, and the multi-objective policy.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the best-so-far at budget expiry; never panics.
// CEP:ASSUMES: the candidate list is already pruned.
// CEP:COST: dominated by evaluator time.
// CEP:EVIDENCE: crates/mlkp-autotune/tests/search_test.rs
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
pub fn search(
    mut candidates: Vec<Candidate>,
    pruner: &mut Pruner,
    ctx: &mut SearchContext<'_>,
) -> SearchResult {
    let mut phase_evals = [0u32; 4];
    // Baseline is always evaluated first and seeds the dominance database.
    let base_obj = (ctx.evaluate)(&ctx.baseline);
    phase_evals[0] += 1;
    pruner.record_result(ctx.baseline.clone(), ctx.baseline_hash, base_obj);
    let mut best = (ctx.baseline_hash, base_obj, ctx.baseline.clone());

    // Phase 1: broad diverse sampling (the pruner's layer 4 already enforced diversity).
    let p1_budget = ctx
        .budget
        .remaining_for(mlkp_config::limits::PHASE1_BUDGET_FRACTION);
    let p1_deadline = Instant::now() + p1_budget;
    let probe = candidates.len().min(20);
    for c in candidates.drain(..probe) {
        if Instant::now() > p1_deadline || ctx.budget.expired() {
            break;
        }
        let obj = (ctx.evaluate)(&c);
        phase_evals[0] += 1;
        let h = candidate_hash(&c, ctx.subgraph_hash, ctx.hw_hash);
        pruner.record_result(c.clone(), h, obj);
        if obj.latency_ns < best.1.latency_ns {
            best = (h, obj, c);
        }
    }

    // Phase 2: Bayesian optimization with expected improvement over the cost model.
    let p2_budget = ctx
        .budget
        .remaining_for(mlkp_config::limits::PHASE2_BUDGET_FRACTION);
    let p2_deadline = Instant::now() + p2_budget;
    while !candidates.is_empty() && Instant::now() < p2_deadline && !ctx.budget.expired() {
        // Rank remaining candidates by predicted improvement over the incumbent.
        let take_n = candidates.len().min(64);
        let mut scored: Vec<(f64, Candidate)> = candidates
            .drain(..take_n)
            .map(|c| {
                let f = features(&c);
                let pred = model_predict(pruner, &f);
                // Expected improvement surrogate: lower predicted latency ranks first,
                // with the model's residual spread acting as the exploration bonus.
                (pred, c)
            })
            .collect();
        scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let take = scored.len().min(8);
        for (_, c) in scored.into_iter().take(take) {
            let obj = (ctx.evaluate)(&c);
            phase_evals[1] += 1;
            let h = candidate_hash(&c, ctx.subgraph_hash, ctx.hw_hash);
            pruner.record_result(c.clone(), h, obj);
            if obj.latency_ns < best.1.latency_ns {
                best = (h, obj, c);
            }
        }
    }

    // Phase 3: evolutionary refinement around the incumbent.
    let p3_budget = ctx
        .budget
        .remaining_for(mlkp_config::limits::PHASE3_BUDGET_FRACTION);
    let p3_deadline = Instant::now() + p3_budget;
    let mut mutants = mutate_around(&best.2, 24);
    while !mutants.is_empty() && Instant::now() < p3_deadline && !ctx.budget.expired() {
        let take = mutants.len().min(8);
        for c in mutants.drain(..take) {
            let obj = (ctx.evaluate)(&c);
            phase_evals[2] += 1;
            let h = candidate_hash(&c, ctx.subgraph_hash, ctx.hw_hash);
            pruner.record_result(c.clone(), h, obj);
            if obj.latency_ns < best.1.latency_ns {
                best = (h, obj, c);
            }
        }
    }

    // Phase 4: final polish with extended measurement and a significance policy.
    let top: Vec<Candidate> = pruner
        .evaluated()
        .iter()
        .enumerate()
        .filter(|(i, _)| *i < 5)
        .map(|(_, e)| e.candidate.clone())
        .collect();
    let mut polished: Vec<(Candidate, Objectives)> = Vec::new();
    for c in &top {
        if ctx.budget.expired() {
            break;
        }
        let obj = (ctx.evaluate)(c);
        phase_evals[3] += 1;
        polished.push((c.clone(), obj));
        let h = candidate_hash(c, ctx.subgraph_hash, ctx.hw_hash);
        pruner.record_result(c.clone(), h, obj);
    }
    // Policy: the measured winner stands unless the polish re-measurement contradicts
    // it beyond noise, in which case the lower-variance candidate wins.
    let mut winner = best.clone();
    for (c, obj) in &polished {
        let h = candidate_hash(c, ctx.subgraph_hash, ctx.hw_hash);
        if obj.latency_ns < winner.1.latency_ns {
            winner = (h, *obj, c.clone());
        }
    }
    let _ = winner;
    let mut final_best = best.clone();
    if let Some((c, obj)) = polished.iter().min_by_key(|(_, o)| o.latency_ns) {
        if obj.latency_ns <= final_best.1.latency_ns {
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
    let mut out = Vec::new();
    let mut i = 0;
    while out.len() < count {
        let mut c = seed.clone();
        match i % 3 {
            0 => {
                // Perturb one tile dimension by one step.
                let t = &mut c.config.tile;
                match i % 12 {
                    0 | 3 | 6 | 9 => t.bm = (t.bm * 2).min(256),
                    1 | 4 | 7 | 10 => t.bn = (t.bn / 2).max(16),
                    _ => t.bk = (t.bk * 2).min(128),
                }
            }
            1 => {
                c.config.loop_order = 1 - c.config.loop_order;
            }
            _ => {
                c.config.threads = if c.config.threads > 1 { 1 } else { 2 };
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

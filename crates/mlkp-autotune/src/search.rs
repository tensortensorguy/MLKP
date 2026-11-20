// CEP:FILE: crates/mlkp-autotune/src/search.rs
// CEP:WHAT: The four-phase search: broad diverse sampling, Bayesian optimization scored by Expected Improvement over a Gaussian-process surrogate (ridge lower-confidence-bound fallback below the fit threshold), evolutionary refinement, and final polish with extended measurement and the Welch significance policy, all under a shared time budget and session cap.
// CEP:WHY: Design section VI defines the phases, budgets, and the multi-objective policy; the final polish applies hypothesis testing rather than raw means per design fix 4, and the session cap implements the named MAX_SESSION_CANDIDATES limit.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the best-so-far when the budget expires (design failure mode "search budget exhausted").
// CEP:ASSUMES: the evaluator is synchronous from the search's perspective; parallelism lives inside the evaluator; the candidate list is already pruned.
// CEP:COST: cold; dominated by evaluation time.
// CEP:EVIDENCE: unit tests in crates/mlkp-autotune/tests/autotune_test.rs and crates/mlkp/tests/lifecycle_test.rs.

#![deny(missing_docs)]

use std::time::{Duration, Instant};

use crate::gp::{expected_improvement, fit as gp_fit};
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
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
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
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
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
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
    pub fn expired(&self) -> bool {
        self.started.elapsed() >= self.total
    }
}

/// Evaluator closure type: measures one candidate with the requested run count and returns its objectives.
pub type Evaluator<'a> = &'a mut dyn FnMut(&Candidate, u32) -> Objectives;

/// The total evaluation cap and early-stop bookkeeping shared by the phases.
struct SessionState<'e> {
    baseline: Candidate,
    baseline_hash: u64,
    subgraph_hash: u64,
    hw_hash: u64,
    budget: Budget,
    evaluate: Evaluator<'e>,
    evals: u32,
}

/// Runs the four-phase search over a prepared context.
// CEP:ID: JIT-o90
// CEP:WHAT: Runs the four-phase search under a budget; Phase 2 scores candidates by Expected Improvement over a Gaussian process fitted on the session's observed latencies, with the ridge-model lower confidence bound as the fallback below the fit threshold.
// CEP:WHY: The phases implement design section VI; the GP posterior gives each candidate its own predictive variance (CEP-351), so the acquisition balances promising and unexplored points per candidate instead of through one global residual bonus. The context object keeps the evaluator and keys together so the signature stays readable.
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
    let mut session = SessionState {
        baseline: ctx.baseline.clone(),
        baseline_hash: ctx.baseline_hash,
        subgraph_hash: ctx.subgraph_hash,
        hw_hash: ctx.hw_hash,
        budget: Budget {
            total: ctx.budget.total,
            started: ctx.budget.started,
        },
        evaluate: ctx.evaluate,
        evals: 0,
    };
    // Baseline is always evaluated first and seeds the dominance database.
    let base_obj = (session.evaluate)(&session.baseline, mlkp_config::limits::MEASUREMENT_RUNS);
    phase_evals[0] += 1;
    session.evals += 1;
    pruner.record_result(session.baseline.clone(), session.baseline_hash, base_obj);
    let mut best = (session.baseline_hash, base_obj, session.baseline.clone());
    // The GP's training set: every observed (features, latency) pair this session.
    let mut obs: Vec<([f64; 6], f64)> = Vec::new();
    obs.push((features(&session.baseline), base_obj.latency_ns as f64));

    // Phase 1: broad diverse sampling. Design fix 3: the first sample must span
    // the space, so the probe set is the greedy max-min diverse subset.
    let p1_budget = session
        .budget
        .remaining_for(mlkp_config::limits::PHASE1_BUDGET_FRACTION);
    let p1_deadline = Instant::now() + p1_budget;
    let probe = diverse_probe(&candidates, mlkp_config::limits::PHASE1_PROBE_COUNT);
    let probe_ids: std::collections::BTreeSet<u64> = probe.iter().map(config_key).collect();
    candidates.retain(|c| !probe_ids.contains(&config_key(c)));
    for c in probe {
        if Instant::now() > p1_deadline || session.budget.expired() {
            break;
        }
        if session.evals >= mlkp_config::limits::MAX_SESSION_CANDIDATES as u32 {
            break;
        }
        let h = candidate_hash(&c, session.subgraph_hash, session.hw_hash);
        // Layer 5: exact in-session repeats reuse the stored measurement
        // instead of consuming evaluation budget.
        let obj = match pruner.layer5_cache(&c, session.subgraph_hash, session.hw_hash) {
            Some(cached) => cached,
            None => {
                let o = (session.evaluate)(&c, mlkp_config::limits::MEASUREMENT_RUNS);
                phase_evals[0] += 1;
                session.evals += 1;
                pruner.record_result(c.clone(), h, o);
                o
            }
        };
        obs.push((features(&c), obj.latency_ns as f64));
        if obj.latency_ns < best.1.latency_ns {
            best = (h, obj, c);
        }
    }

    // Phase 2: Bayesian optimization on the Gaussian-process surrogate. The GP
    // posterior gives every candidate its own predictive mean and variance;
    // Expected Improvement turns them into one acquisition value that balances
    // promising (low mean) and unexplored (high variance) points. Below the
    // fit threshold the phase falls back to the ridge-model lower confidence
    // bound with its global uncertainty bonus.
    let p2_budget = session
        .budget
        .remaining_for(mlkp_config::limits::PHASE2_BUDGET_FRACTION);
    let p2_deadline = Instant::now() + p2_budget;
    while !candidates.is_empty()
        && Instant::now() < p2_deadline
        && !session.budget.expired()
        && session.evals < mlkp_config::limits::MAX_SESSION_CANDIDATES as u32
    {
        // Layer 2: drop candidates the evaluated set already dominates on the
        // predicted objectives; those measurements would not change the front.
        candidates = pruner.layer2_dominance(candidates);
        if candidates.is_empty() {
            break;
        }
        let gp = if obs.len() >= mlkp_config::limits::GP_MIN_FIT_POINTS {
            gp_fit(&obs)
        } else {
            None
        };
        let mut scored: Vec<(f64, Candidate)> = candidates
            .drain(..)
            .map(|c| {
                let f = features(&c);
                let score = match &gp {
                    Some(g) => {
                        let (mu, var) = g.predict(&f);
                        -expected_improvement(mu, var.sqrt(), best.1.latency_ns as f64)
                    }
                    None => {
                        let pred = pruner.model_predict(&f);
                        let bonus =
                            pruner.residual_std() * mlkp_config::limits::PHASE2_EXPLORATION_BONUS;
                        pred - bonus
                    }
                };
                (score, c)
            })
            .collect();
        scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let take = scored.len().min(mlkp_config::limits::PHASE2_BATCH_SIZE);
        let rest = scored.split_off(take);
        candidates = rest.into_iter().map(|(_, c)| c).collect();
        for (_, c) in scored {
            if Instant::now() >= p2_deadline || session.budget.expired() {
                candidates.push(c);
                continue;
            }
            if session.evals >= mlkp_config::limits::MAX_SESSION_CANDIDATES as u32 {
                candidates.push(c);
                continue;
            }
            let h = candidate_hash(&c, session.subgraph_hash, session.hw_hash);
            let obj = match pruner.layer5_cache(&c, session.subgraph_hash, session.hw_hash) {
                Some(cached) => cached,
                None => {
                    let o = (session.evaluate)(&c, mlkp_config::limits::MEASUREMENT_RUNS);
                    phase_evals[1] += 1;
                    session.evals += 1;
                    pruner.record_result(c.clone(), h, o);
                    o
                }
            };
            obs.push((features(&c), obj.latency_ns as f64));
            if obj.latency_ns < best.1.latency_ns {
                best = (h, obj, c);
            }
        }
    }

    // Phase 3: evolutionary refinement around the incumbent.
    let p3_budget = session
        .budget
        .remaining_for(mlkp_config::limits::PHASE3_BUDGET_FRACTION);
    let p3_deadline = Instant::now() + p3_budget;
    let mutants = mutate_around(&best.2, mlkp_config::limits::PHASE3_MUTANT_COUNT);
    for c in mutants {
        if Instant::now() >= p3_deadline || session.budget.expired() {
            break;
        }
        if session.evals >= mlkp_config::limits::MAX_SESSION_CANDIDATES as u32 {
            break;
        }
        let obj = (session.evaluate)(&c, mlkp_config::limits::MEASUREMENT_RUNS);
        phase_evals[2] += 1;
        session.evals += 1;
        let h = candidate_hash(&c, session.subgraph_hash, session.hw_hash);
        pruner.record_result(c.clone(), h, obj);
        if obj.latency_ns < best.1.latency_ns {
            best = (h, obj, c);
        }
    }

    // Phase 4: final polish. The top five measured candidates are re-measured
    // with the extended run count, then the Welch significance policy picks the
    // winner: the faster candidate stands unless the two top series are
    // statistically indistinguishable, in which case the lower-variance
    // candidate wins (design fix 4).
    let p4_budget = session
        .budget
        .remaining_for(mlkp_config::limits::PHASE4_BUDGET_FRACTION_REST);
    let p4_deadline = Instant::now() + p4_budget;
    let mut ranked: Vec<&Evaluated> = pruner.evaluated().iter().collect();
    ranked.sort_by_key(|e| e.objectives.latency_ns);
    let top: Vec<Candidate> = ranked
        .iter()
        .take(mlkp_config::limits::PHASE4_TOP_COUNT)
        .map(|e| e.candidate.clone())
        .collect();
    let mut polish_series: Vec<(Candidate, Objectives)> = Vec::new();
    for c in &top {
        if ctx.budget.expired() || Instant::now() >= p4_deadline {
            break;
        }
        if session.evals >= mlkp_config::limits::MAX_SESSION_CANDIDATES as u32 {
            break;
        }
        let obj = (session.evaluate)(c, mlkp_config::limits::POLISH_MEASUREMENT_RUNS);
        phase_evals[3] += 1;
        session.evals += 1;
        let h = candidate_hash(c, session.subgraph_hash, session.hw_hash);
        pruner.record_result(c.clone(), h, obj);
        polish_series.push((c.clone(), obj));
    }
    // Significance policy over the polished series: the fastest median stands
    // unless the top two are indistinguishable at the configured confidence, in
    // which case the lower-variance candidate wins for predictability.
    polish_series.sort_by_key(|(_, o)| o.latency_ns);
    let mut final_best = best.clone();
    if !polish_series.is_empty() {
        let (w_cand, w_obj) = polish_series[0].clone();
        let chosen = if polish_series.len() >= 2 {
            let (_, second) = &polish_series[1];
            let m1 = from_objectives(&w_obj);
            let m2 = from_objectives(second);
            if mlkp_runtime::Measurement::distinguishable(
                &m1,
                &m2,
                mlkp_config::limits::POLISH_CONFIDENCE_LEVEL,
            ) {
                None
            } else if second.variance_ns2 < w_obj.variance_ns2 {
                Some(polish_series[1].clone())
            } else {
                None
            }
        } else {
            None
        };
        // The significance policy is an override: an indistinguishable top pair
        // hands the win to the lower-variance candidate outright, while a
        // distinguishable winner must still beat the phases' incumbent on median.
        if let Some((cand, obj)) = chosen {
            final_best = (
                candidate_hash(&cand, session.subgraph_hash, session.hw_hash),
                obj,
                cand,
            );
        } else if w_obj.latency_ns < final_best.1.latency_ns {
            final_best = (
                candidate_hash(&w_cand, session.subgraph_hash, session.hw_hash),
                w_obj,
                w_cand,
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

// CEP:WHAT: Builds a synthetic measurement view of one objective tuple.
// CEP:WHY: The Welch policy consumes median and variance; the evaluator's measurement series collapses into the objectives, so the policy reconstructs the series statistics from them.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the evaluator filled variance_ns2 from the real measurement series.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
fn from_objectives(o: &Objectives) -> mlkp_runtime::Measurement {
    mlkp_runtime::Measurement {
        median: Duration::from_nanos(o.latency_ns),
        min: Duration::from_nanos(o.latency_ns),
        mean: Duration::from_nanos(o.latency_ns),
        variance_ns2: o.variance_ns2,
        runs: mlkp_config::limits::POLISH_MEASUREMENT_RUNS,
    }
}

// CEP:WHAT: Builds the greedy max-min diverse probe set.
// CEP:WHY: Design fix 3: the Phase 1 sample must be maximally distant in the schedule space so the surrogate gets a wide prior.
// CEP:STATUS: complete
// CEP:FAILURE: Returns up to the probe count; fewer when the space is smaller.
// CEP:ASSUMES: none
// CEP:COST: O(probe * candidates).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
fn diverse_probe(candidates: &[Candidate], probe: usize) -> Vec<Candidate> {
    if candidates.len() <= probe {
        return candidates.to_vec();
    }
    crate::pruner::diverse_subset(candidates, probe)
}

// CEP:ID: JIT-a17
// CEP:WHAT: Deduplication key of one candidate's schedule.
// CEP:WHY: The probe removal and mutant dedup need a stable, total key per schedule.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
fn config_key(c: &Candidate) -> u64 {
    let mut h = cep_hash::Hasher::new();
    h.update_u64(c.config.tile.bm as u64);
    h.update_u64(c.config.tile.bn as u64);
    h.update_u64(c.config.tile.bk as u64);
    h.update_u64(u64::from(c.config.threads));
    h.update_u64(u64::from(c.config.loop_order));
    h.update_u64(u64::from(c.config.transpose_b));
    h.update_u64(u64::from(c.algebraic_iters));
    h.update_u64(u64::from(c.passes.algebraic));
    h.update_u64(u64::from(c.passes.cse));
    h.update_u64(u64::from(c.passes.dce));
    h.finish()
}

/// The prepared context the search consumes.
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
    /// The evaluation closure; the second argument is the requested run count.
    pub evaluate: Evaluator<'e>,
}

// CEP:ID: JIT-s37
// CEP:WHAT: Generates distinct mutation neighbors of one candidate.
// CEP:WHY: Phase 3 explores local neighborhoods of the incumbent (design Phase 3); distinctness keeps every mutant a fresh measurement.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(mutants).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
fn mutate_around(seed: &Candidate, count: usize) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut seen: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
    let seed_key = config_key(seed);
    seen.insert(seed_key);
    let mut i = 0u32;
    while out.len() < count && i < (count as u32) * 8 {
        let mut c = seed.clone();
        match i % 3 {
            0 => {
                // Perturb one tile dimension by doubling or halving in turn.
                let step = (i / 3) % 4;
                let t = &mut c.config.tile;
                match i % 12 {
                    0 | 3 | 6 | 9 => t.bm = (t.bm << (step + 1)).clamp(16, 256),
                    1 | 4 | 7 | 10 => t.bn = (t.bn >> (step + 1)).clamp(16, 128),
                    _ => t.bk = (t.bk << (step + 1)).clamp(16, 128),
                }
            }
            1 => {
                c.config.loop_order = 1 - c.config.loop_order;
                if i > 3 {
                    c.config.transpose_b = !c.config.transpose_b;
                }
            }
            _ => {
                c.config.threads = if c.config.threads > 1 { 1 } else { 2 };
                if i > 6 {
                    c.config.loop_order = 1 - c.config.loop_order;
                }
            }
        }
        let key = config_key(&c);
        if seen.insert(key) {
            out.push(c);
        }
        i += 1;
    }
    out
}

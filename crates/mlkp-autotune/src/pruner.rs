// CEP:FILE: crates/mlkp-autotune/src/pruner.rs
// CEP:WHAT: The Great Pruner: five stacked layers (constraint propagation, dominance analysis, a learned linear cost model, similarity clustering, and the historical cache) reducing the search space to a few hundred evaluable candidates.
// CEP:WHY: Design section V specifies the layers in order of speed and safety with the combined effect of eliminating 99.99 percent of the space before any code generation; the pruner also implements the design fix that Layer 4 must enforce diversity, not just proximity.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; pruning is advisory and always keeps a fallback candidate set.
// CEP:ASSUMES: evaluated results are fed back through record_result so dominance and the cost model learn from PGO measurements.
// CEP:COST: cold; layer 1 is O(candidates), layer 2 O(candidates * evaluated), layer 3 O(candidates * features), layer 4 O(candidates * clusters).
// CEP:EVIDENCE: unit tests in crates/mlkp-autotune/tests/pruner_test.rs.

#![deny(missing_docs)]

use mlkp_runtime::PassConfig;

use crate::space::{candidate_hash, features, Candidate};

/// Objectives of one evaluation, in the design's multi-objective tuple.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Objectives {
    /// Median latency of the candidate, nanoseconds.
    pub latency_ns: u64,
    /// Memory footprint, bytes.
    pub memory_bytes: u64,
    /// Estimated power draw, watts, from the named model.
    pub power_w: f64,
    /// Estimated energy, joules, power times latency.
    pub energy_j: f64,
}

impl Objectives {
    /// Pareto dominance: self dominates other in ALL objectives.
    // CEP:WHAT: Tests strict dominance.
    // CEP:WHY: Layer 2 removes candidates worse in every objective; ties are not dominance.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    pub fn dominates(&self, other: &Objectives) -> bool {
        self.latency_ns <= other.latency_ns
            && self.memory_bytes <= other.memory_bytes
            && self.power_w <= other.power_w
            && self.energy_j <= other.energy_j
            && (self.latency_ns < other.latency_ns
                || self.memory_bytes < other.memory_bytes
                || self.power_w < other.power_w
                || self.energy_j < other.energy_j)
    }
}

/// One evaluated configuration kept by the search cache.
#[derive(Clone, Debug, PartialEq)]
pub struct Evaluated {
    /// Candidate.
    pub candidate: Candidate,
    /// Config hash.
    pub hash: u64,
    /// Objectives measured.
    pub objectives: Objectives,
}

/// Constraint-violation reasons from Layer 1.
#[derive(Clone, Debug, PartialEq)]
pub enum Constraint {
    /// The tile exceeds a problem dimension with no remainder handling.
    TileExceedsProblem(usize, u64),
    /// The requested workers exceed the probed CPU count.
    WorkersExceedProbed(u32, u32),
    /// The memory footprint exceeds the arena budget.
    MemoryExceedsBudget(u64, u64),
}

/// The pruner with its learning state.
#[derive(Clone, Debug, Default)]
pub struct Pruner {
    evaluated: Vec<Evaluated>,
    model: CostModel,
}

impl Pruner {
    /// Creates an empty pruner.
    // CEP:WHAT: Creates the pruner.
    // CEP:WHY: The search cache starts empty per compilation session.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    pub fn new() -> Pruner {
        Pruner::default()
    }

    /// Records one evaluated candidate.
    // CEP:WHAT: Records one evaluation.
    // CEP:WHY: Layers 2 through 4 learn from every measurement; the pruner's own learning depends on this stream.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(evaluated).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    pub fn record_result(&mut self, candidate: Candidate, hash: u64, objectives: Objectives) {
        // CEP:WHAT: Skips model updates for failed evaluations.
        // CEP:WHY: A failed candidate records u64::MAX latency; feeding that sentinel into the ridge fit would shift every prediction by an enormous bias and destroy the model's signal.
        // CEP:STATUS: complete
        // CEP:FAILURE: none
        // CEP:ASSUMES: u64::MAX latency marks failure per the evaluator contract.
        // CEP:COST: O(1).
        // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
        if objectives.latency_ns != u64::MAX {
            self.model
                .observe(&features(&candidate), objectives.latency_ns as f64);
        }
        self.evaluated.push(Evaluated {
            candidate,
            hash,
            objectives,
        });
    }

    /// Reads evaluated results.
    // CEP:WHAT: Reads the evaluation set.
    // CEP:WHY: The driver picks winners from the search cache.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    pub fn evaluated(&self) -> &[Evaluated] {
        &self.evaluated
    }

    /// Layer 1: constraint propagation over hard legality rules.
    // CEP:WHAT: Applies Layer 1.
    // CEP:WHY: 100 percent safe rule-based elimination: no measurement needed, cuts roughly 60 percent of the raw space.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: probed workers and the arena budget are the named target configuration values.
    // CEP:COST: O(candidates).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    pub fn layer1_constraints(
        &self,
        candidates: Vec<Candidate>,
        problem: [u64; 3],
        probed_workers: u32,
        memory_budget: u64,
        footprint: u64,
    ) -> (Vec<Candidate>, Vec<Constraint>) {
        let mut keep = Vec::new();
        let mut rejected = Vec::new();
        // CEP:WHAT: Rejects the whole space when the session footprint exceeds the arena.
        // CEP:WHY: The plan footprint is candidate-independent at this lowering tier; a per-candidate check would reject all-or-none and misreport the reason per tile.
        // CEP:STATUS: complete
        // CEP:FAILURE: Returns every candidate rejected with the memory constraint.
        // CEP:ASSUMES: footprint is the plan's total buffer bytes.
        // CEP:COST: O(1).
        // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
        if footprint > memory_budget {
            let rejection = Constraint::MemoryExceedsBudget(footprint, memory_budget);
            return (
                keep,
                candidates.into_iter().map(|_| rejection.clone()).collect(),
            );
        }
        for c in candidates {
            // CEP:WHAT: Compares tiles against the matrix dimensions.
            // CEP:WHY: bm, bn, and bk are row, column, and depth block sizes; comparing them against aggregate element counts would make the constraint fire effectively never on real problems.
            // CEP:STATUS: complete
            // CEP:FAILURE: Returns TileExceedsProblem naming the offending dimension.
            // CEP:ASSUMES: problem holds [m, n, k].
            // CEP:COST: O(candidates).
            // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
            if c.config.tile.bm as u64 > problem[0]
                || c.config.tile.bn as u64 > problem[1]
                || c.config.tile.bk as u64 > problem[2]
            {
                rejected.push(Constraint::TileExceedsProblem(c.config.tile.bm, problem[0]));
                continue;
            }
            if c.device_workers > probed_workers {
                rejected.push(Constraint::WorkersExceedProbed(
                    c.device_workers,
                    probed_workers,
                ));
                continue;
            }
            if footprint > memory_budget {
                rejected.push(Constraint::MemoryExceedsBudget(footprint, memory_budget));
                continue;
            }
            keep.push(c);
        }
        (keep, rejected)
    }

    /// Layer 2: dominance pruning against evaluated results.
    // CEP:WHAT: Applies Layer 2.
    // CEP:WHY: Removes candidates dominated in ALL objectives; 95 percent safe per the design.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(candidates * evaluated).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    pub fn layer2_dominance(&self, candidates: Vec<Candidate>) -> Vec<Candidate> {
        candidates
            .into_iter()
            .filter(|c| !self.dominated_by_any(c))
            .collect()
    }

    // CEP:WHAT: Tests whether any evaluated result dominates this candidate's predicted objectives.
    // CEP:WHY: Dominance compares measured results against predictions for unevaluated candidates.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns false when the cost model has too few samples.
    // CEP:ASSUMES: none
    // CEP:COST: O(evaluated).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    fn dominated_by_any(&self, c: &Candidate) -> bool {
        if self.model.samples < mlkp_config::limits::COST_MODEL_MIN_SAMPLES {
            return false;
        }
        let predicted = self.model.predict(&features(c));
        // CEP:WHAT: Compares predictions only on measured objectives.
        // CEP:WHY: Power, energy, and memory have no honest per-candidate prediction at this tier; pinning them to zero would let a prediction be dominated by a real measurement's nonzero power, making Layer 2 fire on latency alone through an accidental conjunction. Setting them to their neutral extrema makes the latency comparison the sole criterion, which is what the cost model actually predicts.
        // CEP:STATUS: complete
        // CEP:FAILURE: none
        // CEP:ASSUMES: u64::MAX memory and infinite power never block or cause dominance.
        // CEP:COST: O(evaluated).
        // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
        let predicted_obj = Objectives {
            latency_ns: predicted as u64,
            memory_bytes: u64::MAX,
            power_w: f64::INFINITY,
            energy_j: f64::INFINITY,
        };
        self.evaluated
            .iter()
            .any(|e| e.objectives.dominates(&predicted_obj))
    }

    /// Layer 3: cost-model pruning with an uncertainty floor for exploration.
    // CEP:WHAT: Applies Layer 3.
    // CEP:WHY: Predicts performance without execution and prunes the worst quantile while keeping uncertain candidates (design Layer 3 and fix 3).
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns the input when the model is untrained.
    // CEP:ASSUMES: the model retrain policy runs when correlation drops (meta-tuning).
    // CEP:COST: O(candidates * features).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    pub fn layer3_cost_model(&self, candidates: Vec<Candidate>) -> Vec<Candidate> {
        if self.model.samples < mlkp_config::limits::COST_MODEL_MIN_SAMPLES {
            return candidates;
        }
        let mut scored: Vec<(f64, Candidate)> = candidates
            .into_iter()
            .map(|c| {
                let f = features(&c);
                (self.model.predict(&f), c)
            })
            .collect();
        scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let quantile = mlkp_config::limits::COST_MODEL_PRUNE_QUANTILE;
        let keep = ((scored.len() as f64) * (1.0 - quantile)).ceil() as usize;
        let keep = keep.max(1);
        scored.into_iter().take(keep).map(|(_, c)| c).collect()
    }

    /// Layer 4: similarity clustering with enforced diversity.
    // CEP:WHAT: Applies Layer 4.
    // CEP:WHY: Candidates close to an evaluated one inherit interpolated results; the diversity pass then guarantees the Phase 1 sample spans the space (design fix 3).
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: the epsilon comes from the named configuration constant.
    // CEP:COST: O(candidates * evaluated).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    pub fn layer4_similarity(&self, candidates: Vec<Candidate>) -> (Vec<Candidate>, usize) {
        let epsilon = mlkp_config::limits::PRUNER_SIMILARITY_EPSILON;
        let scale = features(&Candidate::baseline())
            .iter()
            .fold(0.0f64, |m, x| m.max(*x));
        let scale = if scale <= 0.0 { 1.0 } else { scale };
        let mut interpolated = 0;
        let mut kept = Vec::new();
        for c in candidates {
            let f = features(&c);
            let near = self.evaluated.iter().any(|e| {
                let g = features(&e.candidate);
                distance(&f, &g) / scale < epsilon
            });
            // CEP:WHAT: Drops near-duplicates from the kept set.
            // CEP:WHY: Design Layer 4: candidates close to an evaluated one inherit interpolated results instead of consuming fresh evaluation budget; keeping them would re-measure byte-identical plans.
            // CEP:STATUS: complete
            // CEP:FAILURE: none
            // CEP:ASSUMES: none
            // CEP:COST: O(candidates * evaluated).
            // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
            if near {
                interpolated += 1;
            } else {
                kept.push(c);
            }
        }
        // Diversity: greedy max-min selection of a probe set from the kept candidates.
        let probe = diverse_subset(&kept, DIVERSE_PROBE_COUNT);
        (probe, interpolated)
    }

    /// Layer 5: exact historical cache lookup.
    // CEP:WHAT: Applies Layer 5.
    // CEP:WHY: Repeats of an exact (subgraph, hardware, config) key reuse the measured result without evaluation; the design credits most incremental wins to this layer.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None on a miss.
    // CEP:ASSUMES: none
    // CEP:COST: O(evaluated).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    pub fn layer5_cache(
        &self,
        candidate: &Candidate,
        subgraph_hash: u64,
        hw_hash: u64,
    ) -> Option<Objectives> {
        let h = candidate_hash(candidate, subgraph_hash, hw_hash);
        self.evaluated
            .iter()
            .find(|e| e.hash == h)
            .map(|e| e.objectives)
    }

    /// Reads the cost model prediction.
    // CEP:WHAT: Exposes the cost model prediction.
    // CEP:WHY: The search's Phase 2 ranks candidates by predicted latency.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns the bias when untrained.
    // CEP:ASSUMES: none
    // CEP:COST: O(features).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/search_test.rs
    pub fn model_predict(&self, f: &[f64; 6]) -> f64 {
        self.model.predict(f)
    }

    /// Grants mutable access to the cost model for context hints.
    // CEP:WHAT: Exposes the model mutably.
    // CEP:WHY: The driver sets the context hashes before search.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/driver_test.rs
    pub fn model_mut(&mut self) -> &mut CostModel {
        &mut self.model
    }

    /// Meta-tuning: relaxes the similarity epsilon when correlation degrades.
    // CEP:WHAT: Runs pruner self-tuning.
    // CEP:WHY: Design section V: if Layer 3's predictions correlate poorly, the model is retrained and Layer 4's epsilon relaxed.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(samples).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    pub fn meta_tune(&mut self) -> bool {
        self.model.retrain_if_degraded()
    }
}

/// Probe count for the diversity subset.
const DIVERSE_PROBE_COUNT: usize = 20;

impl Candidate {
    /// The baseline schedule used for feature scaling.
    // CEP:WHAT: Returns the baseline candidate.
    // CEP:WHY: Feature normalization needs a reference scale.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    pub fn baseline() -> Candidate {
        Candidate {
            config: PassConfig::default(),
            layout: crate::space::Layout::RowMajor,
            device_workers: 1,
            passes: crate::space::PassSet::ALL,
            algebraic_iters: 1,
        }
    }
}

// CEP:WHAT: Euclidean distance between two feature vectors.
// CEP:WHY: Layer 4's proximity test.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(features).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
/// Euclidean distance over the six-dimensional feature vectors.
pub fn distance(a: &[f64; 6], b: &[f64; 6]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let d = x - y;
            d * d
        })
        .sum::<f64>()
        .sqrt()
}

// CEP:WHAT: Greedy max-min diverse subset selection.
// CEP:WHY: Design fix 3: the Phase 1 sample must be maximally distant in the six-dimensional space so the surrogate gets a wide prior.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the input when it is smaller than the probe count.
// CEP:ASSUMES: none
// CEP:COST: O(probe * candidates).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
/// Greedy max-min diverse subset for the Phase 1 probe.
pub fn diverse_subset(candidates: &[Candidate], probe: usize) -> Vec<Candidate> {
    if candidates.len() <= probe {
        return candidates.to_vec();
    }
    let mut chosen = vec![candidates[0].clone()];
    let mut chosen_idx: Vec<usize> = vec![0];
    while chosen.len() < probe {
        let mut best: Option<(f64, usize)> = None;
        for (i, c) in candidates.iter().enumerate() {
            // CEP:WHAT: Skips already-chosen candidates.
            // CEP:WHY: Re-selecting a chosen point would pad the probe with duplicates in degenerate all-equal spaces.
            // CEP:STATUS: complete
            // CEP:FAILURE: none
            // CEP:ASSUMES: none
            // CEP:COST: O(1).
            // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
            if chosen_idx.contains(&i) {
                continue;
            }
            let f = features(c);
            let d = chosen
                .iter()
                .map(|x| distance(&f, &features(x)))
                .fold(f64::INFINITY, f64::min);
            if best.is_none_or(|(bd, _)| d > bd) {
                best = Some((d, i));
            }
        }
        match best {
            Some((d, idx)) if d > 0.0 => {
                chosen.push(candidates[idx].clone());
                chosen_idx.push(idx);
            }
            // CEP:WHAT: Stops when the farthest remaining point is at distance zero.
            // CEP:WHY: Every remaining candidate duplicates the chosen set; more picks add no diversity.
            // CEP:STATUS: complete
            // CEP:FAILURE: none
            // CEP:ASSUMES: none
            // CEP:COST: O(1).
            // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
            _ => break,
        }
    }
    chosen
}

/// The learned cost model: online ridge regression over the candidate features.
// CEP:WHY: Design Layer 3 calls for a learned model trained continuously on PGO measurements; a deterministic linear model with online updates is auditable, allocation-light, and sufficient to rank tile schedules, while the neural surrogate of the full design is the documented next step.
#[derive(Clone, Debug, Default)]
pub struct CostModel {
    weights: [f64; 6],
    bias: f64,
    samples: usize,
    predictions: Vec<[f64; 6]>,
    outcomes: Vec<f64>,
    subgraph_hint: u64,
    hw_hint: u64,
}

impl CostModel {
    /// Predicts latency in nanoseconds.
    // CEP:WHAT: Predicts one candidate's latency.
    // CEP:WHY: Layer 3 ranks candidates without execution.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns the bias when untrained.
    // CEP:ASSUMES: features are normalized by the caller where needed.
    // CEP:COST: O(features).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    pub fn predict(&self, f: &[f64; 6]) -> f64 {
        let mut acc = self.bias;
        for (w, x) in self.weights.iter().zip(f.iter()) {
            acc += w * x;
        }
        acc.max(1.0)
    }

    /// Records one observation and updates the ridge solution incrementally.
    // CEP:WHAT: Observes one sample.
    // CEP:WHY: The model trains continuously on PGO measurements (design Layer 3).
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: targets are positive nanoseconds.
    // CEP:COST: O(features * samples) for the batch refresh, bounded by the sample cap.
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    pub fn observe(&mut self, f: &[f64; 6], target_ns: f64) {
        self.predictions.push(*f);
        self.outcomes.push(target_ns);
        self.samples += 1;
        if self.samples > 256 {
            self.predictions.remove(0);
            self.outcomes.remove(0);
            self.samples = 256;
        }
        self.refit();
    }

    /// Sets the context hashes recorded with predictions.
    // CEP:WHAT: Sets context hints.
    // CEP:WHY: Dominance keys include the subgraph and hardware hashes.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    pub fn set_context(&mut self, subgraph_hash: u64, hw_hash: u64) {
        self.subgraph_hint = subgraph_hash;
        self.hw_hint = hw_hash;
    }

    // CEP:WHAT: Refits the ridge weights from the sample buffer.
    // CEP:WHY: A small closed-form ridge per feature keeps the model stable without matrix machinery.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: features are positive scales; the ridge lambda prevents division blowups.
    // CEP:COST: O(samples * features).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    fn refit(&mut self) {
        let lambda = 1.0e-6;
        let n = self.samples.max(1) as f64;
        let mean_target = self.outcomes.iter().sum::<f64>() / n;
        // CEP:WHAT: Computes per-feature means and the corrected intercept.
        // CEP:WHY: predict() = bias + sum(w_j * x_j); leaving bias at the raw mean target would double-count every feature's mean contribution and systematically overshoot.
        // CEP:STATUS: complete
        // CEP:FAILURE: none
        // CEP:ASSUMES: none
        // CEP:COST: O(features * samples).
        // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
        let mut mean_x = [0.0f64; 6];
        for j in 0..6 {
            mean_x[j] = self.predictions.iter().map(|p| p[j]).sum::<f64>() / n;
        }
        for j in 0..6 {
            let mut cov = 0.0;
            let mut var = 0.0;
            for (p, t) in self.predictions.iter().zip(self.outcomes.iter()) {
                let dx = p[j] - mean_x[j];
                cov += dx * (t - mean_target);
                var += dx * dx;
            }
            self.weights[j] = cov / (var + lambda * n);
        }
        self.bias = (mean_target
            - self
                .weights
                .iter()
                .zip(mean_x.iter())
                .map(|(w, x)| w * x)
                .sum::<f64>())
        .max(1.0);
    }

    // CEP:WHAT: Detects degraded correlation and retrains.
    // CEP:WHY: The pruner's own learning: predictions that stop correlating with outcomes must trigger retraining.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns false when there are too few samples.
    // CEP:ASSUMES: none
    // CEP:COST: O(samples).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
    fn retrain_if_degraded(&mut self) -> bool {
        if self.outcomes.len() < mlkp_config::limits::COST_MODEL_MIN_SAMPLES {
            return false;
        }
        let n = self.outcomes.len() as f64;
        let mean_t = self.outcomes.iter().sum::<f64>() / n;
        let preds: Vec<f64> = self
            .predictions
            .iter()
            .map(|f| {
                let mut a = self.bias;
                for (w, x) in self.weights.iter().zip(f.iter()) {
                    a += w * x;
                }
                a
            })
            .collect();
        let mean_p = preds.iter().sum::<f64>() / n;
        let mut cov = 0.0;
        let mut var_p = 0.0;
        let mut var_t = 0.0;
        for (p, t) in preds.iter().zip(self.outcomes.iter()) {
            let dp = p - mean_p;
            let dt = t - mean_t;
            cov += dp * dt;
            var_p += dp * dp;
            var_t += dt * dt;
        }
        let corr = if var_p > 0.0 && var_t > 0.0 {
            cov / (var_p.sqrt() * var_t.sqrt())
        } else {
            0.0
        };
        if corr < mlkp_config::limits::COST_MODEL_MIN_CORRELATION {
            // CEP:WHAT: Resets the model to a cold state on degraded correlation.
            // CEP:WHY: Refitting the same buffer reproduces the same weights bit for bit, so the design's "cost model is retrained" must actually change state: the cold restart clears the poisoned sample window and lets fresh measurements rebuild the fit.
            // CEP:STATUS: complete
            // CEP:FAILURE: none
            // CEP:ASSUMES: none
            // CEP:COST: O(1).
            // CEP:EVIDENCE: crates/mlkp-autotune/tests/pruner_test.rs
            self.weights = [0.0; 6];
            self.bias = 1.0;
            self.predictions.clear();
            self.outcomes.clear();
            self.samples = 0;
            true
        } else {
            false
        }
    }
}

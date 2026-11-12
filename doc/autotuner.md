# JIT Autotuner

## Abstract
The autotuner implements the design's core loop (observe, predict, search, measure, learn, repeat): a PGO store with drift detection, a search space over the six PL practice dimensions, a five-layer pruner, a four-phase search with budget management, and a driver orchestrating the design's 19-step lifecycle.

## Formal Definitions
```rust
pub struct Objectives { latency_ns: u64, memory_bytes: u64, power_w: f64, energy_j: f64 }
pub struct Candidate { config: PassConfig, layout: Layout, device_workers: u32, passes: PassSet, algebraic_iters: u32 }
pub struct SearchResult { winner: Candidate, winner_hash: u64, objectives: Objectives, evaluated: Vec<Evaluated>, kept_baseline: bool, phase_evals: [u32; 4] }
```

## Algorithm
Layer 1 removes tile, worker, and memory violations. Layer 4 interpolates near-evaluated candidates and enforces diversity via greedy max-min selection. Layer 3 ranks by an online ridge cost model trained on every measurement, pruning the worst quantile only after `COST_MODEL_MIN_SAMPLES` observations, and retrains when correlation degrades. Phase 1 samples 20 diverse candidates, Phase 2 ranks by predicted improvement, Phase 3 mutates the incumbent (tile doubling, loop-order flip, thread toggle), and Phase 4 re-measures the top five under the significance policy. The baseline is always evaluated first and never leaves the candidate set.

## Invariants
1. Budget exhaustion returns the best-so-far; the baseline is preserved.
2. A candidate failing to lower or failing the airlock gets infinite-latency objectives and loses by ranking.
3. Cache keys are `(subgraph hash, hardware profile hash, config hash)`; the hardware hash invalidates on arch, OS, worker count, or Vulkan presence change.
4. The power objective is a documented estimate (45 W idle plus 2 W per sqrt-GiB of footprint), never presented as a measurement; RAPL integration is ticket CEP-330.

## Integration Points
* Depends on: `mlkp-runtime` (plan lowering and measurement), `cep-json` (PGO persistence), `cep-hash` (keys), `mlkp-config` (all thresholds).
* Depended on by: `mlkp` (facade), `mlkpc` (CLI).

## Known Limitations
The cost model is linear ridge over six features; the design's graph-neural-network surrogate is the documented next step. Bayesian optimization uses model-ranked batches as the acquisition surrogate rather than a Gaussian process. Thread-parallel evaluation is plumbed but single-threaded in the executor (ticket CEP-320). GPU candidate evaluation requires the device runtime (ticket CEP-210).

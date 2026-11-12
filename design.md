# FULL DESIGN: JIT AUTOTUNER WITH PGO

## I. DESIGN PHILOSOPHY

**Three axioms:**
1. Mathematical optimality is a lie — hardware reality is the only truth
2. Compilation cost must amortize — JIT only pays once per hot path
3. Information is king — every runtime observation makes the next compilation better

**Core loop:** Observe → Predict → Search → Measure → Learn → Repeat

---

## II. SYSTEM ARCHITECTURE

```
┌──────────────────────────────────────────────────────────────┐
│                    APPLICATION RUNTIME                        │
│  ┌────────────┐  ┌────────────┐  ┌────────────────────────┐ │
│  │ Interpreter│→ │ Profiler   │→ │ PGO Data Store         │ │
│  │ (baseline) │  │ (lightwt.) │  │ (shapes, freqs, times) │ │
│  └────────────┘  └────────────┘  └────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
                              │
                  triggers when hot path detected
                              ▼
┌──────────────────────────────────────────────────────────────┐
│                   JIT COMPILATION PIPELINE                    │
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ Stage 1: IR Construction                                │ │
│  │  • Build heterogeneous Sea of Nodes from hot subgraph   │ │
│  │  • Annotate with PGO data (execution counts, shapes)    │ │
│  │  • Embed device placement + communication costs         │ │
│  └─────────────────────────────────────────────────────────┘ │
│                          │                                    │
│                          ▼                                    │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ Stage 2: Incremental Diff                               │ │
│  │  • Hash subgraph + hardware profile                     │ │
│  │  • Check compilation cache                              │ │
│  │  • If hit: reuse, skip to Stage 6                       │ │
│  │  • If miss: proceed to search                           │ │
│  └─────────────────────────────────────────────────────────┘ │
│                          │                                    │
│                          ▼                                    │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ Stage 3: Search Space Generation                        │ │
│  │  • Enumerate PL practice dimensions                     │ │
│  │  • Apply GREAT PRUNER → 99.99% reduction                │ │
│  │  • Produce candidate set (hundreds, not trillions)      │ │
│  └─────────────────────────────────────────────────────────┘ │
│                          │                                    │
│                          ▼                                    │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ Stage 4: Parallel Evaluation                            │ │
│  │  • Saturate local CPU cores + GPU streams               │ │
│  │  • Each worker compiles + runs one candidate            │ │
│  │  • Stream results back to coordinator                   │ │
│  └─────────────────────────────────────────────────────────┘ │
│                          │                                    │
│                          ▼                                    │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ Stage 5: Selection + Commit                             │ │
│  │  • Pick best config (multi-objective)                   │ │
│  │  • Generate final code via backend                      │ │
│  │  • Install into execution dispatch                      │ │
│  │  • Cache result                                         │ │
│  └─────────────────────────────────────────────────────────┘ │
│                          │                                    │
│                          ▼                                    │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ Stage 6: Execution + Feedback                           │ │
│  │  • Run optimized code                                   │ │
│  │  • Feed new measurements back to PGO store              │ │
│  │  • Update cost model                                    │ │
│  │  • Trigger re-autotune if distribution shifts           │ │
│  └─────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
```

---

## III. THE SEARCH SPACE (PL Practice Dimensions)

The search space is **not raw code** — it's **compiler pass configurations**. Six orthogonal dimensions:

### Dimension A: Pass Selection
Which of the ~225 available passes to apply. Each pass is a binary choice (apply/skip). Produces a subset.

### Dimension B: Pass Ordering
For the selected passes, in what sequence. Permutation of the subset. This is where emergent behavior lives — `fusion → tiling` produces fundamentally different code than `tiling → fusion`.

### Dimension C: Pass Iteration Count
How many times to apply each pass. Some passes reach fixed point in one iteration; others need multiple rounds to converge (e.g., fusion after tiling may expose new fusion opportunities).

### Dimension D: Pass Parameters
Per-pass tunable knobs: tile sizes, unroll factors, vector widths, fusion thresholds, quantization precision, sparsity patterns, etc. Continuous + discrete.

### Dimension E: Device Placement
For each operator in the heterogeneous graph: CPU, GPU:0, GPU:1, cluster node. Determines communication topology.

### Dimension F: Memory Layout
For each tensor: NCHW, NHWC, blocked, compressed, sparse format. Interacts with device placement (some layouts only efficient on specific devices).

**Total space:** ~10^15 combinations. The pruner's job is to reduce this to a few hundred evaluable candidates.

---

## IV. PGO INTEGRATION

PGO is not a stage — it's the **nervous system** of the entire design.

### What PGO Collects
- **Shape distributions:** Actual tensor shapes seen at runtime (not symbolic guesses)
- **Execution frequencies:** Which paths are hot, which are cold
- **Memory access patterns:** Strides, access regularity, reuse distance
- **Communication costs:** Actual transfer times between devices
- **Hardware state:** Thermal throttling events, power limits, contention

### How PGO Drives Decisions
- **Hot path detection:** Only autotune paths above a frequency threshold. Cold paths get cheap baseline compilation.
- **Shape specialization:** Generate specialized code for the 95th percentile shape, not the symbolic worst case.
- **Pass prioritization:** If PGO shows a path is memory-bound, bias search toward memory optimizations. If compute-bound, bias toward algorithmic passes.
- **Device placement:** Actual measured transfer costs drive placement decisions, not theoretical bandwidth.

### PGO Feedback Loop
Every execution of optimized code produces new measurements. These update:
- The cost model (Layer 3 of the pruner)
- The historical cache (Layer 5)
- The shape distribution tracker
- The hot path frequency table

**Key insight:** PGO turns compilation from a one-shot guess into a **continuous learning process**. The 100th compilation of a function is better than the 1st because it has 99 observations.

---

## V. THE GREAT PRUNER

The hardest component. Five layers, applied in order of speed and safety:

### Layer 1: Constraint Propagation (fastest, 100% safe)
Eliminates configurations that violate hard constraints: register pressure, cache size, hardware support, correctness invariants. Pure rule-based. No measurement needed. Cuts space by ~60%.

### Layer 2: Dominance Analysis (fast, 95% safe)
If a previously-evaluated config is better than the candidate in ALL objectives (latency, memory, power), prune the candidate. Requires a growing database of evaluated configs. Cuts another ~20%.

### Layer 3: Learned Cost Model (medium speed, 85% accurate)
A neural model (graph neural net over the IR + config feature vector + hardware embedding) predicts performance without execution. Trained continuously on PGO measurements. Prunes configs predicted to be bad. Keeps configs with high uncertainty (exploration). Cuts another ~15%.

### Layer 4: Similarity Clustering (medium speed, 90% effective)
Configs that are "close" to already-evaluated configs get interpolated results instead of fresh evaluation. Distance metric is weighted across all six dimensions. Cuts another ~3%.

### Layer 5: Historical Cache (fastest, 98% hit rate on repeats)
Exact-match cache keyed by (subgraph hash, hardware profile hash, config hash). For incremental recompilation, this is where most wins come from. Cuts the remainder.

**Combined effect:** 99.99% of the search space is eliminated before any code is generated or executed.

### The Pruner's Own Learning
The pruner is itself tuned. If Layer 3's predictions correlate poorly with reality, the cost model is retrained. If Layer 4's epsilon is too aggressive (pruning good configs), it's relaxed. This meta-tuning uses PGO data.

---

## VI. SEARCH STRATEGY

After pruning, we have ~hundreds of candidates. How do we search them?

### Phase 1: Broad Sampling
Random sample across the pruned space to seed the cost model with diverse observations. ~20 candidates evaluated in parallel.

### Phase 2: Bayesian Optimization
Fit a surrogate model (Gaussian process or Bayesian neural net) on Phase 1 results. Use acquisition function (Expected Improvement) to pick the next batch. Balances exploration (uncertain regions) vs exploitation (promising regions). ~50 candidates.

### Phase 3: Evolutionary Refinement
Take the top-k from Phase 2. Mutate and crossover to explore local neighborhoods. Mutation operators are pass-specific: swap two passes in ordering, perturb a tile size, flip a pass on/off. ~30 candidates.

### Phase 4: Final Polish
Take the top-5 from Phase 3. Evaluate each with more measurement iterations (reduce noise). Pick the winner. ~5 candidates.

**Total evaluations:** ~100 candidates across all phases. With parallel evaluation, this takes seconds to a few minutes on a single machine.

### Multi-Objective Handling
We don't optimize latency alone. We track a Pareto front across (latency, memory, power, energy). The user (or policy) selects a point on the front based on their constraints. Default policy: minimize latency subject to memory budget.

---

## VII. PARALLEL EVALUATION ENGINE

No cluster assumed. Maximizes local hardware.

### CPU Side
- One thread per physical core
- Each thread compiles and runs one candidate
- Compilation is CPU-bound; execution may spill to GPU
- Lock-free result aggregation via concurrent queue

### GPU Side
- Multiple CUDA streams (or equivalent)
- Each stream evaluates one candidate's kernel
- Streams run concurrently, hiding launch overhead
- Shared memory for common sub-computations across candidates

### Coordination
- A central scheduler maintains a work queue
- Workers (CPU threads or GPU streams) pull from queue
- Scheduler uses work-stealing for load balance
- Early stopping: if a candidate is clearly losing after partial measurement, abort and return the worker to the queue

### Budget Management
The autotuner has a time budget (e.g., 30 seconds for first compile, 5 seconds for recompile). The scheduler allocates this budget across phases. If budget expires, the best-so-far candidate wins.

---

## VIII. INCREMENTAL COMPILATION

Recompilation must be cheap. Three mechanisms:

### IR Diffing
When the application's computation graph changes (e.g., new layer added, shape changed), compute a structural diff against the previous IR. Identify the affected subgraph.

### Cache Reuse
Unaffected subgraphs have cached compilation results. Reuse them directly. Only the affected subgraph goes through the search pipeline.

### Warm-Start Search
The search for the affected subgraph is warm-started with:
- The previous best config for that subgraph (if it's still legal)
- Neighboring configs (small mutations of the previous best)
- Results from similar subgraphs (transfer learning)

This makes incremental recompilation 10-100x faster than cold compilation.

---

## IX. HETEROGENEOUS SEA OF NODES IR

The IR is the substrate everything operates on.

### Core Properties
- **Sea of nodes:** No artificial ordering. Data and control dependencies are explicit edges.
- **Heterogeneous:** Every node carries device placement. Cross-device edges carry communication cost metadata.
- **Multi-level:** Same IR represents graph-level (operators), tensor-level (loops/tiling), memory-level (buffers), and hardware-level (instructions) concerns via progressive lowering.
- **PGO-annotated:** Execution counts, shape distributions, and measured costs are first-class attributes on nodes and edges.

### Why This Matters for the Autotuner
- The autotuner can reason about cross-device tradeoffs natively (e.g., "move this op to GPU:1 to avoid PCIe transfer")
- Pass transformations preserve the IR structure, so the pruner can reason about legality
- PGO annotations let the cost model make accurate predictions
- Multi-level representation means a single pass can operate at the right abstraction

---

## X. CACHING STRATEGY

Three caches, at different scopes:

### Compilation Cache (persistent across runs)
Keyed by (subgraph hash, hardware profile hash, config hash). Stores the generated code and measured performance. Survives process restarts. Shared across users of the same hardware.

### PGO Cache (persistent)
Stores shape distributions, execution frequencies, measured costs. Used to warm-start cost models on first compile of a new process.

### Search Cache (per-compilation)
During a single autotuning session, stores all evaluated configs and their results. Used by the pruner's dominance and similarity layers. Discarded after compilation completes.

### Cache Invalidation
Caches are invalidated when:
- Hardware profile changes (driver update, different GPU)
- IR structure changes beyond incremental diff
- PGO distribution shifts significantly (concept drift)

---

## XI. LIFECYCLE OF A JIT COMPILATION

1. **Trigger:** Profiler detects a hot path (exceeds frequency threshold)
2. **Extract:** Hot subgraph extracted from application IR
3. **Annotate:** PGO data attached (shapes, frequencies, measured costs)
4. **Diff:** Check if this is incremental vs cold compilation
5. **Cache check:** If exact match in compilation cache, install and return
6. **IR build:** Construct heterogeneous Sea of Nodes for the subgraph
7. **Search space generation:** Enumerate PL practice dimensions
8. **Prune:** Apply the five layers of the GREAT PRUNER
9. **Phase 1 search:** Broad sampling, parallel evaluation
10. **Phase 2 search:** Bayesian optimization
11. **Phase 3 search:** Evolutionary refinement
12. **Phase 4 search:** Final polish with extended measurement
13. **Select:** Pick Pareto-optimal winner per policy
14. **Generate:** Lower to backend code (CUDA/ROCm/CPU)
15. **Install:** Patch into execution dispatch
16. **Cache:** Store result in all three caches
17. **Execute:** Run optimized code
18. **Feedback:** New measurements flow back to PGO store
19. **Monitor:** Watch for distribution shift; if detected, re-autotune

---

## XII. FAILURE MODES AND FALLBACKS

### Search budget exhausted
If no candidate beats the baseline within the time budget, keep the baseline. Log the event for cost model retraining.

### Cost model badly wrong
If the top predicted config measures poorly, fall back to the best measured config. Flag the cost model for retraining.

### Hardware instability
If measurements are noisy (high variance across runs), extend measurement iterations. If still noisy, fall back to a conservative config.

### PGO drift
If runtime behavior diverges from PGO predictions beyond a threshold, trigger re-autotune with fresh data.

### Correctness failure
If a candidate produces wrong output (detected via shadow execution on first few calls), blacklist that config class and retry.

---

## XIII. WHAT MAKES THIS DESIGN UNIQUE

1. **PL practice as search space** — we search over compiler transformations, not raw code. This is the space real compiler engineers think in.
2. **PGO as nervous system** — not a stage, but the continuous feedback loop that makes every compilation better than the last.
3. **Heterogeneous IR from day one** — cross-device optimization isn't an afterthought; it's baked into the substrate.
4. **Five-layer pruner** — the hard problem solved by stacking cheap-safe layers under expensive-accurate ones.
5. **No cluster assumption** — saturates local CPU+GPU, works on any machine.
6. **Incremental by construction** — recompilation is cheap because we cache and diff.
7. **Multi-objective by default** — Pareto front, not single-metric optimization.
8. **Self-tuning pruner** — the pruner itself learns from its mistakes.

---

This is the full design. Every piece connects: the IR enables the pruner, the pruner enables the search, the search enables parallel evaluation, parallel evaluation feeds PGO, PGO improves the pruner. It's a closed loop that gets smarter with every compilation.

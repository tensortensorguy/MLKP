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

## LEVEL 1: GRAPH-LEVEL PASSES (High-Level IR)

### Structural Passes
1. **Dead Code Elimination (DCE)** — Remove unreachable/unused ops
2. **Common Subexpression Elimination (CSE)** — Merge identical computations
3. **Constant Folding** — Evaluate constants at compile time
4. **Constant Propagation** — Propagate known values through graph
5. **Algebraic Simplification** — Apply identities (x*1=x, x+0=x)
6. **Commutativity Reordering** — Reorder commutative ops for better layout
7. **Associativity Reordering** — Regroup operations for parallelism
8. **Distributive Law Application** — Factor out common sub-expressions
9. **Identity Operation Removal** — Strip no-op transformations
10. **Type Promotion/Demotion** — Optimize data types
11. **Shape Inference** — Propagate tensor shapes symbolically
12. **Symbolic Shape Analysis** — Handle dynamic shapes with constraints
13. **Graph Canonicalization** — Normalize graph for better matching

### Operator Fusion Passes
14. **Horizontal Fusion** — Fuse ops sharing same input (e.g., multiple convs)
15. **Vertical Fusion** — Fuse producer-consumer chains (Conv→BN→ReLU)
16. **Element-wise Fusion** — Merge pointwise operations
17. **Reduction Fusion** — Fuse ops with reductions (MatMul+Softmax)
18. **Cross-Operator Fusion** — Fuse different op types
19. **Multi-output Fusion** — Single kernel producing multiple outputs
20. **Prologue/Epilogue Fusion** — Attach pre/post ops to main compute
21. **Attention Fusion** — Specialized QKV fusion for transformers

### Layout & Placement Passes
22. **Layout Inference** — Determine optimal memory layout (NCHW/NHWC)
23. **Layout Transformation** — Insert transposes where beneficial
24. **Device Placement** — Assign ops to CPU/GPU/TPU
25. **Memory Placement** — Assign tensors to memory hierarchy
26. **Replication Decision** — Copy vs recompute for memory/speed tradeoff
27. **Gradient Checkpointing** — Selective recomputation for memory savings

---

## LEVEL 2: TENSOR-LEVEL PASSES (Mid-Level IR)

### Loop & Schedule Passes
28. **Tiling / Blocking** — Break tensors into cache-friendly chunks
29. **Multi-level Tiling** — Hierarchical tiles (L1/L2/L3/DRAM)
30. **Loop Interchange** — Reorder nested loops for locality
31. **Loop Fusion** — Combine adjacent loops
32. **Loop Fission** — Split loops for better cache behavior
33. **Loop Unrolling** — Replicate loop body to reduce overhead
34. **Loop Skewing** — Transform loops for parallelism
35. **Loop Normalization** — Standardize loop bounds
36. **Loop Reversal** — Reverse iteration direction
37. **Strip Mining** — Split loops into fixed-size chunks
38. **Loop Distribution** — Separate independent statements
39. **Loop Collapsing** — Merge nested loops into single loop
40. **Loop Index Set Splitting** — Handle non-rectangular iteration spaces

### Vectorization Passes
41. **Auto-vectorization** — Detect SIMD opportunities
42. **Explicit Vectorization** — Insert vector intrinsics
43. **Width-Aware Vectorization** — Match AVX2/AVX-512/NEON/SVE
44. **Masked Vectorization** — Handle irregular shapes
45. **SVE-style Predication** — Length-agnostic vectorization
46. **Vector Type Promotion** — Widen types for vector ops

### Parallelism Passes
47. **Parallel Loop Detection** — Identify parallelizable loops
48. **Thread Mapping** — Map loops to CPU threads
49. **GPU Block/Grid Mapping** — Map to CUDA blocks/grids
50. **Warp/Wavefront Mapping** — Map to GPU warps
51. **SIMT Conversion** — Convert scalar to SIMT execution
52. **Work Distribution** — Partition work across workers
53. **Load Balancing** — Equalize work per worker
54. **Work Stealing** — Dynamic load balancing
55. **Reduction Parallelization** — Tree-based parallel reductions

### Reordering & Scheduling Passes
56. **Operation Reordering** — Change compute order for locality
57. **Memory Access Reordering** — Optimize access patterns
58. **Dependency-Aware Reordering** — Respect data dependencies
59. **Critical Path Optimization** — Minimize longest path
60. **Software Pipelining** — Overlap loop iterations
61. **Instruction Scheduling** — Order instructions for ILP
62. **Latency Hiding** — Interleave independent operations

---

## LEVEL 3: MEMORY-LEVEL PASSES

### Buffer Management
63. **Buffer Reuse / Liveness Analysis** — Share buffers with non-overlapping lifetimes
64. **In-Place Operation Conversion** — Overwrite input when safe
65. **Memory Planning** — Static memory allocation
66. **Arena Allocation** — Pre-allocate memory pools
67. **Stack vs Heap Decision** — Choose allocation strategy
68. **Memory Coalescing** — Merge small allocations
69. **Memory Alignment** — Align to cache line boundaries
70. **Memory Padding** — Pad for alignment/stride

### Register & Cache
71. **Register Allocation** — Assign values to registers
72. **Register Pressure Management** — Avoid spilling
73. **Spill Code Generation** — Handle register overflow
74. **Register Coalescing** — Merge related values
75. **Cache Blocking** — Optimize for cache size
76. **Prefetch Insertion** — Add explicit prefetches
77. **Cache Line Alignment** — Align data to cache lines
78. **False Sharing Elimination** — Prevent cache line contention

### Memory Access Patterns
79. **Coalesced Access Generation** — GPU-friendly memory access
80. **Strided Access Optimization** — Optimize non-contiguous access
81. **Bank Conflict Elimination** — Avoid shared memory conflicts
82. **Misalignment Handling** — Handle unaligned access
83. **Gather/Scatter Optimization** — Optimize indirect access
84. **Memory Compression** — Reduce bandwidth (FP16/INT8)
85. **Sparse Storage Selection** — CSR/CSC/COO/BSR/BSR

### Lifetime & Scope
86. **Lifetime Analysis** — Determine when values are live
87. **Scope Reduction** — Minimize value lifetimes
88. **Temporary Elimination** — Remove intermediate tensors
89. **View/Reference Optimization** — Avoid copies via views

---

## LEVEL 4: PARALLELISM & DISTRIBUTION PASSES

### CPU Parallelism
90. **Thread Parallelization** — Multi-threaded execution
91. **NUMA-Aware Scheduling** — Respect NUMA topology
92. **Thread Affinity** — Pin threads to cores
93. **Cache-Aware Partitioning** — Partition for cache locality

### GPU Parallelism
94. **Thread Block Sizing** — Choose optimal block size
95. **Grid/Block Dimension Tuning** — Optimize launch config
96. **Occupancy Optimization** — Maximize active warps
97. **Warp-Level Primitives** — Use warp shuffle/reduce
98. **Tensor Core Utilization** — Use WMMA/MMA instructions
99. **Shared Memory Tiling** — Use shared memory effectively
100. **Cooperative Loading** — Parallel shared memory loads
101. **Register File Optimization** — Maximize register usage

### Distributed Parallelism
102. **Data Parallelism** — Split batch across devices
103. **Model Parallelism** — Split model across devices
104. **Pipeline Parallelism** — Stage model across devices
105. **Tensor Parallelism** — Split tensors across devices
106. **Expert Parallelism** — MoE-specific distribution
107. **ZeRO Optimization** — Partition optimizer state
108. **All-Reduce Optimization** — Efficient gradient aggregation
109. **Gradient Compression** — Reduce communication volume
110. **Communication Overlap** — Hide communication latency
111. **Collective Operation Optimization** — Optimize MPI/NCCL ops

### Pipeline Parallelism
112. **Computation-Communication Overlap** — Hide transfers
113. **Multi-Stage Pipelining** — Multiple pipeline stages
114. **Bubble Minimization** — Reduce idle time
115. **Micro-batch Scheduling** — 1F1B, GPipe, etc.
116. **Interleaved Scheduling** — Reduce pipeline bubbles

---

## LEVEL 5: ALGORITHMIC PASSES

### Fast Algorithm Selection
117. **Winograd Convolution** — 2x2, 3x3, 4x4, 6x6 variants
118. **FFT-Based Convolution** — Frequency domain convolution
119. **Implicit GEMM** — im2col-free convolution
120. **Direct Convolution** — For small kernels
121. **Strassen Matrix Multiply** — Sub-cubic multiplication
122. **Fast Softmax** — Numerically stable, fused
123. **Fast LogSumExp** — Optimized reduction
124. **Flash Attention** — IO-aware attention
125. **Memory-Efficient Attention** — XFormers-style
126. **PagedAttention** — vLLM-style KV cache

### Sparsity & Pruning
127. **Structured Sparsity Detection** — 2:4, block sparse
128. **Unstructured Sparsity** — Element-wise zeros
129. **Dynamic Sparsity** — Runtime sparsity patterns
130. **Sparse-Dense Hybrid** — Mixed execution
131. **Weight Pruning** — Magnitude-based removal
132. **Neuron/Filter Pruning** — Remove entire units
133. **Iterative Pruning** — Gradual sparsification

### Precision & Quantization
134. **Post-Training Quantization (PTQ)** — Calibrate post-training
135. **Quantization-Aware Training (QAT)** — Train with quantization
136. **Mixed Precision** — FP32/FP16/BF16/INT8 selection
137. **Dynamic Quantization** — Runtime scale computation
138. **Per-Channel Quantization** — Per-output-channel scales
139. **Per-Tensor Quantization** — Single scale per tensor
140. **Calibration** — Determine quantization ranges
141. **Precision Promotion/Demotion** — Adjust precision per-op

### Approximation
142. **Low-Rank Approximation** — SVD-based decomposition
143. **Taylor Expansion** — Polynomial approximation
144. **Lookup Table Replacement** — Precompute expensive ops
145. **Early Exit** — Skip layers when confident
146. **Adaptive Computation** — Vary compute per input
147. **Knowledge Distillation** — Teacher→student transfer

---

## LEVEL 6: HARDWARE-SPECIFIC PASSES

### Tensor Core / Matrix Unit
148. **WMMA Lowering** — NVIDIA tensor cores
149. **MMA Instruction Selection** — AMD matrix cores
150. **Tile Size Alignment** — Match tensor core requirements
151. **Data Format Conversion** — FP16→FP32 accumulation
152. **FP8/BF16 Support** — Modern precision formats

### GPU-Specific
153. **Shared Memory Optimization** — Bank conflict avoidance
154. **L1/Texture Cache Tuning** — GPU cache configuration
155. **Warp Divergence Elimination** — Remove branch divergence
156. **Atomic Operation Optimization** — Reduce atomic contention
157. **Cooperative Group Usage** — Fine-grained synchronization

### CPU-Specific
158. **AVX/AVX2/AVX-512 Selection** — x86 SIMD
159. **NEON Optimization** — ARM SIMD
160. **SVE/SVE2 Optimization** — Scalable vector extension
161. **AMX Optimization** — Intel matrix extensions
162. **VNNI Optimization** — Vector neural network instructions

### Architecture-Specific
163. **CUDA Lowering** — NVIDIA GPU codegen
164. **ROCm Lowering** — AMD GPU codegen
165. **SYCL/OpenCL Lowering** — Portable GPU
166. **Metal Lowering** — Apple GPU
167. **Vulkan/SPIR-V Lowering** — Cross-vendor GPU
168. **TPU-Specific Optimization** — Google TPU
169. **NPU-Specific Optimization** — Neural processing units
170. **FPGA Lowering** — Custom hardware

### Power & Thermal
171. **Frequency Scaling** — DVFS-aware scheduling
172. **Power Capping** — Respect power budgets
173. **Thermal Throttling Avoidance** — Prevent slowdowns
174. **Energy-Efficient Scheduling** — Minimize joules/op

---

## LEVEL 7: SPECIALIZED DOMAIN PASSES

### Convolution-Specific
175. **Depthwise Separable Optimization** — MobileNet-style
176. **Grouped Convolution** — ResNeXt-style
177. **Dilated Convolution** — Atrous convolution
178. **Transposed Convolution** — Deconvolution optimization
179. **Padding Optimization** — Same/valid/full padding
180. **Stride Optimization** — Subsampling strategies

### Attention/Transformer-Specific
181. **QKV Fusion** — Combine query/key/value projections
182. **Multi-Head Fusion** — Process heads in parallel
183. **KV-Cache Optimization** — Inference-time caching
184. **Sparse Attention Patterns** — BigBird, Longformer
185. **Sliding Window Attention** — Local attention
186. **Ring Attention** — Distributed long-context
187. **Speculative Decoding** — Draft-verify generation

### Normalization-Specific
188. **Fused BatchNorm** — Combine with conv/linear
189. **LayerNorm Optimization** — Transformer normalization
190. **GroupNorm Optimization** — Hybrid normalization
191. **InstanceNorm Optimization** — Style transfer
192. **RMSNorm** — LLaMA-style normalization

### Activation-Specific
193. **Fused Activation** — Attach to preceding op
194. **Approximate GELU/SiLU** — Faster variants
195. **Swish/Hard-Swish Optimization** — Mobile-friendly
196. **Lookup Table Activation** — Precomputed values

### Reduction-Specific
197. **Parallel Tree Reduction** — Logarithmic reduction
198. **Warp-Level Reduction** — GPU primitives
199. **Multi-Stage Reduction** — Hierarchical reduction
200. **Atomic Reduction** — Hardware atomics
201. **Segmented Reduction** — Multiple reductions at once

### Embedding/Lookup-Specific
202. **Embedding Table Optimization** — Large table handling
203. **Hash-based Embedding** — Memory-efficient embeddings
204. **Quantized Embedding** — Compressed embeddings
205. **Distributed Embedding** — Sharded tables

---

## 🔄 LEVEL 8: META-OPTIMIZATION PASSES (Your Territory)

### Pass Ordering & Iteration
206. **Pass Ordering Optimization** — Find best sequence
207. **Pass Iteration Count** — How many times to apply
208. **Fixed-Point Detection** — When to stop iterating
209. **Phase Ordering** — Group passes into phases
210. **Pass Interaction Analysis** — Detect synergies/conflicts

### Search & Pruning
211. **Cost Model Training** — Learn performance predictors
212. **Search Space Pruning** — Eliminate bad configs early
213. **Dominance Analysis** — Remove dominated solutions
214. **Similarity Clustering** — Skip redundant evaluations
215. **Historical Pattern Matching** — Cache-based pruning
216. **Constraint Propagation** — Eliminate illegal configs

### Incremental Compilation
217. **IR Diff Analysis** — Detect what changed
218. **Affected Subgraph Extraction** — Isolate changes
219. **Cache Lookup** — Reuse previous results
220. **Incremental Update** — Patch existing compilation
221. **Cache Invalidation** — Know when to recompile

### Parallel Evaluation
222. **Configuration Batching** — Group evaluations
223. **Dynamic Load Balancing** — Adapt to device speed
224. **Early Stopping** — Kill bad configs quickly
225. **Result Aggregation** — Combine parallel results
226. **Fault Tolerance** — Handle evaluation failures

---

## SUMMARY STATISTICS

| Level | Category | Pass Count |
|-------|----------|------------|
| 1 | Graph-Level | ~27 |
| 2 | Tensor-Level | ~38 |
| 3 | Memory-Level | ~27 |
| 4 | Parallelism | ~27 |
| 5 | Algorithmic | ~31 |
| 6 | Hardware-Specific | ~23 |
| 7 | Domain-Specific | ~31 |
| 8 | Meta-Optimization | ~21 |
| **TOTAL** | | **~225 passes** |

---


then we have the tensor ir, right before sea of nodes hetero ir. then after hetero, machine ir


Your tensor IR should be the highest-information semantic representation in the compiler. Your heterogeneous IR should own the physical implementation.

That means no warps, no lanes, no registers, no memory spaces, no physical layouts, no tiling decisions, and no hardware-specific scheduling.

We should not duplicate responsibilities between the two IRs.

But I would go much further than simply removing L5 and L6. I'd design the tensor IR to preserve almost every useful fact about a computation, including information that conventional compilers tend to discard too early.

# 1. New architectural boundary

Tensor IR

# What does this computation mean?

Mathematics, tensor semantics, shapes, numerical contracts, indexing, sparsity, mathematical properties, constraints, and transformation legality.

SEMANTIC / IMPLEMENTATION BOUNDARY

Heterogeneous IR

# How is this computation executed?

Hardware, memory, layouts, tiling, execution distribution, scheduling, synchronization, and device-specific realization.

The Tensor IR should be able to describe the same computation regardless of whether the heterogeneous IR eventually realizes it on a GPU, CPU, FPGA, accelerator, or something else.

There is an important consequence:

The Tensor IR must not lose mathematical information during lowering.

# 2. The information model

I'd give the Tensor IR eight major information domains.

| Domain           | Information preserved                                       |
| ---------------- | ----------------------------------------------------------- |
| Types            | Element types, precision, tensor rank, composite types      |
| Shapes           | Symbolic dimensions, shape equations, bounds, dependencies  |
| Axes             | Axis identities, roles, relationships, broadcasting         |
| Computation      | Maps, reductions, contractions, scans, indexing             |
| Numerics         | Rounding, overflow, precision, determinism, error contracts |
| Value properties | Ranges, sparsity, finiteness, monotonicity, invariants      |
| Differentiation  | Derivative semantics, gradients, custom rules               |
| Provenance       | Source operations, transformation history, semantic origins |

These are not necessarily eight separate dialects. They are eight categories of information available to the IR.

# 3. Tensor types: much richer than `tensor<[B,M,K], f16>`

A tensor type should describe more than its element type and dimensions.

I'd use a conceptual representation like:

```
TensorType {
    element_type
    rank
    dimensions
    axes
    numeric_contract
    value_domain
    structural_properties
}
```

For example:

```
tensor {
    element: f16

    dimensions: [B, M, K]

    axes: [
        batch,
        sequence,
        feature
    ]

    properties: {
        finite: true,
        nonzero: unknown,
        normalized: false
    }
}
```

Some of this information belongs directly in the type. Other information should be represented as refinements associated with individual SSA values.

I would distinguish these carefully.

### Element types

Don't stop at conventional floating-point and integer types.

Support:

- IEEE FP16, FP32, FP64.
- BF16.
- FP8 variants.
- Arbitrary-width integers.
- Signed and unsigned integers.
- Boolean.
- Complex numbers.
- Quantized integer types.
- Custom floating-point formats.
- Opaque semantic element types.

In particular, distinguish the storage-independent mathematical type from the numerical contract governing operations on it.

A tensor containing FP16 values and a tensor whose operations accumulate in FP32 are different things.

### Composite tensor types

Support more than dense tensors:

- Dense tensors.
- Sparse tensors.
- Block-sparse tensors.
- Ragged tensors.
- Segmented tensors.
- Masked tensors.
- Nested tensors.
- Optional tensors.
- Sequences of tensors.

This is semantic support. Physical sparse encodings and storage formats remain the responsibility of your heterogeneous IR.

# 4. Shape system: first-class symbolic mathematics

I would make shape expressions an actual subsystem, not just arrays of integers and question marks.

For example:

```
shape {
    B : Dim
    M : Dim
    K : Dim

    constraints {
        B > 0
        M > 0
        K > 0

        K % 16 == 0
    }
}
```

The shape engine should understand:

| Capability         | Example                              |
| ------------------ | ------------------------------------ |
| Constants          | `M = 1024`                           |
| Symbols            | `M = N`                              |
| Arithmetic         | `M + K`                              |
| Products           | `B * M`                              |
| Division           | `M / 2`                              |
| Ceiling division   | `ceildiv(M, 16)`                     |
| Minimum/maximum    | `min(M, K)`                          |
| Conditional shapes | `select(C, M, N)`                    |
| Shape equality     | `A.shape[1] == B.shape[0]`           |
| Divisibility       | `K % 16 == 0`                        |
| Bounds             | `0 < B <= 4096`                      |
| Shape dependencies | `output.shape[0] = count_nonzero(A)` |

The last one matters.

There are different kinds of dynamism:

1. Static dimensions.
2. Input-dependent symbolic dimensions.
3. Bounded dynamic dimensions.
4. Data-dependent dimensions.
5. Unbounded dynamic dimensions.

These should be distinct in the IR.

StableHLO's documentation discusses the distinction between shape polymorphism and data-dependent dynamism. I'd use that as a baseline and go further.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub+1



I'd also support shape proofs:

```
prove:
    A.shape[1] == B.shape[0]

result:
    valid_matmul_dimensions
```

The shape system should expose these proofs to every optimization pass.

# 5. Computation semantics: don't make everything an opaque operator

This is where I'd spend a considerable amount of design effort.

You want both:

- High-level semantic operations.
- General mathematical operations capable of expressing computations not covered by the built-in operator set.

I'd define these fundamental computation families:

| Family    | Purpose                                |
| --------- | -------------------------------------- |
| Map       | Elementwise or pointwise computation   |
| Reduce    | Reduction over dimensions              |
| Scan      | Prefix computation                     |
| Contract  | Generalized tensor contraction         |
| Broadcast | Dimension expansion                    |
| Gather    | Indexed selection                      |
| Scatter   | Indexed updates                        |
| Window    | Sliding-window computation             |
| Permute   | Axis permutation                       |
| Reshape   | Logical shape transformation           |
| Slice     | Subtensor extraction                   |
| Segment   | Operations over variable-length groups |
| Sort      | Ordering and selection                 |
| Select    | Conditional value selection            |
| Custom    | User-defined semantic computation      |

But there is a subtle requirement.

A high-level operation should retain its identity even when its implementation is described using more general operations.

Consider matrix multiplication:

\\[ C\_{ij}=\sum_k A\_{ik}B\_{kj} \\]

You want both:

```
tensor.matmul(A, B)
```

and the generalized contraction representation.

Don't erase `matmul` immediately into a generic reduction. Doing that throws away information that might be useful to later optimization passes.

Instead, support semantic refinement:

```
tensor.matmul(A, B)
    refines_to:
        tensor.contract(...)
```

The compiler knows that this is a matrix multiplication, but also has access to its complete mathematical structure.

This is one of the useful ideas behind MLIR Linalg's structured operations and indexing maps.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://mlir.llvm.org\&sz=32)

MLIR



### Indexing semantics

I'd make indexing maps first-class.

For every structured computation, preserve:

- Iteration domain.
- Logical iteration variables.
- Input indexing maps.
- Output indexing maps.
- Reduction axes.
- Parallel-independent axes.
- Scan axes.
- Window axes.
- Broadcast relationships.
- Boundary behavior.
- Masking conditions.

For example:

```
contract {
    domain: [m: M, n: N, k: K]

    inputs {
        A[m, k]
        B[k, n]
    }

    output {
        C[m, n]
    }

    reduction {
        axis: k
        operator: add
    }

    element_operation:
        multiply
}
```

This provides substantially more information than a generic matrix multiplication node.

TVM TensorIR's explicit block-axis properties are another useful reference for this particular design.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://tvm.apache.org\&sz=32)

Apache TVM+1



# 6. Numerical semantics: absolutely explicit

I would make this one of the strictest parts of the specification.

An ML compiler has to distinguish mathematical equivalence from floating-point equivalence.

For every relevant operation, the IR should be capable of specifying:

| Property               | Examples                                     |
| ---------------------- | -------------------------------------------- |
| Input precision        | FP16, BF16, FP32                             |
| Intermediate precision | FP32, FP64                                   |
| Accumulation precision | FP32                                         |
| Result precision       | FP16                                         |
| Rounding               | RNE, RTZ, stochastic                         |
| Overflow               | Wrap, saturate, IEEE                         |
| Underflow              | Preserve, flush                              |
| NaN behavior           | Propagate, canonicalize, specified selection |
| Signed zero            | Preserve or relaxed                          |
| Reassociation          | Allowed or prohibited                        |
| FMA contraction        | Allowed or prohibited                        |
| Reduction ordering     | Ordered, unspecified                         |
| Determinism            | Required or relaxed                          |
| Approximation          | Absolute/relative error bounds               |

I'd introduce a first-class numerical contract:

```
numeric_contract {
    input_precision: f16
    accumulator_precision: f32
    output_precision: f16

    rounding: nearest_even

    overflow: ieee
    underflow: preserve

    reassociation: forbidden
    fma_contraction: permitted

    deterministic: required
}
```

The optimizer must respect it.

I'd also support scoped numerical contracts, so an entire function can declare default behavior while individual operations override it.

### Approximation contracts

This is especially important for ML.

For example:

```
approximation {
    absolute_error: 1e-6
    relative_error: 1e-4

    nan_equivalence: required
}
```

This allows optimizations such as approximate transcendental functions to have explicit legality conditions.

Do not make the optimizer guess how much numerical error the application tolerates.

# 7. Value properties: this is where I'd go unusually far

This is one area where I think you can create a real advantage.

A tensor should be able to carry known mathematical properties.

For example:

```
%weights:
    tensor<f32, [B, N]>

    guarantees {
        finite
        nonnegative
        normalized(axis = 1)
        sum(axis = 1) == 1
    }
```

Potential properties:

Numerical properties

- Finite.
- Non-NaN.
- Non-infinite.
- Positive.
- Nonnegative.
- Nonzero.
- Bounded.
- Integer-valued.
- Normalized.

Structural properties

- Symmetric.
- Hermitian.
- Diagonal.
- Triangular.
- Orthogonal.
- Permutation.
- One-hot.
- Identity.
- Constant along an axis.
- Repeated along an axis.

Ordering properties

- Sorted ascending.
- Sorted descending.
- Monotonic.
- Strictly monotonic.
- Unique.
- Distinct within segments.

Statistical properties

- Probability distribution.
- Mean constraints.
- Variance constraints.
- Known sparsity ratio.
- Known distribution family, where justified.

Why is this useful?

Imagine the optimizer sees:

```
softmax(X)
```

and knows the output is nonnegative, finite under its declared numerical assumptions, and normalized along a particular axis.

That information can influence subsequent operations.

Similarly, a symmetric matrix may enable transformations unavailable for an arbitrary matrix.

But there is an important rule:

Properties must have proof status.

I'd distinguish:

| Classification | Meaning                                                 |
| -------------- | ------------------------------------------------------- |
| Proven         | Established statically                                  |
| Guaranteed     | Provided by the operation's semantic contract           |
| Assumed        | Declared by an external caller                          |
| Guarded        | Validated by a runtime condition                        |
| Inferred       | Derived by analysis but not yet independently validated |
| Hint           | Optimization suggestion with no correctness guarantee   |

Never let an optimization silently treat a user-provided hint as a mathematical proof.

This proof/refinement system could become one of the most valuable components of the entire compiler.

# 8. Differentiation semantics

I would not make automatic differentiation an external afterthought.

The IR should know how its operations behave under differentiation.

For each operation, support:

```
differentiation_contract {
    differentiable: true

    forward_rule: defined
    reverse_rule: defined

    higher_order: supported

    nondifferentiable_inputs: []
}
```

Capabilities should include:

- Forward-mode differentiation (JVP).
- Reverse-mode differentiation (VJP).
- Higher-order differentiation.
- Custom differentiation rules.
- Stop-gradient semantics.
- Straight-through estimators.
- Differentiable indexing where applicable.
- Complex-valued differentiation.
- Piecewise differentiability.
- Explicit nondifferentiable inputs and outputs.

For example, a custom operation could declare its own VJP without forcing the compiler to decompose the entire operation into primitive arithmetic.

I'd also preserve derivative provenance so that a transformed graph can be traced back to the original mathematical operation.

# 9. Sparsity and structural semantics

I would make sparsity a mathematical property rather than a storage choice.

For example:

```
tensor<f32, [M, N]> {
    sparsity {
        kind: structured
        pattern: block
        block_shape: [4, 4]
        guarantee: exact
    }
}
```

Other useful concepts:

- Unstructured sparsity.
- Block sparsity.
- N:M sparsity.
- Banded matrices.
- Diagonal matrices.
- Low-rank structure.
- Known zero regions.
- Known constant regions.
- Dynamic sparsity.
- Sparse value patterns.
- Implicit default values.

The distinction between known zeros and absent elements must be explicit. They aren't necessarily interchangeable when dealing with floating-point exceptional values or custom operations.

The physical representation of those sparse structures belongs to your heterogeneous IR.

# 10. Effects and control flow

I would keep the tensor computation model predominantly functional and SSA-based.

Tensor operations should ordinarily produce new logical values without exposing mutation or physical storage.

However, ML programs have more than pure arithmetic.

You need semantic support for:

- Conditional execution.
- Loops.
- Recursion, where permitted.
- Dynamic iteration.
- Function calls.
- Random number generation.
- Stateful model updates.
- External/custom operations.
- Assertions and runtime constraints.

Random operations should explicitly represent RNG state or keys. Otherwise, transformations such as operation reordering and common-subexpression elimination can silently change program behavior.

Likewise, training operations should represent state transitions explicitly.

For example:

```
%new_params, %new_optimizer =
    optimizer.update(%params, %gradients, %optimizer_state)
```

No physical mutation is implied by this representation.

# 11. Provenance and transformation history

I'd consider this a first-class subsystem.

Every operation should be able to retain:

```
provenance {
    source_framework: ...
    source_operation: ...
    source_location: ...

    original_operation_id: ...
    parent_operations: [...]

    transformation_history: [...]
}
```

This enables:

- Debugging against the original model.
- Explaining fusion decisions.
- Tracking numerical changes.
- Identifying where an unexpected shape originated.
- Reconstructing high-level operations.
- Comparing alternative optimization paths.

There is also a useful distinction between semantic information and optimization metadata.

For instance:

```
semantic:
    input is symmetric

hint:
    symmetric matrix optimizations preferred
```

The first is something the compiler can rely upon if proven. The second is merely a suggestion.

Keep them separate.

# 12. The most important design decision: contracts attached to operations

I'd make every operation describe itself through a structured semantic contract.

Conceptually:

```
OperationContract {
    operand_types
    result_types

    shape_function
    constraints

    mathematical_definition
    indexing_semantics

    numerical_semantics

    value_guarantees

    differentiation_rules

    effects

    refinement_rules
}
```

An operation should expose these interfaces to the compiler.

That means optimization passes don't need to understand every individual operation implementation.

They can ask:

- What is this operation mathematically?
- Which dimensions does it consume?
- What shape does it produce?
- Which transformations preserve its semantics?
- What properties does its result guarantee?
- What numerical errors are permitted?
- Does it have a derivative?
- Does it have observable effects?

This is much more extensible than building a compiler where every optimization pass contains an enormous list of operator-specific special cases.

# 13. What the actual IR might look like

Here's a conceptual example combining most of those ideas.

```
%result = tensor.matmul %A, %B {
    semantic_id: "matmul"

    dimensions {
        batch: B
        rows: M
        columns: N
        reduction: K
    }

    indexing {
        A: [batch, rows, reduction]
        B: [batch, reduction, columns]
        result: [batch, rows, columns]
    }

    numerical {
        input: f16
        accumulation: f32
        output: f16

        rounding: nearest_even
        reassociation: permitted
        deterministic: required
    }

    shape_constraints {
        A.shape[0] == B.shape[0]
        A.shape[2] == B.shape[1]
    }

    guarantees {
        result.shape == [B, M, N]
    }

    differentiation {
        forward: automatic
        reverse: automatic
    }

    refinement {
        equivalent_to: tensor.contract
    }
}
```

This representation says a lot without saying anything about physical execution.

The heterogeneous IR can take this information and make all the implementation decisions.

# 14. How I'd structure the actual compiler

I would not implement the eight domains as eight disconnected IRs.

I'd use this organization:

Tensor IR Core

SSA values, operations, regions, functions, control flow

Semantic Type System

Tensor types, element types, composite types, axis identities

Shape Engine

Symbolic algebra, constraints, proofs, shape functions

Operation Semantics Registry

Mathematical definitions, indexing, numerical contracts, effects

Property and Refinement Engine

Value facts, invariants, guarantees, assumptions, guarded facts

Transformation Framework

Legality checking, canonicalization, fusion, decomposition, equivalence

Differentiation Framework

JVP, VJP, higher-order differentiation, custom rules

Provenance System

Source tracking, transformation history, debugging metadata

These systems should share information through formal interfaces.

For example, the shape engine provides proofs to the transformation framework. The numerical contracts constrain the equivalence checker. The differentiation framework consumes operation semantics.

This is considerably better than maintaining separate, duplicated knowledge in each optimizer.

# 15. What I would explicitly prohibit

To protect the boundary with your heterogeneous IR, I'd put these restrictions in the Tensor IR specification:

| Forbidden in Tensor IR                | Reason                      |
| ------------------------------------- | --------------------------- |
| Warp IDs                              | Execution detail            |
| Lane IDs                              | Execution detail            |
| Thread/block IDs                      | Execution detail            |
| SIMD widths                           | Hardware detail             |
| Physical buffer representations       | Memory implementation       |
| Physical strides                      | Storage detail              |
| Register allocation                   | Execution detail            |
| Shared-memory placement               | Memory implementation       |
| Physical tiling schedules             | Optimization implementation |
| Synchronization instructions          | Execution detail            |
| Async copy operations                 | Hardware implementation     |
| Hardware-specific tensor instructions | Target implementation       |

There are exceptions to terminology, not responsibilities.

For example, a logical sliding window is a mathematical operation, so it belongs here. A physical tiled execution schedule does not.

Similarly, knowing that two output dimensions are mathematically independent is useful semantic information. Assigning those dimensions to GPU warps is not.

# 16. The three properties I'd optimize the entire design around

1\. Semantic completeness

An operation should communicate enough information to determine its mathematical meaning, numerical behavior, shape behavior, and observable effects.

2\. Information preservation

Lowering must not unnecessarily destroy information that later transformations could exploit.

Keep high-level operations recognizable even when they acquire lower-level mathematical representations.

3\. Verifiable optimization

Every transformation needs explicit legality conditions.

The optimizer should distinguish proven facts, external assumptions, runtime-guarded facts, and nonbinding hints.

## My verdict

I would call this a Semantic Tensor IR, rather than a conventional tensor computation IR.

Its purpose isn't to tell hardware how to execute a tensor operation. It's to give the rest of the compiler the richest possible description of that operation.

One final architectural recommendation: don't put every property in the tensor type itself.

Use three complementary mechanisms:

- Types for intrinsic properties.
- Refinements and proofs for value-dependent properties.
- Operation contracts for computational semantics.

That prevents the type system from becoming an enormous metadata container while still preserving the information you care about.


Now we're getting into the backend proper.

And IRC (Iterated Register Coalescing) is a very reasonable choice. I'd actually make it the primary register-allocation algorithm for your general-purpose machine backend.

But I'd design the Machine IR around it, rather than bolt IRC onto an already-designed instruction representation.

There are three things to get right:

1. Machine IR must represent instructions and their constraints precisely.
2. Virtual registers must preserve enough information for optimization and allocation.
3. Register allocation must handle interference, coalescing, spilling, splitting, and target-specific register constraints.

# 1. Machine IR architecture

Given your existing architecture:

- Tensor IR: mathematical semantics.
- Heterogeneous IR: execution model and hardware realization.
- Machine IR: target instruction semantics and executable operations.

I would make Machine IR target-specific, but not immediately tied to physical registers.

In other words, it knows the target ISA, instruction set, register classes, ABI and instruction constraints, but initially uses virtual registers.

Heterogeneous IR

Execution and hardware realization

Machine IR (virtual-register form)

Target instructions, virtual registers, CFG, instruction constraints, register classes, memory operations

SSA preferred initially

Machine optimization

Instruction selection refinement, peepholes, scheduling, liveness, copy propagation

Register allocation

Interference, IRC, spilling, splitting, physical-register assignment

Machine IR (allocated form)

Physical registers, stack slots, resolved copies, concrete instructions

Machine code emission

Encoding, relocations, object files, executable code

I would use one Machine IR with explicit allocation states, rather than inventing a completely different IR for every backend phase.

# 2. Machine IR instruction model

Every instruction needs explicit information.

I'd define an instruction conceptually as:

```
struct MachineInstruction {
    Opcode opcode;

    OperandList operands;

    RegisterConstraints constraints;

    InstructionEffects effects;

    MemoryEffects memory;

    DebugLocation debug_location;
};
```

The important components:

### Operands

Support:

- Virtual registers.
- Physical registers.
- Immediate values.
- Memory references.
- Block references.
- Symbol references.
- Constant-pool references.
- Register pairs and tuples.
- Condition-code operands.

Each operand should carry its access mode:

```
use
def
use_def
```

Don't infer everything from operand position.

### Instruction effects

Explicitly represent:

- Register definitions.
- Register uses.
- Implicit register uses.
- Implicit register definitions.
- Flags modified.
- Memory reads and writes.
- Control-flow effects.
- Calls.
- Traps.
- Side effects.
- Volatile operations.

For example, an x86-64 division instruction has implicit register constraints involving RAX and RDX. Your allocator must know that. Hoping the instruction encoder will sort it out later is not a strategy.

### Register constraints

Instructions should be able to declare:

```
constraints {
    operand[0]: {
        class: GPR64
        allowed: any
    }

    operand[1]: {
        class: GPR64
        allowed: any
        tied_to: operand[0]
    }

    operand[2]: {
        class: GPR64
        early_clobber: true
    }
}
```

This is particularly important for architectures with complicated instruction constraints.

# 3. Virtual register design

I'd use typed virtual registers with explicit register classes.

For example:

```
%v0 : GPR64
%v1 : GPR64
%v2 : FPR32
%v3 : VECTOR128
%v4 : MASK
```

Register classes should be target-defined.

Examples:

| Register class | Example                         |
| -------------- | ------------------------------- |
| GPR32          | 32-bit integer registers        |
| GPR64          | 64-bit integer registers        |
| FPR32          | 32-bit floating-point registers |
| FPR64          | 64-bit floating-point registers |
| VECTOR128      | 128-bit vector registers        |
| VECTOR256      | 256-bit vector registers        |
| VECTOR512      | 512-bit vector registers        |
| MASK           | Predicate/mask registers        |
| SPECIAL        | Architecture-specific registers |

However, don't assume register classes are disjoint.

An x86-64 XMM register and its corresponding YMM/ZMM register overlap physically. ARM also has overlapping SIMD/scalar register views.

You need register-unit modeling.

```
PhysicalRegister {
    register_id
    register_units[]
    supported_classes[]
    aliases[]
    reserved
    allocatable
}
```

Two physical registers interfere if their underlying register units overlap.

This also lets you handle subregisters, register pairs, and architectures with overlapping register views.

### SSA or non-SSA?

My recommendation:

- Machine IR begins in SSA form.
- Machine optimization operates primarily in SSA form.
- Register allocation can consume SSA or a lowered representation.
- Final allocation removes virtual-register SSA semantics.

SSA is particularly valuable because it simplifies dataflow analysis and interference construction.

You can preserve machine-level SSA through instruction selection, even though the final physical machine instructions aren't SSA.

# 4. IRC: Iterated Register Coalescing

My recommended primary allocator

The algorithm comes from George and Appel's work on register allocation.

Its central idea is to combine graph coloring with aggressive copy coalescing, while avoiding unnecessary interference.

Suppose you have:

```
%a = add %x, %y
%b = copy %a
%c = mul %b, %z
```

If `%a` and `%b` can share the same physical register, the copy disappears.

Without coalescing:

```
add r1, r2, r3
mov r4, r1
mul r5, r4, r6
```

With successful coalescing:

```
add r1, r2, r3
mul r5, r1, r6
```

One instruction gone. Humans occasionally produce useful ideas.

## The five IRC operations

IRC maintains an interference graph and repeatedly performs five kinds of operations.

| Operation | Purpose                                                |
| --------- | ------------------------------------------------------ |
| Simplify  | Remove nodes that are guaranteed to be colorable later |
| Coalesce  | Merge compatible non-interfering nodes                 |
| Freeze    | Stop attempting coalescing for a node                  |
| Spill     | Select a node that may need spilling                   |
| Select    | Assign physical registers in reverse removal order     |

The classic IRC algorithm distinguishes two important classes of coalescing:

George criterion: When coalescing with a precolored node, the merge is safe if every relevant neighbor is either low-degree, already adjacent to the precolored node, or precolored.

Briggs criterion: When coalescing two ordinary nodes, the merge is conservative if the resulting node has fewer than K high-degree neighbors.

Here K is the number of available colors in the relevant register class.

This is why IRC is interesting: it tries to remove copies without recklessly increasing graph pressure.

## 5. How I'd structure your allocator

I would implement it as several distinct subsystems.

Register Allocation Engine

1\. Liveness analysis

Live-in/live-out, use/def sets, live intervals

2\. Interference construction

Register graph, physical constraints, register units

3\. IRC core

Simplify, coalesce, freeze, spill, select

4\. Spill management

Spill costs, spill rewriting, rematerialization

5\. Register assignment

Physical assignment, copy resolution, callee-save handling

6\. Allocation verification

No conflicting assignments, constraints satisfied, all virtual registers resolved

I would keep the IRC core independent of target-specific instruction descriptions.

The target supplies register classes, physical register units, allocation constraints and spill behavior. IRC consumes those interfaces.

# 6. Spilling: don't use a naive strategy

This is where a lot of otherwise decent register allocators lose performance.

When register pressure exceeds the available colors, the allocator must decide which values to spill.

A basic spill cost:

\\[ C(v)= \frac{ \text{weighted uses}(v)+\text{weighted defs}(v) }{ \text{degree}(v) } \\]

But I'd go further.

Consider:

```
%a = expensive_computation()
%b = cheap_computation()

use(%a)
use(%b)
```

Spilling `%a` might require recomputing an expensive value. Spilling `%b` might be cheap.

Your allocator should understand rematerialization.

### Spill strategies I'd support

1. Direct spilling: Store and reload the value.
2. Rematerialization: Recompute cheap values instead of loading them.
3. Live-range splitting: Spill only portions of a live range.
4. Block-local spilling: Spill around regions with high register pressure.
5. Frequency-weighted spilling: Prefer spills in cold blocks.
6. Register-class-aware spilling: Account for the scarcity of particular register classes.

I'd also support splitting around calls, loops, and other pressure hotspots.

A single virtual register does not need to occupy one physical register for its entire lifetime.
.

# 8. A feature I'd strongly recommend: register allocation contracts

Don't make the allocator rely on undocumented assumptions.

Every target should expose an allocation description:

```
RegisterAllocationTarget {
    register_classes
    physical_registers
    register_units
    aliases

    reserved_registers
    allocatable_registers

    calling_conventions
    callee_saved
    caller_saved

    instruction_constraints
    spill_requirements
    rematerialization_rules
}
```

This is particularly important for heterogeneous targets.

Your allocator should be able to ask:

- Which registers can hold this value?
- Which registers overlap?
- Which registers are reserved?
- Which instructions require fixed registers?
- Which registers survive function calls?
- What is the cost of spilling this register class?
- Can this value be rematerialized?

The target describes the machine. The allocator solves the assignment.

# 9. Additional machine-level optimizations

I'd include these in the Machine IR optimization pipeline:

| Optimization                 | Purpose                                   |
| ---------------------------- | ----------------------------------------- |
| Copy propagation             | Eliminate redundant copies                |
| Dead instruction elimination | Remove unused computations                |
| Machine CSE                  | Eliminate equivalent instructions         |
| Peephole optimization        | Replace inefficient instruction sequences |
| Instruction combining        | Merge compatible instructions             |
| Constant folding             | Evaluate known constants                  |
| Strength reduction           | Replace expensive operations              |
| Machine LICM                 | Move loop-invariant instructions          |
| Rematerialization            | Reconstruct cheap values                  |
| Branch optimization          | Simplify control flow                     |
| Tail-call optimization       | Avoid unnecessary call frames             |
| Post-RA scheduling           | Improve instruction ordering              |

I'd also include a machine-level equivalence checker for transformations where correctness is difficult to establish by local reasoning.

# 10. My proposed final architecture

Machine Backend

Target Description

ISA, ABI, register classes, instruction definitions, constraints

Machine IR

SSA virtual registers, instructions, operands, CFG, effects

Machine Optimizer

CSE, DCE, peepholes, combining, machine-level simplification

Register Allocation Framework

Liveness, interference, IRC, spilling, splitting, rematerialization

Physical Register Assignment

Register assignment, copy resolution, callee-save handling

Finalization

Post-RA optimization, instruction scheduling, encoding, emission

## 11. What I'd actually implement first

My suggested implementation order:

| Priority | Component                                  |
| -------- | ------------------------------------------ |
| 1        | Machine instruction and operand model      |
| 2        | Target description system                  |
| 3        | Virtual register and register class system |
| 4        | Machine CFG and SSA                        |
| 5        | Liveness analysis                          |
| 6        | Interference graph                         |
| 7        | IRC allocator                              |
| 8        | Spill rewriting and rematerialization      |
| 9        | Physical register assignment               |
| 10       | Machine code emission                      |

One important research decision: I'd benchmark IRC against LLVM's greedy allocator and a linear-scan implementation, using identical instruction selection and target constraints. Don't assume the theoretically attractive allocator wins on your actual workloads.

My strongest recommendation is to make the allocation framework pluggable from day one. IRC should be the flagship, not a hardcoded assumption throughout the backend.

And I'd give the allocator its own verification suite. Every allocation must prove that:

- Interfering live values never share overlapping physical registers.
- Register-class restrictions are respected.
- Fixed-register constraints are satisfied.
- Calls respect caller/callee-save conventions.
- Spill and reload sequences preserve values.
- No unresolved virtual registers remain.

That gives you a machine backend with a fairly clean separation between instruction semantics, target descriptions, and allocation algorithms.

The particularly interesting part is that IRC becomes an optimization engine, not merely a register assignment pass. Coalescing, splitting, rematerialization and spill selection can all interact to improve the resulting machine code.


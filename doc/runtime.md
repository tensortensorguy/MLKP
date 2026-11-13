# Runtime

## Abstract
The runtime executes kernel plans over a preallocated arena. It contains the cycle-exact kernel file (CEP-0), the plan model with its validation airlock, the executor shell, the device probe, and the measurement discipline.

## Formal Definitions
```rust
pub struct KernelPlan { ops: Vec<PlanOp>, buffers: u32, buffer_lens: Vec<u64>, buffer_offsets: Vec<u64>, config: PassConfig }
pub enum PlanOp { Unary{..}, Binary{..}, Reduce{..}, Softmax{..}, MatMul{..} }
pub struct Measurement { median: Duration, min: Duration, mean: Duration, variance_ns2: f64, runs: u32 }
```

## Algorithm
Lowering walks the verified graph, assigns buffers in order, and emits plan ops with concrete extents, the matmul schedule (loop order and B access pattern), and the thread chunking. The airlock proves length relations, tile legality (nonzero powers of two), and per-operation buffer disjointness once; the kernels then debug-assert only. Execution derives disjoint mutable views from one arena slice and dispatches each op through a flat match to the CEP-0 kernels with zero per-op copies; the matmul with a worker count above one splits its output rows across scoped threads, each owning a disjoint range. Measurement discards warmup runs and reports the median with sample variance and the Welch t statistic for pairwise comparison.

## Invariants
1. The kernels allocate nothing, lock nothing, format nothing, and panic on nothing; misuse is a debug assertion.
2. The airlock rejects any plan whose buffer lengths disagree with op extents before execution.
3. The arena is allocated once per evaluation session and reset between runs; buffers never resize mid-measurement.
4. The probe reports honest availability; absent Vulkan is a reason string, never a silent CPU assumption.

## Integration Points
* Depends on: `mlkp-config` (limits), `mlkp-hir` (lowering input).
* Depended on by: `mlkp-autotune` (evaluation), `mlkp` (execute).

## Known Limitations
The probe checks loader presence, not device enumeration (ticket CEP-210). Spill rewriting and rematerialization for the machine backend are deferred (ticket CEP-323). The arena reserves a fixed evaluation capacity; multi-input plans need extended staging (ticket CEP-324).

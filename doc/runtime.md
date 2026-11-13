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
Lowering walks the verified graph, assigns buffers in order, and emits plan ops with concrete extents. The airlock proves buffer table consistency (offsets ascend without overlap), length relations for every op, per-op buffer disjointness, and the power-of-two tile contract once; the kernels then debug-assert only. Execution dispatches each op through a flat match to the CEP-0 kernels. Measurement discards warmup runs and reports the median with sample variance and the Welch t statistic for pairwise comparison.

## Invariants
1. The kernels allocate nothing, lock nothing, format nothing, and panic on nothing; misuse is a debug assertion.
2. The airlock rejects any plan whose buffer table is inconsistent, whose lengths disagree with op extents, whose op buffers overlap in the arena, or whose matmul tiles are not powers of two, before execution.
3. The arena is allocated once per evaluation session and reset between runs; buffers never resize mid-measurement.
4. The probe reports honest availability; absent Vulkan is a reason string, never a silent CPU assumption.

## Integration Points
* Depends on: `mlkp-config` (limits), `mlkp-hir` (lowering input).
* Depended on by: `mlkp-autotune` (evaluation), `mlkp` (execute).

## Known Limitations
The executor is single-threaded; `PassConfig.threads` is plumbed but unused (ticket CEP-320). The probe checks loader presence, not device enumeration (ticket CEP-210). Matmul is row-major with configurable tiles; the transposed-B layout flag is plumbed but not implemented in the kernel (ticket CEP-321).

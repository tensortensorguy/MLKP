# Compiler Pipeline

## Abstract
The compiler pipeline converts a `.mlkp` text program into a measured, verified kernel plan. The front-end parses and lowers into the semantic Tensor IR and the heterogeneous graph; the facade verifies, runs the level-1 passes, and drives the JIT autotune lifecycle; the runtime executes the winning plan. Inputs are the source text plus concrete operand buffers; outputs are the compiled plan and its execution results.

## Formal Definitions
The pipeline stages and their types:

```rust
pub fn compile(source: &str, pgo: &mut PgoStore, budget: Duration, inputs: (Vec<f32>, Vec<f32>))
    -> Result<Compiled, CompilerError>;
pub fn execute(compiled: &Compiled, a: &[f32], b: &[f32]) -> Result<Vec<f32>, CompilerError>;
```

`Compiled` carries the winning `KernelPlan`, the winning config hash, the measured winner latency, a `from_cache` flag, and staging offsets `(a, b, out)` in arena elements.

## Algorithm
1. Parse the source into a `Program` (front-end airlock enforces size and statement caps).
2. Verify the heterogeneous graph; run algebraic simplification, CSE, and DCE in canonical order; verify again.
3. Compute the subgraph hash (`graph.hash64`) and the hardware profile hash.
4. Check the PGO store for a previous winner; on a hit, install and re-check.
5. Enumerate the search space, prune through Layers 1, 4, and 3, run the four-phase search with the configured budget, and select the winner.
6. Lower the winner to a kernel plan; validate the airlock; record the measurement in the PGO store.

Time is dominated by evaluation; compilation outside evaluation is O(graph + candidates * features).

## Invariants
1. Every plan that reaches execution has passed `KernelPlan::validate`.
2. The baseline schedule is always evaluated first; the search can never regress below it without a correctness failure elsewhere.
3. Buffer ids never index the arena directly; all slices go through `KernelPlan::offset_of`.
4. The output of `execute` matches the reference matmul-relu within `CORRECTNESS_ABS_TOLERANCE`.

## Integration Points
* Depends on: `mlkp-front` (parsing), `mlkp-hir` (graph and passes), `mlkp-autotune` (lifecycle), `mlkp-runtime` (plans and execution).
* Depended on by: the `mlkpc` binary and the lifecycle integration test.

## Known Limitations
The CPU plan schedules unary maps, last-axis reductions, softmax, and tiled matmul only; binary elementwise maps, broadcast, permute, and reshape lower to plan errors in v1. Thread chunking is reserved in `PassConfig` but the executor is single-threaded (ticket CEP-320).

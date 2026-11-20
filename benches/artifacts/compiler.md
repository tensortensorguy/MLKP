<!-- CEP:FILE: benches/artifacts/compiler.md -->
<!-- CEP:WHAT: The compiler-line artifact: measured compile-path numbers and the staged-input contract evidence for the mlkpc pipeline. -->
<!-- CEP:WHY: The lifecycle claims (multi-input staging, cache-hit recompile, staged synthesis) are observable only through measured runs; this artifact ties the CEP-324 contract to its numbers. -->
<!-- CEP:COST: the bench suite and the CLI smoke run finish in seconds. -->
<!-- CEP:EVIDENCE: cargo bench -p mlkp --bench pipeline and cargo run --release --bin mlkpc. -->

# Compiler artifact — CEP-324 multi-input plan staging

## The contract change

The compiler pipeline was hardwired to the two-operand facade: `compile` took
`(Vec<f32>, Vec<f32>)`, `Compiled.staging` was a three-tuple, and the autotune
evaluator staged exactly two buffers. The plan lowering itself always aliased
every declared parameter to buffers `0..N` (`graph.inputs` order), so the
hardwiring lived entirely in the facade (`mlkp/src/lib.rs`), the autotune
driver (`mlkp-autotune/src/driver.rs`), and the CLI (`mlkp/src/bin/mlkpc.rs`).

CEP-324 generalizes the contract to N declared parameters:

- `compile(source, pgo, budget, inputs: &[Vec<f32>])` stages one buffer per
  declared parameter; arity is checked before any search work.
- `Compiled` now carries `input_offsets: Vec<u64>` (parameter order),
  `output_offset: u64`, and `param_lens: Vec<u64>` (element counts per
  parameter, so callers can synthesize inputs without re-parsing).
- `execute(compiled, inputs: &[&[f32]])` stages every declared input with
  exact length checks and names the failing input in its error.
- The autotune evaluator's shadow-reference staging (`stage_and_run`) loops
  over all declared inputs; the problem-scale hint sums every staged input.
- The CLI synthesizes a deterministic buffer for every parameter from one
  parse pass (distinct fill pattern per parameter index); symbolic dimensions
  are rejected with a usage error instead of a silent wrong-shape run.

## Evidence

- `multi_input_program_stages_every_declared_parameter` (JIT-p73): a
  three-parameter matmul chain compiles, stages three buffers, executes, and
  matches a naive f32 reference element-wise; two buffers against three
  parameters fail with an arity diagnostic, and a short third input fails
  staging naming `input 2`.
- CLI smoke, three-parameter program (`%a`,`%b`,`%c: f32[48,48]`, chain of two
  matmuls, budget 400 ms):

  ```
  stage=parse ok
  stage=verify ok nodes=5
  stage=autotune winner_hash=1869352678443678359 latency_ns=9839 from_cache=false tile=128x64x128 loop_order=0 transpose_b=false threads=1
  stage=execute ok checksum=42195108.0000
  ```

## Measured compile path (this build, release)

| Scenario | Median | Min |
|---|---|---|
| compile 8-op cold (30 ms budget) | 32.3 ms | 32.1 ms |
| compile 64-op cold (30 ms budget) | 43.4 ms | 42.7 ms |
| compile 64-op warm (cache hit) | 43.2 ms (budget-dominated outliers) | 211.5 µs |
| execute 64-op plan | 224.5 µs | 217.9 µs |
| effective throughput | — | 3507.9 ns/op |

The cold numbers are dominated by the autotune time budget itself (search
phases consume the allocation); the warm cache-hit floor of 211.5 µs is the
staging-plus-lowering cost the budget does not cover. The staged-input change
adds one loop iteration per extra parameter to staging and one arena fill per
parameter to evaluation — inside run-to-run noise on the execution path
(217.9 µs min vs the 218.1 µs recorded for the two-input pipeline before this
change).

## Phase 2 acquisition — CEP-351 Gaussian-process Expected Improvement

Phase 2 previously ranked candidates by the ridge model's predicted latency
minus one global uncertainty bonus (`residual_std`), so an uncertain region
and a certain one with the same residual spread scored identically. CEP-351
fits a Gaussian process (RBF kernel, median-heuristic lengthscale, relative
nugget, packed Cholesky solve — `mlkp-autotune/src/gp.rs`, std-only and
deterministic) on the session's observed (features, latency) pairs and ranks
by Expected Improvement against the incumbent: promising candidates (low
posterior mean) and unexplored candidates (high posterior variance) both
rank high, per candidate rather than globally. Below `GP_MIN_FIT_POINTS`
observations the phase falls back to the previous LCB scoring.

Evidence:

- `gp_fits_and_predicts_held_out_points` (JIT-g31): held-out interpolation
  within 30% of the true function on a quadratic, positive posterior variance,
  and extrapolation variance more than 10x interpolation variance.
- `expected_improvement_properties_hold` (JIT-g37): EI is zero for a certain
  candidate at or worse than the incumbent, grows with sigma, and declines
  with mean latency.
- `phase2_gp_finds_the_landscape_optimum` (JIT-g39): over a synthetic
  landscape with a known optimum, the four-phase search evaluates through the
  EI ranking (`phase_evals[1] > 0`) and returns exactly the argmin of the
  space plus the baseline — the incumbent guarantee is intact.

Winner quality on the real evaluator (same source, same budget, one run each):

| Workload | LCB winner | GP-EI winner |
|---|---|---|
| 48^3 three-input chain (400 ms budget) | 9840 ns (tile 64x64x64, threads 2) | 9839 ns (tile 128x64x128, threads 1) |
| 96^2 synthetic (default budget) | 36410 ns | 35905 ns (-1.4%) |

Compile-path cost is unchanged inside noise: cold 64-op 42.7 ms (was 43.4 ms,
budget-dominated), warm cache-hit floor 218.2 us (was 211.5 us), execute
64-op 217.8 us min and 3470 ns/op effective (was 3508). The GP fit is
O(n^3) over at most `GP_MAX_FIT_POINTS` (160) observations per Phase 2
round, re-fit per batch from the session's accumulated observations.

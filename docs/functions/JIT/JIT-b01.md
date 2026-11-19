# JIT-b01

`time_it`

## Tier 1: Beginner

`time_it` times one scenario.

Inputs and outputs are documented in the signature; the function does not fail.

Context: A median over repeated runs with a warmup discards cold-start noise and one-off scheduler spikes.

## Tier 2: Silicon Reality

Declared cost: O(iters) scenario executions.

Declared preconditions: the closure's work dominates the timing overhead.

Implementation: `crates/mlkp/benches/pipeline.rs` at line 21, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the closure's work dominates the timing overhead..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

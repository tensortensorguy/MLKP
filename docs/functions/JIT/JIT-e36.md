# JIT-e36

`median_of`

## Tier 1: Beginner

`median_of` times one closure over repeated runs and returns the median in microseconds.

Inputs and outputs are documented in the signature; the function does not fail.

Context: A median over repeated runs with warmup discards cold-start noise and scheduler spikes; the profile's numbers must be stable enough to optimize against.

## Tier 2: Silicon Reality

Declared cost: O(iters) executions.

Declared preconditions: the closure's work dominates the timing overhead.

Implementation: `crates/mlkp-runtime/examples/exec_profile.rs` at line 51, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the closure's work dominates the timing overhead..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

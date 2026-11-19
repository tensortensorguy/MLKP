# JIT-x48

`measure`

## Tier 1: Beginner

`measure` measures one closure.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Every candidate evaluation and every bench artifact flows through this single discipline.

## Tier 2: Silicon Reality

Declared cost: O(runs * closure).

Declared preconditions: the closure is deterministic in output; each run must leave the arena so the next run measures the same work.

Implementation: `crates/mlkp-runtime/src/timing.rs` at line 107, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the closure is deterministic in output; each run must leave the arena so the next run measures the same work..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

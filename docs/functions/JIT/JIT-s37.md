# JIT-s37

`mutate_around`

## Tier 1: Beginner

`mutate_around` generates distinct mutation neighbors of one candidate.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Phase 3 explores local neighborhoods of the incumbent (design Phase 3); distinctness keeps every mutant a fresh measurement.

## Tier 2: Silicon Reality

Declared cost: O(mutants).

Declared preconditions: none

Implementation: `crates/mlkp-autotune/src/search.rs` at line 405, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

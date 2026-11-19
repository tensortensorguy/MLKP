# JIT-a17

`config_key`

## Tier 1: Beginner

`config_key` deduplication key of one candidate's schedule.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The probe removal and mutant dedup need a stable, total key per schedule.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: none

Implementation: `crates/mlkp-autotune/src/search.rs` at line 366, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

# JIT-q72

`parse_stored_config`

## Tier 1: Beginner

`parse_stored_config` parses one stored schedule from a record; absent fields yield None.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Older cache files without the schedule must still load; warm start then falls back to the default schedule.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: none

Implementation: `crates/mlkp-autotune/src/pgo.rs` at line 418, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

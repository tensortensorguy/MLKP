# JIT-h14

`search_respects_budget`

## Tier 1: Beginner

`search_respects_budget` verifies the search returns a winner within budget and evaluates the baseline first.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The budget fallback is a design failure mode.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-autotune/tests/autotune_test.rs` at line 189, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..

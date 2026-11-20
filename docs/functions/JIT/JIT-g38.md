# JIT-g38

`expected_improvement_properties_hold`

## Tier 1: Beginner

`expected_improvement_properties_hold` verifies the Expected Improvement acquisition's mathematical properties: zero certainty improvement, monotone growth with uncertainty, and monotone decline with mean latency.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: EI drives Phase 2's exploration-exploitation balance (CEP-351); these three properties are the contract the search relies on, so a broken acquisition would corrupt the phase silently.

## Tier 2: Silicon Reality

Declared cost: cold; arithmetic only.

Declared preconditions: latencies are minimized.

Implementation: `crates/mlkp-autotune/tests/autotune_test.rs` at line 357, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: latencies are minimized..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..

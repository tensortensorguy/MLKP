# JIT-b67

`spill_rewrite_uses_slots_for_computed_values`

## Tier 1: Beginner

`spill_rewrite_uses_slots_for_computed_values` verifies computed spilled values get slot homes with folded memory operands, stores, and RMW preloads.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: Values defined by non-copy instructions cannot rematerialize; the rewrite must give them stack slots, fold their uses into rbp-relative memory operands, and route their definitions through stored temporaries (ticket CEP-323).

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-machine/tests/irc_test.rs` at line 120, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..

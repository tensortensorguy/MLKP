# JIT-w02

`fusion_collapses_double_negation`

## Tier 1: Beginner

`fusion_collapses_double_negation` verifies the fusion pass collapses a double negation: the outer neg is removed and its consumer reads the original input.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: Two IEEE sign flips are exactly the identity; the pair costs a whole kernel pass per execution and the pass deletes it.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-hir/tests/hir_test.rs` at line 259, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..

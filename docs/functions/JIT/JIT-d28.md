# JIT-d28

`long_branch_fixture`

## Tier 1: Beginner

`long_branch_fixture` tests the branch-size fixpoint.

Inputs, outputs, and errors: Panics via assert.

Context: The two-pass layout must grow forms exactly when the displacement overflows a byte.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-machine/tests/encode_test.rs` at line 255, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert..

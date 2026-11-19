# JIT-k54

`fixed_constraints_are_respected`

## Tier 1: Beginner

`fixed_constraints_are_respected` verifies fixed constraints are respected by the allocator and the verifier.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: ABI and instruction constraints are non-negotiable.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-machine/tests/irc_test.rs` at line 249, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..

# JIT-p01

`split3`

## Tier 1: Beginner

`split3` splits one arena slice into three disjoint views.

Inputs, outputs, and errors: Panics on overlap or out-of-range; the profile constructs disjoint ranges.

Context: The profile needs the same view derivation the dispatcher uses without reaching into its private helpers.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: none

Implementation: `crates/mlkp-runtime/examples/exec_profile.rs` at line 202, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics on overlap or out-of-range; the profile constructs disjoint ranges..

# JIT-i79

`build_obj_program`

## Tier 1: Beginner

`build_obj_program` builds the object fixture.

Inputs, outputs, and errors: Panics via expect.

Context: Shared by the structural and link tests.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-machine/tests/object_test.rs` at line 17, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via expect..

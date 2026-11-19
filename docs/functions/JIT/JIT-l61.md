# JIT-l61

`parse_type`

## Tier 1: Beginner

`parse_type` parses a type like f32[M,K].

Inputs, outputs, and errors: Returns an error for unknown element kinds or malformed brackets.

Context: Parameter and output declarations share one type grammar.

## Tier 2: Silicon Reality

Declared cost: O(dims).

Declared preconditions: none

Implementation: `crates/mlkp-front/src/lib.rs` at line 244, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns an error for unknown element kinds or malformed brackets..

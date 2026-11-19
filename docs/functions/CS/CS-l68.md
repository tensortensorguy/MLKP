# CS-l68

`write_value`

## Tier 1: Beginner

`write_value` recursive pretty writer with depth-based indentation.

Inputs, outputs, and errors: none; String writes do not fail.

Context: Human-diffable cache files make regressions reviewable.

## Tier 2: Silicon Reality

Declared cost: cold; O(size of value tree).

Declared preconditions: nesting depth is bounded by the value tree shape; cache values are shallow by construction.

Implementation: `crates/cep-json/src/write.rs` at line 42, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: nesting depth is bounded by the value tree shape; cache values are shallow by construction..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; String writes do not fail..

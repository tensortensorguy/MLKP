# JIT-b82

`dim`

## Tier 1: Beginner

`dim` interns a dimension symbol or parses a constant.

Inputs, outputs, and errors: Returns an error for names beyond the length limit.

Context: Shape symbols like M map to stable DimExpr ids reused across parameters.

## Tier 2: Silicon Reality

Declared cost: O(symbols).

Declared preconditions: none

Implementation: `crates/mlkp-front/src/lib.rs` at line 166, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns an error for names beyond the length limit..

# CS-x95

`array`

## Tier 1: Beginner

`array` parses a JSON array with strict comma placement.

Inputs, outputs, and errors: Returns ParseError on malformed syntax.

Context: Trailing commas are rejected to keep the grammar strict.

## Tier 2: Silicon Reality

Declared cost: cold; O(size of array).

Declared preconditions: none

Implementation: `crates/cep-json/src/parse.rs` at line 218, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns ParseError on malformed syntax..

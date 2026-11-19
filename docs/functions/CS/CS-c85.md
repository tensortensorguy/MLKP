# CS-c85

`string`

## Tier 1: Beginner

`string` parses a JSON string with full escape handling including surrogate pairs.

Inputs, outputs, and errors: Returns ParseError on unterminated strings, raw control bytes, or invalid escapes.

Context: Configuration paths and messages may contain escapes; control bytes are rejected so no invalid UTF-8 enters the value tree.

## Tier 2: Silicon Reality

Declared cost: cold; O(size of string).

Declared preconditions: none

Implementation: `crates/cep-json/src/parse.rs` at line 249, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns ParseError on unterminated strings, raw control bytes, or invalid escapes..

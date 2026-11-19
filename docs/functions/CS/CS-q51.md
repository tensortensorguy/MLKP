# CS-q51

`class`

## Tier 1: Beginner

`class` parses a character class with ranges, negation, and shorthand escapes.

Inputs, outputs, and errors: Returns Err on unterminated classes.

Context: Classes cover sets like [A-Z0-9-] used by rule patterns.

## Tier 2: Silicon Reality

Declared cost: cold; O(size of class).

Declared preconditions: none

Implementation: `tools/cep_lint/src/re.rs` at line 657, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns Err on unterminated classes..

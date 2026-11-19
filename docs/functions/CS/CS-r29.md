# CS-r29

`quantify`

## Tier 1: Beginner

`quantify` wraps an atom in a repeat construct if a quantifier follows.

Inputs, outputs, and errors: Returns Err on malformed braces.

Context: Quantifiers bind to the immediately preceding atom.

## Tier 2: Silicon Reality

Declared cost: cold; O(1).

Declared preconditions: none

Implementation: `tools/cep_lint/src/re.rs` at line 461, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns Err on malformed braces..

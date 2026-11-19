# CS-n76

`literal_value`

## Tier 1: Beginner

`literal_value` extracts the literal token starting at col with prefix, digits, underscores, and suffix.

Inputs, outputs, and errors: Returns None when no digits follow the start position.

Context: The token text is needed for numeric comparison and reporting.

## Tier 2: Silicon Reality

Declared cost: cold; O(token length).

Declared preconditions: none

Implementation: `tools/cep_lint/src/checks/numeric.rs` at line 107, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns None when no digits follow the start position..

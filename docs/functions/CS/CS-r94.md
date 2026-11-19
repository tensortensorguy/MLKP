# CS-r94

`from_value`

## Tier 1: Beginner

`from_value` builds a typed Config from a parsed JSON tree.

Inputs, outputs, and errors: Returns BootError codes 4, 5, 6, 7, or 8 for missing keys, wrong types, unknown primitives, bad severities, or invalid patterns.

Context: Schema validation is centralized so every rule sees an identical parameter contract.

## Tier 2: Silicon Reality

Declared cost: cold; O(config size).

Declared preconditions: none

Implementation: `tools/cep_lint/src/config.rs` at line 170, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns BootError codes 4, 5, 6, 7, or 8 for missing keys, wrong types, unknown primitives, bad severities, or invalid patterns..

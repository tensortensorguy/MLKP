# CS-p79

`atom`

## Tier 1: Beginner

`atom` parses one atom: escape, class, group, anchor, or literal char.

Inputs, outputs, and errors: Returns Err on unknown escapes or unterminated groups and classes.

Context: Atoms are the leaves of the pattern AST.

## Tier 2: Silicon Reality

Declared cost: cold; O(size of atom).

Declared preconditions: none

Implementation: `tools/cep_lint/src/re.rs` at line 561, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns Err on unknown escapes or unterminated groups and classes..

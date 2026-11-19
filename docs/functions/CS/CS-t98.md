# CS-t98

`find_preceding_block`

## Tier 1: Beginner

`find_preceding_block` locates the comment block preceding a function.

Inputs, outputs, and errors: Returns None when no acceptable block exists.

Context: Chapter 50.3: only attributes may separate block and definition; header blocks are never function blocks. Shared with the unsafe-documentation check so "enclosing block" has one meaning.

## Tier 2: Silicon Reality

Declared cost: cold; O(blocks near the function).

Declared preconditions: none

Implementation: `tools/cep_lint/src/checks/func_policy.rs` at line 264, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns None when no acceptable block exists..

# CS-n90

`find_fn_name`

## Tier 1: Beginner

`find_fn_name` finds `fn NAME` in one masked line, returning (start_col, end_col, name).

Inputs, outputs, and errors: Returns None when the line has no function definition.

Context: The extractor needs the name and its column for findings.

## Tier 2: Silicon Reality

Declared cost: cold; O(line length).

Declared preconditions: none

Implementation: `tools/cep_lint/src/checks/func_policy.rs` at line 139, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns None when the line has no function definition..

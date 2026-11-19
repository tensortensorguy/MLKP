# CS-b38

`extract_functions`

## Tier 1: Beginner

`extract_functions` extracts function definitions with statement counts from masked code.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Four rules share the extractor; one implementation keeps their semantics identical.

## Tier 2: Silicon Reality

Declared cost: cold; O(file size).

Declared preconditions: the first `{` after the signature opens the body; `fn` in pointer types has no identifier and is skipped.

Implementation: `tools/cep_lint/src/checks/func_policy.rs` at line 55, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the first `{` after the signature opens the body; `fn` in pointer types has no identifier and is skipped..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

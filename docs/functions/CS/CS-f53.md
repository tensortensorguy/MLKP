# CS-f53

`block_mode`

## Tier 1: Beginner

`block_mode` block mode: requires a CEP comment block before nontrivial functions.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Rule CEP-LINT-FUNC-BLOCK; attributes and blank lines may sit between block and signature per the gap pattern.

## Tier 2: Silicon Reality

Declared cost: cold; O(functions * blocks).

Declared preconditions: none

Implementation: `tools/cep_lint/src/checks/func_policy.rs` at line 196, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

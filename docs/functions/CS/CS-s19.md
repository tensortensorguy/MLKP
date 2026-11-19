# CS-s19

`registry_mode`

## Tier 1: Beginner

`registry_mode` checks registry entries against the documentation tree and the tree against the registry.

Inputs, outputs, and errors: Missing or malformed registry surfaces as a bootstrap error.

Context: Chapter 68.2.3 requires both directions so a stale registry and an orphan file are both defects.

## Tier 2: Silicon Reality

Declared cost: cold; O(tree size).

Declared preconditions: none

Implementation: `tools/cep_lint/src/checks/rosetta.rs` at line 37, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Missing or malformed registry surfaces as a bootstrap error..

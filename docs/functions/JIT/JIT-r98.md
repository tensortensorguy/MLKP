# JIT-r98

`cse_merges_duplicate_chains`

## Tier 1: Beginner

`cse_merges_duplicate_chains` verifies CSE merges duplicate chains and redirects consumers.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: Merging halves kernel work for duplicate subtrees.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-hir/tests/hir_test.rs` at line 149, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..

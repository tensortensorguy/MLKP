# CS-r54

`comment_regex`

## Tier 1: Beginner

`comment_regex` applies a configured pattern to comment text with an optional suppression pattern.

Inputs, outputs, and errors: none; invalid patterns are rejected at load.

Context: The comment-scoped rules (work markers, declared statuses, commented-out code) share one region and one semantics.

## Tier 2: Silicon Reality

Declared cost: cold; O(file size).

Declared preconditions: none

Implementation: `tools/cep_lint/src/checks/regex_checks.rs` at line 18, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; invalid patterns are rejected at load..

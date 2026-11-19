# CS-m79

`banned_token_policy`

## Tier 1: Beginner

`banned_token_policy` runs the banned-token rule over one scanned file.

Inputs, outputs, and errors: none; tokens found produce findings, missing headers are other rules' findings.

Context: Cycle-exact classes must be free of allocation and dispatch constructs; the class is read from the file header and only matching classes are token-scanned.

## Tier 2: Silicon Reality

Declared cost: cold; O(file size * tokens).

Declared preconditions: none

Implementation: `tools/cep_lint/src/checks/banned.rs` at line 17, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; tokens found produce findings, missing headers are other rules' findings..

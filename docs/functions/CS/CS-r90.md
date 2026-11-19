# CS-r90

`sign_equation_components`

## Tier 1: Beginner

`sign_equation_components` replicates the signing equation with internals and asserts every component.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: Public wrapper failures must be attributable to a specific component.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `tools/cep_lint/src/ed25519.rs` at line 1005, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..

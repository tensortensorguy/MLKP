# JIT-q42

`main`

## Tier 1: Beginner

`main` cLI entry point.

Inputs, outputs, and errors: Exit code 2 on compile failure; exit code 3 on usage errors.

Context: The lifecycle is externally observable.

## Tier 2: Silicon Reality

Declared cost: cold.

Declared preconditions: none

Implementation: `crates/mlkp/src/bin/mlkpc.rs` at line 26, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Exit code 2 on compile failure; exit code 3 on usage errors..

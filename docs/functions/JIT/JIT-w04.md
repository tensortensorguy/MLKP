# JIT-w04

`main`

## Tier 1: Beginner

`main` sweeps candidate schedules over the 64-op program and reports the first plan that fails the airlock.

Inputs, outputs, and errors: Prints the diagnosis and continues; the exit report counts the failures.

Context: The buffer-reuse pass must survive every schedule the search enumerates; the sweep turns a search-dependent failure into a deterministic reproduction.

## Tier 2: Silicon Reality

Declared cost: a few seconds.

Declared preconditions: cold-tool only.

Implementation: `crates/mlkp-runtime/examples/repro_reuse.rs` at line 44, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: cold-tool only..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Prints the diagnosis and continues; the exit report counts the failures..

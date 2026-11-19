# JIT-e24

`program`

## Tier 1: Beginner

`program` generates the 64-op mixed source program.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The profile must measure the same op mix the pipeline benchmark executes so their numbers are comparable.

## Tier 2: Silicon Reality

Declared cost: O(ops).

Declared preconditions: the front-end accepts the generated matmul and map chain on f32[64,64].

Implementation: `crates/mlkp-runtime/examples/exec_profile.rs` at line 21, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the front-end accepts the generated matmul and map chain on f32[64,64]..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

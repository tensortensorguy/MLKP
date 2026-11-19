# JIT-e01

`engine`

## Tier 1: Beginner

`engine` returns the cached engine kind for this host.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Feature detection must run once per process, not per kernel call; a relaxed atomic load is one mov and no lock, inside the CEP-0 budget.

## Tier 2: Silicon Reality

Declared cost: O(1), one atomic load after the first call.

Declared preconditions: the engine set is fixed for the life of the process.

Implementation: `crates/mlkp-runtime/src/simd/mod.rs` at line 70, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the engine set is fixed for the life of the process..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

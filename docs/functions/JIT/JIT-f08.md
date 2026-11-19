# JIT-f08

`matmul_epilogue`

## Tier 1: Beginner

`matmul_epilogue` decides the matmul epilogue fusion: the next op is a unary reading the matmul output with matching length, and the output buffer has no later reader.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Folding the consumer into the matmul's stores removes a full buffer pass; the read scan proves the skipped output write is never observed.

## Tier 2: Silicon Reality

Declared cost: O(remaining ops) for the read scan.

Declared preconditions: none; the guard is exhaustive.

Implementation: `crates/mlkp-runtime/src/exec.rs` at line 341, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none; the guard is exhaustive..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

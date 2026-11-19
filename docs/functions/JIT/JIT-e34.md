# JIT-e34

`fma_row1`

## Tier 1: Beginner

`fma_row1` multiplies one single row against one tile with the same accumulator structure reduced to one row.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Ragged row bounds (m not a multiple of four) must keep the vector engine, not fall to a scalar loop.

## Tier 2: Silicon Reality

Declared cost: 2 FMAs and 2 B loads per reduction step in the 16-column body.

Declared preconditions: AVX2 and FMA enabled; i within the row bound.

Implementation: `crates/mlkp-runtime/src/simd/matmul.rs` at line 357, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: AVX2 and FMA enabled; i within the row bound..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

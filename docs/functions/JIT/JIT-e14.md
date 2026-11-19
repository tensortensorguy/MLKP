# JIT-e14

`fma_rows4`

## Tier 1: Beginner

`fma_rows4` multiplies one four-row block against one tile: two 8-wide FMA accumulator pairs per row, 16 columns per iteration, then one 8-wide chunk, then a scalar column tail.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Four rows share each B load pair across four broadcast FMAs, which is the register-blocking shape that reaches FMA throughput at these tile sizes.

## Tier 2: Silicon Reality

Declared cost: 8 FMAs and 2 B loads per reduction step, plus per-row broadcasts.

Declared preconditions: AVX2 and FMA enabled; i + 4 <= the row bound; column and depth extents within the tile.

Implementation: `crates/mlkp-runtime/src/simd/matmul.rs` at line 223, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: AVX2 and FMA enabled; i + 4 <= the row bound; column and depth extents within the tile..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

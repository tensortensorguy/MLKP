# JIT-f04

`buffer_read_after`

## Tier 1: Beginner

`buffer_read_after` reports whether any op from index `from` onward reads one of the given buffer ids.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Both fusion decisions (unary chains and the matmul epilogue) reduce to this one exhaustive read scan.

## Tier 2: Silicon Reality

Declared cost: O(remaining ops); no allocation.

Declared preconditions: none

Implementation: `crates/mlkp-runtime/src/exec.rs` at line 180, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

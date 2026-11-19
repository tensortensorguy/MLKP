# JIT-u19

`push_mem`

## Tier 1: Beginner

`push_mem` encodes a memory operand.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Load and Store address spill slots and kernel buffers; the RBP and RSP special cases follow the ModRM tables exactly.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: base names a valid register; the displacement fits i32.

Implementation: `crates/mlkp-machine/src/encode/bytes.rs` at line 45, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: base names a valid register; the displacement fits i32..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

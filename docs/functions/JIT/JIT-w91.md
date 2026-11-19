# JIT-w91

`compile`

## Tier 1: Beginner

`compile` compiles one function to bytes.

Inputs, outputs, and errors: Returns the module errors; rejects a block named like the reserved epilogue label before any bytes exist.

Context: The design's emission stage: encoding with relocations, deterministic and executable.

## Tier 2: Silicon Reality

Declared cost: O(insns * fixpoint rounds) with a 64-round cap.

Declared preconditions: the allocation is verified; block labels are unique; no block is named "ret".

Implementation: `crates/mlkp-machine/src/encode/mod.rs` at line 109, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the allocation is verified; block labels are unique; no block is named "ret"..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns the module errors; rejects a block named like the reserved epilogue label before any bytes exist..

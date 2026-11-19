# JIT-h57

`decode_one`

## Tier 1: Beginner

`decode_one` decodes one instruction.

Inputs, outputs, and errors: Returns the module errors; every unhandled opcode is OutsideTier.

Context: The general decode walk over arbitrary tier machine code.

## Tier 2: Silicon Reality

Declared cost: O(1) per instruction.

Declared preconditions: none

Implementation: `crates/mlkp-machine/src/isa/decode.rs` at line 238, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns the module errors; every unhandled opcode is OutsideTier..

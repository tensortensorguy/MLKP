# JIT-x59

`disassemble`

## Tier 1: Beginner

`disassemble` disassembles one CodeObject.

Inputs, outputs, and errors: Returns the module errors.

Context: The standard's disassembly-hash evidence and the mechanical bytes-versus-text equivalence check.

## Tier 2: Silicon Reality

Declared cost: O(insns + labels).

Declared preconditions: labels cover every branch and internal call target; every label offset coincides with an instruction start or the epilogue.

Implementation: `crates/mlkp-machine/src/decode/mod.rs` at line 175, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: labels cover every branch and internal call target; every label offset coincides with an instruction start or the epilogue..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns the module errors..

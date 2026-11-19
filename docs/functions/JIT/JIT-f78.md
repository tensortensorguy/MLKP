# JIT-f78

`render_insn`

## Tier 1: Beginner

`render_insn` renders one instruction.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The disassembly text view; the assembler re-encodes canonical programs to identical bytes.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: none

Implementation: `crates/mlkp-machine/src/isa/render.rs` at line 75, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

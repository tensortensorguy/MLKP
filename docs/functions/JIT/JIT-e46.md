# JIT-e46

`assemble`

## Tier 1: Beginner

`assemble` assembles one text program.

Inputs, outputs, and errors: Returns the module errors; branch sizes settle within 64 rounds or the assembly fails.

Context: The tier pair's text entry: emit() text, renderer text, and hand-written programs all encode through one path.

## Tier 2: Silicon Reality

Declared cost: cold; O(text) parse plus O(insns) per fixpoint round.

Declared preconditions: the System V AMD64 ABI; branch targets are labels, symbols, or absolute byte offsets.

Implementation: `crates/mlkp-machine/src/isa/assemble/mod.rs` at line 60, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the System V AMD64 ABI; branch targets are labels, symbols, or absolute byte offsets..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns the module errors; branch sizes settle within 64 rounds or the assembly fails..

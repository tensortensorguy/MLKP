# JIT-l36

`emit_module`

## Tier 1: Beginner

`emit_module` emits one kernel to deterministic SPIR-V words.

Inputs, outputs, and errors: Returns EmitError after running the verifier first.

Context: The design's emitter contract: verified GPU Machine IR in, deterministic binary plus manifest metadata out, no external compiler.

## Tier 2: Silicon Reality

Declared cost: cold; O(insns).

Declared preconditions: none

Implementation: `crates/mlkp-spirv/src/emit/mod.rs` at line 87, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns EmitError after running the verifier first..

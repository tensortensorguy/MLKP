# JIT-z77

`verify_words`

## Tier 1: Beginner

`verify_words` verifies one serialized module.

Inputs, outputs, and errors: Returns EmitError::BadWords on the first structural violation.

Context: The design's ModuleVerifier must prove properties of the binary the driver receives, not only of the kernel IR; id discipline is what a driver trusts.

## Tier 2: Silicon Reality

Declared cost: O(words).

Declared preconditions: the module was produced by this emitter's opcode set.

Implementation: `crates/mlkp-spirv/src/emit/verify.rs` at line 100, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the module was produced by this emitter's opcode set..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns EmitError::BadWords on the first structural violation..

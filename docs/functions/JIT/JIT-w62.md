# JIT-w62

`verify_module`

## Tier 1: Beginner

`verify_module` verifies kernel structure.

Inputs, outputs, and errors: Returns EmitError for the first violation found per category.

Context: The design's ModuleVerifier must reject invalid output before it reaches the driver; it checks terminators, branch targets, bindings, duplicate result definitions, missing result ids, and undefined uses.

## Tier 2: Silicon Reality

Declared cost: O(insns).

Declared preconditions: none

Implementation: `crates/mlkp-spirv/src/emit/verify.rs` at line 16, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns EmitError for the first violation found per category..

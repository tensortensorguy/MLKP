# JIT-w66

`emit_object`

## Tier 1: Beginner

`emit_object` emits one ELF64 object.

Inputs, outputs, and errors: none; the function name and relocation symbols must be valid identifiers, which the front end guarantees.

Context: The design's machine code emission stage ends at object files the standard toolchain links.

## Tier 2: Silicon Reality

Declared cost: O(text + relocs + symbols).

Declared preconditions: one function per object; no timestamps, paths, or environment values are embedded (the emission policy forbids them).

Implementation: `crates/mlkp-machine/src/object.rs` at line 232, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: one function per object; no timestamps, paths, or environment values are embedded (the emission policy forbids them)..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; the function name and relocation symbols must be valid identifiers, which the front end guarantees..

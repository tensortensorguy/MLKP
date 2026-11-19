# JIT-f86

`emit_with_evidence`

## Tier 1: Beginner

`emit_with_evidence` emits one object with evidence.

Inputs, outputs, and errors: Returns the decoder's errors when the disassembly evidence cannot be produced.

Context: The emission policy demands object hash, section list, symbol table hash, relocation table hash, and disassembly hash as evidence.

## Tier 2: Silicon Reality

Declared cost: O(text + relocs + symbols) plus one disassembly pass.

Declared preconditions: see emit_object().

Implementation: `crates/mlkp-machine/src/object.rs` at line 376, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: see emit_object()..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns the decoder's errors when the disassembly evidence cannot be produced..

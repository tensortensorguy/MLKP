# JIT-x66

`parse_operand`

## Tier 1: Beginner

`parse_operand` parses one operand.

Inputs, outputs, and errors: IsaError::Parse with the line number.

Context: Immediates, registers, full memory forms, and bare symbols or absolute offsets.

## Tier 2: Silicon Reality

Declared cost: O(token).

Declared preconditions: the token is trimmed and split on top-level commas.

Implementation: `crates/mlkp-machine/src/isa/assemble/parse.rs` at line 191, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the token is trimmed and split on top-level commas..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: IsaError::Parse with the line number..

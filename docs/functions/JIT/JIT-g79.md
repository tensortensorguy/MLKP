# JIT-g79

`execute`

## Tier 1: Beginner

`execute` executes one compiled program.

Inputs, outputs, and errors: Returns CompilerError::Compile on execution failure.

Context: The installed fast path the JIT produces.

## Tier 2: Silicon Reality

Declared cost: the measured hot path.

Declared preconditions: the arena capacity covers the plan.

Implementation: `crates/mlkp/src/lib.rs` at line 122, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the arena capacity covers the plan..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns CompilerError::Compile on execution failure..

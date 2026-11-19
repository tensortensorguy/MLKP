# JIT-e35

`main`

## Tier 1: Beginner

`main` runs the execution profile: lowers the 64-op plan, times the full execution and the isolated kernel stages, and prints the split.

Inputs, outputs, and errors: Panics via expect on parse, lowering, or execution failure.

Context: The optimizer must attack the real time distribution, not guesses; this harness recorded the 93-percent matmul split that motivated the CEP-327 fast path.

## Tier 2: Silicon Reality

Declared cost: a few seconds; the harness itself is cold-tool.

Declared preconditions: run under release (cargo run --release) or the numbers are meaningless.

Implementation: `crates/mlkp-runtime/examples/exec_profile.rs` at line 73, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: run under release (cargo run --release) or the numbers are meaningless..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via expect on parse, lowering, or execution failure..

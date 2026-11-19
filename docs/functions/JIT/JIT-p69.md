# JIT-p69

`object_links_and_runs`

## Tier 1: Beginner

`object_links_and_runs` runs the link-and-run test.

Inputs, outputs, and errors: Panics when cc is missing, the link fails, or the output is wrong.

Context: Relocations, symbol resolution, and the System V ABI execute together only in a real binary.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: cc on PATH; the SysV ABI passes integer args in rdi, rsi and returns in rax.

Implementation: `crates/mlkp-machine/tests/object_test.rs` at line 197, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: cc on PATH; the SysV ABI passes integer args in rdi, rsi and returns in rax..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics when cc is missing, the link fails, or the output is wrong..

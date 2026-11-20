# JIT-y37

`compile`

## Tier 1: Beginner

`compile` compiles one program.

Inputs, outputs, and errors: Returns CompilerError per the pipeline stage.

Context: The single entry point for the CLI and the integration suite; the autotuner applies each candidate's pass pipeline over the verified source graph, so pass selection is a real search dimension.

## Tier 2: Silicon Reality

Declared cost: bounded by the autotune budget.

Declared preconditions: none

Implementation: `crates/mlkp/src/lib.rs` at line 67, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns CompilerError per the pipeline stage..

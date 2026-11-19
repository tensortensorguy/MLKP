# JIT-b89

`autotune`

## Tier 1: Beginner

`autotune` runs the JIT compilation lifecycle.

Inputs, outputs, and errors: Returns CompileError per the module contract.

Context: This is the design's core loop: observe, predict, search, measure, learn, repeat. The cache-hit path reuses the previous winner's schedule through hash reconstruction, and hot paths re-run the full search once the execution count crosses the trigger.

## Tier 2: Silicon Reality

Declared cost: bounded by the budget; the cache hit path is one lowering plus staging.

Declared preconditions: the graph is verified and fully concrete.

Implementation: `crates/mlkp-autotune/src/driver.rs` at line 86, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the graph is verified and fully concrete..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns CompileError per the module contract..

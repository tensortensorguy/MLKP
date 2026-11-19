# JIT-e46

`run_op`

## Tier 1: Beginner

`run_op` runs one scheduled operation.

Inputs, outputs, and errors: Returns ExecError per the module contract.

Context: The dispatch loop is a flat match so each kernel call stays direct and copy-free.

## Tier 2: Silicon Reality

Declared cost: O(kernel work); no allocation on any path.

Declared preconditions: the airlock proved disjointness for every operation; dispatch re-proves it per view.

Implementation: `crates/mlkp-runtime/src/exec.rs` at line 144, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the airlock proved disjointness for every operation; dispatch re-proves it per view..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns ExecError per the module contract..

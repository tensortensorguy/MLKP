# JIT-f01

`execute`

## Tier 1: Beginner

`execute` executes one kernel plan with exec-level fusion.

Inputs, outputs, and errors: Returns ExecError per the module contract.

Context: The dispatcher's lookahead fuses runs of consecutive unary ops into one pass (one load and one store per run) and folds a unary consumer into the matmul's stores when the matmul output has no later reader; both fusions are bit-identical to the unfused sequence, and the dispatch loop allocates nothing.

## Tier 2: Silicon Reality

Declared cost: O(kernel work); no allocation on any path.

Declared preconditions: the airlock passed; execution re-checks once and fails loudly otherwise; the fusion guards prove skipped buffers are never read again.

Implementation: `crates/mlkp-runtime/src/exec.rs` at line 49, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the airlock passed; execution re-checks once and fails loudly otherwise; the fusion guards prove skipped buffers are never read again..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns ExecError per the module contract..

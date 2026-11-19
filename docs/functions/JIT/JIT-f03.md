# JIT-f03

`chain_fusable`

## Tier 1: Beginner

`chain_fusable` proves a fused unary run safe: the chain's intermediate buffers (the destinations of every op but the last) are never read after the run.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Fusion skips writing intermediates entirely; a later reader would observe stale arena bytes, so the lookahead must prove exclusivity before fusing.

## Tier 2: Silicon Reality

Declared cost: O(intermediates * remaining ops), all in a fixed-size array.

Declared preconditions: none; the read scan is exhaustive over the remaining ops.

Implementation: `crates/mlkp-runtime/src/exec.rs` at line 155, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none; the read scan is exhaustive over the remaining ops..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

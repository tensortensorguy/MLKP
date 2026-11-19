# JIT-h93

`block_live_out`

## Tier 1: Beginner

`block_live_out` computes block liveness.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Interference is built from per-instruction live sets which need block-level live-out seeds.

## Tier 2: Silicon Reality

Declared cost: O(iterations * blocks * insns); the iteration count is bounded by the CFG depth.

Declared preconditions: successor labels are declared on blocks.

Implementation: `crates/mlkp-machine/src/irc/liveness.rs` at line 15, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: successor labels are declared on blocks..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

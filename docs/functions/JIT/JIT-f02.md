# JIT-f02

`chain_extent`

## Tier 1: Beginner

`chain_extent` extends one fused unary run forward from op index start while consecutive unaries read the previous run's destination and share its element count.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Chain fusion saves a load and a store per merged op; four is the register-resident composition depth that keeps the loop allocation-free.

## Tier 2: Silicon Reality

Declared cost: O(chain length).

Declared preconditions: plan.ops[start] is a Unary op (the dispatcher matches before calling).

Implementation: `crates/mlkp-runtime/src/exec.rs` at line 110, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: plan.ops[start] is a Unary op (the dispatcher matches before calling)..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

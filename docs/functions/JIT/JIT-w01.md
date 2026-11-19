# JIT-w01

`one_sweep`

## Tier 1: Beginner

`one_sweep` performs one fusion sweep: every Map(Neg) whose input is another Map(Neg) has its consumers retargeted to the inner input and is removed.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Two negations compose to the identity exactly; the pair costs two kernel passes for nothing.

## Tier 2: Silicon Reality

Declared cost: O(nodes).

Declared preconditions: none

Implementation: `crates/mlkp-hir/src/passes/fusion.rs` at line 41, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

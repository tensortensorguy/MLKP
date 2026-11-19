# JIT-v76

`layer4_similarity`

## Tier 1: Beginner

`layer4_similarity` applies Layer 4.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Candidates within epsilon of an evaluated one inherit that neighbor's interpolated result instead of consuming a fresh evaluation; the surviving list is what later layers and the search see (design Layer 4 and fix 3).

## Tier 2: Silicon Reality

Declared cost: O(candidates * evaluated).

Declared preconditions: the epsilon comes from the named configuration constant.

Implementation: `crates/mlkp-autotune/src/pruner.rs` at line 232, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the epsilon comes from the named configuration constant..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

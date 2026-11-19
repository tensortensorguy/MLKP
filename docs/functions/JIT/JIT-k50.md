# JIT-k50

`insn_size`

## Tier 1: Beginner

`insn_size` computes one instruction size.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The fixpoint and the emit pass must agree exactly; a mismatch would misplace every later branch.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: the shape passed validation.

Implementation: `crates/mlkp-machine/src/encode/bytes.rs` at line 198, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the shape passed validation..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

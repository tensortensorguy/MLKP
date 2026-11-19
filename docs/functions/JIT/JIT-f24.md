# JIT-f24

`render`

## Tier 1: Beginner

`render` renders one decoded instruction.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The bytes-versus-text equivalence check compares this output against emit().

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: the target table maps ids to the same names emit() uses.

Implementation: `crates/mlkp-machine/src/decode/render.rs` at line 16, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the target table maps ids to the same names emit() uses..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

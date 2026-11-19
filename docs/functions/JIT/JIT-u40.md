# JIT-u40

`has_cycle`

## Tier 1: Beginner

`has_cycle` detects cycles with an iterative depth-first search and three-color marking.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Unbounded recursion is banned by Law 6; the iterative stack bounds memory by the node count.

## Tier 2: Silicon Reality

Declared cost: O(nodes + edges).

Declared preconditions: none

Implementation: `crates/mlkp-hir/src/verify.rs` at line 167, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

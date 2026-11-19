# JIT-f17

`alloc_ids`

## Tier 1: Beginner

`alloc_ids` allocates every SPIR-V id deterministically in one pass.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Every id in the module must be below the header bound and allocated from one counter; synthetic arithmetic on ids would leak ids past the bound and collide.

## Tier 2: Silicon Reality

Declared cost: O(insns).

Declared preconditions: the kernel passed verify_module.

Implementation: `crates/mlkp-spirv/src/emit/ids.rs` at line 39, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the kernel passed verify_module..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

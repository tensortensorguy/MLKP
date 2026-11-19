# JIT-b03

`isa_corpus`

## Tier 1: Beginner

`isa_corpus` supplies the ISA corpus.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The tier benchmark must measure the same instruction mix the tests verify, every family and form.

## Tier 2: Silicon Reality

Declared cost: cold.

Declared preconditions: none

Implementation: `crates/mlkp/benches/pipeline.rs` at line 80, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

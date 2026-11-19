# JIT-q36

`parse_mnemonic`

## Tier 1: Beginner

`parse_mnemonic` resolves a mnemonic.

Inputs, outputs, and errors: IsaError::Parse for mnemonics outside the tier.

Context: AT&T suffixes and the two-suffix move grid encode the widths.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: none

Implementation: `crates/mlkp-machine/src/isa/assemble/parse.rs` at line 48, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: IsaError::Parse for mnemonics outside the tier..

# JIT-s47

`parse_prefixes`

## Tier 1: Beginner

`parse_prefixes` parses prefixes.

Inputs, outputs, and errors: OutsideTier for address-size, segment, or lock prefixes and for a second REX; Truncated at end of input.

Context: 66, F2, F3, and REX select widths and register extensions.

## Tier 2: Silicon Reality

Declared cost: O(prefixes).

Declared preconditions: none

Implementation: `crates/mlkp-machine/src/isa/decode.rs` at line 116, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: OutsideTier for address-size, segment, or lock prefixes and for a second REX; Truncated at end of input..

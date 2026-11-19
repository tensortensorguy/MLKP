# JIT-k57

`parse_statement`

## Tier 1: Beginner

`parse_statement` parses one assignment statement.

Inputs, outputs, and errors: Returns ParseError for malformed statements, unknown operations, or unknown operands.

Context: Statements are the only statement-level grammar production.

## Tier 2: Silicon Reality

Declared cost: O(1) per statement.

Declared preconditions: none

Implementation: `crates/mlkp-front/src/lib.rs` at line 568, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns ParseError for malformed statements, unknown operations, or unknown operands..

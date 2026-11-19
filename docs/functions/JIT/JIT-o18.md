# JIT-o18

`infer_result_type`

## Tier 1: Beginner

`infer_result_type` infers the result type of one statement.

Inputs, outputs, and errors: Returns ParseError for arity mismatches or unknown operand shapes.

Context: The heterogeneous IR carries types; matmul and elementwise chains have simple shape functions here.

## Tier 2: Silicon Reality

Declared cost: O(operands).

Declared preconditions: none

Implementation: `crates/mlkp-front/src/lib.rs` at line 686, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns ParseError for arity mismatches or unknown operand shapes..

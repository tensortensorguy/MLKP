# JIT-p73

`multi_input_program_stages_every_declared_parameter`

## Tier 1: Beginner

`multi_input_program_stages_every_declared_parameter` verifies a three-parameter program stages, compiles, and executes with every declared input, and that arity and length violations fail loudly.

Inputs, outputs, and errors: Panics via expect or assert on failure.

Context: CEP-324 closes the two-operand facade contract: plans alias one buffer per declared parameter in order, so a three-input program must stage all three buffers and reject mismatched staging.

## Tier 2: Silicon Reality

Declared cost: cold; bounded by a 2.5 second budget.

Declared preconditions: elementwise tolerance holds for the naive f32 reference of the two-matmul chain.

Implementation: `crates/mlkp/tests/lifecycle_test.rs` at line 193, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: elementwise tolerance holds for the naive f32 reference of the two-matmul chain..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via expect or assert on failure..

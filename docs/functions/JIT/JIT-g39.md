# JIT-g39

`phase2_gp_finds_the_landscape_optimum`

## Tier 1: Beginner

`phase2_gp_finds_the_landscape_optimum` runs the four-phase search over a synthetic landscape with a known optimum and verifies the GP-backed Phase 2 evaluates candidates and finds the true argmin.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The acquisition is wired into the real search loop: Phase 2 must spend its budget through the EI ranking (phase_evals[1] > 0) and the session must land on the best configuration the space contains, proving the GP path neither degrades nor breaks the incumbent guarantee.

## Tier 2: Silicon Reality

Declared cost: cold; the evaluator is arithmetic only, bounded by a generous wall budget.

Declared preconditions: the deterministic evaluator makes every evaluated candidate's objective exact.

Implementation: `crates/mlkp-autotune/tests/autotune_test.rs` at line 384, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the deterministic evaluator makes every evaluated candidate's objective exact..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..

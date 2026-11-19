# JIT-o90

`search`

## Tier 1: Beginner

`search` runs the four-phase search under a budget.

Inputs, outputs, and errors: Returns the best-so-far at budget expiry; never panics.

Context: The phases implement design section VI; the context object keeps the evaluator and keys together so the signature stays readable.

## Tier 2: Silicon Reality

Declared cost: dominated by evaluator time.

Declared preconditions: the candidate list is already pruned and the baseline is included in the context.

Implementation: `crates/mlkp-autotune/src/search.rs` at line 104, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the candidate list is already pruned and the baseline is included in the context..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns the best-so-far at budget expiry; never panics..

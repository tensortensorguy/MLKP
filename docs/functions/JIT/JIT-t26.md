# JIT-t26

`lower`

## Tier 1: Beginner

`lower` lowers one graph into a kernel plan.

Inputs, outputs, and errors: Returns PlanError on arity mismatch, non-concrete shapes, or unavailable device placements.

Context: This is the backend compilation step the design's Stage 5 names; the config parameterization is what the search space varies.

## Tier 2: Silicon Reality

Declared cost: O(nodes).

Declared preconditions: the graph passes the verifier first.

Implementation: `crates/mlkp-runtime/src/plan.rs` at line 243, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the graph passes the verifier first..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns PlanError on arity mismatch, non-concrete shapes, or unavailable device placements..

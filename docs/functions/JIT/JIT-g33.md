# JIT-g33

`expected_improvement`

## Tier 1: Beginner

`expected_improvement` computes the Expected Improvement of one posterior at the incumbent best.

Inputs, outputs, and errors: none; sigma is floored so the division stays finite.

Context: EI is the one-step Bayesian-optimal acquisition for minimization: it rises with both promise (low mean) and uncertainty (high sigma), which is exactly the Phase 2 exploration-exploitation balance the design names.

## Tier 2: Silicon Reality

Declared cost: O(1); the error function is the Abramowitz-Stegun 7.1.26 rational approximation (absolute error below 1.5e-7).

Declared preconditions: latencies are minimized; best is the incumbent's measured latency.

Implementation: `crates/mlkp-autotune/src/gp.rs` at line 194, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: latencies are minimized; best is the incumbent's measured latency..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; sigma is floored so the division stays finite..

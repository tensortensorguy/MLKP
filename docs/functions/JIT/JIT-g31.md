# JIT-g31

`fit`

## Tier 1: Beginner

`fit` fits the GP on observed (features, latency) pairs.

Inputs, outputs, and errors: Returns None below GP_MIN_FIT_POINTS or when the kernel stays non-positive-definite after three jitter doublings; never panics.

Context: Phase 2's acquisition must be backed by all observations gathered so far, so the fit uses the most recent GP_MAX_FIT_POINTS points and derives the lengthscale from the median pairwise distance (deterministic, no hyperparameter search).

## Tier 2: Silicon Reality

Declared cost: O(n^2) kernel build and median distance, O(n^3) Cholesky, capped by GP_MAX_FIT_POINTS.

Declared preconditions: targets are finite; duplicate points are allowed (the nugget keeps the kernel PD).

Implementation: `crates/mlkp-autotune/src/gp.rs` at line 39, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: targets are finite; duplicate points are allowed (the nugget keeps the kernel PD)..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns None below GP_MIN_FIT_POINTS or when the kernel stays non-positive-definite after three jitter doublings; never panics..

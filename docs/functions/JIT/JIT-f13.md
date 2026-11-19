# JIT-f13

`assign_offsets_with_reuse`

## Tier 1: Beginner

`assign_offsets_with_reuse` the buffer-reuse pass: computes each buffer's first-write and last-read time from the fused execution schedule (what the dispatcher really touches, including epilogue writes at the matmul's index and chain steps that skip intermediates), then assigns arena ranges so a buffer whose readers are all done shares its range with a later buffer, appending only when no freed range fits.

Inputs, outputs, and errors: none; liveness comes from the dispatcher's own grouping (exec::fused_touches), and ranges are shared only when the previous buffer's last read is strictly before the new buffer's first write, so the executor's per-step disjoint views stay disjoint.

Context: The memory objective is the arena footprint; dead intermediates dominate it on deep chains, and sharing their ranges compacts the arena without any timed-window copies. Planning against the raw plan ops would collide with fused steps, which write consumer destinations one or more ops early.

## Tier 2: Silicon Reality

Declared cost: O(steps + buffers^2) worst case for the first-fit scan; buffers in the hundreds.

Declared preconditions: fused_touches mirrors the dispatcher exactly; inputs and declared outputs stay live for the whole execution (their ranges are never reused as targets); every non-input buffer is written by at most one step.

Implementation: `crates/mlkp-runtime/src/plan.rs` at line 422, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: fused_touches mirrors the dispatcher exactly; inputs and declared outputs stay live for the whole execution (their ranges are never reused as targets); every non-input buffer is written by at most one step..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; liveness comes from the dispatcher's own grouping (exec::fused_touches), and ranges are shared only when the previous buffer's last read is strictly before the new buffer's first write, so the executor's per-step disjoint views stay disjoint..

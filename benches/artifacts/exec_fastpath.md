<!-- CEP:FILE: benches/artifacts/exec_fastpath.md -->
<!-- CEP:WHAT: The execution fast-path artifact: the measured before/after split of the 64-op plan execution and the four optimizations that produced the 12.5x speedup, with the correctness proof for each. -->
<!-- CEP:WHY: The design's measurement discipline requires that optimization claims rest on recorded numbers and proven-equivalent kernels; this artifact is the evidence for ticket CEP-327. -->
<!-- CEP:CLASS: CEP-1 -->
<!-- CEP:STATUS: complete -->
<!-- CEP:FAILURE: none -->
<!-- CEP:ASSUMES: captured on the shared Linux sandbox (release profile, 2026-11-19); absolute values vary with the host, ratios and the split are the meaningful part. -->
<!-- CEP:COST: the profile harness runs in a few seconds. -->
<!-- CEP:EVIDENCE: cargo run --release -p mlkp-runtime --example exec_profile and cargo bench -p mlkp --bench pipeline. -->

# Execution fast path (CEP-327)

## Before / after

| Metric | Before | After | Speedup |
| --- | ---: | ---: | ---: |
| full 64-op exec | 3247.6 µs | 237.1 µs (profile) / 257.4 µs (bench median) | 12.5–13.7x |
| effective per op | 50.7 µs/op | **3.7 µs/op (profile) / 4.0 µs/op (bench)** | 12.5x |
| matmul 64x64x64, one instance | 231.3 µs | 10.1 µs | 22.9x |
| elementwise exp, 4096 elements | ~4.7 µs (inferred from split) | 2.2 µs measured | ~2x |
| plan.validate() per exec | 2.4 µs | 2.4 µs | 1.0x |

The profile split showed the scalar blocked matmul was 93 percent of execution (13 instances x 231 µs = 3007 µs of 3248 µs); the remaining ~240 µs was the 51 scalar elementwise passes. Both were attacked.

## The four optimizations

**Register-blocked FMA matrix multiply** (crates/mlkp-runtime/src/simd.rs). Four-row blocks with two 8-wide FMA accumulator pairs per row amortize each B load pair across four FMA chains and reach the FMA engine's throughput; the schedule's tile sizes and loop-order dimensions keep their meaning inside the vector traversal (the tile kernel runs per schedule tile). The strided-B schedule keeps the blocked scalar traversal, so search dimension F stays real. Scalar tails cover every ragged extent; the pre-AVX2 fallback is the blocked scalar kernel, which the autotuner measures honestly on such hosts.

**The shared exp polynomial on every path** (no libm anywhere). A degree-6 minimax polynomial fitted in scripts/fit_exp_poly.py (relative error 1.25e-7 vs libm over a 400k-point sweep, exact exp(0) = 1.0) runs in three implementations of one algorithm: AVX2 with FMA, SSE2 without FMA, and the scalar tails — no slow path exists on any host or any tail. Saturation matches IEEE: k = 128 scales by 2^127 and doubles once (overflowing to inf exactly where libm does), k beyond saturates to inf, underflow flushes below 2^-126 (absolute error under 1e-37, inside the plan's absolute-tolerance contract), NaN propagates. Sigmoid composes the same exp with one IEEE divide.

**Vectorized elementwise, reduce, and softmax.** Unary, binary, and fused-chain passes run 8-wide with scalar tails; the last-axis reduce keeps four independent accumulator vectors and a horizontal combine (sum and max); softmax runs max, exp, and normalize passes 8-wide with the sum accumulated in vectors. Rows shorter than 16 elements run the scalar kernel, which is the fast path at that size.

**Exec-level fusion with an exhaustive safety proof** (crates/mlkp-runtime/src/exec.rs). The dispatcher's lookahead composes runs of consecutive unary ops (up to four) into one pass — one load and one store per run instead of one per op — and folds a unary consumer into the matmul's stores when the matmul output has no later reader, writing the consumer's destination directly and skipping the intermediate buffer entirely. Both fusions apply the same transforms in the same order per element, so they are bit-identical to the unfused sequence; the guard is an exhaustive read scan over the remaining ops, which refuses fusion on fan-out (a test proves the refusal path). The dispatch loop allocates nothing: chains live in a fixed `[EwUnary; 4]`.

## Correctness proof

- `matmul_fma_matches_reference`: the vector kernel vs the textbook loop across ragged sizes (13x17x11, 100x37x53, 5x130x7, 9x9x9), tile shapes, and both loop orders, tolerance 1e-3.
- `exp_poly_matches_libm_within_contract`: 400k-point sweep, relative error < 5e-7 above 1e-30, absolute error < 1e-35 below, IEEE saturation boundaries exact.
- `matmul_epilogue_is_bit_exact` and `unary_chain_is_bit_exact`: fused vs unfused bit-identical.
- `exec_fusion_matches_manual_composition`: the dispatcher's fused execution vs the manual kernel composition, bit-identical.
- `exec_refuses_fusion_on_fanout`: the read scan refuses fusion when an intermediate has a second consumer; the unfused result is bit-identical to the reference.
- All 123 pre-existing workspace tests pass unchanged on the new engine (tolerance-based contracts hold).

## Where the time goes now

The 64-op profile: 13 FMA matmuls ≈ 131 µs, the fused unary passes ≈ 60 µs, validate + dispatch ≈ 10 µs. The next lever, if one is ever needed, is packing the matmul's A operand per tile (currently broadcast loads), which the profile shows is worth roughly another 2x on the matmul alone.

## Pass pipeline addendum (CEP-328)

Three more passes joined the pipeline after the fast path landed:

- **Buffer reuse (memory pass)**: liveness is computed from the fused execution schedule (`exec::fused_touches`, kept next to the dispatcher's own grouping so the two cannot drift), because fused steps write consumer destinations one or more ops earlier than the raw plan ops say. Dead ranges are reassigned first-fit; never-touched intermediates are parked inside touched ranges they never co-occur with (checked by assigned ranges, since touched slots are time-shared). The 64-op plan's arena footprint drops 42 percent (270336 to 155648 elements) with latency unchanged (258.9 µs bench median vs 257.3 µs before; 3.7 µs/op in the profile), and `total_elements` now reports the real arena footprint.
- **fused_elementwise as a real dimension**: exec fusion is gated by the plan config, and the space enumerates both sides; candidates measure fused vs unfused execution honestly.
- **HIR fusion pass**: double-negation collapse (IEEE sign-flip is exactly self-inverse), a fourth PassSet dimension applied per candidate.

Correctness: `fusion_dimension_gates_exec_fusion` and `buffer_reuse_pass_compacts_arena` (bit-identical outputs, footprint strictly smaller, airlock green), `fusion_collapses_double_negation` (HIR), `space_covers_fusion_and_reuse_dimensions`; and a deterministic sweep harness (`examples/repro_reuse.rs`) lowers the 64-op program under every candidate schedule class and reports zero airlock failures — the sweep caught two real liveness holes (fused-step write timing, and time-shared slot parking) before they could ship.

## Elementwise ILP addendum (CEP-329)

- **Four vectors in flight**: the unary and fused-chain AVX2 bodies process four 8-wide vectors per iteration (was one). The exp polynomial's Horner form is seven serial FMAs — about thirty cycles per vector — and the one-vector loop exposed that latency on every pass. The interleave hides it: the 64-op pipeline drops from 258.9 to 229.9 µs median (4046 to 3593 ns/op effective, minus 11 percent), with per-element arithmetic and per-element k order unchanged, so results are bit-identical to the one-vector loop.
- **Matmul at the issue peak — measurement closed the packing direction**: the six-row register block (twelve chains, eight loads per twelve FMAs) measured 11.4 µs and explicit memory-broadcast loads 12.0 µs against the four-row kernel's 10.1 µs. Both shapes predict two FMAs per cycle on a two-FMA-port machine; the measured ceiling is one FMA per cycle (262144 MACs in 32768 FMA instructions at 10.1 µs), so this vCPU issues one FMA per cycle and the four-row kernel already sits at that peak. The earlier packing estimate (~2x) assumed two ports and is corrected here: **the matmul microkernel is at this host's ceiling, and the remaining matmul headroom is zero without a different instruction mix.**
- Verified by the full suite (135 tests, bit-exactness proofs included) and the standard gates.

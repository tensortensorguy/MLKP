# Tickets

Open work items referenced by `CEP:TODO` markers and `CEP:TICKET` fields. Owner for all items: ttg.

| Ticket | Component | Summary |
|---|---|---|
| CEP-210 | spirv/runtime | Vulkan device enumeration, pipeline creation, and driver-level execution; probe currently reports loader presence only. |
| CEP-320 | runtime | Closed: thread-parallel plan execution honors `PassConfig.threads` with disjoint output-row chunks. |
| CEP-321 | runtime | Closed: the transposed-B access pattern is a schedule variant of the tiled matmul kernel (access order, bit-identical results). |
| CEP-322 | machine | Closed: byte-level x86-64 encoding with a branch-size fixpoint and call relocations, ELF64 relocatable object emission with evidence hashes, and a subset disassembler; bytes match GNU as output and execute, objects link and run with cc. |
| CEP-323 | machine | Pressure-driven block-local spill decisions; spill rewriting, rematerialization, and split-at-spill temporaries are implemented. |
| CEP-324 | runtime | Multi-input plan staging beyond the two-operand facade contract. |
| CEP-325 | machine | Closed: the complete ISA-tier pair — a general decoder for the scalar tier in every width, prefix, ModRM/SIB/RIP-relative and no-base form with loud rejection outside the tier, an AT&T renderer, and an AT&T assembler whose bytes match GNU as on the conformance corpus and whose text round-trips through decode byte for byte. |
| CEP-330 | autotune | RAPL-based power measurement replacing the documented estimate model. |
| CEP-340 | cep_lint | Git-diff cross-reference of modified functions against signed receipt targets. |
| CEP-350 | autotune | Graph-neural-network cost model surrogate replacing the linear ridge model. |
| CEP-351 | autotune | Gaussian-process acquisition function for Phase 2. |
| CEP-360 | tensor | Data-dependent dimensions and conditional shape solving. |
| CEP-327 | runtime | Closed: the execution fast path — a register-blocked AVX2+FMA matmul honoring the schedule's tile and loop-order dimensions, one minimax exp polynomial shared by AVX2, SSE2, and scalar-tail paths (no libm anywhere, IEEE saturation boundaries matched), vectorized unary/binary/chain/reduce/softmax kernels, and exec-level fusion of unary chains and matmul epilogues behind exhaustive read-scan proofs; 64-op plan execution 3247.6 µs to 257.4 µs (50.7 to 4.0 µs/op effective, 12.5x), bit-exact fusion equivalence and 400k-point polynomial accuracy asserted in tests; recorded in benches/artifacts/exec_fastpath.md. |
| CEP-328 | passes | Closed: three more compiler passes — (1) the buffer-reuse pass: liveness computed from the fused execution schedule (exec::fused_touches, one source of truth with the dispatcher) assigns dead ranges to later buffers and parks never-touched intermediates inside non-co-occurring ranges; the 64-op plan's arena footprint drops 42 percent (270336 to 155648 elements) at unchanged latency, and the memory objective now reflects the real arena; (2) the fused_elementwise dimension wired as a real measured search dimension (exec fusion gated by plan config, both sides enumerated and hashed); (3) the HIR elementwise fusion pass (double-negation collapse, bit-identical under IEEE sign-flip) as a fourth PassSet dimension. Search space grew to cover both sides of each new dimension; StoredConfig schema extended for exact warm-start reconstruction. |
| CEP-329 | kernels | Closed: the elementwise engine runs four AVX2 vectors in flight per iteration in the unary and fused-chain passes (was one), hiding the exp polynomial's serial Horner latency that dominated the elementwise passes; the 64-op pipeline drops from 258.9 to 229.9 microseconds median (4046 to 3593 ns/op effective, minus 11 percent) with per-element arithmetic unchanged, so every result is bit-identical. Measurement also closed the matmul direction: the six-row register block and explicit broadcast loads both measured slower (11.4 to 12.0 vs 10.1 microseconds), which pins this host's engine at one FMA port — the four-row kernel already runs at its issue peak, and the artifact's packing note is corrected accordingly. |

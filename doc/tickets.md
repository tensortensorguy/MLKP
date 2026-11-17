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
| CEP-330 | autotune | RAPL-based power measurement replacing the documented estimate model. |
| CEP-340 | cep_lint | Git-diff cross-reference of modified functions against signed receipt targets. |
| CEP-350 | autotune | Graph-neural-network cost model surrogate replacing the linear ridge model. |
| CEP-351 | autotune | Gaussian-process acquisition function for Phase 2. |
| CEP-360 | tensor | Data-dependent dimensions and conditional shape solving. |

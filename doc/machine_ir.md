# Machine IR and IRC Allocation

## Abstract
The machine backend lowers scheduled kernels to a target-specific instruction form with virtual registers, runs liveness and interference analysis, allocates physical registers with iterated register coalescing, verifies the allocation, and emits deterministic AT&T-style assembly text.

## Formal Definitions
```rust
pub struct MInsn { op: Opcode, ops: Vec<Operand>, implicit_uses: Vec<u32>, implicit_defs: Vec<u32>, fixed: Vec<(usize, u32)> }
pub enum Operand { VReg{id, class, def}, PReg{..}, Imm(i64), Mem{..}, Block(..), Symbol(..) }
pub struct Allocation { assignment: Vec<(u32, u32)>, spilled: Vec<u32>, spill_slots: u32, coalesced_copies: u32 }
```

## Algorithm
Block liveness is a bounded backward dataflow to fixed point, with successor live-in computed by a reverse in-block scan. Interference connects each def to everything live after the instruction, plus fixed-register constraints, and records implicit-def pregs as blocked registers for every value live across the instruction. IRC maintains current degrees during simplification, evaluates George's criterion (every neighbor of the copy source is low-degree, already adjacent to the target, or precolored) and Briggs' criterion (same class; fewer than K high-degree merged neighbors) before merging non-interfering copy pairs, freezes move-related high-degree nodes, and deletes copies whose operands merged; a potential spill that receives a color at select time is not spilled. Selection assigns precolored nodes first so ordinary nodes see their fixed colors, then the first free color avoiding neighbor and blocked registers in target order. Emission routes every return through the shared epilogue, raises an unresolved-spill error instead of rendering a spilled value into a register operand, and aligns the spill area to the 16-byte ABI boundary.

## Invariants
1. Interfering live values never share a physical register; the verifier re-derives interference post-allocation and rejects overlaps.
2. Fixed constraints are respected exactly; the verifier compares assigned registers against every `(operand, preg)` pair.
3. Spilled vregs hold the `u32::MAX` sentinel assignment, never a register.
4. Emission is deterministic: two runs over one allocation produce byte-identical text.
5. The target describes the machine (register units, classes, ABI, callee sets); the allocator solves the assignment; neither hardcodes the other's assumptions.

## Integration Points
* Depends on: nothing outside `std`.
* Depended on by: reserved for kernel-native lowering (design implementation order step 10); the CPU plan path executes via the interpreter kernels in v1.

## Known Limitations
Emission is textual, not encoded object code (ticket CEP-322). Register-unit modeling covers GPR and XMM views without subregister aliasing. Live-range splitting and block-local spilling are listed in the design but not implemented (ticket CEP-323).

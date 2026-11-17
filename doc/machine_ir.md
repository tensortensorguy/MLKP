# Machine IR and IRC Allocation

## Abstract
The machine backend lowers scheduled kernels to a target-specific instruction form with virtual registers, runs liveness and interference analysis, allocates physical registers with iterated register coalescing, verifies the allocation, and emits the result twice: as deterministic AT&T-style assembly text for review, and as executable x86-64 machine bytes with relocations, packaged into a deterministic ELF64 relocatable object with evidence hashes (ticket CEP-322). A subset disassembler decodes the bytes back to text, which mechanically checks that the two emitters agree instruction for instruction.

## Formal Definitions
```rust
pub struct MInsn { op: Opcode, ops: Vec<Operand>, implicit_uses: Vec<u32>, implicit_defs: Vec<u32>, fixed: Vec<(usize, u32)> }
pub enum Operand { VReg{id, class, def}, PReg{..}, Imm(i64), Mem{..}, Block(..), Symbol(..) }
pub struct Allocation { assignment: Vec<(u32, u32)>, spilled: Vec<u32>, spill_slots: u32, coalesced_copies: u32 }
pub struct CodeObject { text: Vec<u8>, relocs: Vec<Reloc>, labels: Vec<(u32, String)> }
pub struct Reloc { offset: u32, symbol: String, addend: i64 }
pub struct ObjectEvidence { object_hash: u64, sections: Vec<String>, symbol_table_hash: u64, relocation_table_hash: u64, disassembly_hash: u64 }
```

## Algorithm
Block liveness is a bounded backward dataflow to fixed point. Interference connects each def to everything live after the instruction, plus fixed-register constraints, plus PReg-operand blocking: a PReg use blocks every vreg live across the read and every same-block def (a def made before the read would have clobbered the incoming value), with the instruction's own defs exempt because a plain move reads before it writes; a PReg def blocks every vreg live across it except the instruction's own vreg uses (a self-move is legal). Coalescing admits a pair only when the pair does not interfere, the register classes match, and either both nodes are ordinary and one criterion passes (George: every neighbor of the source is low-degree, already adjacent to the target, or precolored; Briggs: fewer than K high-degree merged neighbors) or exactly one side is precolored and George's criterion passes on the other side. Simplify removes the lowest-degree remaining node from the working graph, decrementing neighbor degrees, and marks potential spills; select consults the static interference graph, assigns the first free color avoiding assigned neighbor colors and blocked pregs, and cancels the spill mark of any potential spill that receives a color. Spill rewriting registers every fresh temporary in the class table so the next coloring round assigns it.

Encoding runs two passes: a size fixpoint computes label offsets and grows branch forms from rel8 to rel32 when the displacement overflows a byte (sizes only grow, so the loop terminates in at most 64 rounds), then an emit pass writes the prologue, every instruction, and the epilogue, recording a relocation per external call symbol. The encoder maps target-table register ids to hardware encoding numbers in one function because the table orders GPRs by ABI role while the ISA encodes them AX, CX, DX, BX, SP, BP, SI, DI, R8..R15. Object emission lays out the ELF64 header, .text, .rela.text, .symtab, .strtab, and .shstrtab in a fixed order with deterministic padding and rewrites e_shoff once the section table offset is known.

## Invariants
1. Interfering live values never share a physical register; the verifier re-derives interference post-allocation and rejects overlaps.
2. Fixed constraints are respected exactly; the verifier compares assigned registers against every `(operand, preg)` pair.
3. A vreg live across, or defined before, an instruction that reads a physical register never occupies that register, so incoming ABI arguments and fixed-register values are not clobbered before their read.
4. Spilled vregs hold the `u32::MAX` sentinel assignment, never a register; spill-rewrite temporaries are registered in the class table, so no vreg escapes assignment.
5. Encoding and assembly emission render the same program: the decoder's text matches the emitter's byte for byte, and the encoder's bytes match GNU as's output for the same source.
6. Emission is deterministic: two runs over one allocation produce byte-identical text, bytes, and object files, with no timestamps, paths, or environment values embedded.
7. Every spill-slot displacement baked by the rewriter assumes the prologue's worst-case reserve, so the frame formula is shared by the rewriter, the text emitter, and the encoder.
8. The target describes the machine (register units, classes, ABI, callee sets); the allocator solves the assignment; neither hardcodes the other's assumptions.

## Integration Points
* Depends on: `cep-hash` (FNV-1a evidence hashing); nothing outside `std` otherwise.
* Depended on by: reserved for kernel-native lowering (design implementation order step 10); the CPU plan path executes via the interpreter kernels in v1. The encoded `CodeObject` and the ELF64 object are the hand-off artifacts for any executor or linker.

## Known Limitations
Emission covers the encoded subset: integer 64-bit moves, add, sub, imul, cmp, conditional and unconditional branches, calls, returns, loads and stores with base-plus-displacement addressing (RBP and RSP special cases handled), and scalar-double SSE moves, adds, and multiplies; immediates beyond i64, index-register addressing, and vector widths are rejected loudly, not approximated. PReg-use blocking covers same-block defs and live-across values; a physical register read in a block reached through control flow with the incoming value set elsewhere is not modeled. Cross-function inlining and TLS are out of scope. Spill rewriting, rematerialization of constant-defined vregs, split-at-spill temporaries, and fixed rbp-relative slot addressing are implemented (ticket CEP-323); pressure-driven block-local spill decisions and subregister aliasing remain future work.

// CEP:FILE: crates/mlkp-machine/src/encode/mod.rs
// CEP:WHAT: Byte-level x86-64 encoder entry: error types, the compiled CodeObject, and the branch-size fixpoint driver that turns one allocated machine function plus its allocation into executable machine bytes with a shared prologue and epilogue and relocations for external call symbols (ticket CEP-322).
// CEP:WHY: Encoding mirrors emit() instruction for instruction so the two artifacts stay provably equivalent.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns EncodeError for unresolved or spilled virtual registers, immediates outside the encodable range, unknown branch labels, an unstable branch-size fixpoint, or operand shapes the subset does not encode; no silent truncation of any value.
// CEP:ASSUMES: the function is allocated and verified (all vregs resolve through the allocation, none spilled); block labels are unique and the label "ret" is reserved for the epilogue; the System V AMD64 ABI; the frame formula matches emit() exactly (align16(8*max_saved_gprs + 8*spill_slots)).
// CEP:COST: cold; O(insns) per fixpoint round with at most 64 rounds, then O(insns) to emit.
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs, crates/mlkp-machine/tests/exec_test.rs.

#![deny(missing_docs)]

use self::bytes::insn_size;
use self::frame::{
    epilogue_len, frame_size, prologue_len, push_epilogue, push_prologue, saved_regs,
};
use self::insn::{branch_size, emit_insn};
use self::operand::{resolve_insn, EncInsn};
use self::regs::fits_i8;
use crate::mir::MFunction;
use crate::mir::Opcode;
use crate::target::X86_64Target;

pub(crate) mod bytes;
pub(crate) mod frame;
pub(crate) mod insn;
/// One byte, named per the Intel manual, so encoders and decoders read like the ISA.
pub(crate) mod op;
pub(crate) mod operand;
pub(crate) mod regs;

/// Encoder failure modes.
#[derive(Clone, Debug, PartialEq)]
pub enum EncodeError {
    /// An operand still references a virtual register missing from the allocation.
    UnresolvedVReg(u32),
    /// A spilled value was referenced; allocate() rewrites spills away, so this is a corrupt input.
    UnresolvedSpill(u32),
    /// An immediate is outside the range the subset encodes for this instruction.
    ImmOutOfRange(i64),
    /// A branch or call targets an unknown label.
    UnknownLabel(String),
    /// The branch-size fixpoint did not stabilize within the round cap.
    FixpointUnstable,
    /// The operand shape is not encodable (for example a memory operand on Mov).
    BadOperand(&'static str),
    /// A register class does not fit the instruction's expectations.
    BadClass(&'static str),
}

impl std::fmt::Display for EncodeError {
    // CEP:WHAT: Formats the encode error.
    // CEP:WHY: Deterministic diagnostics for backend failures.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::UnresolvedVReg(id) => write!(f, "encode error: vreg {id} unresolved"),
            EncodeError::UnresolvedSpill(id) => write!(f, "encode error: spill {id} unresolved"),
            EncodeError::ImmOutOfRange(v) => write!(f, "encode error: immediate {v} out of range"),
            EncodeError::UnknownLabel(l) => write!(f, "encode error: unknown label {l}"),
            EncodeError::FixpointUnstable => {
                write!(f, "encode error: branch size fixpoint unstable")
            }
            EncodeError::BadOperand(s) => write!(f, "encode error: bad operand ({s})"),
            EncodeError::BadClass(s) => write!(f, "encode error: bad register class ({s})"),
        }
    }
}

/// One relocation the object emitter must resolve: the rel32 field at `offset` refers to `symbol`.
#[derive(Clone, Debug, PartialEq)]
pub struct Reloc {
    /// Byte offset of the rel32 field inside the text section.
    pub offset: u32,
    /// Referenced external symbol.
    pub symbol: String,
    /// Relocation addend (the -4 PC-relative bias of a call).
    pub addend: i64,
}

/// The compiled artifact: executable bytes, relocations in instruction order, and label offsets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodeObject {
    /// Encoded machine bytes (the .text section contents).
    pub text: Vec<u8>,
    /// Relocations in instruction order.
    pub relocs: Vec<Reloc>,
    /// Label byte offsets in program order; the epilogue comes last.
    pub labels: Vec<(u32, String)>,
}

/// Reserved epilogue label; a block with this name makes encoding fail loudly.
pub const EPILOGUE_LABEL: &str = "ret";

/// Cap on branch-size fixpoint rounds; sizes only grow, so the loop terminates far earlier.
pub(crate) const MAX_FIXPOINT_ROUNDS: u32 = 64;

/// Frame alignment in bytes; the System V ABI requires 16-byte rsp alignment at call sites.
pub(crate) const FRAME_ALIGN: i64 = 16;

/// Saved-register and spill-slot width in bytes.
pub(crate) const SLOT_WIDTH: i64 = 8;

/// Compiles one allocated, verified function into a CodeObject.
// CEP:ID: JIT-w91
// CEP:WHAT: Compiles one function to bytes.
// CEP:WHY: The design's emission stage: encoding with relocations, deterministic and executable.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module errors; rejects a block named like the reserved epilogue label before any bytes exist.
// CEP:ASSUMES: the allocation is verified; block labels are unique; no block is named "ret".
// CEP:COST: O(insns * fixpoint rounds) with a 64-round cap.
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs, crates/mlkp-machine/tests/exec_test.rs.
pub fn compile(
    f: &MFunction,
    alloc: &crate::irc::Allocation,
    target: &X86_64Target,
) -> Result<CodeObject, EncodeError> {
    if f.blocks.iter().any(|b| b.label == EPILOGUE_LABEL) {
        return Err(EncodeError::UnknownLabel(format!(
            "reserved label {EPILOGUE_LABEL}"
        )));
    }
    let insns: Vec<EncInsn> = f
        .blocks
        .iter()
        .flat_map(|b| b.insns.iter())
        .map(|i| resolve_insn(i, alloc))
        .collect::<Result<Vec<_>, _>>()?;
    let is_branch =
        |e: &EncInsn| e.op.is_conditional_branch() || matches!(e.op, Opcode::Jmp | Opcode::Ret);
    let saved = saved_regs(alloc, target);
    let frame = frame_size(alloc, target);
    let prolog = prologue_len(&saved, frame);
    let epi = epilogue_len(&saved);
    // Branch fixpoint: start every branch short; grow forms whose displacement overflows i8; repeat until stable.
    let mut long = vec![false; insns.len()];
    let mut rounds = 0u32;
    let (label_offsets, epilogue_offset) = loop {
        let mut label_offsets: Vec<(String, u32)> = Vec::new();
        let mut insn_offsets: Vec<u32> = Vec::with_capacity(insns.len());
        let mut off = prolog as u32;
        let mut idx = 0usize;
        for block in &f.blocks {
            label_offsets.push((block.label.clone(), off));
            for _ in &block.insns {
                insn_offsets.push(off);
                off += insn_size(&insns[idx], long[idx]) as u32;
                idx += 1;
            }
        }
        let epilogue_offset = off;
        // Validate branch targets once the labels exist.
        for e in insns.iter() {
            if !is_branch(e) {
                continue;
            }
            if e.op == Opcode::Ret {
                continue;
            }
            let name = e
                .target
                .clone()
                .ok_or(EncodeError::BadOperand("branch target"))?;
            if !label_offsets.iter().any(|(l, _)| l == &name) {
                return Err(EncodeError::UnknownLabel(name));
            }
        }
        let mut changed = false;
        for (k, e) in insns.iter().enumerate() {
            if !is_branch(e) {
                continue;
            }
            let tgt = if e.op == Opcode::Ret {
                epilogue_offset
            } else {
                let name = e.target.clone().unwrap_or_default();
                label_offsets
                    .iter()
                    .find(|(l, _)| l == &name)
                    .map(|(_, o)| *o)
                    .unwrap_or(epilogue_offset)
            };
            let size = branch_size(e.op, long[k]) as u32;
            let next = insn_offsets[k] + size;
            let disp = i64::from(tgt) - i64::from(next);
            if !fits_i8(disp) && !long[k] {
                long[k] = true;
                changed = true;
            }
        }
        rounds += 1;
        if !changed {
            break (label_offsets, epilogue_offset);
        }
        if rounds >= MAX_FIXPOINT_ROUNDS {
            return Err(EncodeError::FixpointUnstable);
        }
    };
    // Emit pass: prologue, blocks, epilogue.
    let mut out: Vec<u8> = Vec::new();
    push_prologue(&mut out, &saved, frame);
    let mut relocs: Vec<Reloc> = Vec::new();
    let mut idx = 0usize;
    for block in &f.blocks {
        for _ in &block.insns {
            let e = &insns[idx];
            let target_off = if e.op == Opcode::Ret {
                Some(epilogue_offset)
            } else if e.op.is_conditional_branch() || e.op == Opcode::Jmp {
                let name = e
                    .target
                    .clone()
                    .ok_or(EncodeError::BadOperand("branch target"))?;
                Some(
                    label_offsets
                        .iter()
                        .find(|(l, _)| l == &name)
                        .map(|(_, o)| *o)
                        .ok_or(EncodeError::UnknownLabel(name.clone()))?,
                )
            } else if e.op == Opcode::Call {
                let name = e
                    .target
                    .clone()
                    .ok_or(EncodeError::BadOperand("call target"))?;
                label_offsets
                    .iter()
                    .find(|(l, _)| l == &name)
                    .map(|(_, o)| *o)
            } else {
                None
            };
            emit_insn(&mut out, e, long[idx], target_off, &mut relocs)?;
            idx += 1;
        }
    }
    debug_assert_eq!(
        out.len(),
        epilogue_offset as usize,
        "emit pass drifted from the size fixpoint"
    );
    push_epilogue(&mut out, &saved);
    debug_assert_eq!(
        out.len() - epilogue_offset as usize,
        epi,
        "epilogue size drifted from the layout"
    );
    let mut labels: Vec<(u32, String)> =
        label_offsets.iter().map(|(l, o)| (*o, l.clone())).collect();
    labels.push((epilogue_offset, EPILOGUE_LABEL.to_string()));
    Ok(CodeObject {
        text: out,
        relocs,
        labels,
    })
}

/// Convenience: compiles one function and returns only the executable bytes.
// CEP:WHAT: Encodes one function to bytes.
// CEP:WHY: Callers that do not need relocations or labels.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module errors.
// CEP:ASSUMES: see compile().
// CEP:COST: as compile().
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs.
pub fn encode(
    f: &MFunction,
    alloc: &crate::irc::Allocation,
    target: &X86_64Target,
) -> Result<Vec<u8>, EncodeError> {
    compile(f, alloc, target).map(|c| c.text)
}

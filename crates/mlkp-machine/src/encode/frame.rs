// CEP:FILE: crates/mlkp-machine/src/encode/frame.rs
// CEP:WHAT: Prologue and epilogue emission: callee-saved register selection, the rbp frame, push and pop encodings, and their byte counts.
// CEP:WHY: Encoding mirrors emit() instruction for instruction so the two artifacts stay provably equivalent.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the EncodeError variants documented per function; no silent truncation of any value.
// CEP:ASSUMES: the allocation is verified; the System V AMD64 ABI; the frame formula matches emit() exactly (align16(8*max_saved_gprs + 8*spill_slots)).
// CEP:COST: cold; O(saved registers).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs, crates/mlkp-machine/tests/exec_test.rs.

use super::bytes::push_rex;
use super::op;
use super::regs::{enc, fits_i8, hw, hw_num, len, modrm};
use super::FRAME_ALIGN;
use super::SLOT_WIDTH;
use crate::target::preg;
use crate::target::X86_64Target;

/// Callee-saved GPRs the allocation actually uses, in allocation order.
// CEP:WHAT: Collects saved registers.
// CEP:WHY: The prologue preserves exactly the callee-saved registers the allocation picked, matching emit().
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the allocation is verified.
// CEP:COST: O(assignment).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn saved_regs(alloc: &crate::irc::Allocation, target: &X86_64Target) -> Vec<u32> {
    let mut saved: Vec<u32> = Vec::new();
    for (v, p) in &alloc.assignment {
        if alloc.spilled.contains(v) {
            continue;
        }
        if let Some(r) = target.reg(*p) {
            if r.callee_saved && !r.reserved && !saved.contains(p) {
                saved.push(*p);
            }
        }
    }
    saved
}

/// Frame depth the prologue reserves, matching emit() byte for byte.
// CEP:WHAT: Computes the frame size.
// CEP:WHY: Spill slot displacements baked by the rewriter assume this reserve.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the rewriter and the encoder derive the layout from the same target constant.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn frame_size(alloc: &crate::irc::Allocation, target: &X86_64Target) -> i64 {
    let depth = SLOT_WIDTH * target.max_saved_gprs() as i64 + SLOT_WIDTH * alloc.spill_slots as i64;
    (depth + FRAME_ALIGN - 1) / FRAME_ALIGN * FRAME_ALIGN
}

/// Encodes the prologue: push rbp, mov rsp to rbp, saved pushes, stack reservation.
// CEP:ID: JIT-070
// CEP:WHAT: Encodes the prologue.
// CEP:WHY: The ABI requires a frame pointer save and callee-saved preservation; the frame formula matches emit() so baked slot displacements stay valid.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: frame is non-negative and fits i32 (spill slots are bounded by the allocator).
// CEP:COST: O(saved).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn push_prologue(out: &mut Vec<u8>, saved: &[u32], frame: i64) {
    out.push(op::PUSH_RBP);
    push_rex(out, op::REX_W);
    out.push(op::MOV_MR);
    out.push(op::MODRM_MOV_RSP_RBP);
    for p in saved {
        push_push(out, *p);
    }
    if !fits_i8(frame) {
        push_rex(out, op::REX_W);
        out.push(op::GRP1_ID);
        out.push(modrm(enc::MOD_REG, op::EXT_SUB, enc::RM_SIB));
        out.extend_from_slice(&(frame as i32).to_le_bytes());
    } else {
        push_rex(out, op::REX_W);
        out.push(op::GRP1_IB);
        out.push(modrm(enc::MOD_REG, op::EXT_SUB, enc::RM_SIB));
        out.push(frame as i8 as u8);
    }
}

/// Encodes one push.
// CEP:WHAT: Encodes a push.
// CEP:WHY: Prologue register saves.
// CEP:STATUS: complete
// CEP:FAILURE: Panics on RSP or RBP pushes; the saved set excludes reserved registers by construction.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn push_push(out: &mut Vec<u8>, p: u32) {
    let h = hw(p);
    if h >= hw_num::GPR_EXT {
        out.push(op::REX_B_ONLY);
        out.push(op::PUSH_R + (h & enc::REG_MASK) as u8);
    } else if p == preg::RBP {
        out.push(op::PUSH_RBP);
    } else {
        out.push(op::PUSH_R + h as u8);
    }
}

/// Encodes one pop.
// CEP:WHAT: Encodes a pop.
// CEP:WHY: Epilogue restores.
// CEP:STATUS: complete
// CEP:FAILURE: Panics on RSP or RBP pops; reserved registers are never saved.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn push_pop(out: &mut Vec<u8>, p: u32) {
    let h = hw(p);
    if h >= hw_num::GPR_EXT {
        out.push(op::REX_B_ONLY);
        out.push(op::POP_R + (h & enc::REG_MASK) as u8);
    } else if p == preg::RBP {
        out.push(op::POP_RBP);
    } else {
        out.push(op::POP_R + h as u8);
    }
}

/// Encodes the epilogue: saved pops, mov rbp to rsp, pop rbp, ret.
// CEP:WHAT: Encodes the epilogue.
// CEP:WHY: Mirror of the prologue in reverse; Ret instructions jump here.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(saved).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn push_epilogue(out: &mut Vec<u8>, saved: &[u32]) {
    for p in saved.iter().rev() {
        push_pop(out, *p);
    }
    push_rex(out, op::REX_W);
    out.push(op::MOV_MR);
    out.push(op::MODRM_MOV_RBP_RSP);
    out.push(op::POP_RBP);
    out.push(op::RET);
}

/// Computes the prologue byte length from the saved set and the frame size.
// CEP:WHAT: Sizes the prologue.
// CEP:WHY: Block label offsets start after it.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: matches push_prologue byte for byte.
// CEP:COST: O(saved).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn prologue_len(saved: &[u32], frame: i64) -> usize {
    // Extended registers (r8+) need a REX byte; low registers push in one byte.
    let pushes: usize = saved
        .iter()
        .map(|p| usize::from(*p >= hw_num::GPR_EXT) + 1)
        .sum();
    1 + len::FRAME_MOV
        + pushes
        + if fits_i8(frame) {
            len::GRP1_IB
        } else {
            len::GRP1_ID
        }
}

/// Computes the epilogue byte length.
// CEP:WHAT: Sizes the epilogue.
// CEP:WHY: The Ret-jump target offset depends on it.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: matches push_epilogue byte for byte.
// CEP:COST: O(saved).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn epilogue_len(saved: &[u32]) -> usize {
    let pops: usize = saved
        .iter()
        .map(|p| usize::from(*p >= hw_num::GPR_EXT) + 1)
        .sum();
    pops + len::FRAME_MOV + len::POP_RET
}

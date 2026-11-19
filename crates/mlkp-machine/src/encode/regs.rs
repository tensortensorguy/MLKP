// CEP:FILE: crates/mlkp-machine/src/encode/regs.rs
// CEP:WHAT: ISA register mapping and shape constants: table-id to hardware-number translation, ModRM packing, range tests, the branch opcode pairs, and the shared structural byte counts.
// CEP:WHY: Encoding mirrors emit() instruction for instruction so the two artifacts stay provably equivalent.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the EncodeError variants documented per function; no silent truncation of any value.
// CEP:ASSUMES: table ids 0..=15 are GPRs in ABI order and 16.. are xmm ids offset by 16.
// CEP:COST: cold; O(1) per call.
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs, crates/mlkp-machine/tests/exec_test.rs.

use super::op;
use crate::mir::Opcode;
use crate::target::preg;

/// Conditional-branch opcode pairs: (rel8 opcode, rel32 second byte).
// CEP:WHAT: Maps conditional branches to their opcode pairs.
// CEP:WHY: All sixteen conditions share the branch machinery; the table is the one place the ISA opcode assignments live.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None for non-branch opcodes.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn jcc_encoding(opc: Opcode) -> Option<(u8, u8)> {
    match opc {
        Opcode::Je => Some((op::JE_REL8, op::JE_REL32)),
        Opcode::Jne => Some((op::JNE_REL8, op::JNE_REL32)),
        Opcode::Jl => Some((op::JL_REL8, op::JL_REL32)),
        Opcode::Jle => Some((op::JLE_REL8, op::JLE_REL32)),
        Opcode::Jg => Some((op::JG_REL8, op::JG_REL32)),
        Opcode::Jge => Some((op::JGE_REL8, op::JGE_REL32)),
        Opcode::Ja => Some((op::JA_REL8, op::JA_REL32)),
        Opcode::Jae => Some((op::JAE_REL8, op::JAE_REL32)),
        Opcode::Jb => Some((op::JB_REL8, op::JB_REL32)),
        Opcode::Jbe => Some((op::JBE_REL8, op::JBE_REL32)),
        Opcode::Js => Some((op::JS_REL8, op::JS_REL32)),
        Opcode::Jns => Some((op::JNS_REL8, op::JNS_REL32)),
        Opcode::Jo => Some((op::JO_REL8, op::JO_REL32)),
        Opcode::Jno => Some((op::JNO_REL8, op::JNO_REL32)),
        Opcode::Jp => Some((op::JP_REL8, op::JP_REL32)),
        Opcode::Jnp => Some((op::JNP_REL8, op::JNP_REL32)),
        _ => None,
    }
}

/// Hardware register numbers of the ABI registers the target table reorders.
pub(crate) mod hw_num {
    /// Hardware encoding of RSI.
    pub const RSI: u32 = 6;
    /// Hardware encoding of RDI.
    pub const RDI: u32 = 7;
    /// Hardware encoding of RBP.
    pub const RBP: u32 = 5;
    /// Hardware encoding of RSP.
    pub const RSP: u32 = 4;
    /// Highest hardware GPR number.
    pub const GPR_MAX: u32 = 15;
    /// First extended (R8+) hardware number.
    pub const GPR_EXT: u32 = 8;
    /// Table id where the xmm registers begin.
    pub const XMM_TABLE_BASE: u32 = 16;
    /// Table id of xmm8: ids at or above it need a REX extension bit.
    pub const XMM8_TABLE_ID: u32 = 24;
    /// First hardware number whose low-byte view needs a REX prefix.
    pub const BYTE_REX_MIN: u32 = 4;
}

/// Structural byte counts the size fixpoint and the emit pass share.
pub(crate) mod len {
    /// Opcode byte plus ModRM byte.
    pub const OPCODE_MODRM: usize = 2;
    /// Escape plus opcode (two-byte opcode head without REX).
    pub const ESC_OPCODE: usize = 2;
    /// REX plus escape plus opcode plus ModRM (two-byte-opcode reg form).
    pub const RR_ESC: usize = 3;
    /// Prefix plus escape plus opcode plus ModRM (SSE reg form without REX).
    pub const SSE_RR: usize = 4;
    /// REX plus group opcode plus ModRM plus imm8.
    pub const GRP1_IB: usize = 4;
    /// REX plus accumulator opcode plus imm32.
    pub const ACC_IMM32: usize = 6;
    /// REX plus group opcode plus ModRM plus imm32.
    pub const GRP1_ID: usize = 7;
    /// REX plus C7 plus ModRM plus imm32 (register-target move).
    pub const MOV_IMM32: usize = 7;
    /// REX plus movabs opcode plus imm64.
    pub const MOVABS: usize = 10;
    /// CALL rel32.
    pub const CALL: usize = 5;
    /// Branch with rel8.
    pub const BRANCH8: usize = 2;
    /// Branch with rel32 (unconditional).
    pub const BRANCH32_JMP: usize = 5;
    /// Branch with rel32 (conditional, two-byte opcode).
    pub const BRANCH32_JCC: usize = 6;
    /// A rel32 displacement field.
    pub const REL32: usize = 4;
    /// A disp32 displacement.
    pub const DISP32: usize = 4;
    /// The frame-pointer move in the prologue and epilogue.
    pub const FRAME_MOV: usize = 3;
    /// The call relocation addend: the rel32 field length as a negative bias.
    pub const CALL_ADDEND: i64 = -4;
    /// pop rbp plus ret at the epilogue tail.
    pub const POP_RET: usize = 2;
    /// Maximum shift count the ISA uses before masking.
    pub const SHIFT_COUNT_MAX: i64 = 63;
    /// Push imm32.
    pub const PUSH_IMM32_LEN: usize = 5;
    /// Push imm8.
    pub const PUSH_IMM8_LEN: usize = 2;
    /// The Cqo pair (REX.W + 99).
    pub const CQO: usize = 2;
}

/// ModRM and SIB shape constants.
pub(crate) mod enc {
    /// mod 11: register-direct operand.
    pub const MOD_REG: u8 = 3;
    /// mod 01: disp8 follows.
    pub const MOD_DISP8: u8 = 1;
    /// mod 10: disp32 follows.
    pub const MOD_DISP32: u8 = 2;
    /// rm = 100: a SIB byte follows.
    pub const RM_SIB: u8 = 4;
    /// SIB index = 100: no index register.
    pub const SIB_NO_INDEX: u8 = 4;
    /// Low three bits mask of a register number.
    pub const REG_MASK: u32 = 7;
    /// ModRM.reg field bit offset.
    pub const REG_SHIFT: u32 = 3;
    /// ModRM.mod field bit offset.
    pub const MOD_SHIFT: u32 = 6;
}

/// True when the value fits a signed byte.
// CEP:WHAT: Tests i8 range.
// CEP:WHY: Displacement and immediate size selection.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn fits_i8(v: i64) -> bool {
    v >= i64::from(i8::MIN) && v <= i64::from(i8::MAX)
}

/// True when the value fits a signed 32-bit integer.
// CEP:WHAT: Tests i32 range.
// CEP:WHY: Immediate and displacement size selection.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn fits_i32(v: i64) -> bool {
    v >= i64::from(i32::MIN) && v <= i64::from(i32::MAX)
}

/// Maps a target-table register id to its hardware encoding number.
///
/// The target table orders GPRs by ABI role (RAX, RCX, RDX, RBX, RSI, RDI,
/// RBP, RSP, R8..R15) while the ISA encodes them in the order AX, CX, DX, BX,
/// SP, BP, SI, DI, R8..R15; this function is the only place that knows both.
// CEP:WHAT: Maps table ids to hardware numbers.
// CEP:WHY: ModRM, SIB, push, and pop encodings are defined over hardware numbers, not table ids.
// CEP:STATUS: complete
// CEP:FAILURE: Panics for ids above R15; the target table defines exactly 16 GPRs and validation runs first.
// CEP:ASSUMES: table ids 0..=15 are GPRs in ABI order and 16.. are xmm ids offset by 16.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn hw(id: u32) -> u32 {
    const LOW_GPR_MAX: u32 = 3;
    match id {
        0..=LOW_GPR_MAX => id, // RAX, RCX, RDX, RBX encode as themselves
        preg::RSI => hw_num::RSI,
        preg::RDI => hw_num::RDI,
        preg::RBP => hw_num::RBP,
        preg::RSP => hw_num::RSP,
        hw_num::GPR_EXT..=hw_num::GPR_MAX => id, // R8..R15 encode as themselves
        _ => panic!("hw(): non-GPR register id {id}"),
    }
}

/// The low three bits of a register id as the ISA encodes it.
// CEP:WHAT: Extracts the ISA register index.
// CEP:WHY: ModRM and REX encoding.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn enc_reg(id: u32) -> u8 {
    if id >= hw_num::XMM_TABLE_BASE {
        // xmm ids sit above the GPR table and encode linearly (xmm8 = 8).
        ((id - hw_num::XMM_TABLE_BASE) & enc::REG_MASK) as u8
    } else {
        (hw(id) & enc::REG_MASK) as u8
    }
}

/// Builds a ModRM byte from mod, reg, rm.
// CEP:WHAT: Packs a ModRM byte.
// CEP:WHY: Every register or memory operand needs one.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: fields already in range.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn modrm(mod_: u8, reg: u8, rm: u8) -> u8 {
    (mod_ << enc::MOD_SHIFT)
        | ((reg & enc::REG_MASK as u8) << enc::REG_SHIFT as u8)
        | (rm & enc::REG_MASK as u8)
}

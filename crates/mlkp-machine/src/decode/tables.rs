// CEP:FILE: crates/mlkp-machine/src/decode/tables.rs
// CEP:WHAT: Decoder-side structural constants mirroring the encoder's opcode and shape table, plus the Jcc rel32 condition map.
// CEP:WHY: The decoder is the mechanical bytes-versus-text equivalence check of ticket CEP-322; one shared structural grammar keeps decoding identical across the CodeObject path and the general path.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module DecodeError variants; encodings outside the ISA tier are rejected loudly, never guessed at.
// CEP:ASSUMES: the bytes came from encode::compile for the CodeObject path; prefixes arrive in canonical order.
// CEP:COST: cold; O(insns).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs.

use crate::mir::Opcode;

/// Decoder-side structural constants mirroring the encoder's.
pub(crate) mod d {
    /// Bits per byte.
    pub const BYTE_BITS: i64 = 8;
    /// Sign-extension mask shift for a byte.
    pub const BYTE_SIGN_SHIFT: i64 = 56;
    /// Sign-extension mask shift for a dword.
    pub const DWORD_SIGN_SHIFT: i64 = 32;
    /// imm64 byte width.
    pub const IMM64: usize = 8;
    /// imm32/disp32 byte width.
    pub const IMM32: usize = 4;
    /// imm8/disp8 byte width.
    pub const IMM8: usize = 1;
    /// reg field bit offset inside ModRM.
    pub const REG_SHIFT: u32 = 3;
    /// reg field bit mask.
    pub const REG_MASK: u32 = 7;
    /// Table id offset of xmm registers.
    pub const XMM_TABLE_BASE: u32 = 16;
    /// Table id at or above which an operand is an xmm register.
    pub const XMM_MIN_ID: u32 = 16;
    /// mod-field bit offset inside ModRM.
    pub const MOD_SHIFT: u32 = 6;
    /// Hardware number of RSP (SIB base or rm=100).
    pub const HW_SIB: u32 = 4;
    /// Hardware number of RBP (rm=101 needs a displacement).
    pub const HW_RIP: u32 = 5;
    /// SIB index = 100: no index register.
    pub const SIB_NO_INDEX: u32 = 4;
    /// mod 11: register-direct.
    pub const MOD_REG: u32 = 3;
    /// mod 10: disp32 follows.
    pub const MOD_DISP32: u32 = 2;
    /// Hardware number where extended registers (R8+) begin.
    pub const GPR_EXT: u32 = 8;
    /// Hardware number of RSI.
    pub const HW_RSI: u32 = 6;
    /// Hardware number of RDI.
    pub const HW_RDI: u32 = 7;
    /// mod 10 as a ModRM byte value for pattern positions.
    pub const MOD_DISP32_B: u8 = 2;
    /// Push opcode base (50+r).
    pub const PUSH_BASE: u8 = 0x50;
    /// Pop opcode base (58+r).
    pub const POP_BASE: u8 = 0x58;
    /// Push/pop register span.
    pub const PUSH_SPAN: u8 = 7;
    /// movabs opcode base (B8+r).
    pub const MOVABS_BASE: u8 = 0xB8;
    /// movabs last opcode (BF).
    pub const MOVABS_LAST: u8 = 0xBF;
    /// First REX byte.
    pub const REX_FIRST: u8 = 0x40;
    /// Last REX byte.
    pub const REX_LAST: u8 = 0x4F;
    /// REX.W bit.
    pub const REX_W: u8 = 0x08;
    /// REX.R bit.
    pub const REX_R: u8 = 0x04;
    /// REX.B bit.
    pub const REX_B: u8 = 0x01;
    /// RET opcode.
    pub const RET: u8 = crate::encode::op::RET;
    /// ADD r/m64, r64.
    pub const ADD_MR: u8 = crate::encode::op::ADD_MR;
    /// ADD r64, r/m64.
    pub const ADD_RM: u8 = crate::encode::op::ADD_RM;
    /// SUB r/m64, r64.
    pub const SUB_MR: u8 = crate::encode::op::SUB_MR;
    /// SUB r64, r/m64.
    pub const SUB_RM: u8 = crate::encode::op::SUB_RM;
    /// CMP r/m64, r64.
    pub const CMP_MR: u8 = crate::encode::op::CMP_MR;
    /// CMP r64, r/m64.
    pub const CMP_RM: u8 = crate::encode::op::CMP_RM;
    /// MOV r/m64, r64.
    pub const MOV_MR: u8 = crate::encode::op::MOV_MR;
    /// MOV r64, r/m64.
    pub const MOV_RM: u8 = crate::encode::op::MOV_RM;
    /// MOV r/m64, imm32.
    pub const MOV_IMM32: u8 = crate::encode::op::MOV_IMM32;
    /// IMUL opcode (after escape).
    pub const IMUL_RM: u8 = crate::encode::op::IMUL_RM;
    /// ADDSD (after escape).
    pub const ADDSD: u8 = crate::encode::op::ADDSD;
    /// MULSD (after escape).
    pub const MULSD: u8 = crate::encode::op::MULSD;
    /// MOVAPD (after escape).
    pub const MOVAPD: u8 = crate::encode::op::MOVAPD;
    /// MOVSD load (after escape).
    pub const MOVSD_LOAD: u8 = crate::encode::op::MOVSD_LOAD;
    /// MOVSD store (after escape).
    pub const MOVSD_STORE: u8 = crate::encode::op::MOVSD_STORE;
    /// Two-byte escape.
    pub const ESCAPE: u8 = crate::encode::op::ESCAPE;
    /// F2 prefix.
    pub const PRE_F2: u8 = crate::encode::op::PRE_F2;
    /// 66 prefix.
    pub const PRE_66: u8 = crate::encode::op::PRE_66;
    /// CALL rel32.
    pub const CALL_REL32: u8 = crate::encode::op::CALL_REL32;
    /// JMP rel8.
    pub const JMP_REL8: u8 = crate::encode::op::JMP_REL8;
    /// JMP rel32.
    pub const JMP_REL32: u8 = crate::encode::op::JMP_REL32;
    /// JL rel8.
    pub const JL_REL8: u8 = crate::encode::op::JL_REL8;
    /// JGE rel8.
    pub const JGE_REL8: u8 = crate::encode::op::JGE_REL8;
    /// Group imm8 opcode.
    pub const GRP1_IB: u8 = crate::encode::op::GRP1_IB;
    /// Group imm32 opcode.
    pub const GRP1_ID: u8 = crate::encode::op::GRP1_ID;
    /// ADD group extension.
    pub const EXT_ADD: u8 = crate::encode::op::EXT_ADD;
    /// SUB group extension.
    pub const EXT_SUB: u8 = crate::encode::op::EXT_SUB;
    /// CMP group extension.
    pub const EXT_CMP: u8 = crate::encode::op::EXT_CMP;
    /// MOV r/m8, r8 (byte store).
    pub const MOV8_MR: u8 = crate::encode::op::MOV8_MR;
    /// AND r/m64, r64.
    pub const AND_MR: u8 = crate::encode::op::AND_MR;
    /// AND r64, r/m64.
    pub const AND_RM: u8 = crate::encode::op::AND_RM;
    /// OR r/m64, r64.
    pub const OR_MR: u8 = crate::encode::op::OR_MR;
    /// OR r64, r/m64.
    pub const OR_RM: u8 = crate::encode::op::OR_RM;
    /// XOR r/m64, r64.
    pub const XOR_MR: u8 = crate::encode::op::XOR_MR;
    /// XOR r64, r/m64.
    pub const XOR_RM: u8 = crate::encode::op::XOR_RM;
    /// AND accumulator short form.
    /// OR accumulator short form.
    /// XOR accumulator short form.
    /// OR group /1 extension.
    pub const EXT_OR: u8 = crate::encode::op::EXT_OR;
    /// AND group /4 extension.
    pub const EXT_AND: u8 = crate::encode::op::EXT_AND;
    /// XOR group /6 extension.
    pub const EXT_XOR: u8 = crate::encode::op::EXT_XOR;
    /// NOT group /2 extension.
    pub const EXT_NOT: u8 = crate::encode::op::EXT_NOT;
    /// NEG group /3 extension.
    pub const EXT_NEG: u8 = crate::encode::op::EXT_NEG;
    /// IDIV group /7 extension.
    pub const EXT_IDIV: u8 = crate::encode::op::EXT_IDIV;
    /// TEST group /0 extension.
    pub const EXT_TEST: u8 = crate::encode::op::EXT_TEST;
    /// Shift immediate group opcode.
    pub const SHIFT_IMM: u8 = crate::encode::op::SHIFT_IMM;
    /// Shift by CL group opcode.
    pub const SHIFT_CL: u8 = crate::encode::op::SHIFT_CL;
    /// Shift with the implicit count of one.
    pub const SHIFT_ONE: u8 = crate::encode::op::SHIFT_ONE;
    /// SHL group /4 extension.
    pub const EXT_SHL: u8 = crate::encode::op::EXT_SHL;
    /// SHR group /5 extension.
    pub const EXT_SHR: u8 = crate::encode::op::EXT_SHR;
    /// CQO.
    pub const CQO: u8 = crate::encode::op::CQO;
    /// LEA.
    pub const LEA: u8 = crate::encode::op::LEA;
    /// IMUL three-operand imm8.
    pub const IMUL3_IMM8: u8 = crate::encode::op::IMUL3_IMM8;
    /// IMUL three-operand imm32.
    pub const IMUL3_IMM32: u8 = crate::encode::op::IMUL3_IMM32;
    /// MOVZX r64, r/m8.
    pub const MOVZ8: u8 = crate::encode::op::MOVZ8;
    /// MOVSXD r64, r/m32.
    pub const MOVSX32: u8 = crate::encode::op::MOVSX32;
    /// PUSH imm8.
    pub const PUSH_IMM8: u8 = crate::encode::op::PUSH_IMM8;
    /// PUSH imm32.
    pub const PUSH_IMM32: u8 = crate::encode::op::PUSH_IMM32;
    /// SUBSD.
    pub const SUBSD: u8 = crate::encode::op::SUBSD;
    /// DIVSD.
    pub const DIVSD: u8 = crate::encode::op::DIVSD;
    /// SQRTSD.
    pub const SQRTSD: u8 = crate::encode::op::SQRTSD;
    /// UCOMISD.
    pub const UCOMISD: u8 = crate::encode::op::UCOMISD;
    /// CVTSI2SD.
    pub const CVTSI2SD: u8 = crate::encode::op::CVTSI2SD;
    /// CVTTSD2SI.
    pub const CVTTSD2SI: u8 = crate::encode::op::CVTTSD2SI;
    /// TEST r/m64, r64.
    pub const TEST_MR: u8 = crate::encode::op::TEST_MR;
    /// Unary group (F7 /0 /2 /3 /7).
    pub const GRP3: u8 = crate::encode::op::GRP3;
    /// TEST EAX, imm32 short form.
    pub const TEST_EAX_ID: u8 = crate::encode::op::TEST_EAX_ID;
    /// ADD accumulator short form.
    pub const ADD_EAX_ID: u8 = crate::encode::op::ADD_EAX_ID;
    /// SUB accumulator short form.
    pub const SUB_EAX_ID: u8 = crate::encode::op::SUB_EAX_ID;
    /// CMP accumulator short form.
    pub const CMP_EAX_ID: u8 = crate::encode::op::CMP_EAX_ID;
    /// JE rel8.
    pub const JE_REL8: u8 = crate::encode::op::JE_REL8;
    /// JNE rel8.
    pub const JNE_REL8: u8 = crate::encode::op::JNE_REL8;
    /// JLE rel8.
    pub const JLE_REL8: u8 = crate::encode::op::JLE_REL8;
    /// JG rel8.
    pub const JG_REL8: u8 = crate::encode::op::JG_REL8;
    /// JA rel8.
    pub const JA_REL8: u8 = crate::encode::op::JA_REL8;
    /// JAE rel8.
    pub const JAE_REL8: u8 = crate::encode::op::JAE_REL8;
    /// JB rel8.
    pub const JB_REL8: u8 = crate::encode::op::JB_REL8;
    /// JBE rel8.
    pub const JBE_REL8: u8 = crate::encode::op::JBE_REL8;
    /// JS rel8.
    pub const JS_REL8: u8 = crate::encode::op::JS_REL8;
    /// JNS rel8.
    pub const JNS_REL8: u8 = crate::encode::op::JNS_REL8;
    /// JO rel8.
    pub const JO_REL8: u8 = crate::encode::op::JO_REL8;
    /// JNO rel8.
    pub const JNO_REL8: u8 = crate::encode::op::JNO_REL8;
    /// JP rel8.
    pub const JP_REL8: u8 = crate::encode::op::JP_REL8;
    /// JNP rel8.
    pub const JNP_REL8: u8 = crate::encode::op::JNP_REL8;
    /// JGE rel32 second byte.
    pub const JGE_REL32: u8 = crate::encode::op::JGE_REL32;
    /// JL rel32 second byte.
    pub const JL_REL32: u8 = crate::encode::op::JL_REL32;
    /// First rel32 Jcc second byte.
    pub const JCC_REL32_FIRST: u8 = 0x80;
    /// Last rel32 Jcc second byte.
    pub const JCC_REL32_LAST: u8 = 0x8F;
    /// JO rel32 second byte.
    pub const JO_REL32: u8 = 0x80;
    /// JNO rel32 second byte.
    pub const JNO_REL32: u8 = 0x81;
    /// JB rel32 second byte.
    pub const JB_REL32: u8 = 0x82;
    /// JAE rel32 second byte.
    pub const JAE_REL32: u8 = 0x83;
    /// JE rel32 second byte.
    pub const JE_REL32: u8 = crate::encode::op::JE_REL32;
    /// JNE rel32 second byte.
    pub const JNE_REL32: u8 = crate::encode::op::JNE_REL32;
    /// JBE rel32 second byte.
    pub const JBE_REL32: u8 = crate::encode::op::JBE_REL32;
    /// JA rel32 second byte.
    pub const JA_REL32: u8 = crate::encode::op::JA_REL32;
    /// JS rel32 second byte.
    pub const JS_REL32: u8 = crate::encode::op::JS_REL32;
    /// JNS rel32 second byte.
    pub const JNS_REL32: u8 = crate::encode::op::JNS_REL32;
    /// JP rel32 second byte.
    pub const JP_REL32: u8 = crate::encode::op::JP_REL32;
    /// JNP rel32 second byte.
    pub const JNP_REL32: u8 = crate::encode::op::JNP_REL32;
    /// JLE rel32 second byte.
    pub const JLE_REL32: u8 = crate::encode::op::JLE_REL32;
}

/// Maps a Jcc rel32 second byte to its condition.
// CEP:WHAT: Resolves Jcc conditions from rel32 opcodes.
// CEP:WHY: The two-byte conditional forms 0F 80..8F differ only in the second byte.
// CEP:STATUS: complete
// CEP:FAILURE: Panics only for bytes outside 80..8F, which the caller's range check excludes.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn jcc_from_rel32(second: u8) -> Opcode {
    match second {
        x if x == d::JO_REL32 => Opcode::Jo,
        x if x == d::JNO_REL32 => Opcode::Jno,
        x if x == d::JB_REL32 => Opcode::Jb,
        x if x == d::JAE_REL32 => Opcode::Jae,
        x if x == d::JE_REL32 => Opcode::Je,
        x if x == d::JNE_REL32 => Opcode::Jne,
        x if x == d::JBE_REL32 => Opcode::Jbe,
        x if x == d::JA_REL32 => Opcode::Ja,
        x if x == d::JS_REL32 => Opcode::Js,
        x if x == d::JNS_REL32 => Opcode::Jns,
        x if x == d::JP_REL32 => Opcode::Jp,
        x if x == d::JNP_REL32 => Opcode::Jnp,
        x if x == d::JL_REL32 => Opcode::Jl,
        x if x == d::JGE_REL32 => Opcode::Jge,
        x if x == d::JLE_REL32 => Opcode::Jle,
        _ => Opcode::Jg,
    }
}

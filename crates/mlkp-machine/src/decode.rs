// CEP:FILE: crates/mlkp-machine/src/decode.rs
// CEP:WHAT: Disassembler for the encoded x86-64 subset: decodes one CodeObject back to structured instructions with their offsets and renders deterministic AT&T-style text that matches the assembly emitter instruction for instruction (ticket CEP-322).
// CEP:WHY: The standard's emission evidence requires a disassembly hash, and the equivalence between bytes and text must be checked mechanically rather than trusted; the decoder is that check, plus a debugging artifact.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns DecodeError for truncated input, an encoding outside the subset, a branch or internal call to a non-label offset, or trailing bytes after the epilogue.
// CEP:ASSUMES: the bytes came from encode::compile; arbitrary machine code may legitimately fall outside the subset and is rejected, not guessed at.
// CEP:COST: cold; O(insns).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs.

#![deny(missing_docs)]

use crate::encode::CodeObject;
use crate::mir::Opcode;
use crate::target::preg;

/// Decoder-side structural constants mirroring the encoder's.
mod d {
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
fn jcc_from_rel32(second: u8) -> Opcode {
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

/// Decoder failure modes.
#[derive(Clone, Debug, PartialEq)]
pub enum DecodeError {
    /// The byte stream ended inside one instruction.
    Truncated(u32),
    /// The byte sequence at this offset is not in the encodable subset.
    UnknownEncoding(u32),
    /// A branch or internal call resolved to an offset that is no label boundary.
    BadBranchTarget(u32),
}

impl std::fmt::Display for DecodeError {
    // CEP:WHAT: Formats the decode error.
    // CEP:WHY: Deterministic diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Truncated(o) => write!(f, "decode error: truncated at {o}"),
            DecodeError::UnknownEncoding(o) => {
                write!(f, "decode error: encoding outside the subset at {o}")
            }
            DecodeError::BadBranchTarget(o) => {
                write!(f, "decode error: branch lands at {o} which is no label")
            }
        }
    }
}

/// The kind of a decoded instruction: a Machine-IR opcode or a prologue or epilogue register move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DecKind {
    /// A regular instruction with a Machine-IR opcode.
    Insn(Opcode),
    /// Push of the register in `dst`.
    Push,
    /// Pop of the register in `dst`.
    Pop,
}

/// One decoded destination location: a register, a memory operand, or nothing.
#[derive(Clone, Debug, PartialEq)]
pub enum DecDst {
    /// Physical register id from the target table (xmm ids offset by 16).
    Reg(u32),
    /// Memory destination [base + disp] for read-modify-write forms.
    Mem {
        /// Base register id.
        base: u32,
        /// Displacement.
        disp: i64,
    },
    /// No destination operand.
    None,
}

/// One decoded instruction at a known offset.
#[derive(Clone, Debug, PartialEq)]
pub struct DecInsn {
    /// Instruction kind.
    pub kind: DecKind,
    /// Byte offset in the text section.
    pub offset: u32,
    /// Destination location (register, memory, or none).
    pub dst: DecDst,
    /// Operand 1 when present.
    pub src: Option<DecSrc>,
    /// Secondary source operand (the true source of a three-operand multiply).
    pub src2: Option<DecSrc>,
    /// Resolved branch or call target: a label for internal targets, a symbol for external calls.
    pub target: Option<String>,
}

impl DecKind {
    /// The Machine-IR opcode of a regular instruction.
    // CEP:WHAT: Unwraps the instruction kind.
    // CEP:WHY: The renderer keys mnemonic selection off the opcode.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics for Push and Pop, which the renderer handles separately.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
    pub fn into_op(self) -> Opcode {
        match self {
            DecKind::Insn(op) => op,
            _ => panic!("into_op() on a push or pop"),
        }
    }
}

/// One decoded source operand.
#[derive(Clone, Debug, PartialEq)]
pub enum DecSrc {
    /// Physical register id from the target table (xmm ids offset by 16).
    Reg(u32),
    /// Immediate value.
    Imm(i64),
    /// Memory [base + disp].
    Mem {
        /// Base register id.
        base: u32,
        /// Displacement.
        disp: i64,
    },
}

/// A byte reader over the text section.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Peeks one byte without consuming.
    // CEP:WHAT: Peeks one byte.
    // CEP:WHY: Prefix detection needs lookahead.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None at end of input.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    /// Consumes one byte.
    // CEP:WHAT: Consumes one byte.
    // CEP:WHY: Sequential decode.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics only when the caller skipped the bounds peek, which decode_one never does.
    // CEP:ASSUMES: a peek succeeded.
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
    fn take(&mut self) -> u8 {
        let b = self.bytes[self.pos];
        self.pos += 1;
        b
    }

    /// Consumes n bytes as a sign-extended little-endian integer.
    // CEP:WHAT: Reads a signed immediate or displacement.
    // CEP:WHY: imm8, imm32, imm64, disp8, disp32 tails.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics only when decode_insn skipped its bounds check.
    // CEP:ASSUMES: n is 1, 4, or 8 and the bytes exist.
    // CEP:COST: O(n).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
    fn take_int(&mut self, n: usize) -> i64 {
        let mut v: i64 = 0;
        for k in 0..n {
            v |= i64::from(self.take()) << (d::BYTE_BITS * k as i64);
        }
        let shift = match n {
            d::IMM8 => d::BYTE_SIGN_SHIFT,
            d::IMM32 => d::DWORD_SIGN_SHIFT,
            _ => 0, // imm64 needs no sign extension.
        };
        v << shift >> shift
    }
}

/// Decoded prefix state.
#[derive(Default)]
struct Prefixes {
    rex_w: bool,
    rex_r: bool,
    rex_b: bool,
    f2: bool,
    p66: bool,
}

/// Parses prefix bytes up to the primary opcode.
// CEP:WHAT: Parses prefixes.
// CEP:WHY: REX, F2, and 66 modify the following opcode.
// CEP:STATUS: complete
// CEP:FAILURE: Returns UnknownEncoding for a prefix the subset never emits.
// CEP:ASSUMES: the encoder emits prefixes in canonical order (segment-style first, REX last).
// CEP:COST: O(prefixes).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn parse_prefixes(r: &mut Reader, start: u32) -> Result<Prefixes, DecodeError> {
    let mut p = Prefixes::default();
    let mut rex_seen = false;
    loop {
        match r.peek() {
            Some(d::PRE_F2) if !rex_seen && !p.p66 && !p.f2 => {
                p.f2 = true;
                r.take();
            }
            Some(d::PRE_66) if !rex_seen && !p.f2 && !p.p66 => {
                p.p66 = true;
                r.take();
            }
            Some(b) if (d::REX_FIRST..=d::REX_LAST).contains(&b) && !rex_seen => {
                // Canonical order: F2/66 first, REX last; the subset's primary
                // opcodes never start in the REX range, so this is unambiguous.
                p.rex_w = b & d::REX_W != 0;
                p.rex_r = b & d::REX_R != 0;
                p.rex_b = b & d::REX_B != 0;
                rex_seen = true;
                r.take();
            }
            Some(_) => break,
            None => return Err(DecodeError::Truncated(start)),
        }
    }
    Ok(p)
}

/// Decodes one ModRM byte into (mod, reg, rm) as raw hardware numbers with REX bits applied.
///
/// Callers on integer paths map the numbers through `from_hw`; SSE callers use
/// them directly as xmm indexes because xmm encodings have no SP/BP swap.
// CEP:WHAT: Decodes ModRM.
// CEP:WHY: Every register or memory operand is described by it.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn decode_modrm(byte: u8, p: &Prefixes) -> (u8, u32, u32) {
    let b = u32::from(byte);
    let m = (b >> d::MOD_SHIFT) as u8;
    let reg = ((b >> d::REG_SHIFT) & d::REG_MASK) + u32::from(p.rex_r) * d::GPR_EXT;
    let rm = (b & d::REG_MASK) + u32::from(p.rex_b) * d::GPR_EXT;
    (m, reg, rm)
}

/// Maps a hardware register number back to its target-table id (the inverse of encode::hw).
// CEP:WHAT: Maps hardware numbers to table ids.
// CEP:WHY: Decoded registers must name the same table entries the encoder consumed.
// CEP:STATUS: complete
// CEP:FAILURE: Panics for numbers above 15; the subset never encodes more.
// CEP:ASSUMES: hardware numbers 0..=15.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn from_hw(h: u32) -> u32 {
    const LOW_GPR_MAX: u32 = 3;
    const HIGH_GPR_MIN: u32 = 8;
    const HIGH_GPR_MAX: u32 = 15;
    match h {
        0..=LOW_GPR_MAX => h,
        d::HW_SIB => preg::RSP,
        d::HW_RIP => preg::RBP,
        d::HW_RSI => preg::RSI,
        d::HW_RDI => preg::RDI,
        HIGH_GPR_MIN..=HIGH_GPR_MAX => h,
        _ => panic!("from_hw(): hardware number {h} outside the GPR range"),
    }
}

/// Reads the memory operand tail (optional SIB plus displacement) after a ModRM with mod != 3.
// CEP:WHAT: Reads a memory operand.
// CEP:WHY: Load and Store addressing.
// CEP:STATUS: complete
// CEP:FAILURE: Returns UnknownEncoding for SIB shapes the subset never emits (index registers, RIP-relative).
// CEP:ASSUMES: the encoder emits base-plus-displacement addressing only.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn read_mem(r: &mut Reader, m: u8, rm: u32, p: &Prefixes) -> Result<(u32, i64), DecodeError> {
    let mut base = rm;
    if base & d::REG_MASK == d::HW_SIB {
        let sib = r.take();
        let sb = u32::from(sib);
        let sib_base = (sb & d::REG_MASK) + u32::from(p.rex_b) * d::GPR_EXT;
        let sib_index = (sb >> d::REG_SHIFT) & d::REG_MASK;
        if sib_index != d::SIB_NO_INDEX {
            return Err(DecodeError::UnknownEncoding(r.pos as u32));
        }
        base = sib_base;
    }
    if base & d::REG_MASK == d::HW_RIP && m == 0 {
        // mod 00 rm 101 means RIP-relative; the subset never emits it.
        return Err(DecodeError::UnknownEncoding(r.pos as u32));
    }
    let disp = match m {
        0 => 0,
        1 => r.take_int(d::IMM8),
        d::MOD_DISP32_B => r.take_int(d::IMM32),
        _ => return Err(DecodeError::UnknownEncoding(r.pos as u32)),
    };
    Ok((from_hw(base), disp))
}

/// Decodes the whole text section into instructions with offsets.
// CEP:WHAT: Decodes a CodeObject.
// CEP:WHY: Evidence hashing and the bytes-versus-text equivalence check.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module errors.
// CEP:ASSUMES: labels cover every branch and internal call target.
// CEP:COST: O(insns).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs.
pub fn decode_insns(co: &CodeObject) -> Result<Vec<DecInsn>, DecodeError> {
    let mut out = Vec::new();
    let mut r = Reader {
        bytes: &co.text,
        pos: 0,
    };
    while r.pos < co.text.len() {
        let start = r.pos as u32;
        let insn = decode_one(&mut r, co, start)?;
        out.push(insn);
    }
    Ok(out)
}

/// Resolves a relative branch to its label.
// CEP:WHAT: Resolves branch targets.
// CEP:WHY: Branches are relative in bytes and symbolic in the IR.
// CEP:STATUS: complete
// CEP:FAILURE: Returns BadBranchTarget when the absolute offset is no label.
// CEP:ASSUMES: none
// CEP:COST: O(labels).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn resolve_branch(co: &CodeObject, from_end_of_insn: i64) -> Result<String, DecodeError> {
    let abs = (from_end_of_insn) as u32;
    co.labels
        .iter()
        .find(|(o, _)| *o == abs)
        .map(|(_, l)| l.clone())
        .ok_or(DecodeError::BadBranchTarget(abs))
}

/// Decodes exactly one instruction starting at the reader position.
// CEP:WHAT: Decodes one instruction.
// CEP:WHY: The decode walk.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module errors.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn decode_one(r: &mut Reader, co: &CodeObject, start: u32) -> Result<DecInsn, DecodeError> {
    let p = parse_prefixes(r, start)?;
    if r.pos >= r.bytes.len() {
        return Err(DecodeError::Truncated(start));
    }
    let opc = r.take();
    // Prologue and epilogue register moves (no prefixes allowed).
    if !p.f2 && !p.p66 && !p.rex_w && !p.rex_r {
        if (d::PUSH_BASE..=(d::PUSH_BASE + d::PUSH_SPAN)).contains(&opc) {
            let reg = u32::from(opc - d::PUSH_BASE) + u32::from(p.rex_b) * d::GPR_EXT;
            return Ok(DecInsn {
                kind: DecKind::Push,
                offset: start,
                dst: DecDst::Reg(from_hw(reg)),
                src: None,
                src2: None,
                target: None,
            });
        }
        if (d::POP_BASE..=(d::POP_BASE + d::PUSH_SPAN)).contains(&opc) {
            let reg = u32::from(opc - d::POP_BASE) + u32::from(p.rex_b) * d::GPR_EXT;
            return Ok(DecInsn {
                kind: DecKind::Pop,
                offset: start,
                dst: DecDst::Reg(from_hw(reg)),
                src: None,
                src2: None,
                target: None,
            });
        }
    }
    match opc {
        d::RET if !p.rex_w && !p.f2 && !p.p66 => Ok(DecInsn {
            kind: DecKind::Insn(Opcode::Ret),
            offset: start,
            dst: DecDst::None,
            src: None,
            src2: None,
            target: None,
        }),
        d::JMP_REL8
        | d::JL_REL8
        | d::JGE_REL8
        | d::JE_REL8
        | d::JNE_REL8
        | d::JLE_REL8
        | d::JG_REL8
        | d::JA_REL8
        | d::JAE_REL8
        | d::JB_REL8
        | d::JBE_REL8
        | d::JS_REL8
        | d::JNS_REL8
        | d::JO_REL8
        | d::JNO_REL8
        | d::JP_REL8
        | d::JNP_REL8
            if !p.rex_w && !p.f2 && !p.p66 =>
        {
            let rel = r.take_int(d::IMM8);
            let end = r.pos as i64;
            let label = resolve_branch(co, end + rel)?;
            Ok(DecInsn {
                kind: DecKind::Insn(match opc {
                    d::JMP_REL8 => Opcode::Jmp,
                    d::JL_REL8 => Opcode::Jl,
                    d::JGE_REL8 => Opcode::Jge,
                    d::JE_REL8 => Opcode::Je,
                    d::JNE_REL8 => Opcode::Jne,
                    d::JLE_REL8 => Opcode::Jle,
                    d::JG_REL8 => Opcode::Jg,
                    d::JA_REL8 => Opcode::Ja,
                    d::JAE_REL8 => Opcode::Jae,
                    d::JB_REL8 => Opcode::Jb,
                    d::JBE_REL8 => Opcode::Jbe,
                    d::JS_REL8 => Opcode::Js,
                    d::JNS_REL8 => Opcode::Jns,
                    d::JO_REL8 => Opcode::Jo,
                    d::JNO_REL8 => Opcode::Jno,
                    d::JP_REL8 => Opcode::Jp,
                    _ => Opcode::Jnp,
                }),
                offset: start,
                dst: DecDst::None,
                src: None,
                src2: None,
                target: Some(label),
            })
        }
        d::JMP_REL32 if !p.rex_w && !p.f2 && !p.p66 => {
            let rel = r.take_int(d::IMM32);
            let end = r.pos as i64;
            let label = resolve_branch(co, end + rel)?;
            Ok(DecInsn {
                kind: DecKind::Insn(Opcode::Jmp),
                offset: start,
                dst: DecDst::None,
                src: None,
                src2: None,
                target: Some(label),
            })
        }
        d::CALL_REL32 if !p.rex_w && !p.f2 && !p.p66 => {
            let field_pos = r.pos;
            let rel = r.take_int(d::IMM32);
            let end = r.pos as i64;
            let target = if let Some(rel_entry) =
                co.relocs.iter().find(|x| x.offset as usize == field_pos)
            {
                rel_entry.symbol.clone()
            } else {
                let abs = end + rel;
                format!(".L{}", resolve_branch(co, abs)?)
            };
            Ok(DecInsn {
                kind: DecKind::Insn(Opcode::Call),
                offset: start,
                dst: DecDst::None,
                src: None,
                src2: None,
                target: Some(target),
            })
        }
        d::MOVABS_BASE..=d::MOVABS_LAST if p.rex_w && !p.f2 && !p.p66 => {
            let reg = u32::from(opc - d::MOVABS_BASE) + u32::from(p.rex_b) * d::GPR_EXT;
            let imm = r.take_int(d::IMM64);
            Ok(DecInsn {
                kind: DecKind::Insn(Opcode::Mov),
                offset: start,
                dst: DecDst::Reg(from_hw(reg)),
                src: Some(DecSrc::Imm(imm)),
                src2: None,
                target: None,
            })
        }
        d::MOV_IMM32 if p.rex_w && !p.f2 && !p.p66 => {
            let modrm = r.take();
            let (m, reg, rm) = decode_modrm(modrm, &p);
            if m != d::MOD_REG as u8 || reg != 0 {
                return Err(DecodeError::UnknownEncoding(start));
            }
            let imm = r.take_int(d::IMM32);
            Ok(DecInsn {
                kind: DecKind::Insn(Opcode::Mov),
                offset: start,
                dst: DecDst::Reg(from_hw(rm)),
                src: Some(DecSrc::Imm(imm)),
                src2: None,
                target: None,
            })
        }
        d::MOV_MR | d::MOV_RM | d::MOV8_MR if !p.f2 && !p.p66 => {
            let modrm = r.take();
            let (m, reg, rm) = decode_modrm(modrm, &p);
            // The width follows REX.W: set for 64-bit moves, clear for the
            // 32-bit zero-extending moves, and the byte store never sets it.
            if opc == d::MOV8_MR {
                // Byte store: value in reg field, memory in rm.
                if m == d::MOD_REG as u8 {
                    return Err(DecodeError::UnknownEncoding(start));
                }
                let (base, disp) = read_mem(r, m, rm, &p)?;
                return Ok(DecInsn {
                    kind: DecKind::Insn(Opcode::Store8),
                    offset: start,
                    dst: DecDst::Reg(from_hw(reg)),
                    src: Some(DecSrc::Mem { base, disp }),
                    src2: None,
                    target: None,
                });
            }
            // Kind depends on the opcode, the width bit, and the operand location.
            let kind: Opcode = if p.rex_w {
                if opc == d::MOV_RM {
                    Opcode::Load
                } else if m == d::MOD_REG as u8 {
                    Opcode::Mov
                } else {
                    Opcode::Store
                }
            } else if opc == d::MOV_MR {
                Opcode::Store32
            } else {
                Opcode::MovZx32
            };
            if m == d::MOD_REG as u8 {
                // Register-register: 89 moves rm <- reg; 8B moves reg <- rm.
                let (dst, src) = if opc == d::MOV_MR {
                    (DecDst::Reg(from_hw(rm)), DecSrc::Reg(from_hw(reg)))
                } else {
                    (DecDst::Reg(from_hw(reg)), DecSrc::Reg(from_hw(rm)))
                };
                Ok(DecInsn {
                    kind: DecKind::Insn(kind),
                    offset: start,
                    dst,
                    src: Some(src),
                    src2: None,
                    target: None,
                })
            } else {
                let (base, disp) = read_mem(r, m, rm, &p)?;
                // Store, Store32, and Store8 put the value in the reg field;
                // Load and MovZx32 put the destination there.
                Ok(DecInsn {
                    kind: DecKind::Insn(kind),
                    offset: start,
                    dst: DecDst::Reg(from_hw(reg)),
                    src: Some(DecSrc::Mem { base, disp }),
                    src2: None,
                    target: None,
                })
            }
        }
        d::ADD_MR
        | d::SUB_MR
        | d::CMP_MR
        | d::ADD_RM
        | d::SUB_RM
        | d::CMP_RM
        | d::AND_MR
        | d::AND_RM
        | d::OR_MR
        | d::OR_RM
        | d::XOR_MR
        | d::XOR_RM
            if p.rex_w && !p.f2 && !p.p66 =>
        {
            let modrm = r.take();
            let (m, reg, rm) = decode_modrm(modrm, &p);
            let op = match opc {
                d::ADD_MR | d::ADD_RM => Opcode::Add,
                d::SUB_MR | d::SUB_RM => Opcode::Sub,
                d::AND_MR | d::AND_RM => Opcode::And,
                d::OR_MR | d::OR_RM => Opcode::Or,
                d::XOR_MR | d::XOR_RM => Opcode::Xor,
                _ => Opcode::Cmp,
            };
            let is_mr = matches!(
                opc,
                d::ADD_MR | d::SUB_MR | d::CMP_MR | d::AND_MR | d::OR_MR | d::XOR_MR
            );
            if m == d::MOD_REG as u8 {
                // Register-register: destination in rm, source in reg.
                Ok(DecInsn {
                    kind: DecKind::Insn(op),
                    offset: start,
                    dst: DecDst::Reg(from_hw(rm)),
                    src: Some(DecSrc::Reg(from_hw(reg))),
                    src2: None,
                    target: None,
                })
            } else if is_mr {
                // Memory-destination read-modify-write: op [mem], reg.
                let (base, disp) = read_mem(r, m, rm, &p)?;
                Ok(DecInsn {
                    kind: DecKind::Insn(op),
                    offset: start,
                    dst: DecDst::Mem {
                        base: from_hw(base),
                        disp,
                    },
                    src: Some(DecSrc::Reg(from_hw(reg))),
                    src2: None,
                    target: None,
                })
            } else {
                // Memory-source load form: op reg, [base+disp].
                let (base, disp) = read_mem(r, m, rm, &p)?;
                Ok(DecInsn {
                    kind: DecKind::Insn(op),
                    offset: start,
                    dst: DecDst::Reg(from_hw(reg)),
                    src: Some(DecSrc::Mem { base, disp }),
                    src2: None,
                    target: None,
                })
            }
        }
        d::GRP1_IB | d::GRP1_ID if p.rex_w && !p.f2 && !p.p66 => {
            let modrm = r.take();
            let (m, ext, rm) = decode_modrm(modrm, &p);
            if m != d::MOD_REG as u8 {
                return Err(DecodeError::UnknownEncoding(start));
            }
            let op = match ext {
                x if x == u32::from(d::EXT_ADD) => Opcode::Add,
                x if x == u32::from(d::EXT_SUB) => Opcode::Sub,
                x if x == u32::from(d::EXT_CMP) => Opcode::Cmp,
                x if x == u32::from(d::EXT_AND) => Opcode::And,
                x if x == u32::from(d::EXT_OR) => Opcode::Or,
                x if x == u32::from(d::EXT_XOR) => Opcode::Xor,
                _ => return Err(DecodeError::UnknownEncoding(start)),
            };
            let width = if opc == d::GRP1_IB { d::IMM8 } else { d::IMM32 };
            let imm = r.take_int(width);
            if m == d::MOD_REG as u8 {
                Ok(DecInsn {
                    kind: DecKind::Insn(op),
                    offset: start,
                    dst: DecDst::Reg(from_hw(rm)),
                    src: Some(DecSrc::Imm(imm)),
                    src2: None,
                    target: None,
                })
            } else {
                let (base, disp) = read_mem(r, m, rm, &p)?;
                Ok(DecInsn {
                    kind: DecKind::Insn(op),
                    offset: start,
                    dst: DecDst::Mem {
                        base: from_hw(base),
                        disp,
                    },
                    src: Some(DecSrc::Imm(imm)),
                    src2: None,
                    target: None,
                })
            }
        }
        // TEST r/m64, r64: destination side in rm, source in reg.
        d::TEST_MR if p.rex_w && !p.f2 && !p.p66 => {
            let modrm = r.take();
            let (m, reg, rm) = decode_modrm(modrm, &p);
            if m == d::MOD_REG as u8 {
                Ok(DecInsn {
                    kind: DecKind::Insn(Opcode::Test),
                    offset: start,
                    dst: DecDst::Reg(from_hw(rm)),
                    src: Some(DecSrc::Reg(from_hw(reg))),
                    src2: None,
                    target: None,
                })
            } else {
                let (base, disp) = read_mem(r, m, rm, &p)?;
                Ok(DecInsn {
                    kind: DecKind::Insn(Opcode::Test),
                    offset: start,
                    dst: DecDst::Mem {
                        base: from_hw(base),
                        disp,
                    },
                    src: Some(DecSrc::Reg(from_hw(reg))),
                    src2: None,
                    target: None,
                })
            }
        }
        // F7 unary group: test imm32 (/0), not (/2), neg (/3), idiv (/7).
        d::GRP3 if p.rex_w && !p.f2 && !p.p66 => {
            let modrm = r.take();
            let (m, ext, rm) = decode_modrm(modrm, &p);
            let dst = if m == d::MOD_REG as u8 {
                DecDst::Reg(from_hw(rm))
            } else {
                let (base, disp) = read_mem(r, m, rm, &p)?;
                DecDst::Mem {
                    base: from_hw(base),
                    disp,
                }
            };
            match ext {
                x if x == u32::from(d::EXT_TEST) => {
                    // The encoder uses the A9 short form for RAX, so F7 /0 only
                    // appears for other registers and for memory.
                    let imm = r.take_int(d::IMM32);
                    Ok(DecInsn {
                        kind: DecKind::Insn(Opcode::Test),
                        offset: start,
                        dst,
                        src: Some(DecSrc::Imm(imm)),
                        src2: None,
                        target: None,
                    })
                }
                x if x == u32::from(d::EXT_NOT) => Ok(DecInsn {
                    kind: DecKind::Insn(Opcode::Not),
                    offset: start,
                    dst,
                    src: None,
                    src2: None,
                    target: None,
                }),
                x if x == u32::from(d::EXT_NEG) => Ok(DecInsn {
                    kind: DecKind::Insn(Opcode::Neg),
                    offset: start,
                    dst,
                    src: None,
                    src2: None,
                    target: None,
                }),
                x if x == u32::from(d::EXT_IDIV) => Ok(DecInsn {
                    kind: DecKind::Insn(Opcode::IDiv),
                    offset: start,
                    dst,
                    src: None,
                    src2: None,
                    target: None,
                }),
                _ => Err(DecodeError::UnknownEncoding(start)),
            }
        }
        d::TEST_EAX_ID if p.rex_w && !p.f2 && !p.p66 => {
            let imm = r.take_int(d::IMM32);
            Ok(DecInsn {
                kind: DecKind::Insn(Opcode::Test),
                offset: start,
                dst: DecDst::Reg(preg::RAX),
                src: Some(DecSrc::Imm(imm)),
                src2: None,
                target: None,
            })
        }
        // Shifts: immediate counts and the CL form.
        d::SHIFT_IMM if p.rex_w && !p.f2 && !p.p66 => {
            let modrm = r.take();
            let (m, ext, rm) = decode_modrm(modrm, &p);
            let op = match ext {
                x if x == u32::from(d::EXT_SHL) => Opcode::Shl,
                x if x == u32::from(d::EXT_SHR) => Opcode::Shr,
                x if x == u32::from(d::EXT_IDIV) => Opcode::Sar, // SAR shares /7.
                _ => return Err(DecodeError::UnknownEncoding(start)),
            };
            let cnt = r.take_int(d::IMM8);
            if m == d::MOD_REG as u8 {
                Ok(DecInsn {
                    kind: DecKind::Insn(op),
                    offset: start,
                    dst: DecDst::Reg(from_hw(rm)),
                    src: Some(DecSrc::Imm(cnt)),
                    src2: None,
                    target: None,
                })
            } else {
                let (base, disp) = read_mem(r, m, rm, &p)?;
                Ok(DecInsn {
                    kind: DecKind::Insn(op),
                    offset: start,
                    dst: DecDst::Mem {
                        base: from_hw(base),
                        disp,
                    },
                    src: Some(DecSrc::Imm(cnt)),
                    src2: None,
                    target: None,
                })
            }
        }
        d::SHIFT_ONE if p.rex_w && !p.f2 && !p.p66 => {
            let modrm = r.take();
            let (m, ext, rm) = decode_modrm(modrm, &p);
            let op = match ext {
                x if x == u32::from(d::EXT_SHL) => Opcode::Shl,
                x if x == u32::from(d::EXT_SHR) => Opcode::Shr,
                x if x == u32::from(d::EXT_IDIV) => Opcode::Sar,
                _ => return Err(DecodeError::UnknownEncoding(start)),
            };
            if m == d::MOD_REG as u8 {
                Ok(DecInsn {
                    kind: DecKind::Insn(op),
                    offset: start,
                    dst: DecDst::Reg(from_hw(rm)),
                    src: Some(DecSrc::Imm(1)),
                    src2: None,
                    target: None,
                })
            } else {
                let (base, disp) = read_mem(r, m, rm, &p)?;
                Ok(DecInsn {
                    kind: DecKind::Insn(op),
                    offset: start,
                    dst: DecDst::Mem {
                        base: from_hw(base),
                        disp,
                    },
                    src: Some(DecSrc::Imm(1)),
                    src2: None,
                    target: None,
                })
            }
        }
        d::SHIFT_CL if p.rex_w && !p.f2 && !p.p66 => {
            let modrm = r.take();
            let (m, ext, rm) = decode_modrm(modrm, &p);
            let op = match ext {
                x if x == u32::from(d::EXT_SHL) => Opcode::Shl,
                x if x == u32::from(d::EXT_SHR) => Opcode::Shr,
                x if x == u32::from(d::EXT_IDIV) => Opcode::Sar,
                _ => return Err(DecodeError::UnknownEncoding(start)),
            };
            if m == d::MOD_REG as u8 {
                Ok(DecInsn {
                    kind: DecKind::Insn(op),
                    offset: start,
                    dst: DecDst::Reg(from_hw(rm)),
                    src: Some(DecSrc::Reg(preg::RCX)),
                    src2: None,
                    target: None,
                })
            } else {
                let (base, disp) = read_mem(r, m, rm, &p)?;
                Ok(DecInsn {
                    kind: DecKind::Insn(op),
                    offset: start,
                    dst: DecDst::Mem {
                        base: from_hw(base),
                        disp,
                    },
                    src: Some(DecSrc::Reg(preg::RCX)),
                    src2: None,
                    target: None,
                })
            }
        }
        // CQO.
        d::CQO if p.rex_w && !p.f2 && !p.p66 => Ok(DecInsn {
            kind: DecKind::Insn(Opcode::Cqo),
            offset: start,
            dst: DecDst::None,
            src: None,
            src2: None,
            target: None,
        }),
        // LEA.
        d::LEA if p.rex_w && !p.f2 && !p.p66 => {
            let modrm = r.take();
            let (m, reg, rm) = decode_modrm(modrm, &p);
            if m == d::MOD_REG as u8 {
                return Err(DecodeError::UnknownEncoding(start));
            }
            let (base, disp) = read_mem(r, m, rm, &p)?;
            Ok(DecInsn {
                kind: DecKind::Insn(Opcode::Lea),
                offset: start,
                dst: DecDst::Reg(from_hw(reg)),
                src: Some(DecSrc::Mem { base, disp }),
                src2: None,
                target: None,
            })
        }
        // Three-operand multiply.
        d::IMUL3_IMM8 | d::IMUL3_IMM32 if p.rex_w && !p.f2 && !p.p66 => {
            let modrm = r.take();
            let (m, reg, rm) = decode_modrm(modrm, &p);
            let src2 = if m == d::MOD_REG as u8 {
                DecSrc::Reg(from_hw(rm))
            } else {
                let (base, disp) = read_mem(r, m, rm, &p)?;
                DecSrc::Mem { base, disp }
            };
            let imm = if opc == d::IMUL3_IMM8 {
                r.take_int(d::IMM8)
            } else {
                r.take_int(d::IMM32)
            };
            Ok(DecInsn {
                kind: DecKind::Insn(Opcode::IMul3),
                offset: start,
                dst: DecDst::Reg(from_hw(reg)),
                src: Some(DecSrc::Imm(imm)),
                src2: Some(src2),
                target: None,
            })
        }
        // PUSH imm.
        d::PUSH_IMM8 if !p.rex_w && !p.f2 && !p.p66 => {
            let imm = r.take_int(d::IMM8);
            Ok(DecInsn {
                kind: DecKind::Push,
                offset: start,
                dst: DecDst::None,
                src: Some(DecSrc::Imm(imm)),
                src2: None,
                target: None,
            })
        }
        d::PUSH_IMM32 if !p.rex_w && !p.f2 && !p.p66 => {
            let imm = r.take_int(d::IMM32);
            Ok(DecInsn {
                kind: DecKind::Push,
                offset: start,
                dst: DecDst::None,
                src: Some(DecSrc::Imm(imm)),
                src2: None,
                target: None,
            })
        }
        d::ADD_EAX_ID | d::SUB_EAX_ID | d::CMP_EAX_ID if p.rex_w && !p.f2 && !p.p66 => {
            let imm = r.take_int(d::IMM32);
            let op = match opc {
                d::ADD_EAX_ID => Opcode::Add,
                d::SUB_EAX_ID => Opcode::Sub,
                _ => Opcode::Cmp,
            };
            Ok(DecInsn {
                kind: DecKind::Insn(op),
                offset: start,
                dst: DecDst::Reg(preg::RAX),
                src: Some(DecSrc::Imm(imm)),
                src2: None,
                target: None,
            })
        }
        d::ESCAPE => {
            let opc2 = if r.pos < r.bytes.len() {
                r.take()
            } else {
                return Err(DecodeError::Truncated(start));
            };
            match (p.f2, p.p66, opc2) {
                (
                    true,
                    false,
                    d::ADDSD
                    | d::MULSD
                    | d::SUBSD
                    | d::DIVSD
                    | d::SQRTSD
                    | d::MOVSD_LOAD
                    | d::MOVSD_STORE
                    | d::CVTSI2SD
                    | d::CVTTSD2SI,
                ) => decode_sse(r, opc2, &p, start),
                (false, true, d::UCOMISD | d::MOVAPD) => decode_sse(r, opc2, &p, start),
                (false, false, d::MOVZ8 | d::MOVSX32) => {
                    if !p.rex_w {
                        return Err(DecodeError::UnknownEncoding(start));
                    }
                    let modrm = r.take();
                    let (m, reg, rm) = decode_modrm(modrm, &p);
                    let op = if opc2 == d::MOVZ8 {
                        Opcode::MovZx8
                    } else {
                        Opcode::MovSx32
                    };
                    if m == d::MOD_REG as u8 {
                        Ok(DecInsn {
                            kind: DecKind::Insn(op),
                            offset: start,
                            dst: DecDst::Reg(from_hw(reg)),
                            src: Some(DecSrc::Reg(from_hw(rm))),
                            src2: None,
                            target: None,
                        })
                    } else {
                        let (base, disp) = read_mem(r, m, rm, &p)?;
                        Ok(DecInsn {
                            kind: DecKind::Insn(op),
                            offset: start,
                            dst: DecDst::Reg(from_hw(reg)),
                            src: Some(DecSrc::Mem { base, disp }),
                            src2: None,
                            target: None,
                        })
                    }
                }
                (false, false, second)
                    if (d::JCC_REL32_FIRST..=d::JCC_REL32_LAST).contains(&second) =>
                {
                    let rel = r.take_int(d::IMM32);
                    let end = r.pos as i64;
                    let label = resolve_branch(co, end + rel)?;
                    Ok(DecInsn {
                        kind: DecKind::Insn(jcc_from_rel32(second)),
                        offset: start,
                        dst: DecDst::None,
                        src: None,
                        src2: None,
                        target: Some(label),
                    })
                }
                (false, false, d::IMUL_RM) => {
                    if !p.rex_w {
                        return Err(DecodeError::UnknownEncoding(start));
                    }
                    let modrm = r.take();
                    let (m, reg, rm) = decode_modrm(modrm, &p);
                    if m == d::MOD_REG as u8 {
                        Ok(DecInsn {
                            kind: DecKind::Insn(Opcode::IMul),
                            offset: start,
                            dst: DecDst::Reg(from_hw(rm)),
                            src: Some(DecSrc::Reg(from_hw(reg))),
                            src2: None,
                            target: None,
                        })
                    } else {
                        let (base, disp) = read_mem(r, m, rm, &p)?;
                        Ok(DecInsn {
                            kind: DecKind::Insn(Opcode::IMul),
                            offset: start,
                            dst: DecDst::Reg(from_hw(reg)),
                            src: Some(DecSrc::Mem { base, disp }),
                            src2: None,
                            target: None,
                        })
                    }
                }
                (false, false, d::JL_REL32 | d::JGE_REL32) => {
                    let rel = r.take_int(d::IMM32);
                    let end = r.pos as i64;
                    let label = resolve_branch(co, end + rel)?;
                    Ok(DecInsn {
                        kind: DecKind::Insn(if opc2 == d::JL_REL32 {
                            Opcode::Jl
                        } else {
                            Opcode::Jge
                        }),
                        offset: start,
                        dst: DecDst::None,
                        src: None,
                        src2: None,
                        target: Some(label),
                    })
                }
                _ => Err(DecodeError::UnknownEncoding(start)),
            }
        }
        _ => Err(DecodeError::UnknownEncoding(start)),
    }
}

/// Decodes one SSE instruction (addsd, mulsd, movsd load and store).
// CEP:WHAT: Decodes SSE forms.
// CEP:WHY: Shared tail of the F2-prefixed two-byte opcodes.
// CEP:STATUS: complete
// CEP:FAILURE: Returns UnknownEncoding for memory or register shapes the subset never emits.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn decode_sse(r: &mut Reader, opc2: u8, p: &Prefixes, start: u32) -> Result<DecInsn, DecodeError> {
    let modrm = r.take();
    let (m, reg, rm) = decode_modrm(modrm, p);
    let xmm_id = |v: u32| v + d::XMM_TABLE_BASE;
    // Scalar-double binary operations: destination in reg, source in rm.
    let binop = match opc2 {
        x if x == d::ADDSD => Some(Opcode::AddSd),
        x if x == d::SUBSD => Some(Opcode::SubSd),
        x if x == d::MULSD => Some(Opcode::MulSd),
        x if x == d::DIVSD => Some(Opcode::DivSd),
        x if x == d::SQRTSD => Some(Opcode::SqrtSd),
        x if x == d::UCOMISD && p.p66 => Some(Opcode::UComisd),
        _ => None,
    };
    if let Some(op) = binop {
        return if m == d::MOD_REG as u8 {
            // SSE ModRM: reg is the destination, rm the source.
            Ok(DecInsn {
                kind: DecKind::Insn(op),
                offset: start,
                dst: DecDst::Reg(xmm_id(reg)),
                src: Some(DecSrc::Reg(xmm_id(rm))),
                src2: None,
                target: None,
            })
        } else {
            let (base, disp) = read_mem(r, m, rm, p)?;
            Ok(DecInsn {
                kind: DecKind::Insn(op),
                offset: start,
                dst: DecDst::Reg(xmm_id(reg)),
                src: Some(DecSrc::Mem { base, disp }),
                src2: None,
                target: None,
            })
        };
    }
    // Integer-to-double and double-to-integer conversions.
    if opc2 == d::CVTSI2SD && p.f2 && p.rex_w {
        if m == d::MOD_REG as u8 {
            return Ok(DecInsn {
                kind: DecKind::Insn(Opcode::CvtSi2Sd),
                offset: start,
                dst: DecDst::Reg(xmm_id(reg)),
                src: Some(DecSrc::Reg(from_hw(rm))),
                src2: None,
                target: None,
            });
        }
        let (base, disp) = read_mem(r, m, rm, p)?;
        return Ok(DecInsn {
            kind: DecKind::Insn(Opcode::CvtSi2Sd),
            offset: start,
            dst: DecDst::Reg(xmm_id(reg)),
            src: Some(DecSrc::Mem { base, disp }),
            src2: None,
            target: None,
        });
    }
    if opc2 == d::CVTTSD2SI && p.f2 && p.rex_w {
        if m == d::MOD_REG as u8 {
            return Ok(DecInsn {
                kind: DecKind::Insn(Opcode::CvtSd2Si),
                offset: start,
                dst: DecDst::Reg(from_hw(reg)),
                src: Some(DecSrc::Reg(xmm_id(rm))),
                src2: None,
                target: None,
            });
        }
        let (base, disp) = read_mem(r, m, rm, p)?;
        return Ok(DecInsn {
            kind: DecKind::Insn(Opcode::CvtSd2Si),
            offset: start,
            dst: DecDst::Reg(from_hw(reg)),
            src: Some(DecSrc::Mem { base, disp }),
            src2: None,
            target: None,
        });
    }
    match (opc2, m) {
        (d::MOVSD_LOAD, m) if m <= d::MOD_DISP32 as u8 => {
            let (base, disp) = read_mem(r, m, rm, p)?;
            Ok(DecInsn {
                kind: DecKind::Insn(Opcode::Load),
                offset: start,
                dst: DecDst::Reg(xmm_id(reg)),
                src: Some(DecSrc::Mem { base, disp }),
                src2: None,
                target: None,
            })
        }
        (d::MOVSD_STORE, m) if m <= d::MOD_DISP32 as u8 => {
            let (base, disp) = read_mem(r, m, rm, p)?;
            Ok(DecInsn {
                kind: DecKind::Insn(Opcode::Store),
                offset: start,
                dst: DecDst::Reg(xmm_id(reg)),
                src: Some(DecSrc::Mem { base, disp }),
                src2: None,
                target: None,
            })
        }
        (d::MOVAPD, m) if m == d::MOD_REG as u8 && p.p66 => {
            // movapd reg-reg: SSE ModRM puts the destination in reg.
            Ok(DecInsn {
                kind: DecKind::Insn(Opcode::Mov),
                offset: start,
                dst: DecDst::Reg(xmm_id(reg)),
                src: Some(DecSrc::Reg(xmm_id(rm))),
                src2: None,
                target: None,
            })
        }
        _ => Err(DecodeError::UnknownEncoding(start)),
    }
}

/// Renders one decoded instruction exactly as the assembly emitter does.
// CEP:WHAT: Renders one decoded instruction.
// CEP:WHY: The bytes-versus-text equivalence check compares this output against emit().
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the target table maps ids to the same names emit() uses.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn render(insn: &DecInsn, target: &crate::target::X86_64Target) -> String {
    let name = |id: u32| -> String { format!("%{}", target.reg_name(id)) };
    let name32 = |id: u32| -> String { format!("%{}", crate::target::reg_name32(id)) };
    let name8 = |id: u32| -> String { format!("%{}", crate::target::reg_name8(id)) };
    let dst_str = match &insn.dst {
        DecDst::Reg(r) => name(*r),
        DecDst::Mem { base, disp } => format!("{disp}({})", name(*base)),
        DecDst::None => String::new(),
    };
    let src_str = match &insn.src {
        Some(DecSrc::Reg(r)) => name(*r),
        Some(DecSrc::Imm(v)) => format!("${v}"),
        Some(DecSrc::Mem { base, disp }) => format!("{disp}({})", name(*base)),
        None => String::new(),
    };
    // The ISA renames narrower register views; width moves must render the
    // names the width actually addresses.
    let width_dst = match (&insn.dst, insn.kind) {
        (DecDst::Reg(r), DecKind::Insn(Opcode::MovZx32 | Opcode::Store32)) => name32(*r),
        (DecDst::Reg(r), DecKind::Insn(Opcode::Store8)) => name8(*r),
        (r, _) => match r {
            DecDst::Reg(v) => name(*v),
            DecDst::Mem { base, disp } => format!("{disp}({})", name(*base)),
            DecDst::None => String::new(),
        },
    };
    let mnemonic = |op: Opcode| -> &'static str {
        match op {
            Opcode::Mov => match &insn.dst {
                DecDst::Reg(r) if *r >= d::XMM_MIN_ID => "movapd",
                _ => "movq",
            },
            Opcode::MovZx8 => "movzbq",
            Opcode::MovZx32 => "movl",
            Opcode::MovSx32 => "movslq",
            Opcode::Lea => "leaq",
            Opcode::Add => "addq",
            Opcode::Sub => "subq",
            Opcode::IMul => "imulq",
            Opcode::IMul3 => "imulq",
            Opcode::IDiv => "idivq",
            Opcode::Cqo => "cqto",
            Opcode::And => "andq",
            Opcode::Or => "orq",
            Opcode::Xor => "xorq",
            Opcode::Not => "notq",
            Opcode::Neg => "negq",
            Opcode::Shl => "shlq",
            Opcode::Shr => "shrq",
            Opcode::Sar => "sarq",
            Opcode::Cmp => "cmpq",
            Opcode::Test => "testq",
            Opcode::Push => "pushq",
            Opcode::Pop => "popq",
            Opcode::Je => "je",
            Opcode::Jne => "jne",
            Opcode::Jl => "jl",
            Opcode::Jle => "jle",
            Opcode::Jg => "jg",
            Opcode::Jge => "jge",
            Opcode::Ja => "ja",
            Opcode::Jae => "jae",
            Opcode::Jb => "jb",
            Opcode::Jbe => "jbe",
            Opcode::Js => "js",
            Opcode::Jns => "jns",
            Opcode::Jo => "jo",
            Opcode::Jno => "jno",
            Opcode::Jp => "jp",
            Opcode::Jnp => "jnp",
            Opcode::Jmp => "jmp",
            Opcode::Call => "call",
            Opcode::Ret => "ret",
            Opcode::Load => match &insn.dst {
                DecDst::Reg(r) if *r >= d::XMM_MIN_ID => "movsd",
                _ => "movq",
            },
            Opcode::Store => match &insn.src {
                Some(DecSrc::Reg(r)) if *r >= d::XMM_MIN_ID => "movsd",
                _ => "movq",
            },
            Opcode::Store8 => "movb",
            Opcode::Store32 => "movl",
            Opcode::AddSd => "addsd",
            Opcode::SubSd => "subsd",
            Opcode::MulSd => "mulsd",
            Opcode::DivSd => "divsd",
            Opcode::SqrtSd => "sqrtsd",
            Opcode::UComisd => "ucomisd",
            Opcode::CvtSi2Sd => "cvtsi2sdq",
            Opcode::CvtSd2Si => "cvttsd2siq",
        }
    };
    match insn.kind {
        DecKind::Push => match &insn.src {
            Some(DecSrc::Imm(v)) => format!("  pushq ${v}\n"),
            _ => format!("  pushq {}\n", width_dst),
        },
        DecKind::Pop => format!("  popq {}\n", width_dst),
        DecKind::Insn(op) => {
            let m = mnemonic(op);
            match op {
                Opcode::Load => format!("  {m} {src_str}, {width_dst}\n"),
                // AT&T stores print the value first, then the address.
                Opcode::Store | Opcode::Store8 | Opcode::Store32 => {
                    format!("  {m} {dst_str}, {src_str}\n")
                }
                Opcode::Ret => "  ret\n".to_string(),
                Opcode::Cqo => "  cqto\n".to_string(),
                Opcode::Jmp => format!("  jmp .L{}\n", insn.target.clone().unwrap_or_default()),
                Opcode::Call => format!("  call {}\n", insn.target.clone().unwrap_or_default()),
                op if op.is_conditional_branch() => {
                    format!("  {m} .L{}\n", insn.target.clone().unwrap_or_default())
                }
                Opcode::IMul3 => {
                    let imm = match &insn.src {
                        Some(DecSrc::Imm(v)) => *v,
                        _ => 0,
                    };
                    let true_src = match &insn.src2 {
                        Some(x) => match x {
                            DecSrc::Reg(r) => name(*r),
                            DecSrc::Mem { base, disp } => format!("{disp}({})", name(*base)),
                            DecSrc::Imm(_) => String::new(),
                        },
                        None => String::new(),
                    };
                    format!("  {m} ${imm}, {true_src}, {width_dst}\n")
                }
                Opcode::Shl | Opcode::Shr | Opcode::Sar => match &insn.src {
                    Some(DecSrc::Reg(r)) => format!("  {m} {}, {}\n", name8(*r), width_dst),
                    _ => format!("  {m} {src_str}, {width_dst}\n"),
                },
                Opcode::MovSx32 => match &insn.src {
                    Some(DecSrc::Reg(r)) => format!("  {m} {}, {}\n", name32(*r), width_dst),
                    _ => format!("  {m} {src_str}, {width_dst}\n"),
                },
                Opcode::MovZx8 => match &insn.src {
                    Some(DecSrc::Reg(r)) => format!("  {m} {}, {}\n", name8(*r), width_dst),
                    _ => format!("  {m} {src_str}, {width_dst}\n"),
                },
                Opcode::CvtSd2Si | Opcode::CvtSi2Sd => format!("  {m} {src_str}, {width_dst}\n"),
                _ => {
                    if src_str.is_empty() {
                        format!("  {m} {width_dst}\n")
                    } else {
                        format!("  {m} {src_str}, {width_dst}\n")
                    }
                }
            }
        }
    }
}
/// Disassembles one CodeObject into assembly text matching emit() minus its header comments.
// CEP:WHAT: Disassembles one CodeObject.
// CEP:WHY: The standard's disassembly-hash evidence and the mechanical bytes-versus-text equivalence check.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module errors.
// CEP:ASSUMES: labels cover every branch and internal call target; every label offset coincides with an instruction start or the epilogue.
// CEP:COST: O(insns + labels).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs.
pub fn disassemble(
    co: &CodeObject,
    function: &str,
    target: &crate::target::X86_64Target,
) -> Result<String, DecodeError> {
    let insns = decode_insns(co)?;
    let mut out = String::new();
    out.push_str(".text\n");
    out.push_str(&format!(".globl {function}\n{function}:\n"));
    let mut labels = co.labels.clone();
    labels.sort_by_key(|(o, _)| *o);
    let mut li = 0usize;
    for insn in &insns {
        while li < labels.len() && labels[li].0 == insn.offset {
            let (_, name) = &labels[li];
            out.push_str(&format!(".L{name}:\n"));
            li += 1;
        }
        out.push_str(&render(insn, target));
    }
    // Remaining labels sit at or after the final instruction (the epilogue label
    // lands exactly at the ret-bearing epilogue start, already printed above
    // when it coincides; anything left is printed in order).
    while li < labels.len() {
        let (_, name) = &labels[li];
        out.push_str(&format!(".L{name}:\n"));
        li += 1;
    }
    // emit() closes the function with a blank line; match it byte for byte.
    out.push('\n');
    Ok(out)
}

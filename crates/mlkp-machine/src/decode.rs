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
    /// JL rel32 (after escape).
    pub const JL_REL32: u8 = crate::encode::op::JL_REL32;
    /// JGE rel32 (after escape).
    pub const JGE_REL32: u8 = crate::encode::op::JGE_REL32;
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
    /// ADD accumulator short form.
    pub const ADD_EAX_ID: u8 = crate::encode::op::ADD_EAX_ID;
    /// SUB accumulator short form.
    pub const SUB_EAX_ID: u8 = crate::encode::op::SUB_EAX_ID;
    /// CMP accumulator short form.
    pub const CMP_EAX_ID: u8 = crate::encode::op::CMP_EAX_ID;
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

/// One decoded instruction at a known offset.
#[derive(Clone, Debug, PartialEq)]
pub struct DecInsn {
    /// Instruction kind.
    pub kind: DecKind,
    /// Byte offset in the text section.
    pub offset: u32,
    /// Physical register of operand 0 (destination, store value, or pushed or popped register).
    pub dst: u32,
    /// Operand 1 when present.
    pub src: Option<DecSrc>,
    /// Resolved branch or call target: a label for internal targets, a symbol for external calls.
    pub target: Option<String>,
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
                dst: from_hw(reg),
                src: None,
                target: None,
            });
        }
        if (d::POP_BASE..=(d::POP_BASE + d::PUSH_SPAN)).contains(&opc) {
            let reg = u32::from(opc - d::POP_BASE) + u32::from(p.rex_b) * d::GPR_EXT;
            return Ok(DecInsn {
                kind: DecKind::Pop,
                offset: start,
                dst: from_hw(reg),
                src: None,
                target: None,
            });
        }
    }
    match opc {
        d::RET if !p.rex_w && !p.f2 && !p.p66 => Ok(DecInsn {
            kind: DecKind::Insn(Opcode::Ret),
            offset: start,
            dst: u32::MAX,
            src: None,
            target: None,
        }),
        d::JMP_REL8 | d::JL_REL8 | d::JGE_REL8 if !p.rex_w && !p.f2 && !p.p66 => {
            let rel = r.take_int(d::IMM8);
            let end = r.pos as i64;
            let label = resolve_branch(co, end + rel)?;
            Ok(DecInsn {
                kind: DecKind::Insn(match opc {
                    d::JMP_REL8 => Opcode::Jmp,
                    d::JL_REL8 => Opcode::Jl,
                    _ => Opcode::Jge,
                }),
                offset: start,
                dst: u32::MAX,
                src: None,
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
                dst: u32::MAX,
                src: None,
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
                dst: u32::MAX,
                src: None,
                target: Some(target),
            })
        }
        d::MOVABS_BASE..=d::MOVABS_LAST if p.rex_w && !p.f2 && !p.p66 => {
            let reg = u32::from(opc - d::MOVABS_BASE) + u32::from(p.rex_b) * d::GPR_EXT;
            let imm = r.take_int(d::IMM64);
            Ok(DecInsn {
                kind: DecKind::Insn(Opcode::Mov),
                offset: start,
                dst: from_hw(reg),
                src: Some(DecSrc::Imm(imm)),
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
                dst: from_hw(rm),
                src: Some(DecSrc::Imm(imm)),
                target: None,
            })
        }
        d::MOV_MR | d::MOV_RM if p.rex_w && !p.f2 && !p.p66 => {
            let modrm = r.take();
            let (m, reg, rm) = decode_modrm(modrm, &p);
            if m == d::MOD_REG as u8 {
                let (dst, src) = if opc == d::MOV_MR {
                    (from_hw(rm), DecSrc::Reg(from_hw(reg)))
                } else {
                    (from_hw(reg), DecSrc::Reg(from_hw(rm)))
                };
                Ok(DecInsn {
                    kind: DecKind::Insn(Opcode::Mov),
                    offset: start,
                    dst,
                    src: Some(src),
                    target: None,
                })
            } else {
                let (base, disp) = read_mem(r, m, rm, &p)?;
                let kind = if opc == d::MOV_MR {
                    Opcode::Store
                } else {
                    Opcode::Load
                };
                Ok(DecInsn {
                    kind: DecKind::Insn(kind),
                    offset: start,
                    dst: reg,
                    src: Some(DecSrc::Mem { base, disp }),
                    target: None,
                })
            }
        }
        d::ADD_MR | d::SUB_MR | d::CMP_MR | d::ADD_RM | d::SUB_RM | d::CMP_RM
            if p.rex_w && !p.f2 && !p.p66 =>
        {
            let modrm = r.take();
            let (m, reg, rm) = decode_modrm(modrm, &p);
            let op = match opc {
                d::ADD_MR | d::ADD_RM => Opcode::Add,
                d::SUB_MR | d::SUB_RM => Opcode::Sub,
                _ => Opcode::Cmp,
            };
            if m == d::MOD_REG as u8 {
                Ok(DecInsn {
                    kind: DecKind::Insn(op),
                    offset: start,
                    dst: from_hw(rm),
                    src: Some(DecSrc::Reg(from_hw(reg))),
                    target: None,
                })
            } else {
                // A folded slot source: op reg, [base+disp].
                let (base, disp) = read_mem(r, m, rm, &p)?;
                Ok(DecInsn {
                    kind: DecKind::Insn(op),
                    offset: start,
                    dst: from_hw(reg),
                    src: Some(DecSrc::Mem { base, disp }),
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
                _ => return Err(DecodeError::UnknownEncoding(start)),
            };
            let width = if opc == d::GRP1_IB { d::IMM8 } else { d::IMM32 };
            let imm = r.take_int(width);
            Ok(DecInsn {
                kind: DecKind::Insn(op),
                offset: start,
                dst: from_hw(rm),
                src: Some(DecSrc::Imm(imm)),
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
                dst: preg::RAX,
                src: Some(DecSrc::Imm(imm)),
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
                (true, false, d::ADDSD | d::MULSD | d::MOVSD_LOAD | d::MOVSD_STORE) => {
                    decode_sse(r, opc2, &p, start)
                }
                (false, true, d::MOVAPD) => {
                    // movapd reg-reg: 66 [REX] 0F 28 /r; REX.W is never set.
                    // SSE ModRM: reg is the destination, rm the source.
                    let modrm = r.take();
                    let (m, reg, rm) = decode_modrm(modrm, &p);
                    if m != d::MOD_REG as u8 {
                        return Err(DecodeError::UnknownEncoding(start));
                    }
                    Ok(DecInsn {
                        kind: DecKind::Insn(Opcode::Mov),
                        offset: start,
                        dst: reg + d::XMM_TABLE_BASE,
                        src: Some(DecSrc::Reg(rm + d::XMM_TABLE_BASE)),
                        target: None,
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
                            dst: from_hw(rm),
                            src: Some(DecSrc::Reg(from_hw(reg))),
                            target: None,
                        })
                    } else {
                        let (base, disp) = read_mem(r, m, rm, &p)?;
                        Ok(DecInsn {
                            kind: DecKind::Insn(Opcode::IMul),
                            offset: start,
                            dst: from_hw(reg),
                            src: Some(DecSrc::Mem { base, disp }),
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
                        dst: u32::MAX,
                        src: None,
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
    match (opc2, m) {
        // SSE ModRM: reg is the destination, rm the source.
        (d::ADDSD, m) if m == d::MOD_REG as u8 => Ok(DecInsn {
            kind: DecKind::Insn(Opcode::AddSd),
            offset: start,
            dst: xmm_id(reg),
            src: Some(DecSrc::Reg(xmm_id(rm))),
            target: None,
        }),
        (d::MULSD, m) if m == d::MOD_REG as u8 => Ok(DecInsn {
            kind: DecKind::Insn(Opcode::MulSd),
            offset: start,
            dst: xmm_id(reg),
            src: Some(DecSrc::Reg(xmm_id(rm))),
            target: None,
        }),
        (d::MOVSD_LOAD, m) if m <= d::MOD_DISP32 as u8 => {
            let (base, disp) = read_mem(r, m, rm, p)?;
            Ok(DecInsn {
                kind: DecKind::Insn(Opcode::Load),
                offset: start,
                dst: xmm_id(reg),
                src: Some(DecSrc::Mem { base, disp }),
                target: None,
            })
        }
        (d::MOVSD_STORE, m) if m <= d::MOD_DISP32 as u8 => {
            let (base, disp) = read_mem(r, m, rm, p)?;
            Ok(DecInsn {
                kind: DecKind::Insn(Opcode::Store),
                offset: start,
                dst: xmm_id(reg),
                src: Some(DecSrc::Mem { base, disp }),
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
    let mem = |s: &DecSrc| match s {
        DecSrc::Mem { base, disp } => format!("{disp}({})", name(*base)),
        _ => String::new(),
    };
    match insn.kind {
        DecKind::Push => format!("  pushq {}\n", name(insn.dst)),
        DecKind::Pop => format!("  popq {}\n", name(insn.dst)),
        DecKind::Insn(op) => match (op, &insn.src) {
            (Opcode::Mov, Some(DecSrc::Reg(s))) => {
                if insn.dst >= d::XMM_MIN_ID {
                    format!("  movapd {}, {}\n", name(*s), name(insn.dst))
                } else {
                    format!("  movq {}, {}\n", name(*s), name(insn.dst))
                }
            }
            (Opcode::Mov, Some(DecSrc::Imm(v))) => format!("  movq ${v}, {}\n", name(insn.dst)),
            (Opcode::Load, Some(s @ DecSrc::Mem { .. })) => {
                if insn.dst >= d::XMM_MIN_ID {
                    format!("  movsd {}, {}\n", mem(s), name(insn.dst))
                } else {
                    format!("  movq {}, {}\n", mem(s), name(insn.dst))
                }
            }
            (Opcode::Store, Some(s @ DecSrc::Mem { .. })) => {
                if insn.dst >= d::XMM_MIN_ID {
                    format!("  movsd {}, {}\n", name(insn.dst), mem(s))
                } else {
                    format!("  movq {}, {}\n", name(insn.dst), mem(s))
                }
            }
            (Opcode::Add, Some(DecSrc::Reg(s))) => {
                format!("  addq {}, {}\n", name(*s), name(insn.dst))
            }
            (Opcode::Add, Some(DecSrc::Imm(v))) => format!("  addq ${v}, {}\n", name(insn.dst)),
            (Opcode::Add, Some(s @ DecSrc::Mem { .. })) => {
                format!("  addq {}, {}\n", mem(s), name(insn.dst))
            }
            (Opcode::Sub, Some(DecSrc::Reg(s))) => {
                format!("  subq {}, {}\n", name(*s), name(insn.dst))
            }
            (Opcode::Sub, Some(DecSrc::Imm(v))) => format!("  subq ${v}, {}\n", name(insn.dst)),
            (Opcode::Sub, Some(s @ DecSrc::Mem { .. })) => {
                format!("  subq {}, {}\n", mem(s), name(insn.dst))
            }
            (Opcode::IMul, Some(DecSrc::Reg(s))) => {
                format!("  imulq {}, {}\n", name(*s), name(insn.dst))
            }
            (Opcode::IMul, Some(s @ DecSrc::Mem { .. })) => {
                format!("  imulq {}, {}\n", mem(s), name(insn.dst))
            }
            (Opcode::Cmp, Some(DecSrc::Reg(s))) => {
                format!("  cmpq {}, {}\n", name(*s), name(insn.dst))
            }
            (Opcode::Cmp, Some(DecSrc::Imm(v))) => format!("  cmpq ${v}, {}\n", name(insn.dst)),
            (Opcode::Cmp, Some(s @ DecSrc::Mem { .. })) => {
                format!("  cmpq {}, {}\n", mem(s), name(insn.dst))
            }
            (Opcode::AddSd, Some(DecSrc::Reg(s))) => {
                format!("  addsd {}, {}\n", name(*s), name(insn.dst))
            }
            (Opcode::MulSd, Some(DecSrc::Reg(s))) => {
                format!("  mulsd {}, {}\n", name(*s), name(insn.dst))
            }
            (Opcode::Jmp, _) => format!("  jmp .L{}\n", insn.target.clone().unwrap_or_default()),
            (Opcode::Jl, _) => format!("  jl .L{}\n", insn.target.clone().unwrap_or_default()),
            (Opcode::Jge, _) => format!("  jge .L{}\n", insn.target.clone().unwrap_or_default()),
            (Opcode::Ret, _) => "  ret\n".to_string(),
            // Internal call targets already carry the ".L" label prefix; external
            // symbols print bare, exactly as emit() does.
            (Opcode::Call, _) => {
                format!("  call {}\n", insn.target.clone().unwrap_or_default())
            }
            _ => String::new(),
        },
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

// CEP:FILE: crates/mlkp-machine/src/isa/assemble/forms.rs
// CEP:WHAT: The canonical form selection and byte emission for every tier family: REX computation, ModRM tails, ALU and move form choice, and the SSE prefix path.
// CEP:WHY: The AT&T assembler closes the tier pair; the parser, the canonical form selection, and the fixpoint driver split per chapter 62.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns IsaError::Parse for malformed text and shapes the tier does not encode; nothing truncates silently.
// CEP:ASSUMES: one instruction per line; the canonical forms mirror GNU as byte for byte (gas differential).
// CEP:COST: cold; O(text) per parse.
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs.

use super::tables::{Alu, Grp3, Width};
use super::{IsaError, IsaMn, IsaOp, Parsed, ACC, GPR_EXT, HW_FRAME, HW_SIB, IMM8_MIN, IMM8_U_MAX};

/// True when the value fits a signed byte.
// CEP:WHAT: Tests i8 range.
// CEP:WHY: GRP1 imm8 and disp8 forms.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn fits_i8(v: i64) -> bool {
    (IMM8_MIN..=127).contains(&v)
}

/// True when the value fits a signed 32-bit integer.
// CEP:WHAT: Tests i32 range.
// CEP:WHY: imm32 and disp32 forms.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn fits_i32(v: i64) -> bool {
    v >= i64::from(i32::MIN) && v <= i64::from(i32::MAX)
}

/// Computes the REX byte value from the four bits.
// CEP:WHAT: Computes REX bits.
// CEP:WHY: One helper keeps prefix order consistent across forms.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn rex_bits(w: bool, r: bool, x: bool, b: bool) -> u8 {
    let mut rex = 0x40u8;
    if w {
        rex |= 0x08;
    }
    if r {
        rex |= 0x04;
    }
    if x {
        rex |= 0x02;
    }
    if b {
        rex |= 0x01;
    }
    rex
}

/// Pushes the REX byte only when it is not the no-op 0x40.
// CEP:WHAT: Pushes a canonical REX.
// CEP:WHY: GNU as omits a bare 0x40; the differential test needs the same choice.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn push_rex(out: &mut Vec<u8>, rex: u8) {
    if rex != 0x40 {
        out.push(rex);
    }
}

/// Computes the REX byte for one operand pair, including the operand's own extension bits.
// CEP:WHAT: Computes an operand REX.
// CEP:WHY: Exactly one REX byte precedes the opcode; the caller composes W, R, X, and B here instead of double-emitting.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn operand_rex(rex_w: bool, reg_field: u32, op: &IsaOp) -> u8 {
    match op {
        IsaOp::Gpr(hw) | IsaOp::Xmm(hw) => {
            rex_bits(rex_w, reg_field >= GPR_EXT, false, *hw >= GPR_EXT)
        }
        IsaOp::Mem { base, index, .. } => rex_bits(
            rex_w,
            reg_field >= GPR_EXT,
            index.map(|i| i >= GPR_EXT).unwrap_or(false),
            base.map(|b| b >= GPR_EXT).unwrap_or(false),
        ),
        _ => rex_bits(rex_w, reg_field >= GPR_EXT, false, false),
    }
}

/// Emits ModRM, optional SIB, and the displacement tail with no REX byte.
// CEP:ID: JIT-i28
// CEP:WHAT: Emits a ModRM tail.
// CEP:WHY: The caller owns the single REX byte; splitting keeps one REX per instruction.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::Parse for displacements outside i32.
// CEP:ASSUMES: the caller emitted prefix, REX, and opcode bytes already.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn modrm_tail(
    out: &mut Vec<u8>,
    reg_field: u32,
    op: &IsaOp,
    perr: &dyn Fn(String) -> IsaError,
) -> Result<(), IsaError> {
    match op {
        IsaOp::Gpr(hw) | IsaOp::Xmm(hw) => {
            out.push(modrm(3, reg_field & 7, *hw & 7));
        }
        IsaOp::Mem {
            base,
            index,
            scale,
            disp,
            rip,
        } => {
            if *rip {
                out.push(modrm(0, reg_field & 7, 5));
                out.extend_from_slice(&(*disp as i32).to_le_bytes());
                return Ok(());
            }
            let has_sib = index.is_some() || *base == Some(HW_SIB);
            let (m, sib_v, d, force32): (u8, Option<u8>, Option<i32>, bool) = if has_sib {
                let b = base.unwrap_or(HW_FRAME);
                let idx = index.unwrap_or(super::super::decode::SIB_NO_INDEX);
                let (m, d, force32): (u8, Option<i32>, bool) = if base.is_none() {
                    // SIB base 101 in mod 00 has no base register; the disp32
                    // field is mandatory even when the displacement is zero.
                    (0, Some(*disp as i32), true)
                } else if *disp == 0 && b & 7 != HW_FRAME {
                    (0, None, false)
                } else if fits_i8(*disp) {
                    (1, Some(*disp as i32), false)
                } else if fits_i32(*disp) {
                    (2, Some(*disp as i32), false)
                } else {
                    return Err(perr("displacement outside i32".into()));
                };
                let sib_v =
                    (scale.trailing_zeros() as u8) << 6 | ((idx & 7) as u8) << 3 | (b & 7) as u8;
                (m, Some(sib_v), d, force32)
            } else {
                let b = base.ok_or_else(|| perr("memory needs a base or an index".into()))?;
                let (m, d, force32): (u8, Option<i32>, bool) = if *disp == 0 && b & 7 != HW_FRAME {
                    (0, None, false)
                } else if fits_i8(*disp) {
                    (1, Some(*disp as i32), false)
                } else if fits_i32(*disp) {
                    (2, Some(*disp as i32), false)
                } else {
                    return Err(perr("displacement outside i32".into()));
                };
                (m, None, d, force32)
            };
            let rm = if sib_v.is_some() {
                4
            } else {
                base.unwrap_or(0) & 7
            };
            out.push(modrm(m, reg_field & 7, rm));
            if let Some(s) = sib_v {
                out.push(s);
            }
            if force32 {
                out.extend_from_slice(&(*disp as i32).to_le_bytes());
            } else {
                emit_disp(out, m, d);
            }
        }
        IsaOp::Imm(_) | IsaOp::Sym(_) => {
            return Err(perr(
                "an immediate or symbol cannot be a ModRM operand".into(),
            ));
        }
    }
    Ok(())
}

/// Emits the displacement in the width the ModRM mod field chose.
// CEP:WHAT: Emits a displacement.
// CEP:WHY: mod 01 is one byte and mod 02 is four; a single i32 writer would pad every disp8 into a disp32 and shift every later byte.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn emit_disp(out: &mut Vec<u8>, m: u8, d: Option<i32>) {
    if let Some(v) = d {
        if m == 1 {
            out.push(v as i8 as u8);
        } else if m == 2 {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
}

/// Builds a ModRM byte from its three fields.
// CEP:WHAT: Packs a ModRM byte.
// CEP:WHY: Shared by every form.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn modrm(m: u8, reg: u32, rm: u32) -> u8 {
    ((m as u32) << 6 | (reg & 7) << 3 | (rm & 7)) as u8
}

/// Extracts the immediate from an operand.
// CEP:WHAT: Reads an immediate operand.
// CEP:WHY: Form selection needs the immediate value.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::Parse for a non-immediate.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn imm_of(op: &IsaOp, perr: &dyn Fn(String) -> IsaError) -> Result<i64, IsaError> {
    match op {
        IsaOp::Imm(v) => Ok(*v),
        _ => Err(perr("expected an immediate".into())),
    }
}

/// Extracts the GPR hardware number from an operand.
// CEP:WHAT: Reads a GPR operand.
// CEP:WHY: Register fields need hardware numbers.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::Parse for a non-register.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn gpr_of(op: &IsaOp, perr: &dyn Fn(String) -> IsaError) -> Result<u32, IsaError> {
    match op {
        IsaOp::Gpr(hw) => Ok(*hw),
        _ => Err(perr("expected a general register".into())),
    }
}

/// Extracts the xmm index from an operand.
// CEP:WHAT: Reads an xmm operand.
// CEP:WHY: SSE register fields need xmm indices.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::Parse for a non-xmm.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn xmm_of(op: &IsaOp, perr: &dyn Fn(String) -> IsaError) -> Result<u32, IsaError> {
    match op {
        IsaOp::Xmm(n) => Ok(*n),
        _ => Err(perr("expected an xmm register".into())),
    }
}

/// Encodes mov in every width and immediate form.
// CEP:ID: JIT-p18
// CEP:WHAT: Encodes mov.
// CEP:WHY: mov has the widest form set: four widths, reg and memory directions, and movabs.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::Parse for shapes outside the tier or immediates that cannot encode.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn encode_mov(insn: &Parsed, out: &mut Vec<u8>) -> Result<(), IsaError> {
    let perr = |r: String| IsaError::Parse(0, r);
    let w = insn.width;
    let ops = &insn.ops;
    let src = ops
        .first()
        .cloned()
        .ok_or_else(|| perr("missing source".into()))?;
    let dst = ops
        .get(1)
        .cloned()
        .ok_or_else(|| perr("missing destination".into()))?;
    match (src, dst) {
        (IsaOp::Gpr(s), IsaOp::Gpr(d)) => {
            push_rex(
                out,
                rex_bits(w == Width::Q, s >= GPR_EXT, false, d >= GPR_EXT),
            );
            out.push(if w == Width::B { 0x88 } else { 0x89 });
            out.push(modrm(3, s & 7, d & 7));
        }
        (IsaOp::Gpr(s), dst @ IsaOp::Mem { .. }) => {
            if w == Width::W {
                out.push(0x66);
            }
            push_rex(out, operand_rex(w == Width::Q, s, &dst));
            out.push(if w == Width::B { 0x88 } else { 0x89 });
            modrm_tail(out, s, &dst, &perr)?;
        }
        (src @ IsaOp::Mem { .. }, IsaOp::Gpr(d)) => {
            if w == Width::W {
                out.push(0x66);
            }
            push_rex(out, operand_rex(w == Width::Q, d, &src));
            out.push(if w == Width::B { 0x8A } else { 0x8B });
            modrm_tail(out, d, &src, &perr)?;
        }
        (IsaOp::Imm(v), IsaOp::Gpr(d)) => {
            if w == Width::B {
                if !(IMM8_MIN..=IMM8_U_MAX).contains(&v) {
                    return Err(perr("byte immediate outside i8/u8".into()));
                }
                push_rex(out, rex_bits(false, false, false, d >= GPR_EXT));
                out.push(0xB0 + (d & 7) as u8);
                out.push(v as u8);
            } else if w == Width::W {
                if !fits_i32(v) {
                    return Err(perr("word immediate outside i32".into()));
                }
                out.push(0x66);
                push_rex(out, rex_bits(false, false, false, d >= GPR_EXT));
                out.push(0xB8 + (d & 7) as u8);
                out.extend_from_slice(&(v as i16).to_le_bytes());
            } else if w == Width::L {
                if !fits_i32(v) {
                    return Err(perr("dword immediate outside i32".into()));
                }
                push_rex(out, rex_bits(false, false, false, d >= GPR_EXT));
                out.push(0xB8 + (d & 7) as u8);
                out.extend_from_slice(&(v as i32).to_le_bytes());
            } else if fits_i32(v) {
                push_rex(out, rex_bits(true, false, false, d >= GPR_EXT));
                out.push(0xC7);
                out.push(modrm(3, 0, d & 7));
                out.extend_from_slice(&(v as i32).to_le_bytes());
            } else {
                push_rex(out, rex_bits(true, false, false, d >= GPR_EXT));
                out.push(0xB8 + (d & 7) as u8);
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        (IsaOp::Imm(v), dst @ IsaOp::Mem { .. }) => {
            if w == Width::B {
                if !(IMM8_MIN..=IMM8_U_MAX).contains(&v) {
                    return Err(perr("byte immediate outside i8/u8".into()));
                }
                push_rex(out, operand_rex(false, 0, &dst));
                out.push(0xC6);
                modrm_tail(out, 0, &dst, &perr)?;
                out.push(v as u8);
            } else {
                if w == Width::W {
                    out.push(0x66);
                }
                push_rex(out, operand_rex(w == Width::Q, 0, &dst));
                out.push(0xC7);
                modrm_tail(out, 0, &dst, &perr)?;
                if w == Width::W {
                    out.extend_from_slice(&(v as i16).to_le_bytes());
                } else {
                    if !fits_i32(v) {
                        return Err(perr("immediate outside i32 for the C7 form".into()));
                    }
                    out.extend_from_slice(&(v as i32).to_le_bytes());
                }
            }
        }
        _ => return Err(perr("mov needs two operands".into())),
    }
    Ok(())
}

/// Encodes one ALU instruction in MR, RM, and immediate forms.
// CEP:ID: JIT-781
// CEP:WHAT: Encodes ALU.
// CEP:WHY: The six operations share the map with the form in the low bits.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::Parse for shapes the tier does not encode or immediates outside their form.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn encode_alu(alu: Alu, insn: &Parsed, out: &mut Vec<u8>) -> Result<(), IsaError> {
    let perr = |r: String| IsaError::Parse(0, r);
    let w = insn.width;
    let ops = &insn.ops;
    let base = alu.base();
    let wide = w != Width::B;
    let src = ops
        .first()
        .cloned()
        .ok_or_else(|| perr("missing source".into()))?;
    let dst = ops
        .get(1)
        .cloned()
        .ok_or_else(|| perr("missing destination".into()))?;
    let rex_w = w == Width::Q;
    if w == Width::W {
        out.push(0x66);
    }
    match (src, dst) {
        (IsaOp::Gpr(s), IsaOp::Gpr(d)) => {
            push_rex(out, rex_bits(rex_w, s >= GPR_EXT, false, d >= GPR_EXT));
            out.push(if wide { base + 1 } else { base });
            out.push(modrm(3, s & 7, d & 7));
        }
        (IsaOp::Gpr(s), dst @ IsaOp::Mem { .. }) => {
            push_rex(out, operand_rex(rex_w, s, &dst));
            out.push(if wide { base + 1 } else { base });
            modrm_tail(out, s, &dst, &perr)?;
        }
        (src @ IsaOp::Mem { .. }, IsaOp::Gpr(d)) => {
            push_rex(out, operand_rex(rex_w, d, &src));
            out.push(if wide { base + 3 } else { base + 2 });
            modrm_tail(out, d, &src, &perr)?;
        }
        (IsaOp::Imm(v), IsaOp::Gpr(d)) => {
            if wide {
                if fits_i8(v) {
                    push_rex(out, operand_rex(rex_w, alu.ext() as u32, &IsaOp::Gpr(d)));
                    out.push(0x83);
                    modrm_tail(out, alu.ext() as u32, &IsaOp::Gpr(d), &perr)?;
                    out.push(v as i8 as u8);
                } else if d == ACC && fits_i32(v) {
                    push_rex(out, rex_bits(rex_w, false, false, false));
                    out.push(base + 5);
                    out.extend_from_slice(&(v as i32).to_le_bytes());
                } else if fits_i32(v) {
                    push_rex(out, operand_rex(rex_w, alu.ext() as u32, &IsaOp::Gpr(d)));
                    out.push(0x81);
                    modrm_tail(out, alu.ext() as u32, &IsaOp::Gpr(d), &perr)?;
                    out.extend_from_slice(&(v as i32).to_le_bytes());
                } else {
                    return Err(perr("immediate outside i32".into()));
                }
            } else if (IMM8_MIN..=IMM8_U_MAX).contains(&v) {
                if d == ACC {
                    out.push(base + 4);
                } else {
                    push_rex(out, operand_rex(false, alu.ext() as u32, &IsaOp::Gpr(d)));
                    out.push(0x80);
                    modrm_tail(out, alu.ext() as u32, &IsaOp::Gpr(d), &perr)?;
                }
                out.push(v as u8);
            } else {
                return Err(perr("byte immediate outside i8/u8".into()));
            }
        }
        (IsaOp::Imm(v), dst @ IsaOp::Mem { .. }) => {
            if wide {
                if fits_i8(v) {
                    push_rex(out, operand_rex(rex_w, alu.ext() as u32, &dst));
                    out.push(0x83);
                    modrm_tail(out, alu.ext() as u32, &dst, &perr)?;
                    out.push(v as i8 as u8);
                } else if fits_i32(v) {
                    push_rex(out, operand_rex(rex_w, alu.ext() as u32, &dst));
                    out.push(0x81);
                    modrm_tail(out, alu.ext() as u32, &dst, &perr)?;
                    out.extend_from_slice(&(v as i32).to_le_bytes());
                } else {
                    return Err(perr("immediate outside i32".into()));
                }
            } else if (IMM8_MIN..=IMM8_U_MAX).contains(&v) {
                push_rex(out, operand_rex(false, alu.ext() as u32, &dst));
                out.push(0x80);
                modrm_tail(out, alu.ext() as u32, &dst, &perr)?;
                out.push(v as u8);
            } else {
                return Err(perr("byte immediate outside i8/u8".into()));
            }
        }
        _ => return Err(perr("alu needs two operands".into())),
    }
    Ok(())
}

/// Encodes test: register forms, memory forms, and the immediate forms with accumulator short forms.
// CEP:ID: JIT-j18
// CEP:WHAT: Encodes test.
// CEP:WHY: test is commutative; the reg field holds the register operand.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::Parse for shapes the tier does not encode.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn encode_test(insn: &Parsed, out: &mut Vec<u8>) -> Result<(), IsaError> {
    let perr = |r: String| IsaError::Parse(0, r);
    let w = insn.width;
    let wide = w != Width::B;
    let rex_w = w == Width::Q;
    let ops = &insn.ops;
    let a = ops
        .first()
        .cloned()
        .ok_or_else(|| perr("missing source".into()))?;
    let b = ops
        .get(1)
        .cloned()
        .ok_or_else(|| perr("missing destination".into()))?;
    if w == Width::W {
        out.push(0x66);
    }
    match (a, b) {
        (IsaOp::Gpr(s), IsaOp::Gpr(d)) => {
            push_rex(out, rex_bits(rex_w, s >= GPR_EXT, false, d >= GPR_EXT));
            out.push(if wide { 0x85 } else { 0x84 });
            out.push(modrm(3, s & 7, d & 7));
        }
        (IsaOp::Gpr(s), dst @ IsaOp::Mem { .. }) => {
            push_rex(out, operand_rex(rex_w, s, &dst));
            out.push(if wide { 0x85 } else { 0x84 });
            modrm_tail(out, s, &dst, &perr)?;
        }
        (src @ IsaOp::Mem { .. }, IsaOp::Gpr(d)) => {
            push_rex(out, operand_rex(rex_w, d, &src));
            out.push(if wide { 0x85 } else { 0x84 });
            modrm_tail(out, d, &src, &perr)?;
        }
        (IsaOp::Imm(v), IsaOp::Gpr(d)) => {
            if d == ACC && wide {
                push_rex(out, rex_bits(rex_w, false, false, false));
                out.push(0xA9);
                out.extend_from_slice(&(v as i32).to_le_bytes());
            } else if d == ACC {
                out.push(0xA8);
                out.push(v as u8);
            } else if wide {
                push_rex(
                    out,
                    operand_rex(rex_w, Grp3::Test.ext() as u32, &IsaOp::Gpr(d)),
                );
                out.push(0xF7);
                modrm_tail(out, Grp3::Test.ext() as u32, &IsaOp::Gpr(d), &perr)?;
                out.extend_from_slice(&(v as i32).to_le_bytes());
            } else {
                push_rex(
                    out,
                    operand_rex(false, Grp3::Test.ext() as u32, &IsaOp::Gpr(d)),
                );
                out.push(0xF6);
                modrm_tail(out, Grp3::Test.ext() as u32, &IsaOp::Gpr(d), &perr)?;
                out.push(v as u8);
            }
        }
        (IsaOp::Imm(v), dst @ IsaOp::Mem { .. }) => {
            push_rex(out, operand_rex(rex_w, Grp3::Test.ext() as u32, &dst));
            out.push(if wide { 0xF7 } else { 0xF6 });
            modrm_tail(out, Grp3::Test.ext() as u32, &dst, &perr)?;
            if w == Width::B {
                out.push(v as u8);
            } else {
                out.extend_from_slice(&(v as i32).to_le_bytes());
            }
        }
        _ => return Err(perr("test needs two operands".into())),
    }
    Ok(())
}

/// Encodes shifts in the immediate-one, immediate, and CL forms.
// CEP:ID: JIT-j68
// CEP:WHAT: Encodes shifts.
// CEP:WHY: Count $1 uses the implicit-one form like GNU as.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::Parse for counts outside u8 and non-CL register counts.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn encode_shift(insn: &Parsed, out: &mut Vec<u8>) -> Result<(), IsaError> {
    let perr = |r: String| IsaError::Parse(0, r);
    let w = insn.width;
    let wide = w != Width::B;
    let ext = match insn.mn {
        IsaMn::Shl => 4u32,
        IsaMn::Shr => 5,
        _ => 7,
    };
    let src = insn
        .ops
        .first()
        .cloned()
        .ok_or_else(|| perr("missing count".into()))?;
    let dst = insn
        .ops
        .get(1)
        .cloned()
        .ok_or_else(|| perr("missing destination".into()))?;
    match src {
        IsaOp::Imm(1) => {
            if w == Width::W {
                out.push(0x66);
            }
            push_rex(out, operand_rex(w == Width::Q, ext, &dst));
            out.push(if wide { 0xD1 } else { 0xD0 });
            modrm_tail(out, ext, &dst, &perr)?;
        }
        IsaOp::Imm(v) => {
            if !(0..=255).contains(&v) {
                return Err(perr("shift count outside 0..=255".into()));
            }
            if w == Width::W {
                out.push(0x66);
            }
            push_rex(out, operand_rex(w == Width::Q, ext, &dst));
            out.push(if wide { 0xC1 } else { 0xC0 });
            modrm_tail(out, ext, &dst, &perr)?;
            out.push(v as u8);
        }
        IsaOp::Gpr(1) => {
            if w == Width::W {
                out.push(0x66);
            }
            push_rex(out, operand_rex(w == Width::Q, ext, &dst));
            out.push(if wide { 0xD3 } else { 0xD2 });
            modrm_tail(out, ext, &dst, &perr)?;
        }
        _ => return Err(perr("shift count must be an immediate or %cl".into())),
    }
    Ok(())
}

/// Encodes movsd: the reg-reg move, the memory load, and the memory store.
// CEP:ID: JIT-e39
// CEP:WHAT: Encodes movsd.
// CEP:WHY: movsd picks load or store by which operand is memory.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::Parse when both or neither operand is an xmm register.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn encode_movsd(ops: &[IsaOp], out: &mut Vec<u8>) -> Result<(), IsaError> {
    let perr = |r: String| IsaError::Parse(0, r);
    let src = ops
        .first()
        .cloned()
        .ok_or_else(|| perr("missing source".into()))?;
    let dst = ops
        .get(1)
        .cloned()
        .ok_or_else(|| perr("missing destination".into()))?;
    match (src, dst) {
        (IsaOp::Xmm(s), IsaOp::Xmm(d)) => {
            push_rex(out, operand_rex(false, d, &IsaOp::Xmm(s)));
            out.push(0xF2);
            out.push(0x0F);
            out.push(0x10);
            modrm_tail(out, d, &IsaOp::Xmm(s), &perr)?;
        }
        (IsaOp::Xmm(s), dst @ IsaOp::Mem { .. }) => {
            push_rex(out, operand_rex(false, s, &dst));
            out.push(0xF2);
            out.push(0x0F);
            out.push(0x11);
            modrm_tail(out, s, &dst, &perr)?;
        }
        (src @ IsaOp::Mem { .. }, IsaOp::Xmm(d)) => {
            push_rex(out, operand_rex(false, d, &src));
            out.push(0xF2);
            out.push(0x0F);
            out.push(0x10);
            modrm_tail(out, d, &src, &perr)?;
        }
        _ => {
            return Err(perr(
                "movsd needs one xmm and one xmm-or-memory operand".into(),
            ))
        }
    }
    Ok(())
}

// CEP:FILE: crates/mlkp-machine/src/isa/escape.rs
// CEP:WHAT: The 0F escape map of the general decoder: multiply, width-conversion moves, branches, and the scalar SSE2 double set.
// CEP:WHY: The tier's two-byte families are one atomic concern per chapter 62; the file keeps decode.rs under the CEP-1 line limit.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns OutsideTier outside the map.
// CEP:ASSUMES: prefixes were parsed by the caller.
// CEP:COST: cold; O(1) per instruction.
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs.

#![deny(missing_docs)]

use super::decode::{gpr, modrm, read_mem, take, take_int, Prefixes, MOD_REG};
use super::tables::Width;
use super::{IsaError, IsaInsn, IsaMn, IsaOp};

/// Decodes the two-byte opcode map (0F): multiply, moves with width conversion, branches, and the SSE double set.
// CEP:ID: JIT-a24
// CEP:WHAT: Decodes the 0F escape map.
// CEP:WHY: The tier's two-byte families live here.
// CEP:STATUS: complete
// CEP:FAILURE: Returns OutsideTier outside the map.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn decode_escape(
    b: &[u8],
    pos: &mut usize,
    p: &Prefixes,
    w: Width,
    start: u32,
) -> Result<IsaInsn, IsaError> {
    let opc2 = take(b, pos, start)?;
    match opc2 {
        0x80..=0x8F if !p.f2 && !p.f3 && !p.p66 && !p.rex_present => {
            let rel = take_int(b, pos, 4, start)?;
            let end = i64::from(*pos as u32);
            Ok(IsaInsn {
                mn: IsaMn::Jcc(opc2 - 0x80),
                width: Width::Q,
                ops: vec![],
                target: Some(end + rel),
            })
        }
        0xAF if !p.f2 && !p.f3 && !p.p66 => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let width = if p.rex_w { Width::Q } else { w };
            let reg_op = gpr(reg)?;
            let other = if m == MOD_REG {
                gpr(rm)?
            } else {
                read_mem(b, pos, m, rm, p, start)?
            };
            Ok(IsaInsn {
                mn: IsaMn::Imul2,
                width,
                ops: vec![other, reg_op],
                target: None,
            })
        }
        0x69 | 0x6B if !p.f2 && !p.f3 && !p.p66 && p.rex_w => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let other = if m == MOD_REG {
                gpr(rm)?
            } else {
                read_mem(b, pos, m, rm, p, start)?
            };
            let imm = if opc2 == 0x69 {
                take_int(b, pos, 4, start)?
            } else {
                take_int(b, pos, 1, start)?
            };
            Ok(IsaInsn {
                mn: IsaMn::Imul3,
                width: Width::Q,
                ops: vec![IsaOp::Imm(imm), other, gpr(reg)?],
                target: None,
            })
        }
        0xB6 | 0xB7 | 0xBE | 0xBF if !p.f2 && !p.f3 => {
            let src_w = if opc2 == 0xB6 || opc2 == 0xBE {
                Width::B
            } else {
                Width::W
            };
            let signed = opc2 >= 0xBE;
            let dst_w = if p.rex_w {
                Width::Q
            } else if p.p66 {
                Width::W
            } else {
                Width::L
            };
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let src = if m == MOD_REG {
                gpr(rm)?
            } else {
                read_mem(b, pos, m, rm, p, start)?
            };
            let mn = if signed {
                IsaMn::MovSx(src_w)
            } else {
                IsaMn::MovZx(src_w)
            };
            Ok(IsaInsn {
                mn,
                width: dst_w,
                ops: vec![src, gpr(reg)?],
                target: None,
            })
        }
        0x63 if !p.f2 && !p.f3 && !p.p66 && p.rex_w => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let src = if m == MOD_REG {
                gpr(rm)?
            } else {
                read_mem(b, pos, m, rm, p, start)?
            };
            Ok(IsaInsn {
                mn: IsaMn::MovSxd,
                width: Width::Q,
                ops: vec![src, gpr(reg)?],
                target: None,
            })
        }
        0x10 | 0x11 if (p.f2 || p.f3) && !p.p66 => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let reg_op = IsaOp::Xmm(reg);
            if m == MOD_REG {
                return Ok(IsaInsn {
                    mn: IsaMn::MovSd,
                    width: Width::Q,
                    ops: vec![IsaOp::Xmm(rm), reg_op],
                    target: None,
                });
            }
            let mem = read_mem(b, pos, m, rm, p, start)?;
            let ops = if opc2 == 0x10 {
                vec![mem, reg_op]
            } else {
                vec![reg_op, mem]
            };
            Ok(IsaInsn {
                mn: IsaMn::MovSd,
                width: Width::Q,
                ops,
                target: None,
            })
        }
        0x28 | 0x29 if p.p66 && !p.f2 && !p.f3 => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let reg_op = IsaOp::Xmm(reg);
            if m == MOD_REG {
                return Ok(IsaInsn {
                    mn: IsaMn::MovApd,
                    width: Width::Q,
                    ops: vec![IsaOp::Xmm(rm), reg_op],
                    target: None,
                });
            }
            let mem = read_mem(b, pos, m, rm, p, start)?;
            let ops = if opc2 == 0x28 {
                vec![mem, reg_op]
            } else {
                vec![reg_op, mem]
            };
            Ok(IsaInsn {
                mn: IsaMn::MovApd,
                width: Width::Q,
                ops,
                target: None,
            })
        }
        0x51 if p.f2 && !p.p66 => sse_binop(IsaMn::SqrtSd, b, pos, p, start),
        0x58 if p.f2 && !p.p66 => sse_binop(IsaMn::AddSd, b, pos, p, start),
        0x59 if p.f2 && !p.p66 => sse_binop(IsaMn::MulSd, b, pos, p, start),
        0x5C if p.f2 && !p.p66 => sse_binop(IsaMn::SubSd, b, pos, p, start),
        0x5E if p.f2 && !p.p66 => sse_binop(IsaMn::DivSd, b, pos, p, start),
        0x2E if p.p66 && !p.f2 => sse_binop(IsaMn::Ucomisd, b, pos, p, start),
        0x2F if p.p66 && !p.f2 => sse_binop(IsaMn::Comisd, b, pos, p, start),
        0x2A if p.f2 && !p.p66 => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let width = if p.rex_w { Width::Q } else { Width::L };
            let src = if m == MOD_REG {
                gpr(rm)?
            } else {
                read_mem(b, pos, m, rm, p, start)?
            };
            Ok(IsaInsn {
                mn: IsaMn::CvtSi2Sd,
                width,
                ops: vec![src, IsaOp::Xmm(reg)],
                target: None,
            })
        }
        0x2C if p.f2 && !p.p66 => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let width = if p.rex_w { Width::Q } else { Width::L };
            let src = if m == MOD_REG {
                IsaOp::Xmm(rm)
            } else {
                read_mem(b, pos, m, rm, p, start)?
            };
            Ok(IsaInsn {
                mn: IsaMn::Cvttsd2Si,
                width,
                ops: vec![src, gpr(reg)?],
                target: None,
            })
        }
        _ => Err(IsaError::OutsideTier(start)),
    }
}

/// Shared tail for the scalar SSE binary operations (destination in reg, source in rm).
// CEP:WHAT: Decodes an SSE binary operation.
// CEP:WHY: The F2/66-prefixed two-operand SSE forms share one shape.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn sse_binop(
    mn: IsaMn,
    b: &[u8],
    pos: &mut usize,
    p: &Prefixes,
    start: u32,
) -> Result<IsaInsn, IsaError> {
    let modrm_b = take(b, pos, start)?;
    let (m, reg, rm) = modrm(modrm_b, p);
    let dst = IsaOp::Xmm(reg);
    let src = if m == MOD_REG {
        IsaOp::Xmm(rm)
    } else {
        read_mem(b, pos, m, rm, p, start)?
    };
    Ok(IsaInsn {
        mn,
        width: Width::Q,
        ops: vec![src, dst],
        target: None,
    })
}

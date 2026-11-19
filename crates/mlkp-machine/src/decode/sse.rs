// CEP:FILE: crates/mlkp-machine/src/decode/sse.rs
// CEP:WHAT: The scalar SSE decoding tail: arithmetic, compare, conversion, and move forms over xmm registers and memory.
// CEP:WHY: The decoder is the mechanical bytes-versus-text equivalence check of ticket CEP-322; one shared structural grammar keeps decoding identical across the CodeObject path and the general path.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module DecodeError variants; encodings outside the ISA tier are rejected loudly, never guessed at.
// CEP:ASSUMES: the bytes came from encode::compile for the CodeObject path; prefixes arrive in canonical order.
// CEP:COST: cold; O(insns).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs.

use super::reader::{decode_modrm, from_hw, read_mem, Prefixes, Reader};
use super::tables::d;
use super::{DecDst, DecInsn, DecKind, DecSrc, DecodeError};
use crate::mir::Opcode;

/// Decodes one SSE instruction (addsd, mulsd, movsd load and store).
// CEP:ID: JIT-248
// CEP:WHAT: Decodes SSE forms.
// CEP:WHY: Shared tail of the F2-prefixed two-byte opcodes.
// CEP:STATUS: complete
// CEP:FAILURE: Returns UnknownEncoding for memory or register shapes the subset never emits.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn decode_sse(
    r: &mut Reader,
    opc2: u8,
    p: &Prefixes,
    start: u32,
) -> Result<DecInsn, DecodeError> {
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

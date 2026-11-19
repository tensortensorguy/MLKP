// CEP:FILE: crates/mlkp-machine/src/decode/insn.rs
// CEP:WHAT: The one-instruction decoder: every opcode of the encoded subset, its operand shapes, and its branch and call resolution.
// CEP:WHY: The decoder is the mechanical bytes-versus-text equivalence check of ticket CEP-322; one shared structural grammar keeps decoding identical across the CodeObject path and the general path.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module DecodeError variants; encodings outside the ISA tier are rejected loudly, never guessed at.
// CEP:ASSUMES: the bytes came from encode::compile for the CodeObject path; prefixes arrive in canonical order.
// CEP:COST: cold; O(insns).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs.

use super::reader::{decode_modrm, from_hw, parse_prefixes, read_mem, Reader};
use super::sse::decode_sse;
use super::tables::{d, jcc_from_rel32};
use super::{resolve_branch, DecDst, DecInsn, DecKind, DecSrc, DecodeError};
use crate::encode::CodeObject;
use crate::mir::Opcode;
use crate::target::preg;

/// Decodes exactly one instruction starting at the reader position.
// CEP:ID: JIT-k68
// CEP:WHAT: Decodes one instruction.
// CEP:WHY: The decode walk.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module errors.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn decode_one(
    r: &mut Reader,
    co: &CodeObject,
    start: u32,
) -> Result<DecInsn, DecodeError> {
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

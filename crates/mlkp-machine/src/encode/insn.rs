// CEP:FILE: crates/mlkp-machine/src/encode/insn.rs
// CEP:WHAT: Per-instruction byte emission: the emit_insn dispatcher over every encodable shape, the ALU family, SSE selection, and branch displacement computation with the short and long forms.
// CEP:WHY: Encoding mirrors emit() instruction for instruction so the two artifacts stay provably equivalent.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the EncodeError variants documented per function; no silent truncation of any value.
// CEP:ASSUMES: the instruction passed validation and the fixpoint settled the branch form.
// CEP:COST: cold; O(1) amortized per instruction.
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs, crates/mlkp-machine/tests/exec_test.rs.

use super::bytes::{
    load_shape, push_mem, push_rex, rex_rb, rex_rmem, rex_rr, rex_rr_w, rex_sse, rex_wb,
    store_shape,
};
use super::frame::{push_pop, push_push};
use super::op;
use super::operand::{EncInsn, Src};
use super::regs::{enc, enc_reg, fits_i32, fits_i8, hw, hw_num, jcc_encoding, len, modrm};
use super::EncodeError;
use super::Reloc;
use super::EPILOGUE_LABEL;
use crate::mir::Opcode;
use crate::mir::RegClass;
use crate::target::preg;

/// Emits one instruction at the current cursor with the final branch form.
// CEP:ID: JIT-v80
// CEP:WHAT: Emits one instruction.
// CEP:WHY: The emit pass of the two-phase encode.
// CEP:STATUS: complete
// CEP:FAILURE: Returns FixpointUnstable if a branch displacement no longer fits its chosen form; other errors are unreachable after validation.
// CEP:ASSUMES: the fixpoint settled the branch form.
// CEP:COST: O(1) amortized.
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn emit_insn(
    out: &mut Vec<u8>,
    e: &EncInsn,
    long: bool,
    target_off: Option<u32>,
    relocs: &mut Vec<Reloc>,
) -> Result<(), EncodeError> {
    let before = out.len() as u32;
    // The six-member integer ALU family shares one emission path.
    if matches!(
        e.op,
        Opcode::Add | Opcode::Sub | Opcode::And | Opcode::Or | Opcode::Xor | Opcode::Cmp
    ) {
        emit_alu(out, e)?;
        return Ok(());
    }
    match e.op {
        Opcode::Mov => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Imm(v))) => {
                let mut rex = op::REX_W;
                if hw(d.preg) >= hw_num::GPR_EXT {
                    rex |= op::REX_B;
                }
                if fits_i32(*v) {
                    push_rex(out, rex);
                    out.push(op::MOV_IMM32);
                    out.push(modrm(enc::MOD_REG, 0, enc_reg(d.preg)));
                    out.extend_from_slice(&(*v as i32).to_le_bytes());
                } else {
                    push_rex(out, rex);
                    out.push(op::MOV_IMM64 + enc_reg(d.preg));
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
            (Some(d), Some(Src::Reg(s))) => {
                if d.class == RegClass::Xmm64 {
                    let rex = rex_sse(Some(s), Some(d), None);
                    out.push(op::PRE_66);
                    push_rex(out, rex);
                    out.push(op::ESCAPE);
                    out.push(op::MOVAPD);
                    // SSE ModRM: reg is the destination, rm the source.
                    out.push(modrm(enc::MOD_REG, enc_reg(d.preg), enc_reg(s.preg)));
                } else {
                    let rex = rex_rr(d, s);
                    push_rex(out, rex);
                    out.push(op::MOV_MR);
                    out.push(modrm(enc::MOD_REG, enc_reg(s.preg), enc_reg(d.preg)));
                }
            }
            // A folded memory source encodes exactly like a Load.
            (Some(_), Some(Src::Mem { .. })) => {
                let load = EncInsn {
                    op: Opcode::Load,
                    dst: e.dst,
                    dst_mem: None,
                    src: e.src.clone(),
                    imm: None,
                    target: None,
                };
                return emit_insn(out, &load, long, None, relocs);
            }
            _ => unreachable!("validated at resolve time"),
        },
        Opcode::Lea => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Mem { base, disp })) => {
                let rex = rex_rmem(d, *base);
                push_rex(out, rex);
                out.push(op::LEA);
                push_mem(out, *base, *disp, enc_reg(d.preg));
            }
            _ => unreachable!("validated at resolve time"),
        },
        Opcode::IMul3 => match (&e.dst, &e.src, e.imm) {
            (Some(d), Some(Src::Reg(s)), Some(v)) => {
                let rex = rex_rr(d, s);
                push_rex(out, rex);
                // Three-operand imul puts the destination in reg and the source in rm.
                if fits_i8(v) {
                    out.push(op::IMUL3_IMM8);
                    out.push(modrm(enc::MOD_REG, enc_reg(d.preg), enc_reg(s.preg)));
                    out.push(v as i8 as u8);
                } else {
                    out.push(op::IMUL3_IMM32);
                    out.push(modrm(enc::MOD_REG, enc_reg(d.preg), enc_reg(s.preg)));
                    out.extend_from_slice(&(v as i32).to_le_bytes());
                }
            }
            (Some(d), Some(Src::Mem { base, disp }), Some(v)) => {
                let rex = rex_rmem(d, *base);
                push_rex(out, rex);
                if fits_i8(v) {
                    out.push(op::IMUL3_IMM8);
                    push_mem(out, *base, *disp, enc_reg(d.preg));
                    out.push(v as i8 as u8);
                } else {
                    out.push(op::IMUL3_IMM32);
                    push_mem(out, *base, *disp, enc_reg(d.preg));
                    out.extend_from_slice(&(v as i32).to_le_bytes());
                }
            }
            _ => unreachable!("validated at resolve time"),
        },
        Opcode::IDiv => match (&e.dst, &e.dst_mem) {
            (Some(d), None) => {
                push_rex(out, rex_rr_w(d));
                out.push(op::GRP3);
                out.push(modrm(enc::MOD_REG, op::EXT_IDIV, enc_reg(d.preg)));
            }
            (None, Some((base, disp))) => {
                push_rex(out, rex_wb(*base));
                out.push(op::GRP3);
                push_mem(out, *base, *disp, op::EXT_IDIV);
            }
            _ => unreachable!("validated at resolve time"),
        },
        Opcode::Cqo => {
            push_rex(out, op::REX_W);
            out.push(op::CQO);
        }
        Opcode::Not | Opcode::Neg => {
            let ext = if e.op == Opcode::Not {
                op::EXT_NOT
            } else {
                op::EXT_NEG
            };
            match (&e.dst, &e.dst_mem) {
                (Some(d), None) => {
                    push_rex(out, rex_rr_w(d));
                    out.push(op::GRP3);
                    out.push(modrm(enc::MOD_REG, ext, enc_reg(d.preg)));
                }
                (None, Some((base, disp))) => {
                    push_rex(out, rex_wb(*base));
                    out.push(op::GRP3);
                    push_mem(out, *base, *disp, ext);
                }
                _ => unreachable!("validated at resolve time"),
            }
        }
        Opcode::Shl | Opcode::Shr | Opcode::Sar => {
            let ext = match e.op {
                Opcode::Shl => op::EXT_SHL,
                Opcode::Shr => op::EXT_SHR,
                _ => op::EXT_IDIV, // SAR shares the /7 extension.
            };
            match (&e.dst, &e.dst_mem, &e.src) {
                (Some(d), None, Some(Src::Imm(v))) => {
                    push_rex(out, rex_rr_w(d));
                    if *v == 1 {
                        // The implicit-1 form matches gas and saves the imm byte.
                        out.push(op::SHIFT_ONE);
                        out.push(modrm(enc::MOD_REG, ext, enc_reg(d.preg)));
                    } else {
                        out.push(op::SHIFT_IMM);
                        out.push(modrm(enc::MOD_REG, ext, enc_reg(d.preg)));
                        out.push(*v as i8 as u8);
                    }
                }
                (Some(d), None, Some(Src::Reg(_))) => {
                    push_rex(out, rex_rr_w(d));
                    out.push(op::SHIFT_CL);
                    out.push(modrm(enc::MOD_REG, ext, enc_reg(d.preg)));
                }
                (None, Some((base, disp)), Some(Src::Imm(v))) => {
                    push_rex(out, rex_wb(*base));
                    if *v == 1 {
                        out.push(op::SHIFT_ONE);
                        push_mem(out, *base, *disp, ext);
                    } else {
                        out.push(op::SHIFT_IMM);
                        push_mem(out, *base, *disp, ext);
                        out.push(*v as i8 as u8);
                    }
                }
                (None, Some((base, disp)), Some(Src::Reg(_))) => {
                    push_rex(out, rex_wb(*base));
                    out.push(op::SHIFT_CL);
                    push_mem(out, *base, *disp, ext);
                }
                _ => unreachable!("validated at resolve time"),
            }
        }
        Opcode::Test => match (&e.dst, &e.dst_mem, &e.src) {
            (Some(d), None, Some(Src::Reg(s))) => {
                let rex = rex_rr(d, s);
                push_rex(out, rex);
                out.push(op::TEST_MR);
                out.push(modrm(enc::MOD_REG, enc_reg(s.preg), enc_reg(d.preg)));
            }
            (Some(d), None, Some(Src::Imm(v))) => {
                let mut rex = op::REX_W;
                if hw(d.preg) >= hw_num::GPR_EXT {
                    rex |= op::REX_B;
                }
                if d.preg == preg::RAX {
                    push_rex(out, rex);
                    out.push(op::TEST_EAX_ID);
                    out.extend_from_slice(&(*v as i32).to_le_bytes());
                } else {
                    push_rex(out, rex);
                    out.push(op::GRP3);
                    out.push(modrm(enc::MOD_REG, op::EXT_TEST, enc_reg(d.preg)));
                    out.extend_from_slice(&(*v as i32).to_le_bytes());
                }
            }
            (None, Some((base, disp)), Some(Src::Reg(s))) => {
                let rex = rex_rmem(s, *base);
                push_rex(out, rex);
                out.push(op::TEST_MR);
                push_mem(out, *base, *disp, enc_reg(s.preg));
            }
            _ => unreachable!("validated at resolve time"),
        },
        Opcode::Push => match (&e.dst, &e.src) {
            (Some(d), None) => push_push(out, d.preg),
            (None, Some(Src::Imm(v))) => {
                if fits_i8(*v) {
                    out.push(op::PUSH_IMM8);
                    out.push(*v as i8 as u8);
                } else {
                    out.push(op::PUSH_IMM32);
                    out.extend_from_slice(&(*v as i32).to_le_bytes());
                }
            }
            _ => unreachable!("validated at resolve time"),
        },
        Opcode::Pop => match e.dst {
            Some(d) => push_pop(out, d.preg),
            None => unreachable!("validated at resolve time"),
        },
        Opcode::Je
        | Opcode::Jne
        | Opcode::Jl
        | Opcode::Jle
        | Opcode::Jg
        | Opcode::Jge
        | Opcode::Ja
        | Opcode::Jae
        | Opcode::Jb
        | Opcode::Jbe
        | Opcode::Js
        | Opcode::Jns
        | Opcode::Jo
        | Opcode::Jno
        | Opcode::Jp
        | Opcode::Jnp => {
            let t = target_off
                .ok_or_else(|| EncodeError::UnknownLabel(e.target.clone().unwrap_or_default()))?;
            let disp = branch_disp(before, e.op, long, t)?;
            push_branch(out, e.op, long, disp);
        }
        Opcode::Jmp => {
            let t = target_off
                .ok_or_else(|| EncodeError::UnknownLabel(e.target.clone().unwrap_or_default()))?;
            let disp = branch_disp(before, e.op, long, t)?;
            push_branch(out, e.op, long, disp);
        }
        Opcode::Ret => {
            let t = target_off.ok_or(EncodeError::UnknownLabel(EPILOGUE_LABEL.to_string()))?;
            let disp = branch_disp(before, Opcode::Jmp, long, t)?;
            push_branch(out, Opcode::Jmp, long, disp);
        }
        Opcode::Call => {
            out.push(op::CALL_REL32);
            let field = out.len() as u32;
            out.extend_from_slice(&[0, 0, 0, 0]);
            let name = e
                .target
                .clone()
                .ok_or(EncodeError::BadOperand("call target"))?;
            match target_off {
                Some(t) => {
                    // Internal label: resolve directly, no relocation.
                    let disp = i64::from(t) - i64::from(field + len::REL32 as u32);
                    let bytes = (disp as i32).to_le_bytes();
                    out[field as usize..field as usize + len::REL32].copy_from_slice(&bytes);
                }
                None => relocs.push(Reloc {
                    offset: field,
                    symbol: name,
                    addend: len::CALL_ADDEND,
                }),
            }
        }
        Opcode::Load | Opcode::MovZx8 | Opcode::MovZx32 | Opcode::MovSx32 => {
            let opc = match e.op {
                Opcode::Load => op::MOV_RM,
                Opcode::MovZx8 => op::MOVZ8,
                Opcode::MovZx32 => op::MOV_RM,
                Opcode::MovSx32 => op::MOVSX32,
                _ => unreachable!("narrowed above"),
            };
            match (&e.dst, &e.src) {
                (Some(d), Some(Src::Mem { base, disp })) => {
                    let (w, escaped) = load_shape(e.op);
                    let rex = if w {
                        rex_rmem(d, *base)
                    } else {
                        rex_rb(d, *base)
                    };
                    if escaped {
                        push_rex(out, rex);
                        out.push(op::ESCAPE);
                        out.push(opc);
                    } else {
                        push_rex(out, rex);
                        out.push(opc);
                    }
                    push_mem(out, *base, *disp, enc_reg(d.preg));
                }
                (Some(d), Some(Src::Reg(s))) => {
                    let (w, escaped) = load_shape(e.op);
                    let mut rex = if w { op::REX_W } else { 0 };
                    if hw(d.preg) >= hw_num::GPR_EXT {
                        rex |= op::REX_R;
                    }
                    if hw(s.preg) >= hw_num::GPR_EXT {
                        rex |= op::REX_B;
                    }
                    if escaped {
                        push_rex(out, rex);
                        out.push(op::ESCAPE);
                        out.push(opc);
                    } else {
                        push_rex(out, rex);
                        out.push(opc);
                    }
                    out.push(modrm(enc::MOD_REG, enc_reg(d.preg), enc_reg(s.preg)));
                }
                _ => unreachable!("validated at resolve time"),
            }
        }
        Opcode::Store | Opcode::Store8 | Opcode::Store32 => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Mem { base, disp })) => {
                let opc = match e.op {
                    Opcode::Store => op::MOV_MR,
                    Opcode::Store8 => op::MOV8_MR,
                    Opcode::Store32 => op::MOV_MR,
                    _ => unreachable!("narrowed above"),
                };
                let (rex, _) = store_shape(e.op, d.preg, *base);
                push_rex(out, rex);
                out.push(opc);
                push_mem(out, *base, *disp, enc_reg(d.preg));
            }
            _ => unreachable!("validated at resolve time"),
        },
        Opcode::AddSd
        | Opcode::SubSd
        | Opcode::MulSd
        | Opcode::DivSd
        | Opcode::SqrtSd
        | Opcode::UComisd => {
            let (prefix, opc) = sse_binop(e.op);
            match (&e.dst, &e.src) {
                (Some(d), Some(Src::Reg(s))) => {
                    let rex = rex_sse(Some(s), Some(d), None);
                    out.push(prefix);
                    push_rex(out, rex);
                    out.push(op::ESCAPE);
                    out.push(opc);
                    // SSE ModRM: reg is the destination, rm the source.
                    out.push(modrm(enc::MOD_REG, enc_reg(d.preg), enc_reg(s.preg)));
                }
                (Some(d), Some(Src::Mem { base, disp })) => {
                    let rex = rex_sse(None, Some(d), Some(*base));
                    out.push(prefix);
                    push_rex(out, rex);
                    out.push(op::ESCAPE);
                    out.push(opc);
                    push_mem(out, *base, *disp, enc_reg(d.preg));
                }
                _ => unreachable!("validated at resolve time"),
            }
        }
        Opcode::CvtSi2Sd | Opcode::CvtSd2Si => {
            let opc = if e.op == Opcode::CvtSi2Sd {
                op::CVTSI2SD
            } else {
                op::CVTTSD2SI
            };
            match (&e.dst, &e.src) {
                (Some(d), Some(Src::Reg(s))) => {
                    // REX.W is always set: cvtsi2sdq and cvttsd2siq are 64-bit.
                    out.push(op::PRE_F2);
                    push_rex(out, op::REX_W);
                    out.push(op::ESCAPE);
                    out.push(opc);
                    // SSE ModRM: reg is the destination, rm the source.
                    out.push(modrm(enc::MOD_REG, enc_reg(d.preg), enc_reg(s.preg)));
                }
                (Some(d), Some(Src::Mem { base, disp })) => {
                    out.push(op::PRE_F2);
                    push_rex(out, op::REX_W);
                    out.push(op::ESCAPE);
                    out.push(opc);
                    push_mem(out, *base, *disp, enc_reg(d.preg));
                }
                _ => unreachable!("validated at resolve time"),
            }
        }
        _ => return Err(EncodeError::BadOperand("instruction not encodable")),
    }
    Ok(())
}

/// Emits the shared integer ALU family in all five operand shapes.
// CEP:ID: JIT-d82
// CEP:WHAT: Emits integer ALU forms.
// CEP:WHY: Add, Sub, And, Or, Xor, and Cmp share opcode families and shapes; one path keeps them byte-consistent.
// CEP:STATUS: complete
// CEP:FAILURE: Returns BadOperand for shapes validation already rejected.
// CEP:ASSUMES: the instruction passed validation.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn emit_alu(out: &mut Vec<u8>, e: &EncInsn) -> Result<(), EncodeError> {
    let (mr, rm, ext, short) = match e.op {
        Opcode::Add => (op::ADD_MR, op::ADD_RM, op::EXT_ADD, op::ADD_EAX_ID),
        Opcode::Sub => (op::SUB_MR, op::SUB_RM, op::EXT_SUB, op::SUB_EAX_ID),
        Opcode::And => (op::AND_MR, op::AND_RM, op::EXT_AND, op::AND_EAX_ID),
        Opcode::Or => (op::OR_MR, op::OR_RM, op::EXT_OR, op::OR_EAX_ID),
        Opcode::Xor => (op::XOR_MR, op::XOR_RM, op::EXT_XOR, op::XOR_EAX_ID),
        Opcode::Cmp => (op::CMP_MR, op::CMP_RM, op::EXT_CMP, op::CMP_EAX_ID),
        _ => return Err(EncodeError::BadOperand("not an integer ALU op")),
    };
    match (&e.dst, &e.dst_mem, &e.src) {
        (Some(d), None, Some(Src::Reg(s))) => {
            let rex = rex_rr(d, s);
            push_rex(out, rex);
            out.push(mr);
            out.push(modrm(enc::MOD_REG, enc_reg(s.preg), enc_reg(d.preg)));
        }
        (Some(d), None, Some(Src::Imm(v))) => {
            let mut rex = op::REX_W;
            if hw(d.preg) >= hw_num::GPR_EXT {
                rex |= op::REX_B;
            }
            if fits_i8(*v) {
                push_rex(out, rex);
                out.push(op::GRP1_IB);
                out.push(modrm(enc::MOD_REG, ext, enc_reg(d.preg)));
                out.push(*v as i8 as u8);
            } else if d.preg == preg::RAX {
                push_rex(out, rex);
                out.push(short);
                out.extend_from_slice(&(*v as i32).to_le_bytes());
            } else {
                push_rex(out, rex);
                out.push(op::GRP1_ID);
                out.push(modrm(enc::MOD_REG, ext, enc_reg(d.preg)));
                out.extend_from_slice(&(*v as i32).to_le_bytes());
            }
        }
        (Some(d), None, Some(Src::Mem { base, disp })) => {
            let rex = rex_rmem(d, *base);
            push_rex(out, rex);
            out.push(rm);
            push_mem(out, *base, *disp, enc_reg(d.preg));
        }
        (None, Some((base, disp)), Some(Src::Reg(s))) => {
            let rex = rex_rmem(s, *base);
            push_rex(out, rex);
            out.push(mr);
            push_mem(out, *base, *disp, enc_reg(s.preg));
        }
        (None, Some((base, disp)), Some(Src::Imm(v))) => {
            let rex = rex_wb(*base);
            push_rex(out, rex);
            if fits_i8(*v) {
                out.push(op::GRP1_IB);
            } else {
                out.push(op::GRP1_ID);
            }
            push_mem(out, *base, *disp, ext);
            if fits_i8(*v) {
                out.push(*v as i8 as u8);
            } else {
                out.extend_from_slice(&(*v as i32).to_le_bytes());
            }
        }
        _ => return Err(EncodeError::BadOperand("alu operand shape")),
    }
    Ok(())
}

/// (prefix, opcode) of the scalar-double binary SSE family.
// CEP:WHAT: Maps the SSE binary family.
// CEP:WHY: Adds, subs, muls, divs, and sqrts share one shape with different prefixes and opcodes.
// CEP:STATUS: complete
// CEP:FAILURE: Returns a default prefix for non-SSE opcodes; validation rejects those earlier.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn sse_binop(op: Opcode) -> (u8, u8) {
    match op {
        Opcode::AddSd => (op::PRE_F2, op::ADDSD),
        Opcode::SubSd => (op::PRE_F2, op::SUBSD),
        Opcode::MulSd => (op::PRE_F2, op::MULSD),
        Opcode::DivSd => (op::PRE_F2, op::DIVSD),
        Opcode::SqrtSd => (op::PRE_F2, op::SQRTSD),
        Opcode::UComisd => (op::PRE_66, op::UCOMISD),
        _ => (op::PRE_F2, op::ADDSD),
    }
}

/// Computes a branch displacement for the chosen form, failing loudly on overflow.
// CEP:WHAT: Computes branch displacements.
// CEP:WHY: The displacement is target minus next-instruction address; the fixpoint guarantees the form fits, and any residual overflow is a fixpoint bug that must stop the build.
// CEP:STATUS: complete
// CEP:FAILURE: Returns FixpointUnstable when the displacement outgrows the chosen form.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn branch_disp(
    before: u32,
    opc: Opcode,
    long: bool,
    target: u32,
) -> Result<i64, EncodeError> {
    let next = before + branch_size(opc, long) as u32;
    let disp = i64::from(target) - i64::from(next);
    if long {
        if !fits_i32(disp) {
            return Err(EncodeError::FixpointUnstable);
        }
    } else if !fits_i8(disp) {
        return Err(EncodeError::FixpointUnstable);
    }
    Ok(disp)
}

/// Branch size for the current form.
// CEP:WHAT: Computes branch sizes.
// CEP:WHY: Shared by the size fixpoint and the displacement math.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn branch_size(opc: Opcode, long: bool) -> usize {
    match (opc, long) {
        (Opcode::Jmp, false) | (Opcode::Ret, false) => len::BRANCH8,
        (Opcode::Jmp, true) | (Opcode::Ret, true) => len::BRANCH32_JMP,
        (_, false) => len::BRANCH8,
        (_, true) => len::BRANCH32_JCC,
    }
}

/// Pushes a branch in the chosen form.
// CEP:ID: JIT-s73
// CEP:WHAT: Encodes branch forms.
// CEP:WHY: Jmp, Jl, Jge, and the Ret jump share the displacement logic.
// CEP:STATUS: complete
// CEP:FAILURE: Panics only on a non-branch opcode, which the caller dispatches away.
// CEP:ASSUMES: disp fits the form (checked by branch_disp).
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn push_branch(out: &mut Vec<u8>, opc: Opcode, long: bool, disp: i64) {
    if opc == Opcode::Jmp {
        if long {
            out.push(op::JMP_REL32);
            out.extend_from_slice(&(disp as i32).to_le_bytes());
        } else {
            out.push(op::JMP_REL8);
            out.push(disp as i8 as u8);
        }
        return;
    }
    let (short, long_opc) =
        jcc_encoding(opc).unwrap_or_else(|| panic!("push_branch on non-branch opcode"));
    if long {
        out.push(op::ESCAPE);
        out.push(long_opc);
        out.extend_from_slice(&(disp as i32).to_le_bytes());
    } else {
        out.push(short);
        out.push(disp as i8 as u8);
    }
}

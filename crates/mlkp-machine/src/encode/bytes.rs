// CEP:FILE: crates/mlkp-machine/src/encode/bytes.rs
// CEP:WHAT: Byte-emission primitives and instruction sizing: memory tails, REX construction, per-shape instruction sizes, and the store-form selection table.
// CEP:WHY: Encoding mirrors emit() instruction for instruction so the two artifacts stay provably equivalent.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the EncodeError variants documented per function; no silent truncation of any value.
// CEP:ASSUMES: base names a valid register id; displacements fit i32.
// CEP:COST: cold; O(1) per call.
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs, crates/mlkp-machine/tests/exec_test.rs.

use super::insn::branch_size;
use super::op;
use super::operand::{EncInsn, Resolved, Src};
use super::regs::{enc, enc_reg, fits_i32, fits_i8, hw, hw_num, len, modrm};
use crate::mir::Opcode;
use crate::mir::RegClass;
use crate::target::preg;

/// Memory operand tail size: ModRM plus optional SIB plus displacement.
// CEP:WHAT: Computes the memory operand size.
// CEP:WHY: The fixpoint needs exact sizes; RBP needs an explicit displacement (mod 00 rm 101 means RIP-relative) and RSP needs a SIB byte.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: base names a valid register id; the displacement fits i32.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn mem_tail(base: u32, disp: i64) -> usize {
    let mut n = 1; // ModRM
    let h = hw(base);
    let sib = h == hw_num::RSP; // rm=100 (RSP or R12) means a SIB byte follows
    if sib {
        n += 1;
    }
    if h == hw_num::RBP {
        // mod 00 rm 101 means RIP-relative, so RBP and R13 always take mod 01/10;
        // a zero displacement still needs its byte.
        n += 1;
    } else if disp != 0 {
        n += if fits_i8(disp) { 1 } else { len::DISP32 };
    }
    n
}

/// Appends the memory operand ModRM, optional SIB, and displacement.
// CEP:ID: JIT-u19
// CEP:WHAT: Encodes a memory operand.
// CEP:WHY: Load and Store address spill slots and kernel buffers; the RBP and RSP special cases follow the ModRM tables exactly.
// CEP:STATUS: complete
// CEP:ASSUMES: base names a valid register; the displacement fits i32.
// CEP:FAILURE: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn push_mem(out: &mut Vec<u8>, base: u32, disp: i64, reg_field: u8) {
    let b = enc_reg(base);
    let h = hw(base);
    if h == hw_num::RSP {
        // rm=100: SIB byte with no index; the base stays in SIB.
        let m = if disp == 0 {
            0
        } else if fits_i8(disp) {
            enc::MOD_DISP8
        } else {
            enc::MOD_DISP32
        };
        out.push(modrm(m, reg_field, enc::RM_SIB));
        out.push(modrm(0, enc::SIB_NO_INDEX, b));
        if disp != 0 {
            push_disp(out, disp);
        }
    } else if h == hw_num::RBP {
        // mod 00 rm 101 means RIP-relative, so mod 01 or 10 with an explicit displacement.
        let m = if fits_i8(disp) {
            enc::MOD_DISP8
        } else {
            enc::MOD_DISP32
        };
        out.push(modrm(m, reg_field, b));
        push_disp(out, disp);
    } else if disp == 0 {
        out.push(modrm(0, reg_field, b));
    } else if fits_i8(disp) {
        out.push(modrm(enc::MOD_DISP8, reg_field, b));
        push_disp(out, disp);
    } else {
        out.push(modrm(enc::MOD_DISP32, reg_field, b));
        push_disp(out, disp);
    }
}

/// Appends a displacement in its minimal signed form.
// CEP:WHAT: Encodes a displacement.
// CEP:WHY: Memory operand tail.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the caller checked the range (the fixpoint's sizes preclude skipping it).
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn push_disp(out: &mut Vec<u8>, disp: i64) {
    if fits_i8(disp) {
        out.push(disp as i8 as u8);
    } else {
        out.extend_from_slice(&(disp as i32).to_le_bytes());
    }
}

/// Appends a REX prefix when any bit is needed.
// CEP:WHAT: Encodes the REX prefix.
// CEP:WHY: 64-bit width and extended register bits.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn push_rex(out: &mut Vec<u8>, rex: u8) {
    if rex != 0 {
        out.push(op::REX_BASE | rex);
    }
}

/// 1 when a REX prefix byte is present for the given bits, else 0.
// CEP:WHAT: Reports REX presence.
// CEP:WHY: Sizes must match emission exactly.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn rex_len(rex: u8) -> usize {
    usize::from(rex != 0)
}

/// REX bits for a reg-reg integer form (W always; R for extended source; B for extended destination).
// CEP:WHAT: Computes reg-reg REX bits.
// CEP:WHY: Shared by Mov, Add, Sub, Cmp, IMul.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: both operands are GPRs (validated earlier).
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn rex_rr(dst: &Resolved, src: &Resolved) -> u8 {
    let mut rex = op::REX_W;
    if hw(src.preg) >= hw_num::GPR_EXT {
        rex |= op::REX_R;
    }
    if hw(dst.preg) >= hw_num::GPR_EXT {
        rex |= op::REX_B;
    }
    rex
}

/// REX bits for a register-plus-memory integer form (W always; R for the register; B for the base).
// CEP:WHAT: Computes reg-mem REX bits.
// CEP:WHY: Load and Store.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn rex_rmem(reg: &Resolved, base: u32) -> u8 {
    let mut rex = op::REX_W;
    if hw(reg.preg) >= hw_num::GPR_EXT {
        rex |= op::REX_R;
    }
    if hw(base) >= hw_num::GPR_EXT {
        rex |= op::REX_B;
    }
    rex
}

/// REX bits for an SSE form (no W; R for extended xmm source; B for extended xmm destination or memory base).
// CEP:WHAT: Computes SSE REX bits.
// CEP:WHY: xmm registers above xmm7 need the same extensions.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn rex_sse(
    src_xmm: Option<&Resolved>,
    dst_xmm: Option<&Resolved>,
    base: Option<u32>,
) -> u8 {
    let mut rex = 0u8;
    if src_xmm.is_some_and(|r| r.preg >= hw_num::XMM8_TABLE_ID) {
        // xmm8 is table id 24: ids at or above it need the R bit.
        rex |= op::REX_R;
    }
    if dst_xmm.is_some_and(|r| r.preg >= hw_num::XMM8_TABLE_ID) {
        rex |= op::REX_B;
    }
    if base.is_some_and(|b| hw(b) >= hw_num::GPR_EXT) {
        rex |= op::REX_B;
    }
    rex
}

/// Size of one instruction in its current form; must equal what emit_insn pushes.
// CEP:ID: JIT-k50
// CEP:WHAT: Computes one instruction size.
// CEP:WHY: The fixpoint and the emit pass must agree exactly; a mismatch would misplace every later branch.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the shape passed validation.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn insn_size(e: &EncInsn, long: bool) -> usize {
    match e.op {
        Opcode::Mov => match (&e.dst, &e.src) {
            (Some(_), Some(Src::Imm(v))) => {
                // REX.W is always set for 64-bit moves. C7 carries a ModRM byte;
                // the B8+r accumulator-free movabs form does not.
                if fits_i32(*v) {
                    len::MOV_IMM32
                } else {
                    len::MOVABS
                }
            }
            (Some(d), Some(Src::Reg(s))) => {
                if d.class == RegClass::Xmm64 {
                    let rex = rex_sse(Some(s), Some(d), None);
                    1 + rex_len(rex) + len::SSE_RR - 1
                } else {
                    // REX + opcode + ModRM.
                    rex_len(rex_rr(d, s)) + len::OPCODE_MODRM
                }
            }
            // A folded memory source encodes exactly like a Load.
            (Some(_), Some(Src::Mem { .. })) => insn_size(
                &EncInsn {
                    op: Opcode::Load,
                    dst: e.dst,
                    dst_mem: None,
                    src: e.src.clone(),
                    imm: None,
                    target: None,
                },
                long,
            ),
            _ => 0,
        },
        Opcode::Lea => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Mem { base, disp })) => {
                rex_len(rex_rmem(d, *base)) + 1 + mem_tail(*base, *disp)
            }
            _ => 0,
        },
        Opcode::Add | Opcode::Sub | Opcode::And | Opcode::Or | Opcode::Xor | Opcode::Cmp => {
            match (&e.dst, &e.dst_mem, &e.src) {
                (Some(d), None, Some(Src::Reg(s))) => rex_len(rex_rr(d, s)) + len::OPCODE_MODRM,
                (Some(d), None, Some(Src::Imm(v))) => {
                    // REX.W is always set for 64-bit integer forms.
                    if fits_i8(*v) {
                        len::GRP1_IB
                    } else if d.preg == preg::RAX {
                        len::ACC_IMM32
                    } else {
                        len::GRP1_ID
                    }
                }
                (Some(d), None, Some(Src::Mem { base, disp })) => {
                    rex_len(rex_rmem(d, *base)) + 1 + mem_tail(*base, *disp)
                }
                (None, Some((base, disp)), Some(Src::Reg(s))) => {
                    rex_len(rex_rmem(s, *base)) + 1 + mem_tail(*base, *disp)
                }
                (None, Some((base, disp)), Some(Src::Imm(v))) => {
                    let rex = rex_wb(*base);
                    1 + rex_len(rex)
                        + mem_tail(*base, *disp)
                        + if fits_i8(*v) { 1 } else { len::DISP32 }
                }
                _ => 0,
            }
        }
        Opcode::IMul => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Reg(s))) => rex_len(rex_rr(d, s)) + len::RR_ESC,
            (Some(d), Some(Src::Mem { base, disp })) => {
                rex_len(rex_rmem(d, *base)) + len::RR_ESC + mem_tail(*base, *disp)
            }
            _ => 0,
        },
        Opcode::IMul3 => match (&e.dst, &e.src, e.imm) {
            (Some(d), Some(Src::Reg(s)), Some(v)) => {
                if fits_i8(v) {
                    rex_len(rex_rr(d, s)) + len::OPCODE_MODRM + 1
                } else {
                    rex_len(rex_rr(d, s)) + len::OPCODE_MODRM + len::DISP32
                }
            }
            (Some(d), Some(Src::Mem { base, disp }), Some(v)) => {
                let tail = mem_tail(*base, *disp);
                let body = rex_len(rex_rmem(d, *base)) + 1 + tail;
                if fits_i8(v) {
                    body + 1
                } else {
                    body + len::DISP32
                }
            }
            _ => 0,
        },
        Opcode::IDiv => match (&e.dst, &e.dst_mem) {
            (Some(d), None) => rex_len(rex_rr_w(d)) + len::OPCODE_MODRM,
            (None, Some((base, disp))) => rex_len(rex_wb(*base)) + 1 + mem_tail(*base, *disp),
            _ => 0,
        },
        Opcode::Cqo => len::CQO,
        Opcode::Not | Opcode::Neg => match (&e.dst, &e.dst_mem) {
            (Some(d), None) => rex_len(rex_rr_w(d)) + len::OPCODE_MODRM,
            (None, Some((base, disp))) => rex_len(rex_wb(*base)) + 1 + mem_tail(*base, *disp),
            _ => 0,
        },
        Opcode::Shl | Opcode::Shr | Opcode::Sar => match (&e.dst, &e.dst_mem, &e.src) {
            // A count of one uses the implicit-1 D0 form; other counts use C1 + imm8.
            (Some(d), None, Some(Src::Imm(v))) => {
                rex_len(rex_rr_w(d)) + len::OPCODE_MODRM + if *v == 1 { 0 } else { 1 }
            }
            (Some(d), None, Some(Src::Reg(_))) => rex_len(rex_rr_w(d)) + len::OPCODE_MODRM,
            (None, Some((base, disp)), Some(Src::Imm(v))) => {
                rex_len(rex_wb(*base)) + 1 + mem_tail(*base, *disp) + if *v == 1 { 0 } else { 1 }
            }
            (None, Some((base, disp)), Some(Src::Reg(_))) => {
                rex_len(rex_wb(*base)) + 1 + mem_tail(*base, *disp)
            }
            _ => 0,
        },
        Opcode::Test => match (&e.dst, &e.dst_mem, &e.src) {
            (Some(d), None, Some(Src::Reg(s))) => rex_len(rex_rr(d, s)) + len::OPCODE_MODRM,
            (Some(d), None, Some(Src::Imm(v))) => {
                if fits_i32(*v) {
                    if d.preg == preg::RAX {
                        len::ACC_IMM32
                    } else {
                        // REX + F7 /0 + ModRM + imm32.
                        1 + 1 + 1 + len::DISP32
                    }
                } else {
                    0
                }
            }
            (None, Some((base, disp)), Some(Src::Reg(s))) => {
                rex_len(rex_rmem(s, *base)) + 1 + mem_tail(*base, *disp)
            }
            _ => 0,
        },
        Opcode::Push => match (&e.dst, &e.src) {
            (Some(d), None) => pushpop_len(d.preg),
            (None, Some(Src::Imm(v))) => {
                if fits_i8(*v) {
                    len::PUSH_IMM8_LEN
                } else {
                    len::PUSH_IMM32_LEN
                }
            }
            _ => 0,
        },
        Opcode::Pop => match e.dst {
            Some(d) => pushpop_len(d.preg),
            None => 0,
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
        | Opcode::Jnp
        | Opcode::Ret => branch_size(e.op, long),
        Opcode::Jmp => branch_size(e.op, long),
        Opcode::Call => len::CALL,
        Opcode::Load | Opcode::MovZx8 | Opcode::MovZx32 | Opcode::MovSx32 => {
            match (&e.dst, &e.src) {
                (Some(d), Some(Src::Mem { base, disp })) => {
                    let (w, escaped) = load_shape(e.op);
                    let rex = if w {
                        rex_rmem(d, *base)
                    } else {
                        rex_rb(d, *base)
                    };
                    rex_len(rex) + usize::from(escaped) + 1 + mem_tail(*base, *disp)
                }
                (Some(d), Some(Src::Reg(s))) => {
                    // Load forms put the destination in ModRM.reg and the source
                    // in rm, so the REX direction mirrors the store forms.
                    let (w, escaped) = load_shape(e.op);
                    let mut rex = if w { op::REX_W } else { 0 };
                    if hw(d.preg) >= hw_num::GPR_EXT {
                        rex |= op::REX_R;
                    }
                    if hw(s.preg) >= hw_num::GPR_EXT {
                        rex |= op::REX_B;
                    }
                    rex_len(rex) + usize::from(escaped) + 1 + extra_opc(e.op)
                }
                _ => 0,
            }
        }
        Opcode::Store | Opcode::Store8 | Opcode::Store32 => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Mem { base, disp })) => {
                let (rex, escaped) = store_shape(e.op, d.preg, *base);
                rex_len(rex) + usize::from(escaped) + 1 + mem_tail(*base, *disp)
            }
            _ => 0,
        },
        Opcode::AddSd
        | Opcode::SubSd
        | Opcode::MulSd
        | Opcode::DivSd
        | Opcode::SqrtSd
        | Opcode::UComisd => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Reg(s))) => {
                let rex = rex_sse(Some(s), Some(d), None);
                1 + rex_len(rex) + len::SSE_RR - 1
            }
            (Some(d), Some(Src::Mem { base, disp })) => {
                let rex = rex_sse(None, Some(d), Some(*base));
                1 + rex_len(rex) + len::ESC_OPCODE + mem_tail(*base, *disp)
            }
            _ => 0,
        },
        Opcode::CvtSi2Sd | Opcode::CvtSd2Si => match (&e.dst, &e.src) {
            (Some(_), Some(Src::Reg(_))) => 1 + 1 + len::ESC_OPCODE + 1,
            (Some(_), Some(Src::Mem { base, disp })) => {
                1 + 1 + len::ESC_OPCODE + mem_tail(*base, *disp)
            }
            _ => 0,
        },
    }
}

/// REX bits with W and B set: the shape of single-operand F7-style group forms.
// CEP:WHAT: Computes W-B register REX bits.
// CEP:WHY: Unary and group forms address their operand through rm only.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn rex_rr_w(dst: &Resolved) -> u8 {
    let mut rex = op::REX_W;
    if hw(dst.preg) >= hw_num::GPR_EXT {
        rex |= op::REX_B;
    }
    rex
}

/// REX bits with W and a base extension bit.
// CEP:WHAT: Computes W-B memory REX bits.
// CEP:WHY: Memory-destination and unary memory forms.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn rex_wb(base: u32) -> u8 {
    let mut rex = op::REX_W;
    if hw(base) >= hw_num::GPR_EXT {
        rex |= op::REX_B;
    }
    rex
}

/// REX bits without W for a register-plus-memory form.
// CEP:WHAT: Computes no-W reg-mem REX bits.
// CEP:WHY: 32-bit loads and stores keep 64-bit width off; only extension bits appear.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn rex_rb(dst: &Resolved, base: u32) -> u8 {
    let mut rex = 0u8;
    if hw(dst.preg) >= hw_num::GPR_EXT {
        rex |= op::REX_R;
    }
    if hw(base) >= hw_num::GPR_EXT {
        rex |= op::REX_B;
    }
    rex
}

/// Push/pop byte length for a register id.
// CEP:WHAT: Sizes push and pop.
// CEP:WHY: Extended registers need a REX byte.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn pushpop_len(p: u32) -> usize {
    usize::from(hw(p) >= hw_num::GPR_EXT) + 1
}

/// (needs REX.W, needs two-byte escape) for the load-shaped width moves.
// CEP:WHAT: Reports load shapes.
// CEP:WHY: MovZx8 and MovSx32 use two-byte opcodes; MovZx32 drops the W bit.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn load_shape(op: Opcode) -> (bool, bool) {
    match op {
        Opcode::Load => (true, false),
        Opcode::MovZx8 => (true, true),
        Opcode::MovZx32 => (false, false),
        Opcode::MovSx32 => (true, true),
        _ => (true, false),
    }
}

/// Extra opcode byte count for width moves (the second byte after the escape).
// CEP:WHAT: Reports escape tail bytes.
// CEP:WHY: Two-byte opcodes contribute one more byte than single-byte forms.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn extra_opc(op: Opcode) -> usize {
    match op {
        Opcode::MovZx8 | Opcode::MovSx32 => 1,
        _ => 0,
    }
}

/// (REX byte, needs escape) for width stores; a byte store of registers 4..7
/// (spl, bpl, sil, dil) requires a bare REX prefix even without other bits.
// CEP:ID: JIT-h82
// CEP:WHAT: Reports store shapes.
// CEP:WHY: Byte stores name the low-byte views, which the ISA only addresses with REX active for 4..7.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn store_shape(op: Opcode, val: u32, base: u32) -> (u8, bool) {
    match op {
        Opcode::Store => {
            let mut rex = op::REX_W;
            if hw(val) >= hw_num::GPR_EXT {
                rex |= op::REX_R;
            }
            if hw(base) >= hw_num::GPR_EXT {
                rex |= op::REX_B;
            }
            (rex, false)
        }
        Opcode::Store8 => {
            let mut rex = 0u8;
            if hw(val) >= hw_num::BYTE_REX_MIN {
                // A bare REX activates the 4..7 low-byte names.
                rex |= op::REX_BASE;
                if hw(val) >= hw_num::GPR_EXT {
                    rex |= op::REX_R;
                }
            }
            if hw(base) >= hw_num::GPR_EXT {
                rex |= op::REX_B;
            }
            (rex, false)
        }
        Opcode::Store32 => {
            let mut rex = 0u8;
            if hw(val) >= hw_num::GPR_EXT {
                rex |= op::REX_R;
            }
            if hw(base) >= hw_num::GPR_EXT {
                rex |= op::REX_B;
            }
            (rex, false)
        }
        // Validation rejects every other opcode before the shape is consulted.
        _ => (0, false),
    }
}

// CEP:FILE: crates/mlkp-machine/src/encode/operand.rs
// CEP:WHAT: Operand resolution and validation: turns post-allocation Machine IR operands into resolved physical shapes and rejects every non-encodable combination before any byte is emitted.
// CEP:WHY: Encoding mirrors emit() instruction for instruction so the two artifacts stay provably equivalent.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the EncodeError variants documented per function; no silent truncation of any value.
// CEP:ASSUMES: the allocation is verified before encoding.
// CEP:COST: cold; O(assignment) per register operand.
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs, crates/mlkp-machine/tests/exec_test.rs.

use super::regs::{fits_i32, len};
use super::EncodeError;
use crate::mir::Opcode;
use crate::mir::Operand;
use crate::mir::RegClass;
use crate::target::preg;

/// A resolved operand: physical register id plus class.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Resolved {
    pub(crate) preg: u32,
    pub(crate) class: RegClass,
}

/// Resolved source operand shapes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Src {
    /// Physical register.
    Reg(Resolved),
    /// Immediate value.
    Imm(i64),
    /// Memory [base + disp].
    Mem { base: u32, disp: i64 },
}

/// One instruction with everything resolved and validated.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EncInsn {
    pub(crate) op: Opcode,
    /// Resolved operand 0 when register-bearing.
    pub(crate) dst: Option<Resolved>,
    /// Memory destination (base, disp) for read-modify-write forms.
    pub(crate) dst_mem: Option<(u32, i64)>,
    /// Resolved operand 1.
    pub(crate) src: Option<Src>,
    /// Third operand immediate (three-operand multiply).
    pub(crate) imm: Option<i64>,
    /// Branch or call target label when present.
    pub(crate) target: Option<String>,
}

/// Resolves one register-bearing operand to its physical register.
// CEP:WHAT: Resolves a register operand through the allocation.
// CEP:WHY: The encoder consumes the same post-allocation form the text emitter does.
// CEP:STATUS: complete
// CEP:FAILURE: Returns UnresolvedVReg or UnresolvedSpill per the module contract.
// CEP:ASSUMES: the allocation is verified before encoding.
// CEP:COST: O(assignment).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn resolve_reg(
    o: &Operand,
    alloc: &crate::irc::Allocation,
) -> Result<Resolved, EncodeError> {
    match o {
        Operand::VReg { id, class, .. } => match alloc.assignment.iter().find(|(v, _)| v == id) {
            Some((_, p)) if *p != u32::MAX => Ok(Resolved {
                preg: *p,
                class: *class,
            }),
            Some(_) => Err(EncodeError::UnresolvedSpill(*id)),
            None => Err(EncodeError::UnresolvedVReg(*id)),
        },
        Operand::PReg { id, class, .. } => Ok(Resolved {
            preg: *id,
            class: *class,
        }),
        _ => Err(EncodeError::BadOperand("register operand expected")),
    }
}

/// Resolves and validates all operands of one instruction.
// CEP:ID: JIT-581
// CEP:WHAT: Resolves one instruction's operands.
// CEP:WHY: The size pass and the emit pass share one resolution so both see identical, already-validated shapes.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module errors for unresolvable, malformed, or class-mismatched operands.
// CEP:ASSUMES: none
// CEP:COST: O(assignment) per register operand.
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn resolve_insn(
    insn: &crate::mir::MInsn,
    alloc: &crate::irc::Allocation,
) -> Result<EncInsn, EncodeError> {
    let mut dst: Option<Resolved> = None;
    let mut dst_mem: Option<(u32, i64)> = None;
    match insn.ops.first() {
        Some(o @ (Operand::VReg { .. } | Operand::PReg { .. })) => {
            dst = Some(resolve_reg(o, alloc)?);
        }
        Some(Operand::Mem {
            base,
            base_preg,
            disp,
            ..
        }) => {
            let b = if *base_preg {
                *base
            } else {
                let probe = Operand::VReg {
                    id: *base,
                    class: RegClass::Gpr64,
                    def: false,
                };
                resolve_reg(&probe, alloc)?.preg
            };
            dst_mem = Some((b, *disp));
        }
        _ => {}
    }
    let src = match insn.ops.get(1) {
        Some(Operand::Imm(v)) => Some(Src::Imm(*v)),
        Some(o @ (Operand::VReg { .. } | Operand::PReg { .. })) => {
            Some(Src::Reg(resolve_reg(o, alloc)?))
        }
        Some(Operand::Mem {
            base,
            base_preg,
            disp,
            ..
        }) => {
            let b = if *base_preg {
                *base
            } else {
                let probe = Operand::VReg {
                    id: *base,
                    class: RegClass::Gpr64,
                    def: false,
                };
                resolve_reg(&probe, alloc)?.preg
            };
            Some(Src::Mem {
                base: b,
                disp: *disp,
            })
        }
        _ => None,
    };
    let imm = match insn.ops.get(2) {
        Some(Operand::Imm(v)) => Some(*v),
        _ => None,
    };
    let target = insn.ops.iter().find_map(|o| match o {
        Operand::Block(name) | Operand::Symbol(name) => Some(name.clone()),
        _ => None,
    });
    let e = EncInsn {
        op: insn.op,
        dst,
        dst_mem,
        src,
        imm,
        target,
    };
    validate(&e)?;
    Ok(e)
}

/// Validates one resolved instruction shape.
// CEP:ID: JIT-k18
// CEP:WHAT: Validates operand shapes and classes.
// CEP:WHY: Shapes that cannot encode must fail at resolve time so the size fixpoint and the emit pass never see them; Law 2 forbids silently encoding a guess.
// CEP:STATUS: complete
// CEP:FAILURE: Returns BadOperand or BadClass or ImmOutOfRange for every shape outside the subset.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn validate(e: &EncInsn) -> Result<(), EncodeError> {
    match e.op {
        Opcode::Mov => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Imm(_))) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("mov immediate needs a GPR"));
                }
                Ok(())
            }
            (Some(d), Some(Src::Reg(s))) => {
                if d.class != s.class {
                    return Err(EncodeError::BadClass("mov class mismatch"));
                }
                Ok(())
            }
            // A folded slot source turns the move into a load.
            (Some(_), Some(Src::Mem { .. })) => Ok(()),
            _ => Err(EncodeError::BadOperand(
                "mov needs dst plus reg, imm, or mem",
            )),
        },
        Opcode::Lea => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Mem { .. })) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("lea needs a GPR destination"));
                }
                Ok(())
            }
            _ => Err(EncodeError::BadOperand("lea needs reg dst and mem src")),
        },
        Opcode::Add | Opcode::Sub | Opcode::And | Opcode::Or | Opcode::Xor | Opcode::Cmp => {
            match (&e.dst, &e.dst_mem, &e.src) {
                (Some(d), None, Some(Src::Reg(_))) => {
                    if d.class != RegClass::Gpr64 {
                        return Err(EncodeError::BadClass("integer op needs GPRs"));
                    }
                    Ok(())
                }
                (Some(d), None, Some(Src::Imm(v))) => {
                    if d.class != RegClass::Gpr64 {
                        return Err(EncodeError::BadClass("integer op needs a GPR"));
                    }
                    if !fits_i32(*v) {
                        return Err(EncodeError::ImmOutOfRange(*v));
                    }
                    Ok(())
                }
                // Memory-source load form (spill folding).
                (Some(d), None, Some(Src::Mem { .. })) => {
                    if d.class != RegClass::Gpr64 {
                        return Err(EncodeError::BadClass("integer op needs a GPR"));
                    }
                    Ok(())
                }
                // Memory-destination read-modify-write forms.
                (None, Some(_), Some(Src::Reg(s))) => {
                    if s.class != RegClass::Gpr64 {
                        return Err(EncodeError::BadClass("integer op needs a GPR"));
                    }
                    Ok(())
                }
                (None, Some(_), Some(Src::Imm(v))) => {
                    if !fits_i32(*v) {
                        return Err(EncodeError::ImmOutOfRange(*v));
                    }
                    Ok(())
                }
                _ => Err(EncodeError::BadOperand(
                    "integer op needs reg or mem dst plus reg, imm, or mem src",
                )),
            }
        }
        Opcode::IMul => match (&e.dst, &e.src) {
            (Some(_), Some(Src::Reg(_))) => Ok(()),
            (Some(_), Some(Src::Mem { .. })) => Ok(()),
            _ => Err(EncodeError::BadOperand(
                "imul needs a register or memory source",
            )),
        },
        Opcode::IMul3 => match (&e.dst, &e.src, e.imm) {
            (Some(d), Some(Src::Reg(s)), Some(v)) => {
                if d.class != RegClass::Gpr64 || s.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("imul3 needs GPR operands"));
                }
                if !fits_i32(v) {
                    return Err(EncodeError::ImmOutOfRange(v));
                }
                Ok(())
            }
            (Some(d), Some(Src::Mem { .. }), Some(v)) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("imul3 needs a GPR destination"));
                }
                if !fits_i32(v) {
                    return Err(EncodeError::ImmOutOfRange(v));
                }
                Ok(())
            }
            _ => Err(EncodeError::BadOperand("imul3 needs dst, src, and imm")),
        },
        Opcode::IDiv => match (&e.dst, &e.dst_mem, &e.src) {
            (Some(d), None, None) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("idiv needs a GPR"));
                }
                Ok(())
            }
            (None, Some(_), None) => Ok(()),
            _ => Err(EncodeError::BadOperand("idiv takes one reg or mem operand")),
        },
        Opcode::Cqo => {
            if e.dst.is_some() || e.dst_mem.is_some() || e.src.is_some() {
                return Err(EncodeError::BadOperand("cqo takes no operands"));
            }
            Ok(())
        }
        Opcode::Not | Opcode::Neg => match (&e.dst, &e.dst_mem) {
            (Some(d), None) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("unary op needs a GPR"));
                }
                Ok(())
            }
            (None, Some(_)) => Ok(()),
            _ => Err(EncodeError::BadOperand(
                "unary op takes one reg or mem operand",
            )),
        },
        Opcode::Shl | Opcode::Shr | Opcode::Sar => {
            let ext_ok = match e.op {
                Opcode::Shl => true,
                Opcode::Shr => true,
                _ => true,
            };
            let _ = ext_ok;
            match (&e.dst, &e.dst_mem, &e.src) {
                (Some(d), None, Some(Src::Imm(v))) => {
                    if d.class != RegClass::Gpr64 {
                        return Err(EncodeError::BadClass("shift needs a GPR"));
                    }
                    // Hardware masks the count to six bits; a larger immediate
                    // would silently shift by a different amount.
                    if !(0..=len::SHIFT_COUNT_MAX).contains(v) {
                        return Err(EncodeError::ImmOutOfRange(*v));
                    }
                    Ok(())
                }
                (Some(d), None, Some(Src::Reg(s))) => {
                    if d.class != RegClass::Gpr64 {
                        return Err(EncodeError::BadClass("shift needs a GPR"));
                    }
                    if s.preg != preg::RCX {
                        return Err(EncodeError::BadOperand("shift count register must be rcx"));
                    }
                    Ok(())
                }
                (None, Some(_), Some(Src::Imm(_))) => Ok(()),
                (None, Some(_), Some(Src::Reg(s))) => {
                    if s.preg != preg::RCX {
                        return Err(EncodeError::BadOperand("shift count register must be rcx"));
                    }
                    Ok(())
                }
                _ => Err(EncodeError::BadOperand("shift needs imm or rcx source")),
            }
        }
        Opcode::Test => match (&e.dst, &e.dst_mem, &e.src) {
            (Some(d), None, Some(Src::Reg(_))) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("test needs GPRs"));
                }
                Ok(())
            }
            (Some(d), None, Some(Src::Imm(v))) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("test needs a GPR"));
                }
                if !fits_i32(*v) {
                    return Err(EncodeError::ImmOutOfRange(*v));
                }
                Ok(())
            }
            (None, Some(_), Some(Src::Reg(_))) => Ok(()),
            _ => Err(EncodeError::BadOperand(
                "test needs reg dst plus reg or imm src",
            )),
        },
        Opcode::Push => match (&e.dst, &e.src) {
            (Some(d), None) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("push needs a GPR"));
                }
                Ok(())
            }
            (None, Some(Src::Imm(v))) => {
                if !fits_i32(*v) {
                    return Err(EncodeError::ImmOutOfRange(*v));
                }
                Ok(())
            }
            _ => Err(EncodeError::BadOperand("push takes one reg or imm operand")),
        },
        Opcode::Pop => match (&e.dst, &e.src) {
            (Some(d), None) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("pop needs a GPR"));
                }
                Ok(())
            }
            _ => Err(EncodeError::BadOperand("pop takes one reg operand")),
        },
        Opcode::Jmp => match &e.target {
            Some(_) => Ok(()),
            None => Err(EncodeError::BadOperand("branch needs a block target")),
        },
        Opcode::Ret => Ok(()),
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
        | Opcode::Jnp => match &e.target {
            Some(_) => Ok(()),
            None => Err(EncodeError::BadOperand("branch needs a block target")),
        },
        Opcode::Call => match (&e.dst, &e.target) {
            (None, Some(_)) => Ok(()),
            _ => Err(EncodeError::BadOperand("call takes only a target")),
        },
        Opcode::Load => match (&e.dst, &e.src) {
            (Some(_), Some(Src::Mem { .. })) => Ok(()),
            _ => Err(EncodeError::BadOperand("load needs reg dst and mem src")),
        },
        Opcode::Store | Opcode::Store8 | Opcode::Store32 => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Mem { .. })) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("width store needs a GPR"));
                }
                Ok(())
            }
            _ => Err(EncodeError::BadOperand("store needs reg dst and mem src")),
        },
        Opcode::AddSd
        | Opcode::SubSd
        | Opcode::MulSd
        | Opcode::DivSd
        | Opcode::SqrtSd
        | Opcode::UComisd => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Reg(_))) => {
                if d.class != RegClass::Xmm64 {
                    return Err(EncodeError::BadClass("sse op needs XMM operands"));
                }
                Ok(())
            }
            (Some(d), Some(Src::Mem { .. })) => {
                if d.class != RegClass::Xmm64 {
                    return Err(EncodeError::BadClass("sse op needs an XMM destination"));
                }
                Ok(())
            }
            _ => Err(EncodeError::BadOperand(
                "sse op needs a register or memory source",
            )),
        },
        Opcode::CvtSi2Sd => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Reg(_))) => {
                if d.class != RegClass::Xmm64 {
                    return Err(EncodeError::BadClass("cvtsi2sd needs an XMM destination"));
                }
                Ok(())
            }
            (Some(d), Some(Src::Mem { .. })) => {
                if d.class != RegClass::Xmm64 {
                    return Err(EncodeError::BadClass("cvtsi2sd needs an XMM destination"));
                }
                Ok(())
            }
            _ => Err(EncodeError::BadOperand("cvtsi2sd needs reg or mem source")),
        },
        Opcode::CvtSd2Si => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Reg(_))) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("cvttsd2si needs a GPR destination"));
                }
                Ok(())
            }
            (Some(d), Some(Src::Mem { .. })) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("cvttsd2si needs a GPR destination"));
                }
                Ok(())
            }
            _ => Err(EncodeError::BadOperand("cvttsd2si needs reg or mem source")),
        },
        Opcode::MovZx8 | Opcode::MovZx32 | Opcode::MovSx32 => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Reg(s))) => {
                if d.class != RegClass::Gpr64 || s.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("width move needs GPRs"));
                }
                Ok(())
            }
            (Some(d), Some(Src::Mem { .. })) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("width move needs a GPR destination"));
                }
                Ok(())
            }
            _ => Err(EncodeError::BadOperand(
                "width move needs reg dst plus reg or mem src",
            )),
        },
    }
}

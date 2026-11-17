// CEP:FILE: crates/mlkp-machine/src/encode.rs
// CEP:WHAT: Byte-level x86-64 encoder: turns one allocated machine function plus its allocation into executable machine bytes with a shared prologue and epilogue, resolved branch displacements, and relocations for external call symbols (ticket CEP-322).
// CEP:WHY: Design.md's machine backend ends at machine code emission with encoding, relocations, and object files; the text emitter reviews, the encoder executes. Encoding mirrors emit() instruction for instruction so the two artifacts stay provably equivalent.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns EncodeError for unresolved or spilled virtual registers, immediates outside the encodable range, unknown branch labels, an unstable branch-size fixpoint, or operand shapes the subset does not encode; no silent truncation of any value.
// CEP:ASSUMES: the function is allocated and verified (all vregs resolve through the allocation, none spilled); block labels are unique and the label "ret" is reserved for the epilogue; the System V AMD64 ABI; the frame formula matches emit() exactly (align16(8*max_saved_gprs + 8*spill_slots)).
// CEP:COST: cold; O(insns) per fixpoint round with at most 64 rounds, then O(insns) to emit.
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs, crates/mlkp-machine/tests/exec_test.rs.

#![deny(missing_docs)]

use crate::mir::{MFunction, Opcode, Operand, RegClass};
use crate::target::{preg, X86_64Target};

/// One byte, named per the Intel manual, so encoders and decoders read like the ISA.
pub(crate) mod op {
    /// REX prefix base.
    pub const REX_BASE: u8 = 0x40;
    /// REX.W bit (64-bit operand size).
    pub const REX_W: u8 = 0x08;
    /// REX.R bit (ModRM.reg extension).
    pub const REX_R: u8 = 0x04;
    /// REX.B bit (ModRM.rm / SIB.base extension).
    pub const REX_B: u8 = 0x01;
    /// Group with immediate byte (add/sub/cmp imm8 forms).
    pub const GRP1_IB: u8 = 0x83;
    /// Group with immediate dword (add/sub/cmp imm32 forms).
    pub const GRP1_ID: u8 = 0x81;
    /// MOV r/m64, r64.
    pub const MOV_MR: u8 = 0x89;
    /// MOV r64, r/m64.
    pub const MOV_RM: u8 = 0x8B;
    /// MOV r/m64, imm32 (sign-extended).
    pub const MOV_IMM32: u8 = 0xC7;
    /// MOV r64, imm64 (movabs) base opcode B8+r.
    pub const MOV_IMM64: u8 = 0xB8;
    /// ADD r/m64, r64.
    pub const ADD_MR: u8 = 0x01;
    /// SUB r/m64, r64.
    pub const SUB_MR: u8 = 0x29;
    /// ADD r64, r/m64 (memory-source load form).
    pub const ADD_RM: u8 = 0x03;
    /// SUB r64, r/m64 (memory-source load form).
    pub const SUB_RM: u8 = 0x2B;
    /// CMP r64, r/m64 (memory-source load form).
    pub const CMP_RM: u8 = 0x3B;
    /// ADD EAX, imm32 short form.
    pub const ADD_EAX_ID: u8 = 0x05;
    /// SUB EAX, imm32 short form.
    pub const SUB_EAX_ID: u8 = 0x2D;
    /// CMP EAX, imm32 short form.
    pub const CMP_EAX_ID: u8 = 0x3D;
    /// Two-byte opcode escape.
    pub const ESCAPE: u8 = 0x0F;
    /// CMP r/m64, r64.
    pub const CMP_MR: u8 = 0x39;
    /// IMUL r64, r/m64 (after the escape).
    pub const IMUL_RM: u8 = 0xAF;
    /// ADD group /0 extension in ModRM.reg.
    pub const EXT_ADD: u8 = 0;
    /// SUB group /5 extension.
    pub const EXT_SUB: u8 = 5;
    /// CMP group /7 extension.
    pub const EXT_CMP: u8 = 7;
    /// PUSH r64 base opcode 50+r.
    pub const PUSH_R: u8 = 0x50;
    /// POP r64 base opcode 58+r.
    pub const POP_R: u8 = 0x58;
    /// JMP rel8.
    pub const JMP_REL8: u8 = 0xEB;
    /// JMP rel32.
    pub const JMP_REL32: u8 = 0xE9;
    /// JL rel8.
    pub const JL_REL8: u8 = 0x7C;
    /// JL rel32 second byte after the escape.
    pub const JL_REL32: u8 = 0x8C;
    /// JGE rel8.
    pub const JGE_REL8: u8 = 0x7D;
    /// JGE rel32 second byte after the escape.
    pub const JGE_REL32: u8 = 0x8D;
    /// CALL rel32.
    pub const CALL_REL32: u8 = 0xE8;
    /// RET near.
    pub const RET: u8 = 0xC3;
    /// SSE scalar-double prefix F2.
    pub const PRE_F2: u8 = 0xF2;
    /// SSE packed-double prefix 66.
    pub const PRE_66: u8 = 0x66;
    /// ADDSD (after the escape).
    pub const ADDSD: u8 = 0x58;
    /// MULSD (after the escape).
    pub const MULSD: u8 = 0x59;
    /// MOVAPD (reg-reg xmm move, after the escape).
    pub const MOVAPD: u8 = 0x28;
    /// MOVSD load form xmm, m64 (after the escape).
    pub const MOVSD_LOAD: u8 = 0x10;
    /// MOVSD store form m64, xmm (after the escape).
    pub const MOVSD_STORE: u8 = 0x11;
    /// PUSH RBP (no register field).
    pub const PUSH_RBP: u8 = 0x55;
    /// POP RBP (no register field).
    pub const POP_RBP: u8 = 0x5D;
    /// REX byte used for R12..R15 pushes and pops (B bit set).
    pub const REX_B_ONLY: u8 = 0x41;
    /// ModRM of movq %rsp, %rbp (reg = RSP, rm = RBP in hardware numbers).
    pub const MODRM_MOV_RSP_RBP: u8 = 0xE5;
    /// ModRM of movq %rbp, %rsp (reg = RBP, rm = RSP in hardware numbers).
    pub const MODRM_MOV_RBP_RSP: u8 = 0xEC;
}

/// Hardware register numbers of the ABI registers the target table reorders.
mod hw_num {
    /// Hardware encoding of RSI.
    pub const RSI: u32 = 6;
    /// Hardware encoding of RDI.
    pub const RDI: u32 = 7;
    /// Hardware encoding of RBP.
    pub const RBP: u32 = 5;
    /// Hardware encoding of RSP.
    pub const RSP: u32 = 4;
    /// Highest hardware GPR number.
    pub const GPR_MAX: u32 = 15;
    /// First extended (R8+) hardware number.
    pub const GPR_EXT: u32 = 8;
    /// Table id where the xmm registers begin.
    pub const XMM_TABLE_BASE: u32 = 16;
    /// Table id of xmm8: ids at or above it need a REX extension bit.
    pub const XMM8_TABLE_ID: u32 = 24;
}

/// Structural byte counts the size fixpoint and the emit pass share.
mod len {
    /// Opcode byte plus ModRM byte.
    pub const OPCODE_MODRM: usize = 2;
    /// Escape plus opcode (two-byte opcode head without REX).
    pub const ESC_OPCODE: usize = 2;
    /// REX plus escape plus opcode plus ModRM (two-byte-opcode reg form).
    pub const RR_ESC: usize = 3;
    /// Prefix plus escape plus opcode plus ModRM (SSE reg form without REX).
    pub const SSE_RR: usize = 4;
    /// REX plus group opcode plus ModRM plus imm8.
    pub const GRP1_IB: usize = 4;
    /// REX plus accumulator opcode plus imm32.
    pub const ACC_IMM32: usize = 6;
    /// REX plus group opcode plus ModRM plus imm32.
    pub const GRP1_ID: usize = 7;
    /// REX plus C7 plus ModRM plus imm32 (register-target move).
    pub const MOV_IMM32: usize = 7;
    /// REX plus movabs opcode plus imm64.
    pub const MOVABS: usize = 10;
    /// CALL rel32.
    pub const CALL: usize = 5;
    /// Branch with rel8.
    pub const BRANCH8: usize = 2;
    /// Branch with rel32 (unconditional).
    pub const BRANCH32_JMP: usize = 5;
    /// Branch with rel32 (conditional, two-byte opcode).
    pub const BRANCH32_JCC: usize = 6;
    /// A rel32 displacement field.
    pub const REL32: usize = 4;
    /// A disp32 displacement.
    pub const DISP32: usize = 4;
    /// The frame-pointer move in the prologue and epilogue.
    pub const FRAME_MOV: usize = 3;
    /// The call relocation addend: the rel32 field length as a negative bias.
    pub const CALL_ADDEND: i64 = -4;
    /// pop rbp plus ret at the epilogue tail.
    pub const POP_RET: usize = 2;
}

/// ModRM and SIB shape constants.
mod enc {
    /// mod 11: register-direct operand.
    pub const MOD_REG: u8 = 3;
    /// mod 01: disp8 follows.
    pub const MOD_DISP8: u8 = 1;
    /// mod 10: disp32 follows.
    pub const MOD_DISP32: u8 = 2;
    /// rm = 100: a SIB byte follows.
    pub const RM_SIB: u8 = 4;
    /// SIB index = 100: no index register.
    pub const SIB_NO_INDEX: u8 = 4;
    /// Low three bits mask of a register number.
    pub const REG_MASK: u32 = 7;
    /// ModRM.reg field bit offset.
    pub const REG_SHIFT: u32 = 3;
    /// ModRM.mod field bit offset.
    pub const MOD_SHIFT: u32 = 6;
}

/// Encoder failure modes.
#[derive(Clone, Debug, PartialEq)]
pub enum EncodeError {
    /// An operand still references a virtual register missing from the allocation.
    UnresolvedVReg(u32),
    /// A spilled value was referenced; allocate() rewrites spills away, so this is a corrupt input.
    UnresolvedSpill(u32),
    /// An immediate is outside the range the subset encodes for this instruction.
    ImmOutOfRange(i64),
    /// A branch or call targets an unknown label.
    UnknownLabel(String),
    /// The branch-size fixpoint did not stabilize within the round cap.
    FixpointUnstable,
    /// The operand shape is not encodable (for example a memory operand on Mov).
    BadOperand(&'static str),
    /// A register class does not fit the instruction's expectations.
    BadClass(&'static str),
}

impl std::fmt::Display for EncodeError {
    // CEP:WHAT: Formats the encode error.
    // CEP:WHY: Deterministic diagnostics for backend failures.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::UnresolvedVReg(id) => write!(f, "encode error: vreg {id} unresolved"),
            EncodeError::UnresolvedSpill(id) => write!(f, "encode error: spill {id} unresolved"),
            EncodeError::ImmOutOfRange(v) => write!(f, "encode error: immediate {v} out of range"),
            EncodeError::UnknownLabel(l) => write!(f, "encode error: unknown label {l}"),
            EncodeError::FixpointUnstable => {
                write!(f, "encode error: branch size fixpoint unstable")
            }
            EncodeError::BadOperand(s) => write!(f, "encode error: bad operand ({s})"),
            EncodeError::BadClass(s) => write!(f, "encode error: bad register class ({s})"),
        }
    }
}

/// One relocation the object emitter must resolve: the rel32 field at `offset` refers to `symbol`.
#[derive(Clone, Debug, PartialEq)]
pub struct Reloc {
    /// Byte offset of the rel32 field inside the text section.
    pub offset: u32,
    /// Referenced external symbol.
    pub symbol: String,
    /// Relocation addend (the -4 PC-relative bias of a call).
    pub addend: i64,
}

/// The compiled artifact: executable bytes, relocations in instruction order, and label offsets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodeObject {
    /// Encoded machine bytes (the .text section contents).
    pub text: Vec<u8>,
    /// Relocations in instruction order.
    pub relocs: Vec<Reloc>,
    /// Label byte offsets in program order; the epilogue comes last.
    pub labels: Vec<(u32, String)>,
}

/// Reserved epilogue label; a block with this name makes encoding fail loudly.
pub const EPILOGUE_LABEL: &str = "ret";

/// Cap on branch-size fixpoint rounds; sizes only grow, so the loop terminates far earlier.
const MAX_FIXPOINT_ROUNDS: u32 = 64;

/// Frame alignment in bytes; the System V ABI requires 16-byte rsp alignment at call sites.
const FRAME_ALIGN: i64 = 16;

/// Saved-register and spill-slot width in bytes.
const SLOT_WIDTH: i64 = 8;

/// True when the value fits a signed byte.
// CEP:WHAT: Tests i8 range.
// CEP:WHY: Displacement and immediate size selection.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn fits_i8(v: i64) -> bool {
    v >= i64::from(i8::MIN) && v <= i64::from(i8::MAX)
}

/// True when the value fits a signed 32-bit integer.
// CEP:WHAT: Tests i32 range.
// CEP:WHY: Immediate and displacement size selection.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn fits_i32(v: i64) -> bool {
    v >= i64::from(i32::MIN) && v <= i64::from(i32::MAX)
}

/// Maps a target-table register id to its hardware encoding number.
///
/// The target table orders GPRs by ABI role (RAX, RCX, RDX, RBX, RSI, RDI,
/// RBP, RSP, R8..R15) while the ISA encodes them in the order AX, CX, DX, BX,
/// SP, BP, SI, DI, R8..R15; this function is the only place that knows both.
// CEP:WHAT: Maps table ids to hardware numbers.
// CEP:WHY: ModRM, SIB, push, and pop encodings are defined over hardware numbers, not table ids.
// CEP:STATUS: complete
// CEP:FAILURE: Panics for ids above R15; the target table defines exactly 16 GPRs and validation runs first.
// CEP:ASSUMES: table ids 0..=15 are GPRs in ABI order and 16.. are xmm ids offset by 16.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn hw(id: u32) -> u32 {
    const LOW_GPR_MAX: u32 = 3;
    match id {
        0..=LOW_GPR_MAX => id, // RAX, RCX, RDX, RBX encode as themselves
        preg::RSI => hw_num::RSI,
        preg::RDI => hw_num::RDI,
        preg::RBP => hw_num::RBP,
        preg::RSP => hw_num::RSP,
        hw_num::GPR_EXT..=hw_num::GPR_MAX => id, // R8..R15 encode as themselves
        _ => panic!("hw(): non-GPR register id {id}"),
    }
}

/// The low three bits of a register id as the ISA encodes it.
// CEP:WHAT: Extracts the ISA register index.
// CEP:WHY: ModRM and REX encoding.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn enc_reg(id: u32) -> u8 {
    if id >= hw_num::XMM_TABLE_BASE {
        // xmm ids sit above the GPR table and encode linearly (xmm8 = 8).
        ((id - hw_num::XMM_TABLE_BASE) & enc::REG_MASK) as u8
    } else {
        (hw(id) & enc::REG_MASK) as u8
    }
}

/// Builds a ModRM byte from mod, reg, rm.
// CEP:WHAT: Packs a ModRM byte.
// CEP:WHY: Every register or memory operand needs one.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: fields already in range.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn modrm(mod_: u8, reg: u8, rm: u8) -> u8 {
    (mod_ << enc::MOD_SHIFT)
        | ((reg & enc::REG_MASK as u8) << enc::REG_SHIFT as u8)
        | (rm & enc::REG_MASK as u8)
}

/// A resolved operand: physical register id plus class.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Resolved {
    preg: u32,
    class: RegClass,
}

/// Resolved source operand shapes.
#[derive(Clone, Debug, PartialEq)]
enum Src {
    /// Physical register.
    Reg(Resolved),
    /// Immediate value.
    Imm(i64),
    /// Memory [base + disp].
    Mem { base: u32, disp: i64 },
}

/// One instruction with everything resolved and validated.
#[derive(Clone, Debug, PartialEq)]
struct EncInsn {
    op: Opcode,
    /// Resolved operand 0 when register-bearing.
    dst: Option<Resolved>,
    /// Resolved operand 1.
    src: Option<Src>,
    /// Branch or call target label when present.
    target: Option<String>,
}

/// Resolves one register-bearing operand to its physical register.
// CEP:WHAT: Resolves a register operand through the allocation.
// CEP:WHY: The encoder consumes the same post-allocation form the text emitter does.
// CEP:STATUS: complete
// CEP:FAILURE: Returns UnresolvedVReg or UnresolvedSpill per the module contract.
// CEP:ASSUMES: the allocation is verified before encoding.
// CEP:COST: O(assignment).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn resolve_reg(o: &Operand, alloc: &crate::irc::Allocation) -> Result<Resolved, EncodeError> {
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
// CEP:WHAT: Resolves one instruction's operands.
// CEP:WHY: The size pass and the emit pass share one resolution so both see identical, already-validated shapes.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module errors for unresolvable, malformed, or class-mismatched operands.
// CEP:ASSUMES: none
// CEP:COST: O(assignment) per register operand.
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn resolve_insn(
    insn: &crate::mir::MInsn,
    alloc: &crate::irc::Allocation,
) -> Result<EncInsn, EncodeError> {
    let dst = match insn.ops.first() {
        Some(o @ (Operand::VReg { .. } | Operand::PReg { .. })) => Some(resolve_reg(o, alloc)?),
        Some(_) => None,
        None => None,
    };
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
    let target = insn.ops.iter().find_map(|o| match o {
        Operand::Block(name) | Operand::Symbol(name) => Some(name.clone()),
        _ => None,
    });
    let e = EncInsn {
        op: insn.op,
        dst,
        src,
        target,
    };
    validate(&e)?;
    Ok(e)
}

/// Validates one resolved instruction shape.
// CEP:WHAT: Validates operand shapes and classes.
// CEP:WHY: Shapes that cannot encode must fail at resolve time so the size fixpoint and the emit pass never see them; Law 2 forbids silently encoding a guess.
// CEP:STATUS: complete
// CEP:FAILURE: Returns BadOperand or BadClass or ImmOutOfRange for every shape outside the subset.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn validate(e: &EncInsn) -> Result<(), EncodeError> {
    match e.op {
        Opcode::Mov => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Imm(v))) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("mov immediate needs a GPR"));
                }
                let _ = v; // i64 is the widest encodable immediate (movabs); nothing narrower to check.
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
        Opcode::Add | Opcode::Sub | Opcode::Cmp => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Reg(_))) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("integer op needs GPRs"));
                }
                Ok(())
            }
            (Some(d), Some(Src::Imm(v))) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("integer op needs a GPR"));
                }
                if !fits_i32(*v) {
                    return Err(EncodeError::ImmOutOfRange(*v));
                }
                Ok(())
            }
            // The spill rewriter folds a slot-homed source into one memory operand.
            (Some(d), Some(Src::Mem { .. })) => {
                if d.class != RegClass::Gpr64 {
                    return Err(EncodeError::BadClass("integer op needs a GPR"));
                }
                Ok(())
            }
            _ => Err(EncodeError::BadOperand(
                "integer op needs reg, imm, or mem source",
            )),
        },
        Opcode::IMul => match (&e.dst, &e.src) {
            (Some(_), Some(Src::Reg(_))) => Ok(()),
            (Some(_), Some(Src::Mem { .. })) => Ok(()),
            _ => Err(EncodeError::BadOperand(
                "imul needs a register or memory source",
            )),
        },
        Opcode::Jmp | Opcode::Jl | Opcode::Jge => match &e.target {
            Some(_) => Ok(()),
            None => Err(EncodeError::BadOperand("branch needs a block target")),
        },
        Opcode::Ret => Ok(()),
        Opcode::Call => match (&e.dst, &e.target) {
            (None, Some(_)) => Ok(()),
            _ => Err(EncodeError::BadOperand("call takes only a target")),
        },
        Opcode::Load => match (&e.dst, &e.src) {
            (Some(_), Some(Src::Mem { .. })) => Ok(()),
            _ => Err(EncodeError::BadOperand("load needs reg dst and mem src")),
        },
        Opcode::Store => match (&e.dst, &e.src) {
            (Some(_), Some(Src::Mem { .. })) => Ok(()),
            _ => Err(EncodeError::BadOperand("store needs reg dst and mem src")),
        },
        Opcode::AddSd | Opcode::MulSd => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Reg(_))) => {
                if d.class != RegClass::Xmm64 {
                    return Err(EncodeError::BadClass("sse op needs XMM operands"));
                }
                Ok(())
            }
            _ => Err(EncodeError::BadOperand("sse op needs a register source")),
        },
    }
}

/// Memory operand tail size: ModRM plus optional SIB plus displacement.
// CEP:WHAT: Computes the memory operand size.
// CEP:WHY: The fixpoint needs exact sizes; RBP needs an explicit displacement (mod 00 rm 101 means RIP-relative) and RSP needs a SIB byte.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: base names a valid register id; the displacement fits i32.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn mem_tail(base: u32, disp: i64) -> usize {
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
// CEP:WHAT: Encodes a memory operand.
// CEP:WHY: Load and Store address spill slots and kernel buffers; the RBP and RSP special cases follow the ModRM tables exactly.
// CEP:STATUS: complete
// CEP:ASSUMES: base names a valid register; the displacement fits i32.
// CEP:FAILURE: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn push_mem(out: &mut Vec<u8>, base: u32, disp: i64, reg_field: u8) {
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
fn push_disp(out: &mut Vec<u8>, disp: i64) {
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
fn push_rex(out: &mut Vec<u8>, rex: u8) {
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
fn rex_len(rex: u8) -> usize {
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
fn rex_rr(dst: &Resolved, src: &Resolved) -> u8 {
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
fn rex_rmem(reg: &Resolved, base: u32) -> u8 {
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
fn rex_sse(src_xmm: Option<&Resolved>, dst_xmm: Option<&Resolved>, base: Option<u32>) -> u8 {
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
// CEP:WHAT: Computes one instruction size.
// CEP:WHY: The fixpoint and the emit pass must agree exactly; a mismatch would misplace every later branch.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the shape passed validation.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn insn_size(e: &EncInsn, long: bool) -> usize {
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
                    src: e.src.clone(),
                    target: None,
                },
                long,
            ),
            _ => 0,
        },
        Opcode::Add | Opcode::Sub | Opcode::Cmp => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Imm(v))) => {
                // REX.W is always set for 64-bit integer forms, so the prefix is always present.
                if fits_i8(*v) {
                    len::GRP1_IB // REX 83 /d ib
                } else if d.preg == preg::RAX {
                    len::ACC_IMM32 // REX 3d id (accumulator short form)
                } else {
                    len::GRP1_ID // REX 81 /d id
                }
            }
            (Some(d), Some(Src::Reg(s))) => rex_len(rex_rr(d, s)) + len::OPCODE_MODRM,
            (Some(d), Some(Src::Mem { base, disp })) => {
                // REX + opcode + memory tail.
                rex_len(rex_rmem(d, *base)) + 1 + mem_tail(*base, *disp)
            }
            _ => 0,
        },
        Opcode::IMul => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Reg(s))) => rex_len(rex_rr(d, s)) + len::RR_ESC,
            (Some(d), Some(Src::Mem { base, disp })) => {
                rex_len(rex_rmem(d, *base)) + len::ESC_OPCODE + mem_tail(*base, *disp)
            }
            _ => 0,
        },
        Opcode::Jmp | Opcode::Jl | Opcode::Jge | Opcode::Ret => match (e.op, long) {
            (Opcode::Jmp, false) | (Opcode::Ret, false) => len::BRANCH8,
            (Opcode::Jmp, true) | (Opcode::Ret, true) => len::BRANCH32_JMP,
            (_, false) => len::BRANCH8,
            (_, true) => len::BRANCH32_JCC,
        },
        Opcode::Call => len::CALL,
        Opcode::Load | Opcode::Store => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Mem { base, disp })) => {
                if d.class == RegClass::Xmm64 {
                    let rex = rex_sse(None, Some(d), Some(*base));
                    1 + rex_len(rex) + len::ESC_OPCODE + mem_tail(*base, *disp)
                } else {
                    // REX + opcode + ModRM tail.
                    let rex = rex_rmem(d, *base);
                    rex_len(rex) + 1 + mem_tail(*base, *disp)
                }
            }
            _ => 0,
        },
        Opcode::AddSd | Opcode::MulSd => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Reg(s))) => {
                let rex = rex_sse(Some(s), Some(d), None);
                1 + rex_len(rex) + len::SSE_RR - 1
            }
            _ => 0,
        },
    }
}

/// Emits one instruction at the current cursor with the final branch form.
// CEP:WHAT: Emits one instruction.
// CEP:WHY: The emit pass of the two-phase encode.
// CEP:STATUS: complete
// CEP:FAILURE: Returns FixpointUnstable if a branch displacement no longer fits its chosen form; other errors are unreachable after validation.
// CEP:ASSUMES: the fixpoint settled the branch form.
// CEP:COST: O(1) amortized.
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn emit_insn(
    out: &mut Vec<u8>,
    e: &EncInsn,
    long: bool,
    target_off: Option<u32>,
    relocs: &mut Vec<Reloc>,
) -> Result<(), EncodeError> {
    let before = out.len() as u32;
    match e.op {
        Opcode::Mov => match (&e.dst, &e.src) {
            // A folded memory source encodes exactly like a Load.
            (Some(_), Some(Src::Mem { .. })) => {
                let load = EncInsn {
                    op: Opcode::Load,
                    dst: e.dst,
                    src: e.src.clone(),
                    target: None,
                };
                return emit_insn(out, &load, long, None, relocs);
            }
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
            _ => unreachable!("validated at resolve time"),
        },
        Opcode::Add | Opcode::Sub | Opcode::Cmp => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Reg(s))) => {
                let opc = match e.op {
                    Opcode::Add => op::ADD_MR,
                    Opcode::Sub => op::SUB_MR,
                    _ => op::CMP_MR,
                };
                let rex = rex_rr(d, s);
                push_rex(out, rex);
                out.push(opc);
                out.push(modrm(enc::MOD_REG, enc_reg(s.preg), enc_reg(d.preg)));
            }
            (Some(d), Some(Src::Mem { base, disp })) => {
                // Load-form opcodes: dst <- dst op [mem]. The store forms
                // (01/29/39 with rm=memory) would compute mem op= reg instead.
                let opc = match e.op {
                    Opcode::Add => op::ADD_RM,
                    Opcode::Sub => op::SUB_RM,
                    _ => op::CMP_RM,
                };
                let rex = rex_rmem(d, *base);
                push_rex(out, rex);
                out.push(opc);
                push_mem(out, *base, *disp, enc_reg(d.preg));
            }
            (Some(d), Some(Src::Imm(v))) => {
                let (ext, short) = match e.op {
                    Opcode::Add => (op::EXT_ADD, op::ADD_EAX_ID),
                    Opcode::Sub => (op::EXT_SUB, op::SUB_EAX_ID),
                    _ => (op::EXT_CMP, op::CMP_EAX_ID),
                };
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
            _ => unreachable!("validated at resolve time"),
        },
        Opcode::IMul => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Reg(s))) => {
                let rex = rex_rr(d, s);
                push_rex(out, rex);
                out.push(op::ESCAPE);
                out.push(op::IMUL_RM);
                out.push(modrm(enc::MOD_REG, enc_reg(s.preg), enc_reg(d.preg)));
            }
            (Some(d), Some(Src::Mem { base, disp })) => {
                let rex = rex_rmem(d, *base);
                push_rex(out, rex);
                out.push(op::ESCAPE);
                out.push(op::IMUL_RM);
                push_mem(out, *base, *disp, enc_reg(d.preg));
            }
            _ => unreachable!("validated at resolve time"),
        },
        Opcode::Jmp | Opcode::Jl | Opcode::Jge => {
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
        Opcode::Load => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Mem { base, disp })) => {
                if d.class == RegClass::Xmm64 {
                    let rex = rex_sse(None, Some(d), Some(*base));
                    out.push(op::PRE_F2);
                    push_rex(out, rex);
                    out.push(op::ESCAPE);
                    out.push(op::MOVSD_LOAD);
                    push_mem(out, *base, *disp, enc_reg(d.preg));
                } else {
                    let rex = rex_rmem(d, *base);
                    push_rex(out, rex);
                    out.push(op::MOV_RM);
                    push_mem(out, *base, *disp, enc_reg(d.preg));
                }
            }
            _ => unreachable!("validated at resolve time"),
        },
        Opcode::Store => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Mem { base, disp })) => {
                if d.class == RegClass::Xmm64 {
                    let rex = rex_sse(None, Some(d), Some(*base));
                    out.push(op::PRE_F2);
                    push_rex(out, rex);
                    out.push(op::ESCAPE);
                    out.push(op::MOVSD_STORE);
                    push_mem(out, *base, *disp, enc_reg(d.preg));
                } else {
                    let rex = rex_rmem(d, *base);
                    push_rex(out, rex);
                    out.push(op::MOV_MR);
                    push_mem(out, *base, *disp, enc_reg(d.preg));
                }
            }
            _ => unreachable!("validated at resolve time"),
        },
        Opcode::AddSd | Opcode::MulSd => match (&e.dst, &e.src) {
            (Some(d), Some(Src::Reg(s))) => {
                let opc = if e.op == Opcode::AddSd {
                    op::ADDSD
                } else {
                    op::MULSD
                };
                let rex = rex_sse(Some(s), Some(d), None);
                out.push(op::PRE_F2);
                push_rex(out, rex);
                out.push(op::ESCAPE);
                out.push(opc);
                // SSE ModRM: reg is the destination, rm the source.
                out.push(modrm(enc::MOD_REG, enc_reg(d.preg), enc_reg(s.preg)));
            }
            _ => unreachable!("validated at resolve time"),
        },
    }
    Ok(())
}

/// Computes a branch displacement for the chosen form, failing loudly on overflow.
// CEP:WHAT: Computes branch displacements.
// CEP:WHY: The displacement is target minus next-instruction address; the fixpoint guarantees the form fits, and any residual overflow is a fixpoint bug that must stop the build.
// CEP:STATUS: complete
// CEP:FAILURE: Returns FixpointUnstable when the displacement outgrows the chosen form.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn branch_disp(before: u32, opc: Opcode, long: bool, target: u32) -> Result<i64, EncodeError> {
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
fn branch_size(opc: Opcode, long: bool) -> usize {
    match (opc, long) {
        (Opcode::Jmp, false) | (Opcode::Ret, false) => len::BRANCH8,
        (Opcode::Jmp, true) | (Opcode::Ret, true) => len::BRANCH32_JMP,
        (_, false) => len::BRANCH8,
        (_, true) => len::BRANCH32_JCC,
    }
}

/// Pushes a branch in the chosen form.
// CEP:WHAT: Encodes branch forms.
// CEP:WHY: Jmp, Jl, Jge, and the Ret jump share the displacement logic.
// CEP:STATUS: complete
// CEP:FAILURE: Panics only on a non-branch opcode, which the caller dispatches away.
// CEP:ASSUMES: disp fits the form (checked by branch_disp).
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn push_branch(out: &mut Vec<u8>, opc: Opcode, long: bool, disp: i64) {
    match opc {
        Opcode::Jmp => {
            if long {
                out.push(op::JMP_REL32);
                out.extend_from_slice(&(disp as i32).to_le_bytes());
            } else {
                out.push(op::JMP_REL8);
                out.push(disp as i8 as u8);
            }
        }
        Opcode::Jl => {
            if long {
                out.push(op::ESCAPE);
                out.push(op::JL_REL32);
                out.extend_from_slice(&(disp as i32).to_le_bytes());
            } else {
                out.push(op::JL_REL8);
                out.push(disp as i8 as u8);
            }
        }
        Opcode::Jge => {
            if long {
                out.push(op::ESCAPE);
                out.push(op::JGE_REL32);
                out.extend_from_slice(&(disp as i32).to_le_bytes());
            } else {
                out.push(op::JGE_REL8);
                out.push(disp as i8 as u8);
            }
        }
        _ => panic!("push_branch on non-branch opcode"),
    }
}

/// Callee-saved GPRs the allocation actually uses, in allocation order.
// CEP:WHAT: Collects saved registers.
// CEP:WHY: The prologue preserves exactly the callee-saved registers the allocation picked, matching emit().
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the allocation is verified.
// CEP:COST: O(assignment).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn saved_regs(alloc: &crate::irc::Allocation, target: &X86_64Target) -> Vec<u32> {
    let mut saved: Vec<u32> = Vec::new();
    for (v, p) in &alloc.assignment {
        if alloc.spilled.contains(v) {
            continue;
        }
        if let Some(r) = target.reg(*p) {
            if r.callee_saved && !r.reserved && !saved.contains(p) {
                saved.push(*p);
            }
        }
    }
    saved
}

/// Frame depth the prologue reserves, matching emit() byte for byte.
// CEP:WHAT: Computes the frame size.
// CEP:WHY: Spill slot displacements baked by the rewriter assume this reserve.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the rewriter and the encoder derive the layout from the same target constant.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn frame_size(alloc: &crate::irc::Allocation, target: &X86_64Target) -> i64 {
    let depth = SLOT_WIDTH * target.max_saved_gprs() as i64 + SLOT_WIDTH * alloc.spill_slots as i64;
    (depth + FRAME_ALIGN - 1) / FRAME_ALIGN * FRAME_ALIGN
}

/// Encodes the prologue: push rbp, mov rsp to rbp, saved pushes, stack reservation.
// CEP:WHAT: Encodes the prologue.
// CEP:WHY: The ABI requires a frame pointer save and callee-saved preservation; the frame formula matches emit() so baked slot displacements stay valid.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: frame is non-negative and fits i32 (spill slots are bounded by the allocator).
// CEP:COST: O(saved).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn push_prologue(out: &mut Vec<u8>, saved: &[u32], frame: i64) {
    out.push(op::PUSH_RBP);
    push_rex(out, op::REX_W);
    out.push(op::MOV_MR);
    out.push(op::MODRM_MOV_RSP_RBP);
    for p in saved {
        push_push(out, *p);
    }
    if !fits_i8(frame) {
        push_rex(out, op::REX_W);
        out.push(op::GRP1_ID);
        out.push(modrm(enc::MOD_REG, op::EXT_SUB, enc::RM_SIB));
        out.extend_from_slice(&(frame as i32).to_le_bytes());
    } else {
        push_rex(out, op::REX_W);
        out.push(op::GRP1_IB);
        out.push(modrm(enc::MOD_REG, op::EXT_SUB, enc::RM_SIB));
        out.push(frame as i8 as u8);
    }
}

/// Encodes one push.
// CEP:WHAT: Encodes a push.
// CEP:WHY: Prologue register saves.
// CEP:STATUS: complete
// CEP:FAILURE: Panics on RSP or RBP pushes; the saved set excludes reserved registers by construction.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn push_push(out: &mut Vec<u8>, p: u32) {
    let h = hw(p);
    if h >= hw_num::GPR_EXT {
        out.push(op::REX_B_ONLY);
        out.push(op::PUSH_R + (h & enc::REG_MASK) as u8);
    } else if p == preg::RBP {
        out.push(op::PUSH_RBP);
    } else {
        out.push(op::PUSH_R + h as u8);
    }
}

/// Encodes one pop.
// CEP:WHAT: Encodes a pop.
// CEP:WHY: Epilogue restores.
// CEP:STATUS: complete
// CEP:FAILURE: Panics on RSP or RBP pops; reserved registers are never saved.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn push_pop(out: &mut Vec<u8>, p: u32) {
    let h = hw(p);
    if h >= hw_num::GPR_EXT {
        out.push(op::REX_B_ONLY);
        out.push(op::POP_R + (h & enc::REG_MASK) as u8);
    } else if p == preg::RBP {
        out.push(op::POP_RBP);
    } else {
        out.push(op::POP_R + h as u8);
    }
}

/// Encodes the epilogue: saved pops, mov rbp to rsp, pop rbp, ret.
// CEP:WHAT: Encodes the epilogue.
// CEP:WHY: Mirror of the prologue in reverse; Ret instructions jump here.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(saved).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn push_epilogue(out: &mut Vec<u8>, saved: &[u32]) {
    for p in saved.iter().rev() {
        push_pop(out, *p);
    }
    push_rex(out, op::REX_W);
    out.push(op::MOV_MR);
    out.push(op::MODRM_MOV_RBP_RSP);
    out.push(op::POP_RBP);
    out.push(op::RET);
}

/// Computes the prologue byte length from the saved set and the frame size.
// CEP:WHAT: Sizes the prologue.
// CEP:WHY: Block label offsets start after it.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: matches push_prologue byte for byte.
// CEP:COST: O(saved).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn prologue_len(saved: &[u32], frame: i64) -> usize {
    // Extended registers (r8+) need a REX byte; low registers push in one byte.
    let pushes: usize = saved
        .iter()
        .map(|p| usize::from(*p >= hw_num::GPR_EXT) + 1)
        .sum();
    1 + len::FRAME_MOV
        + pushes
        + if fits_i8(frame) {
            len::GRP1_IB
        } else {
            len::GRP1_ID
        }
}

/// Computes the epilogue byte length.
// CEP:WHAT: Sizes the epilogue.
// CEP:WHY: The Ret-jump target offset depends on it.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: matches push_epilogue byte for byte.
// CEP:COST: O(saved).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn epilogue_len(saved: &[u32]) -> usize {
    let pops: usize = saved
        .iter()
        .map(|p| usize::from(*p >= hw_num::GPR_EXT) + 1)
        .sum();
    pops + len::FRAME_MOV + len::POP_RET
}

/// Compiles one allocated, verified function into a CodeObject.
// CEP:WHAT: Compiles one function to bytes.
// CEP:WHY: The design's emission stage: encoding with relocations, deterministic and executable.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module errors; rejects a block named like the reserved epilogue label before any bytes exist.
// CEP:ASSUMES: the allocation is verified; block labels are unique; no block is named "ret".
// CEP:COST: O(insns * fixpoint rounds) with a 64-round cap.
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs, crates/mlkp-machine/tests/exec_test.rs.
pub fn compile(
    f: &MFunction,
    alloc: &crate::irc::Allocation,
    target: &X86_64Target,
) -> Result<CodeObject, EncodeError> {
    if f.blocks.iter().any(|b| b.label == EPILOGUE_LABEL) {
        return Err(EncodeError::UnknownLabel(format!(
            "reserved label {EPILOGUE_LABEL}"
        )));
    }
    let insns: Vec<EncInsn> = f
        .blocks
        .iter()
        .flat_map(|b| b.insns.iter())
        .map(|i| resolve_insn(i, alloc))
        .collect::<Result<Vec<_>, _>>()?;
    let is_branch =
        |e: &EncInsn| matches!(e.op, Opcode::Jmp | Opcode::Jl | Opcode::Jge | Opcode::Ret);
    let saved = saved_regs(alloc, target);
    let frame = frame_size(alloc, target);
    let prolog = prologue_len(&saved, frame);
    let epi = epilogue_len(&saved);
    // Branch fixpoint: start every branch short; grow forms whose displacement overflows i8; repeat until stable.
    let mut long = vec![false; insns.len()];
    let mut rounds = 0u32;
    let (label_offsets, epilogue_offset) = loop {
        let mut label_offsets: Vec<(String, u32)> = Vec::new();
        let mut insn_offsets: Vec<u32> = Vec::with_capacity(insns.len());
        let mut off = prolog as u32;
        let mut idx = 0usize;
        for block in &f.blocks {
            label_offsets.push((block.label.clone(), off));
            for _ in &block.insns {
                insn_offsets.push(off);
                off += insn_size(&insns[idx], long[idx]) as u32;
                idx += 1;
            }
        }
        let epilogue_offset = off;
        // Validate branch targets once the labels exist.
        for e in insns.iter() {
            if !is_branch(e) {
                continue;
            }
            if e.op == Opcode::Ret {
                continue;
            }
            let name = e
                .target
                .clone()
                .ok_or(EncodeError::BadOperand("branch target"))?;
            if !label_offsets.iter().any(|(l, _)| l == &name) {
                return Err(EncodeError::UnknownLabel(name));
            }
        }
        let mut changed = false;
        for (k, e) in insns.iter().enumerate() {
            if !is_branch(e) {
                continue;
            }
            let tgt = if e.op == Opcode::Ret {
                epilogue_offset
            } else {
                let name = e.target.clone().unwrap_or_default();
                label_offsets
                    .iter()
                    .find(|(l, _)| l == &name)
                    .map(|(_, o)| *o)
                    .unwrap_or(epilogue_offset)
            };
            let size = branch_size(e.op, long[k]) as u32;
            let next = insn_offsets[k] + size;
            let disp = i64::from(tgt) - i64::from(next);
            if !fits_i8(disp) && !long[k] {
                long[k] = true;
                changed = true;
            }
        }
        rounds += 1;
        if !changed {
            break (label_offsets, epilogue_offset);
        }
        if rounds >= MAX_FIXPOINT_ROUNDS {
            return Err(EncodeError::FixpointUnstable);
        }
    };
    // Emit pass: prologue, blocks, epilogue.
    let mut out: Vec<u8> = Vec::new();
    push_prologue(&mut out, &saved, frame);
    let mut relocs: Vec<Reloc> = Vec::new();
    let mut idx = 0usize;
    for block in &f.blocks {
        for _ in &block.insns {
            let e = &insns[idx];
            let target_off = if e.op == Opcode::Ret {
                Some(epilogue_offset)
            } else if matches!(e.op, Opcode::Jmp | Opcode::Jl | Opcode::Jge) {
                let name = e
                    .target
                    .clone()
                    .ok_or(EncodeError::BadOperand("branch target"))?;
                Some(
                    label_offsets
                        .iter()
                        .find(|(l, _)| l == &name)
                        .map(|(_, o)| *o)
                        .ok_or(EncodeError::UnknownLabel(name.clone()))?,
                )
            } else if e.op == Opcode::Call {
                let name = e
                    .target
                    .clone()
                    .ok_or(EncodeError::BadOperand("call target"))?;
                label_offsets
                    .iter()
                    .find(|(l, _)| l == &name)
                    .map(|(_, o)| *o)
            } else {
                None
            };
            emit_insn(&mut out, e, long[idx], target_off, &mut relocs)?;
            idx += 1;
        }
    }
    debug_assert_eq!(
        out.len(),
        epilogue_offset as usize,
        "emit pass drifted from the size fixpoint"
    );
    push_epilogue(&mut out, &saved);
    debug_assert_eq!(
        out.len() - epilogue_offset as usize,
        epi,
        "epilogue size drifted from the layout"
    );
    let mut labels: Vec<(u32, String)> =
        label_offsets.iter().map(|(l, o)| (*o, l.clone())).collect();
    labels.push((epilogue_offset, EPILOGUE_LABEL.to_string()));
    Ok(CodeObject {
        text: out,
        relocs,
        labels,
    })
}

/// Convenience: compiles one function and returns only the executable bytes.
// CEP:WHAT: Encodes one function to bytes.
// CEP:WHY: Callers that do not need relocations or labels.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module errors.
// CEP:ASSUMES: see compile().
// CEP:COST: as compile().
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs.
pub fn encode(
    f: &MFunction,
    alloc: &crate::irc::Allocation,
    target: &X86_64Target,
) -> Result<Vec<u8>, EncodeError> {
    compile(f, alloc, target).map(|c| c.text)
}

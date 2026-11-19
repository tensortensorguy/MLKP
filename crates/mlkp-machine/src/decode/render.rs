// CEP:FILE: crates/mlkp-machine/src/decode/render.rs
// CEP:WHAT: Renders one decoded instruction exactly as the assembly emitter does, including width-specific register naming.
// CEP:WHY: The decoder is the mechanical bytes-versus-text equivalence check of ticket CEP-322; one shared structural grammar keeps decoding identical across the CodeObject path and the general path.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module DecodeError variants; encodings outside the ISA tier are rejected loudly, never guessed at.
// CEP:ASSUMES: the bytes came from encode::compile for the CodeObject path; prefixes arrive in canonical order.
// CEP:COST: cold; O(insns).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs.

use super::tables::d;
use super::{DecDst, DecInsn, DecKind, DecSrc};
use crate::mir::Opcode;

/// Renders one decoded instruction exactly as the assembly emitter does.
// CEP:ID: JIT-f24
// CEP:WHAT: Renders one decoded instruction.
// CEP:WHY: The bytes-versus-text equivalence check compares this output against emit().
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the target table maps ids to the same names emit() uses.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn render(insn: &DecInsn, target: &crate::target::X86_64Target) -> String {
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

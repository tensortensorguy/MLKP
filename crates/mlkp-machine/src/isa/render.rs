// CEP:FILE: crates/mlkp-machine/src/isa/render.rs
// CEP:WHY: Ticket CEP-325 pairs the general decoder with a deterministic AT&T text view whose text the assembler re-encodes to the same bytes for canonical programs.
// CEP:WHAT: Renders one decoded tier instruction as deterministic AT&T text: width suffixes, four-width register names, canonical memory operands, absolute branch offsets, and the cqto/cdq and movabsq renames.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: hardware register numbering; branch targets render as absolute byte offsets of the containing buffer, which the assembler accepts for flat buffers.
// CEP:COST: cold; O(1) per instruction.
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs.

#![deny(missing_docs)]

use super::tables::{reg_name, Width};
use super::{IsaInsn, IsaMn, IsaOp};

/// True when the value fits a signed 32-bit integer.
// CEP:WHAT: Tests i32 range.
// CEP:WHY: movabsq is rendered only when the immediate exceeds the mov imm32 form.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
fn fits_i32(v: i64) -> bool {
    v >= i64::from(i32::MIN) && v <= i64::from(i32::MAX)
}

/// Renders a memory operand as AT&T text.
// CEP:WHAT: Renders a memory operand.
// CEP:WHY: Canonical AT&T order is disp(base,index,scale); RIP-relative and absolute forms have dedicated renderings.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
fn mem_text(op: &IsaOp) -> String {
    let IsaOp::Mem {
        base,
        index,
        scale,
        disp,
        rip,
    } = op
    else {
        return String::new();
    };
    if *rip {
        if *disp == 0 {
            return "(%rip)".to_string();
        }
        return format!("{disp}(%rip)");
    }
    let inner = match (base, index) {
        (Some(b), Some(i)) => format!(
            "%{},%{},{}",
            reg_name(*b, Width::Q),
            reg_name(*i, Width::Q),
            scale
        ),
        (Some(b), None) => format!("%{}", reg_name(*b, Width::Q)),
        (None, Some(i)) => format!(",%{},{}", reg_name(*i, Width::Q), scale),
        (None, None) => String::new(),
    };
    if inner.is_empty() {
        format!("{disp}")
    } else if *disp == 0 {
        format!("({inner})")
    } else {
        format!("{disp}({inner})")
    }
}

/// Renders a register operand at an explicit width (the move grids need it).
// CEP:WHAT: Renders a width-pinned register.
// CEP:WHY: movzbq reads a byte register even though the instruction width is quadword.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
fn src_text(op: &IsaOp, src_w: Width) -> String {
    match op {
        IsaOp::Gpr(hw) => format!("%{}", reg_name(*hw, src_w)),
        other => op_text(other, src_w),
    }
}

/// Renders one operand.
// CEP:WHAT: Renders one operand.
// CEP:WHY: Registers render at the instruction width; immediates carry the $ sigil.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
fn op_text(op: &IsaOp, w: Width) -> String {
    match op {
        IsaOp::Gpr(hw) => format!("%{}", reg_name(*hw, w)),
        IsaOp::Xmm(idx) => format!("%xmm{idx}"),
        IsaOp::Imm(v) => format!("${v}"),
        IsaOp::Mem { .. } => mem_text(op),
        IsaOp::Sym(s) => s.clone(),
    }
}

/// Renders one decoded instruction as AT&T text.
// CEP:ID: JIT-f78
// CEP:WHAT: Renders one instruction.
// CEP:WHY: The disassembly text view; the assembler re-encodes canonical programs to identical bytes.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub fn render_insn(insn: &IsaInsn) -> String {
    let w = insn.width;
    let s = |op: &IsaOp| op_text(op, w);
    let ops = &insn.ops;
    let one = |m: &str| -> String { format!("  {m}\n") };
    let two = |m: &str, a: &str, b: &str| -> String { format!("  {m} {a}, {b}\n") };
    match insn.mn {
        IsaMn::Mov => {
            if w == Width::Q {
                if let Some(IsaOp::Imm(v)) = ops.first() {
                    if !fits_i32(*v) {
                        return two("movabsq", &s(&ops[0]), &s(&ops[1]));
                    }
                }
            }
            two(&format!("mov{}", w.suffix()), &s(&ops[0]), &s(&ops[1]))
        }
        IsaMn::MovZx(src_w) => {
            let m = format!("movz{}{}", src_w.suffix(), w.suffix());
            two(&m, &src_text(&ops[0], src_w), &s(&ops[1]))
        }
        IsaMn::MovSx(src_w) => {
            let m = format!("movs{}{}", src_w.suffix(), w.suffix());
            two(&m, &src_text(&ops[0], src_w), &s(&ops[1]))
        }
        IsaMn::MovSxd => two("movslq", &src_text(&ops[0], Width::L), &s(&ops[1])),
        IsaMn::Lea => two(&format!("lea{}", w.suffix()), &s(&ops[0]), &s(&ops[1])),
        IsaMn::Alu(alu) => {
            let name = match alu {
                super::tables::Alu::Add => "add",
                super::tables::Alu::Or => "or",
                super::tables::Alu::And => "and",
                super::tables::Alu::Sub => "sub",
                super::tables::Alu::Xor => "xor",
                super::tables::Alu::Cmp => "cmp",
            };
            two(&format!("{name}{}", w.suffix()), &s(&ops[0]), &s(&ops[1]))
        }
        IsaMn::Test => two(&format!("test{}", w.suffix()), &s(&ops[0]), &s(&ops[1])),
        IsaMn::Not => format!("  not{} {}\n", w.suffix(), s(&ops[0])),
        IsaMn::Neg => format!("  neg{} {}\n", w.suffix(), s(&ops[0])),
        IsaMn::Mul => format!("  mul{} {}\n", w.suffix(), s(&ops[0])),
        IsaMn::Imul => format!("  imul{} {}\n", w.suffix(), s(&ops[0])),
        IsaMn::Div => format!("  div{} {}\n", w.suffix(), s(&ops[0])),
        IsaMn::Idiv => format!("  idiv{} {}\n", w.suffix(), s(&ops[0])),
        IsaMn::Cqo => one(if w == Width::Q { "cqto" } else { "cdq" }),
        IsaMn::Shl => shift("shl", w, ops),
        IsaMn::Shr => shift("shr", w, ops),
        IsaMn::Sar => shift("sar", w, ops),
        IsaMn::Imul2 => two(&format!("imul{}", w.suffix()), &s(&ops[0]), &s(&ops[1])),
        IsaMn::Imul3 => three(w, ops),
        IsaMn::Push => match ops.first() {
            Some(IsaOp::Imm(_)) => format!("  pushq {}\n", s(&ops[0])),
            _ => format!("  pushq {}\n", s(&ops[0])),
        },
        IsaMn::Pop => format!("  popq {}\n", s(&ops[0])),
        IsaMn::Inc => format!("  inc{} {}\n", w.suffix(), s(&ops[0])),
        IsaMn::Dec => format!("  dec{} {}\n", w.suffix(), s(&ops[0])),
        IsaMn::Call => match insn.target {
            Some(t) => format!("  call {t}\n"),
            None => format!("  call {}\n", s(&ops[0])),
        },
        IsaMn::Jmp => match insn.target {
            Some(t) => format!("  jmp {t}\n"),
            None => format!("  jmp {}\n", s(&ops[0])),
        },
        IsaMn::Jcc(c) => {
            let cond = super::tables::COND_NAMES[c as usize];
            match insn.target {
                Some(t) => format!("  j{cond} {t}\n"),
                None => format!("  j{cond} {}\n", s(&ops[0])),
            }
        }
        IsaMn::Ret => one("ret"),
        IsaMn::Nop => one("nop"),
        IsaMn::MovSd => two("movsd", &s(&ops[0]), &s(&ops[1])),
        IsaMn::MovApd => two("movapd", &s(&ops[0]), &s(&ops[1])),
        IsaMn::AddSd => two("addsd", &s(&ops[0]), &s(&ops[1])),
        IsaMn::SubSd => two("subsd", &s(&ops[0]), &s(&ops[1])),
        IsaMn::MulSd => two("mulsd", &s(&ops[0]), &s(&ops[1])),
        IsaMn::DivSd => two("divsd", &s(&ops[0]), &s(&ops[1])),
        IsaMn::SqrtSd => two("sqrtsd", &s(&ops[0]), &s(&ops[1])),
        IsaMn::Ucomisd => two("ucomisd", &s(&ops[0]), &s(&ops[1])),
        IsaMn::Comisd => two("comisd", &s(&ops[0]), &s(&ops[1])),
        IsaMn::CvtSi2Sd => two(&format!("cvtsi2sd{}", w.suffix()), &s(&ops[0]), &s(&ops[1])),
        IsaMn::Cvttsd2Si => two(
            &format!("cvttsd2si{}", w.suffix()),
            &s(&ops[0]),
            &s(&ops[1]),
        ),
    }
}

/// Renders the three-operand multiply: imul imm, src, dst.
// CEP:WHAT: Renders an imul3.
// CEP:WHY: AT&T order places the immediate first and the destination last.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
fn three(w: Width, ops: &[IsaOp]) -> String {
    let imm = match &ops[0] {
        IsaOp::Imm(v) => format!("${v}"),
        _ => String::new(),
    };
    let src = match &ops[1] {
        IsaOp::Gpr(hw) => format!("%{}", reg_name(*hw, w)),
        IsaOp::Mem { .. } => mem_text(&ops[1]),
        _ => String::new(),
    };
    let dst = match &ops[2] {
        IsaOp::Gpr(hw) => format!("%{}", reg_name(*hw, w)),
        _ => String::new(),
    };
    format!("  imul{} {imm}, {src}, {dst}\n", w.suffix())
}

/// Renders a shift: the CL count always prints as %cl, immediates carry $.
// CEP:WHAT: Renders a shift.
// CEP:WHY: The CL count form names the byte register at every width.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
fn shift(name: &str, w: Width, ops: &[IsaOp]) -> String {
    let src = match &ops[0] {
        IsaOp::Gpr(1) => "%cl".to_string(),
        other => op_text(other, w),
    };
    format!(
        "  {}{} {}, {}\n",
        name,
        w.suffix(),
        src,
        op_text(&ops[1], w)
    )
}

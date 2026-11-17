// CEP:FILE: crates/mlkp-machine/src/emit.rs
// CEP:WHAT: Assembly emission: renders an allocated machine function as deterministic AT&T-style x86-64 text with a prologue and epilogue that save used callee-saved registers and address spill slots off the frame pointer at the fixed displacements the spill rewriter baked into its loads and stores.
// CEP:WHY: The design's implementation order ends at machine code emission; textual assembly is the reviewable, testable artifact for v1, and byte-level encoding is deferred with a ticket (doc/machine_ir.md Known Limitations).
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns an EmitError when an operand still holds an unresolved virtual register or when a spilled value is still referenced, which only a hand-built or corrupt allocation can produce because allocate() rewrites spills away.
// CEP:ASSUMES: the allocation is verified before emission; the frame reserves the worst-case callee-saved area below rbp, matching the rewriter's slot displacements.
// CEP:COST: cold; O(insns).
// CEP:EVIDENCE: unit tests in crates/mlkp-machine/tests/irc_test.rs.

#![deny(missing_docs)]

use crate::mir::{MFunction, Opcode, Operand};
use crate::target::X86_64Target;

/// Emission failure.
#[derive(Clone, Debug, PartialEq)]
pub enum EmitError {
    /// An operand still references a virtual register after allocation.
    UnresolvedVReg(u32),
    /// A spilled value was referenced without a stack slot.
    ///
    /// allocate() rewrites spills away (CEP-323), so only a hand-built or
    /// corrupt allocation reaches this path.
    UnresolvedSpill(u32),
}

impl std::fmt::Display for EmitError {
    // CEP:WHAT: Formats the emission error.
    // CEP:WHY: Deterministic diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmitError::UnresolvedVReg(id) => write!(f, "emit error: vreg {id} unresolved"),
            EmitError::UnresolvedSpill(id) => write!(f, "emit error: spill {id} unresolved"),
        }
    }
}

/// Renders one function to assembly text.
// CEP:WHAT: Emits one function.
// CEP:WHY: Deterministic text is the v1 emission artifact.
// CEP:STATUS: complete
// CEP:FAILURE: Returns EmitError per the module contract.
// CEP:ASSUMES: none
// CEP:COST: O(insns).
// CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
pub fn emit(
    f: &MFunction,
    alloc: &crate::irc::Allocation,
    target: &X86_64Target,
) -> Result<String, EmitError> {
    let mut out = String::new();
    out.push_str("// GENERATED ASSEMBLY\n");
    // Generator identity per chapter 32.7.
    out.push_str("// Generator: mlkp-machine emit\n");
    let gprs_used: Vec<u32> = alloc
        .assignment
        .iter()
        .filter(|(v, _)| !alloc.spilled.contains(v))
        .map(|(_, p)| *p)
        .filter(|p| {
            target
                .reg(*p)
                .is_some_and(|r| r.callee_saved && !r.reserved)
        })
        .collect();
    let mut saved: Vec<u32> = Vec::new();
    for p in gprs_used.iter().copied() {
        if !saved.contains(&p) {
            saved.push(p);
        }
    }
    let spill_area = 8 * alloc.spill_slots as i64;
    // CEP:WHAT: Sizes the frame from the worst-case saved area plus the spill slots, aligned to the SysV 16-byte stack boundary.
    // CEP:WHY: The spill rewriter baked displacements of -(8*max_saved + 8*(i+1)) into its loads and stores, so the frame must reserve that whole depth regardless of how many callee-saved registers this allocation actually pushes; an odd total would leave rsp misaligned at call sites and fault vector instructions in callees.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: the rewriter and the emitter derive slot addresses from the same target constant.
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    let frame_depth = 8 * target.max_saved_gprs() as i64 + spill_area;
    let frame = (frame_depth + 15) / 16 * 16;
    out.push_str(&format!(".text\n.globl {}\n{}:\n", f.name, f.name));
    out.push_str("  pushq %rbp\n  movq %rsp, %rbp\n");
    for p in &saved {
        out.push_str(&format!("  pushq %{}\n", target.reg_name(*p)));
    }
    out.push_str(&format!("  subq ${frame}, %rsp\n"));
    // Spill slot i lives at -(8*max_saved_gprs + 8*(i+1)) relative to rbp; the
    // rewriter baked exactly that displacement into its loads and stores.
    for block in f.blocks.iter() {
        // CEP:WHAT: Emits every block label, including the entry block.
        // CEP:WHY: A branch to the entry block would otherwise reference an undefined label at assembly time.
        // CEP:STATUS: complete
        // CEP:FAILURE: none
        // CEP:ASSUMES: none
        // CEP:COST: O(blocks).
        // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
        out.push_str(&format!(".L{}:\n", block.label));
        for insn in &block.insns {
            let line = render_insn(insn, alloc, target)?;
            out.push_str(&line);
        }
    }
    out.push_str(".Lret:\n");
    for p in saved.iter().rev() {
        out.push_str(&format!("  popq %{}\n", target.reg_name(*p)));
    }
    out.push_str("  movq %rbp, %rsp\n  popq %rbp\n  ret\n\n");
    Ok(out)
}

/// True when the instruction moves an xmm register (operand 0 carries the class).
// CEP:WHAT: Reports xmm-class operands.
// CEP:WHY: The text emitter prints movapd and movsd for xmm moves so the rendered assembly is directly assemblable; the encoder and the decoder use the same class distinction.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
fn dst_is_xmm(insn: &crate::mir::MInsn) -> bool {
    insn.ops.first().is_some_and(|o| {
        matches!(
            o,
            Operand::VReg {
                class: crate::mir::RegClass::Xmm64,
                ..
            } | Operand::PReg {
                class: crate::mir::RegClass::Xmm64,
                ..
            }
        )
    })
}

// CEP:WHAT: Renders one instruction line.
// CEP:WHY: Shared by the function emitter.
// CEP:STATUS: complete
// CEP:FAILURE: Returns EmitError for unresolved or spilled registers.
// CEP:ASSUMES: none
// CEP:COST: O(operands).
// CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
fn render_insn(
    insn: &crate::mir::MInsn,
    alloc: &crate::irc::Allocation,
    target: &X86_64Target,
) -> Result<String, EmitError> {
    let preg_of = |v: u32| -> Result<u32, EmitError> {
        match alloc.assignment.iter().find(|(id, _)| id == &v) {
            Some((_, p)) if *p != u32::MAX => Ok(*p),
            // CEP:WHAT: Reports spilled values as unresolved spills.
            // CEP:WHY: A spilled vreg has no register; rendering %unknown would produce silently corrupt assembly, so the declared failure path must fire.
            // CEP:STATUS: complete
            // CEP:FAILURE: Returns EmitError::UnresolvedSpill for u32::MAX assignments.
            // CEP:ASSUMES: none
            // CEP:COST: O(assignment).
            // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
            Some(_) => Err(EmitError::UnresolvedSpill(v)),
            None => Err(EmitError::UnresolvedVReg(v)),
        }
    };
    let opnd = |o: &Operand| -> Result<String, EmitError> {
        match o {
            Operand::VReg { id, .. } => {
                let p = preg_of(*id)?;
                if p == u32::MAX {
                    // The sentinel marks a spilled value; after the CEP-323
                    // rewrite no instruction references one, so a remaining
                    // reference is a broken allocation and fails loudly.
                    return Err(EmitError::UnresolvedSpill(*id));
                }
                Ok(format!("%{}", target.reg_name(p)))
            }
            Operand::PReg { id, .. } => Ok(format!("%{}", target.reg_name(*id))),
            Operand::Imm(v) => Ok(format!("${v}")),
            Operand::Mem {
                base,
                base_preg,
                disp,
                ..
            } => {
                let name = if *base_preg {
                    target.reg_name(*base).to_string()
                } else {
                    let p = preg_of(*base)?;
                    if p == u32::MAX {
                        return Err(EmitError::UnresolvedSpill(*base));
                    }
                    target.reg_name(p).to_string()
                };
                Ok(format!("{disp}(%{name})"))
            }
            Operand::Block(label) => Ok(format!(".L{label}")),
            Operand::Symbol(name) => Ok(name.clone()),
        }
    };
    let mnemonic = match insn.op {
        Opcode::Mov => {
            if dst_is_xmm(insn) {
                "movapd"
            } else {
                "movq"
            }
        }
        Opcode::Add => "addq",
        Opcode::Sub => "subq",
        Opcode::IMul => "imulq",
        Opcode::Cmp => "cmpq",
        Opcode::Jl => "jl",
        Opcode::Jge => "jge",
        Opcode::Jmp => "jmp",
        Opcode::Call => "call",
        Opcode::Ret => "ret",
        Opcode::Load => {
            if dst_is_xmm(insn) {
                "movsd"
            } else {
                "movq"
            }
        }
        Opcode::Store => {
            if dst_is_xmm(insn) {
                "movsd"
            } else {
                "movq"
            }
        }
        Opcode::AddSd => "addsd",
        Opcode::MulSd => "mulsd",
    };
    let src = if insn.ops.len() >= 2 {
        opnd(&insn.ops[1])?
    } else {
        String::new()
    };
    let dst = insn.ops.first().map(opnd).transpose()?.unwrap_or_default();
    // AT&T order: op src, dst; memory ops swap direction.
    let line = match insn.op {
        Opcode::Load => {
            let addr = opnd(&insn.ops[1])?;
            format!("  {mnemonic} {}, {}\n", addr, dst)
        }
        Opcode::Store => {
            let addr = opnd(&insn.ops[1])?;
            let val = dst;
            format!("  {mnemonic} {}, {}\n", val, addr)
        }
        Opcode::Ret => "  jmp .Lret\n".to_string(),
        Opcode::Jl | Opcode::Jge | Opcode::Jmp => format!("  {mnemonic} {dst}\n"),
        Opcode::Call => format!("  call {dst}\n"),
        _ => {
            if src.is_empty() {
                format!("  {mnemonic} {dst}\n")
            } else {
                format!("  {mnemonic} {src}, {dst}\n")
            }
        }
    };
    Ok(line)
}

/// The allocation verifier: proves every allocation invariant of the design.
// CEP:WHAT: Verifies one allocation.
// CEP:WHY: The design requires each allocation to prove non-overlap of interfering live values, class restrictions, fixed constraints, and complete resolution.
// CEP:STATUS: complete
// CEP:FAILURE: Returns every violation found.
// CEP:ASSUMES: none
// CEP:COST: O(insns * live).
// CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
pub fn verify_allocation(
    f: &MFunction,
    alloc: &crate::irc::Allocation,
    target: &X86_64Target,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    // All vregs resolved.
    for (v, _) in &f.vreg_classes {
        if !alloc.assignment.iter().any(|(id, _)| id == v) {
            errors.push(format!("vreg {v} has no assignment"));
        }
    }
    // Fixed constraints satisfied.
    for block in &f.blocks {
        for insn in &block.insns {
            for (idx, preg_id) in &insn.fixed {
                if let Some(Operand::VReg { id, .. }) = insn.ops.get(*idx) {
                    let assigned = alloc
                        .assignment
                        .iter()
                        .find(|(v, _)| v == id)
                        .map(|(_, p)| *p);
                    if assigned != Some(*preg_id) {
                        errors.push(format!(
                            "vreg {id} must occupy fixed register {preg_id}, got {assigned:?}"
                        ));
                    }
                }
            }
            // Spilled vregs may not be assigned a register.
            for v in &alloc.spilled {
                if let Some((_, p)) = alloc.assignment.iter().find(|(id, _)| id == v) {
                    if *p != u32::MAX {
                        errors.push(format!("spilled vreg {v} also holds register {p}"));
                    }
                }
            }
        }
    }
    // Interfering live values must not share a register.
    let ig = crate::irc::build_interference(f, target);
    for (v, neighbors) in &ig.adj {
        let pv = alloc
            .assignment
            .iter()
            .find(|(id, _)| id == v)
            .map(|(_, p)| *p);
        for n in neighbors {
            let pn = alloc
                .assignment
                .iter()
                .find(|(id, _)| id == n)
                .map(|(_, p)| *p);
            if let (Some(a), Some(b)) = (pv, pn) {
                if a != u32::MAX && a == b {
                    errors.push(format!("vregs {v} and {n} share register {a}"));
                }
            }
        }
    }
    // Class restrictions respected.
    for (v, class) in &f.vreg_classes {
        if let Some((_, p)) = alloc.assignment.iter().find(|(id, _)| id == v) {
            if *p != u32::MAX {
                match target.reg(*p) {
                    Some(r) if r.class == *class => {}
                    _ => errors.push(format!("vreg {v} assigned register outside its class")),
                }
            }
        }
    }
    // CEP:WHAT: Checks preg blocking from implicit defs.
    // CEP:WHY: A value live across a call must not occupy a caller-saved register the call clobbers; the allocator's blocked-preg contract is checked independently here.
    // CEP:STATUS: complete
    // CEP:FAILURE: Pushes an error per violation.
    // CEP:ASSUMES: none
    // CEP:COST: O(blocked).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    let ig2 = crate::irc::build_interference(f, target);
    for (v, pg) in &ig2.blocked_pregs {
        if let Some((_, p)) = alloc.assignment.iter().find(|(id, _)| id == v) {
            if *p == *pg {
                errors.push(format!("vreg {v} occupies blocked register {pg}"));
            }
        }
    }
    errors.sort();
    errors.dedup();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

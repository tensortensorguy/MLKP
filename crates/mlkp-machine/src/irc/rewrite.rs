// CEP:FILE: crates/mlkp-machine/src/irc/rewrite.rs
// CEP:WHAT: Spill rewriting with rematerialization, rbp-relative slot addressing, and split-at-spill temporaries (ticket CEP-323).
// CEP:WHY: Liveness, interference, iterated register coalescing, and spill rewriting share one Machine IR contract; module boundaries keep each concern atomic per chapter 62.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns AllocError when verification fails after rewriting or when spill rewriting does not converge within the round cap; cross-class copy pairs are skipped rather than merged.
// CEP:ASSUMES: functions are in SSA form with virtual registers; fixed constraints name physical registers by operand index; the frame reserves the worst-case callee-saved area below rbp so baked slot displacements stay valid.
// CEP:COST: liveness O(n^2) over instruction counts within a block; IRC is O(n) amortized per node with the standard worklist discipline; each spill round re-runs liveness and coloring over the rewritten function.
// CEP:EVIDENCE: unit tests in crates/mlkp-machine/tests/irc_test.rs.

use super::{AllocError, Allocation, SpillRewrite, EXPANSION_FACTOR, SPILL_SLOT_WIDTH};
use crate::mir::{MBlock, MFunction, MInsn, Opcode, Operand, RegClass};
use crate::target::{preg, X86_64Target};

/// Rewrites every spilled vreg to its stack-slot home with folded or split access.
///
/// Source-side uses fold directly into immediates for rematerialized vregs or
/// into one rbp-relative memory operand for slot-homed vregs; anything that
/// cannot fold (fixed-constrained operands, store values, extra memory
/// references) loads into a fresh temporary that lives only up to the
/// consuming instruction. Destinations reroute through a fresh temporary that
/// is pre-loaded for read-modify-write opcodes and stored to the slot after.
// CEP:ID: JIT-d78
// CEP:WHAT: Rewrites spilled vregs into folded operands, loads, stores, and rematerialized constants.
// CEP:WHY: Spill selection without rewriting leaves values unexecutable; folding keeps the consumer's register pressure unchanged so the fixpoint converges, per-use temporaries split each spilled live range at its uses and defs (ticket CEP-323), and rematerialization drops constant-defined vregs from the frame entirely.
// CEP:STATUS: complete
// CEP:FAILURE: Returns AllocError::Verify when a spilled vreg somehow lacks a slot; the allocator's own invariants make this unreachable.
// CEP:ASSUMES: slots are addressed at fixed rbp-relative offsets below the worst-case saved-register area, which the emitter reserves identically; x86-64 accepts at most one memory operand per instruction, so at most one slot use folds per instruction.
// CEP:COST: O(insns * spilled) per round.
// CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
pub fn rewrite_spills(
    f: &mut MFunction,
    alloc: &Allocation,
    target: &X86_64Target,
    next_vreg: &mut u32,
) -> Result<SpillRewrite, AllocError> {
    if alloc.spilled.is_empty() {
        return Ok(SpillRewrite::default());
    }
    let mut spilled = alloc.spilled.clone();
    spilled.sort();
    // Rematerialization scan: a spilled vreg whose single def is `mov s, imm`
    // never needs a slot; every use rewrites to the constant itself.
    let mut def_sites: Vec<(u32, usize, usize)> = Vec::new();
    for (bi, block) in f.blocks.iter().enumerate() {
        for (ii, insn) in block.insns.iter().enumerate() {
            for d in insn.defined_vregs() {
                if spilled.contains(&d) {
                    def_sites.push((d, bi, ii));
                }
            }
        }
    }
    let mut remat: Vec<(u32, i64)> = Vec::new();
    let mut dead: Vec<(usize, usize)> = Vec::new();
    for &s in &spilled {
        let sites: Vec<(usize, usize)> = def_sites
            .iter()
            .filter(|(v, _, _)| *v == s)
            .map(|(_, bi, ii)| (*bi, *ii))
            .collect();
        if sites.len() == 1 {
            let (bi, ii) = sites[0];
            let insn = &f.blocks[bi].insns[ii];
            if insn.op == Opcode::Mov {
                if let (Some(Operand::VReg { id, def: true, .. }), Some(Operand::Imm(c))) =
                    (insn.ops.first(), insn.ops.get(1))
                {
                    if *id == s {
                        remat.push((s, *c));
                        dead.push((bi, ii));
                    }
                }
            }
        }
    }
    // Slot homes for the non-remat spilled vregs, in ascending vreg order.
    let mut slots: Vec<(u32, u32)> = Vec::new();
    for &s in &spilled {
        if !remat.iter().any(|(id, _)| *id == s) {
            let idx = slots.len() as u32;
            slots.push((s, idx));
        }
    }
    let slot_disp = |idx: u32| -> i64 {
        -(SPILL_SLOT_WIDTH * target.max_saved_gprs() as i64 + SPILL_SLOT_WIDTH * (idx as i64 + 1))
    };
    let mut temps = 0u32;
    // Every fresh temporary with its register class; registered in the class
    // table after the walk so the next round colors and assigns them.
    let mut new_temps: Vec<(u32, RegClass)> = Vec::new();
    let mut new_blocks: Vec<MBlock> = Vec::with_capacity(f.blocks.len());
    for (bi, block) in f.blocks.iter().enumerate() {
        // Worst case doubles the instruction count: one load or remat before,
        // one store after each rewritten instruction.
        let mut out_insns: Vec<MInsn> = Vec::with_capacity(block.insns.len() * EXPANSION_FACTOR);
        for (ii, insn) in block.insns.iter().enumerate() {
            if dead.iter().any(|(db, di)| *db == bi && *di == ii) {
                // The rematerialized def disappears; its uses re-create the
                // constant at each consume point.
                continue;
            }
            let mut pre: Vec<MInsn> = Vec::new();
            let mut post: Vec<MInsn> = Vec::new();
            let mut mem_folded = false;
            // One temp per spilled vreg per instruction for unfolded uses: a
            // value read twice must read the same old value once.
            let mut local: Vec<(u32, u32)> = Vec::new();
            let mut rewritten = insn.clone();
            for j in 0..rewritten.ops.len() {
                match &mut rewritten.ops[j] {
                    Operand::VReg { id, class, def } => {
                        let v = *id;
                        if !spilled.contains(&v) {
                            continue;
                        }
                        let fixed_here = insn.fixed.iter().any(|(idx, _)| *idx == j);
                        let slot = slots.iter().find(|(sv, _)| *sv == v).map(|(_, s)| *s);
                        if *def {
                            // Fresh def temp; read-modify-write opcodes consume
                            // the old value, so the temp is pre-loaded from the
                            // slot, and the store publishes the result after.
                            let t = *next_vreg;
                            *next_vreg += 1;
                            temps += 1;
                            new_temps.push((t, *class));
                            *id = t;
                            let idx = match slot {
                                Some(s) => s,
                                None => {
                                    return Err(AllocError::Verify(format!(
                                        "spilled vreg {v} lacks a slot home"
                                    )))
                                }
                            };
                            if insn.op.reads_dst() {
                                pre.push(MInsn::new(
                                    Opcode::Load,
                                    vec![
                                        Operand::vreg_def(t, *class),
                                        Operand::Mem {
                                            base: preg::RBP,
                                            base_preg: true,
                                            disp: slot_disp(idx),
                                            reads: true,
                                            writes: false,
                                        },
                                    ],
                                ));
                            }
                            post.push(MInsn::new(
                                Opcode::Store,
                                vec![
                                    Operand::vreg_use(t, *class),
                                    Operand::Mem {
                                        base: preg::RBP,
                                        base_preg: true,
                                        disp: slot_disp(idx),
                                        reads: false,
                                        writes: true,
                                    },
                                ],
                            ));
                        } else if fixed_here {
                            // A constrained operand must reach the instruction
                            // in its register, so it goes through a temporary
                            // that inherits the constraint.
                            let t = match local.iter().find(|(sv, _)| *sv == v) {
                                Some((_, t)) => *t,
                                None => {
                                    let t = *next_vreg;
                                    *next_vreg += 1;
                                    temps += 1;
                                    new_temps.push((t, *class));
                                    local.push((v, t));
                                    match remat.iter().find(|(rv, _)| *rv == v) {
                                        Some((_, c)) => pre.push(MInsn::new(
                                            Opcode::Mov,
                                            vec![Operand::vreg_def(t, *class), Operand::Imm(*c)],
                                        )),
                                        None => {
                                            let idx = match slot {
                                                Some(s) => s,
                                                None => {
                                                    return Err(AllocError::Verify(format!(
                                                        "spilled vreg {v} lacks a slot home"
                                                    )))
                                                }
                                            };
                                            pre.push(MInsn::new(
                                                Opcode::Load,
                                                vec![
                                                    Operand::vreg_def(t, *class),
                                                    Operand::Mem {
                                                        base: preg::RBP,
                                                        base_preg: true,
                                                        disp: slot_disp(idx),
                                                        reads: true,
                                                        writes: false,
                                                    },
                                                ],
                                            ));
                                        }
                                    }
                                    t
                                }
                            };
                            *id = t;
                        } else if j >= 1
                            && matches!(
                                insn.op,
                                Opcode::Mov
                                    | Opcode::Add
                                    | Opcode::Sub
                                    | Opcode::IMul
                                    | Opcode::Cmp
                            )
                        {
                            // Source operands fold without pressure: constants
                            // fold to immediates, slot homes fold to the memory
                            // operand the ISA accepts (one memory reference per
                            // instruction at most).
                            match remat.iter().find(|(rv, _)| *rv == v) {
                                Some((_, c)) => {
                                    rewritten.ops[j] = Operand::Imm(*c);
                                }
                                None => {
                                    let idx = match slot {
                                        Some(s) => s,
                                        None => {
                                            return Err(AllocError::Verify(format!(
                                                "spilled vreg {v} lacks a slot home"
                                            )))
                                        }
                                    };
                                    if !mem_folded {
                                        mem_folded = true;
                                        rewritten.ops[j] = Operand::Mem {
                                            base: preg::RBP,
                                            base_preg: true,
                                            disp: slot_disp(idx),
                                            reads: true,
                                            writes: false,
                                        };
                                    } else {
                                        let t = match local.iter().find(|(sv, _)| *sv == v) {
                                            Some((_, t)) => *t,
                                            None => {
                                                let t = *next_vreg;
                                                *next_vreg += 1;
                                                temps += 1;
                                                new_temps.push((t, *class));
                                                local.push((v, t));
                                                pre.push(MInsn::new(
                                                    Opcode::Load,
                                                    vec![
                                                        Operand::vreg_def(t, *class),
                                                        Operand::Mem {
                                                            base: preg::RBP,
                                                            base_preg: true,
                                                            disp: slot_disp(idx),
                                                            reads: true,
                                                            writes: false,
                                                        },
                                                    ],
                                                ));
                                                t
                                            }
                                        };
                                        *id = t;
                                    }
                                }
                            }
                        } else {
                            // Unfolded use: load (or rematerialize) into a
                            // short-lived temporary.
                            let t = match local.iter().find(|(sv, _)| *sv == v) {
                                Some((_, t)) => *t,
                                None => {
                                    let t = *next_vreg;
                                    *next_vreg += 1;
                                    temps += 1;
                                    new_temps.push((t, *class));
                                    local.push((v, t));
                                    match remat.iter().find(|(rv, _)| *rv == v) {
                                        Some((_, c)) => pre.push(MInsn::new(
                                            Opcode::Mov,
                                            vec![Operand::vreg_def(t, *class), Operand::Imm(*c)],
                                        )),
                                        None => {
                                            let idx = match slot {
                                                Some(s) => s,
                                                None => {
                                                    return Err(AllocError::Verify(format!(
                                                        "spilled vreg {v} lacks a slot home"
                                                    )))
                                                }
                                            };
                                            pre.push(MInsn::new(
                                                Opcode::Load,
                                                vec![
                                                    Operand::vreg_def(t, *class),
                                                    Operand::Mem {
                                                        base: preg::RBP,
                                                        base_preg: true,
                                                        disp: slot_disp(idx),
                                                        reads: true,
                                                        writes: false,
                                                    },
                                                ],
                                            ));
                                        }
                                    }
                                    t
                                }
                            };
                            *id = t;
                        }
                    }
                    Operand::Mem {
                        base, base_preg, ..
                    } => {
                        // A memory operand addressed by a spilled vreg needs
                        // the base in a register.
                        if !*base_preg && spilled.contains(base) {
                            let v = *base;
                            let idx = match slots.iter().find(|(sv, _)| *sv == v) {
                                Some((_, s)) => *s,
                                None => {
                                    return Err(AllocError::Verify(format!(
                                        "spilled vreg {v} lacks a slot home"
                                    )))
                                }
                            };
                            let class = f
                                .vreg_classes
                                .iter()
                                .find(|(cid, _)| *cid == v)
                                .map(|(_, c)| *c)
                                .unwrap_or(RegClass::Gpr64);
                            let t = *next_vreg;
                            *next_vreg += 1;
                            temps += 1;
                            new_temps.push((t, RegClass::Gpr64));
                            pre.push(MInsn::new(
                                Opcode::Load,
                                vec![
                                    Operand::vreg_def(t, RegClass::Gpr64),
                                    Operand::Mem {
                                        base: preg::RBP,
                                        base_preg: true,
                                        disp: slot_disp(idx),
                                        reads: true,
                                        writes: false,
                                    },
                                ],
                            ));
                            *base = t;
                            let _ = class;
                        }
                    }
                    _ => {}
                }
            }
            out_insns.extend(pre);
            out_insns.push(rewritten);
            out_insns.extend(post);
        }
        new_blocks.push(MBlock {
            label: block.label.clone(),
            insns: out_insns,
            succs: block.succs.clone(),
        });
    }
    f.blocks = new_blocks;
    // CEP:WHAT: Registers the rewrite's fresh temporaries in the class table.
    // CEP:WHY: The next coloring round builds interference from the class table; unregistered temps would escape assignment and surface later as unresolved registers at emission.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: temp ids are fresh (next_vreg monotonically increases).
    // CEP:COST: O(temps).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
    for (t, class) in &new_temps {
        f.vreg_classes.push((*t, *class));
    }
    // The spilled vregs are memory-resident now; they leave the class table so
    // the next round colors only live values and the verifier sees a complete
    // assignment over the remaining vregs.
    f.vreg_classes.retain(|(id, _)| !spilled.contains(id));
    Ok(SpillRewrite {
        slots,
        remat,
        temps,
    })
}

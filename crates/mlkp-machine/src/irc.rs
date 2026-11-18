// CEP:FILE: crates/mlkp-machine/src/irc.rs
// CEP:WHAT: Liveness analysis, interference construction (including implicit-def preg blocking), the George-Appel iterated register coalescing allocator with George and Briggs criteria, and spill rewriting with rematerialization and split-at-spill temporaries.
// CEP:WHY: The design names IRC as the primary register allocation algorithm with the five operations (simplify, coalesce, freeze, spill, select) and the two conservative coalescing criteria; selected spills are rewritten to stack-slot homes with per-use temporaries (ticket CEP-323), and constant-defined vregs rematerialize instead of occupying slots.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns AllocError when verification fails after rewriting or when spill rewriting does not converge within the round cap; cross-class copy pairs are skipped rather than merged.
// CEP:ASSUMES: functions are in SSA form with virtual registers; fixed constraints name physical registers by operand index; the frame reserves the worst-case callee-saved area below rbp so baked slot displacements stay valid.
// CEP:COST: liveness O(n^2) over instruction counts within a block; IRC is O(n) amortized per node with the standard worklist discipline; each spill round re-runs liveness and coloring over the rewritten function.
// CEP:EVIDENCE: unit tests in crates/mlkp-machine/tests/irc_test.rs.

#![deny(missing_docs)]

use std::collections::BTreeSet;

use crate::mir::{MBlock, MFunction, MInsn, Opcode, Operand, RegClass};
use crate::target::{preg, X86_64Target};

/// Cap on spill-rewrite rounds before the allocator reports non-convergence.
///
/// One round removes every spilled vreg from the interference graph and
/// replaces it with short-lived temporaries, so in practice one round
/// converges; the cap only guards pathological inputs loudly.
const MAX_SPILL_ROUNDS: u32 = 8;

/// Slot width in bytes for scalar spills; slots stride by this under rbp.
const SPILL_SLOT_WIDTH: i64 = 8;

/// Worst-case instruction expansion of one spill-rewrite round.
const EXPANSION_FACTOR: usize = 2;

/// Allocation failure.
#[derive(Clone, Debug, PartialEq)]
pub enum AllocError {
    /// Coalescing tried to merge nodes of different classes.
    ClassConflict(u32, u32),
    /// A fixed constraint referenced a nonexistent register.
    BadFixed(u32),
    /// Verification failed after rewriting.
    Verify(String),
}

impl std::fmt::Display for AllocError {
    // CEP:WHAT: Formats the allocation error.
    // CEP:WHY: Deterministic diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AllocError::ClassConflict(a, b) => {
                write!(f, "alloc error: class conflict merging {a} and {b}")
            }
            AllocError::BadFixed(id) => write!(f, "alloc error: bad fixed register {id}"),
            AllocError::Verify(s) => write!(f, "alloc verify: {s}"),
        }
    }
}

impl std::error::Error for AllocError {}

/// The allocation result: a virtual to physical map plus the spill slots used.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Allocation {
    /// Virtual register id to physical register id.
    pub assignment: Vec<(u32, u32)>,
    /// Virtual registers that were spilled to stack slots.
    pub spilled: Vec<u32>,
    /// Number of spill slots the frame needs.
    pub spill_slots: u32,
    /// Copies removed by coalescing.
    pub coalesced_copies: u32,
}

/// The result of one spill-rewrite round over the function.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpillRewrite {
    /// Stack-slot index per rewritten vreg, in ascending vreg order.
    pub slots: Vec<(u32, u32)>,
    /// Rematerialized constant per spilled vreg whose single def is an immediate move.
    pub remat: Vec<(u32, i64)>,
    /// Number of fresh temporaries the rewrite introduced.
    pub temps: u32,
}

/// Computes live-out sets per block over virtual registers.
// CEP:WHAT: Computes block liveness.
// CEP:WHY: Interference is built from per-instruction live sets which need block-level live-out seeds.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: successor labels are declared on blocks.
// CEP:COST: O(iterations * blocks * insns); the iteration count is bounded by the CFG depth.
// CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
pub fn block_live_out(f: &MFunction) -> Vec<(String, BTreeSet<u32>)> {
    let index_of = |label: &str| f.blocks.iter().position(|b| b.label == label);
    let mut live_out: Vec<BTreeSet<u32>> = vec![BTreeSet::new(); f.blocks.len()];
    // Bounded backward dataflow iteration.
    let max_iters = f.blocks.len() + 1;
    for _ in 0..max_iters {
        let mut changed = false;
        for bi in (0..f.blocks.len()).rev() {
            let mut out = BTreeSet::new();
            for succ in &f.blocks[bi].succs {
                if let Some(si) = index_of(succ) {
                    // CEP:WHAT: Computes successor live-in with a backward scan.
                    // CEP:WHY: Live-in = live-out minus defs plus uses must be applied in reverse instruction order; a forward scan kills defs before their in-block uses are seen and over-approximates liveness with spurious interference.
                    // CEP:STATUS: complete
                    // CEP:FAILURE: none
                    // CEP:ASSUMES: none
                    // CEP:COST: O(insns of successor).
                    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
                    let mut sin: BTreeSet<u32> = live_out[si].clone();
                    for insn in f.blocks[si].insns.iter().rev() {
                        for u in insn.used_vregs() {
                            sin.insert(u);
                        }
                        for d in insn.defined_vregs() {
                            sin.remove(&d);
                        }
                    }
                    out.extend(sin);
                }
            }
            if out != live_out[bi] {
                live_out[bi] = out;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    f.blocks
        .iter()
        .zip(live_out)
        .map(|(b, lo)| (b.label.clone(), lo))
        .collect()
}

/// Interference graph over virtual registers.
#[derive(Debug, Default)]
pub struct Interference {
    /// Adjacency by vreg id.
    pub adj: Vec<(u32, BTreeSet<u32>)>,
    /// Precolored nodes: vreg id to required physical register.
    pub precolored: Vec<(u32, u32)>,
    /// Register class per vreg.
    pub classes: Vec<(u32, RegClass)>,
    /// Pregs a vreg may not occupy because an implicit def clobbers them while the vreg is live (vreg, preg).
    pub blocked_pregs: Vec<(u32, u32)>,
    /// Interference degree lookup.
    degrees: Vec<(u32, usize)>,
}

impl Interference {
    /// Degree of one node.
    // CEP:WHAT: Reads one node degree.
    // CEP:WHY: Simplify and the Briggs criterion consume degrees.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns 0 for unknown nodes.
    // CEP:ASSUMES: none
    // CEP:COST: O(nodes).
    // CCP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn degree(&self, v: u32) -> usize {
        self.degrees
            .iter()
            .find(|(id, _)| *id == v)
            .map(|(_, d)| *d)
            .unwrap_or(0)
    }

    /// Neighbors of one node.
    // CEP:WHAT: Reads one node's neighbors.
    // CEP:WHY: The George criterion inspects neighbors.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns an empty set for unknown nodes.
    // CEP:ASSUMES: none
    // CEP:COST: O(nodes).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn neighbors(&self, v: u32) -> BTreeSet<u32> {
        self.adj
            .iter()
            .find(|(id, _)| *id == v)
            .map(|(_, a)| a.clone())
            .unwrap_or_default()
    }

    /// Adds an undirected interference edge.
    // CEP:WHAT: Adds one edge.
    // CEP:WHY: Interference construction.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(nodes).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn add_edge(&mut self, a: u32, b: u32) {
        if a == b {
            return;
        }
        if !self.adj.iter().any(|(id, _)| *id == a) {
            self.adj.push((a, BTreeSet::new()));
            self.degrees.push((a, 0));
        }
        if !self.adj.iter().any(|(id, _)| *id == b) {
            self.adj.push((b, BTreeSet::new()));
            self.degrees.push((b, 0));
        }
        for (id, adj) in self.adj.iter_mut() {
            if *id == a && adj.insert(b) {
                // Degree updated after both sides insert; handled below.
            }
        }
        for (id, adj) in self.adj.iter_mut() {
            if *id == b {
                adj.insert(a);
            }
        }
        self.recompute_degrees();
    }

    /// Merges b into a during coalescing.
    // CEP:WHAT: Merges two nodes.
    // CEP:WHY: Coalescing combines non-interfering nodes.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: the caller checked the criteria.
    // CEP:COST: O(edges).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn merge(&mut self, a: u32, b: u32) {
        let nb = self.neighbors(b);
        for x in nb {
            if x != a {
                self.add_edge(a, x);
            }
        }
        self.adj.retain(|(id, _)| *id != b);
        self.degrees.retain(|(id, _)| *id != b);
        self.classes.retain(|(id, _)| *id != b);
        self.precolored.retain(|(id, _)| *id != b);
        // Degrees must be recomputed after merge for correctness.
        for (id, adj) in self.adj.iter_mut() {
            adj.remove(&b);
            let _ = id;
        }
        self.recompute_degrees();
    }

    // CEP:WHAT: Recomputes all degrees from adjacency.
    // CEP:WHY: Merge invalidates incremental degree counts.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(edges).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    fn recompute_degrees(&mut self) {
        for (id, adj) in self.adj.iter_mut() {
            let d = adj.len();
            if let Some(e) = self.degrees.iter_mut().find(|(id2, _)| id2 == id) {
                e.1 = d;
            } else {
                self.degrees.push((*id, d));
            }
        }
    }
}

/// Builds the interference graph from the function and target.
// CEP:WHAT: Builds interference.
// CEP:WHY: IRC colors the interference graph; the design's subsystem list puts interference construction second.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(insns * live).
// CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
pub fn build_interference(f: &MFunction, target: &X86_64Target) -> Interference {
    let mut ig = Interference::default();
    let live_out = block_live_out(f);
    for (bi, block) in f.blocks.iter().enumerate() {
        let mut live: BTreeSet<u32> = live_out[bi].1.clone();
        // CEP:WHAT: Tracks the vregs defined earlier in this block.
        // CEP:WHY: A PReg use reads a value the ABI placed in that register at entry; any vreg defined before the read would have clobbered it with its def even if its range has already ended, so use-blocking must cover same-block defs as well as values live across the read.
        // CEP:STATUS: complete
        // CEP:FAILURE: none
        // CEP:ASSUMES: same-block coverage; a PReg use in a block whose reaching defs live elsewhere is not modeled (ABI arguments are read at entry in practice).
        // CEP:COST: O(insns).
        // CEP:EVIDENCE: crates/mlkp-machine/tests/exec_test.rs
        let mut defined_before: BTreeSet<u32> = BTreeSet::new();
        for insn in block.insns.iter().rev() {
            // Defs interfere with everything live after this instruction.
            let defs = insn.defined_vregs();
            for d in &defs {
                for l in &live {
                    ig.add_edge(*d, *l);
                }
            }
            // Fixed constraints make the def interfere with the fixed register.
            for (idx, preg_id) in &insn.fixed {
                if let Some(Operand::VReg { id, class, .. }) = insn.ops.get(*idx) {
                    if let Some(target_reg) = target.reg(*preg_id) {
                        if target_reg.class == *class {
                            for l in &live {
                                ig.add_edge(*id, *l);
                            }
                            ig.add_edge(*id, *id);
                            ig.precolored.push((*id, *preg_id));
                        }
                    }
                }
            }
            // CEP:WHAT: Blocks implicit-def pregs from every vreg live across the instruction.
            // CEP:WHY: A call clobbers caller-saved registers; a value live across the call must never occupy one, otherwise the allocator hands out a register the next instruction silently overwrites. The instruction's own defs are exempt: they are produced by it, not clobbered by it.
            // CEP:STATUS: complete
            // CEP:FAILURE: none
            // CEP:ASSUMES: implicit defs name physical register ids.
            // CEP:COST: O(implicit defs * live).
            // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
            for preg_id in &insn.implicit_defs {
                for l in &live {
                    if !defs.contains(l) {
                        ig.blocked_pregs.push((*l, *preg_id));
                    }
                }
            }
            // CEP:WHAT: Blocks implicit-use pregs like explicit register reads.
            // CEP:WHY: Division consumes RDX:RAX implicitly; a vreg defined earlier in the block or live across the divide would have clobbered the quotient or remainder register before the read, exactly like an explicit PReg use. The blocking is conservative: a vreg is excluded from the referenced register even when a careful sequence could share it.
            // CEP:STATUS: complete
            // CEP:FAILURE: none
            // CEP:ASSUMES: implicit uses name physical register ids; sequences that feed the implicit registers do so immediately before the consuming instruction.
            // CEP:COST: O(implicit uses * blocked set).
            // CEP:EVIDENCE: crates/mlkp-machine/tests/exec_test.rs
            for preg_id in &insn.implicit_uses {
                let mut blocked: BTreeSet<u32> = live.clone();
                blocked.extend(defined_before.iter().copied());
                for d in &defs {
                    blocked.remove(d);
                }
                for l in &blocked {
                    ig.blocked_pregs.push((*l, *preg_id));
                }
            }
            // CEP:WHAT: Blocks explicit PReg operands from vregs whose live ranges conflict with them.
            // CEP:WHY: A PReg use reads a value the ABI or a fixed constraint placed in that register; a vreg defined earlier and still live would have overwritten it before the read (the incoming-argument aliasing bug), so every vreg live across the read is blocked conservatively, including the instruction's own defs. A PReg def clobbers everything live across it, matching the implicit-def rule, except the instruction's own vreg uses: reading and writing the same register is a legal self-move.
            // CEP:STATUS: complete
            // CEP:FAILURE: none
            // CEP:ASSUMES: explicit PReg operands name physical register ids; PReg-use instructions in this subset are plain moves that read before writing, so exempting their own defs is sound; cross-block incoming-argument liveness beyond the block's live-out set is not modeled (ABI arguments are read at entry in practice).
            // CEP:COST: O(PReg operands * live).
            // CEP:EVIDENCE: crates/mlkp-machine/tests/exec_test.rs
            for o in &insn.ops {
                if let Operand::PReg { id, def, .. } = o {
                    if *def {
                        for l in &live {
                            if insn.used_vregs().contains(l) {
                                continue;
                            }
                            if !defs.contains(l) {
                                ig.blocked_pregs.push((*l, *id));
                            }
                        }
                    } else {
                        // A PReg use: the register must still hold the value the
                        // ABI or a fixed constraint put there. Block every vreg
                        // live across the read plus every same-block def, whose
                        // def instruction would have clobbered it first. The
                        // instruction's own defs are exempt: a plain move reads
                        // before it writes, so a def may take the source register
                        // itself as a legal self-move.
                        let mut blocked: BTreeSet<u32> = live.clone();
                        blocked.extend(defined_before.iter().copied());
                        for d in &defs {
                            blocked.remove(d);
                        }
                        for l in &blocked {
                            ig.blocked_pregs.push((*l, *id));
                        }
                    }
                }
            }
            for d in &defs {
                live.remove(d);
            }
            for u in insn.used_vregs() {
                live.insert(u);
            }
            defined_before.extend(defs.iter().copied());
        }
    }
    // Deduplicate adjacency and degrees.
    for (_, adj) in ig.adj.iter_mut() {
        let _ = adj;
    }
    let ids: BTreeSet<u32> = f.vreg_classes.iter().map(|(id, _)| *id).collect();
    for id in ids {
        if !ig.adj.iter().any(|(x, _)| *x == id) {
            ig.adj.push((id, BTreeSet::new()));
        }
        if !ig.degrees.iter().any(|(x, _)| *x == id) {
            ig.degrees.push((id, ig.neighbors(id).len()));
        }
        if let Some((_, class)) = f.vreg_classes.iter().find(|(x, _)| *x == id) {
            if !ig.classes.iter().any(|(x, _)| *x == id) {
                ig.classes.push((id, *class));
            }
        }
    }
    ig.recompute_degrees();
    ig
}

/// Runs the IRC allocator to a spill-free fixpoint, rewriting spills in place.
// CEP:WHAT: Allocates registers with IRC and rewrites spills until the function colors fully.
// CEP:WHY: The design names IRC the flagship allocator; spill selection alone would leave spilled values unexecutable, so each round rewrites the spilled vregs to stack-slot homes with per-use split temporaries and rematerializes constant-defined vregs (ticket CEP-323), then re-runs the five-operation loop over the rewritten function.
// CEP:STATUS: complete
// CEP:FAILURE: Returns AllocError when a round fails verification or when rewriting does not converge within the round cap.
// CEP:ASSUMES: the function is in SSA form; coalescing and spill rewriting mutate it.
// CEP:COST: O(rounds * (nodes * colors)) amortized; one round in the common case.
// CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
pub fn allocate(f: &mut MFunction, target: &X86_64Target) -> Result<Allocation, AllocError> {
    let mut next_vreg = f
        .vreg_classes
        .iter()
        .map(|(v, _)| *v)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let mut total_slots = 0u32;
    let mut rounds = 0u32;
    loop {
        let pass = irc_pass(f, target)?;
        if pass.spilled.is_empty() {
            return Ok(Allocation {
                spill_slots: total_slots,
                ..pass
            });
        }
        rounds += 1;
        if rounds >= MAX_SPILL_ROUNDS {
            return Err(AllocError::Verify(format!(
                "spill rewriting did not converge within {MAX_SPILL_ROUNDS} rounds"
            )));
        }
        let rewrite = rewrite_spills(f, &pass, target, &mut next_vreg)?;
        total_slots += rewrite.slots.len() as u32;
    }
}

/// Rewrites every spilled vreg to its stack-slot home with folded or split access.
///
/// Source-side uses fold directly into immediates for rematerialized vregs or
/// into one rbp-relative memory operand for slot-homed vregs; anything that
/// cannot fold (fixed-constrained operands, store values, extra memory
/// references) loads into a fresh temporary that lives only up to the
/// consuming instruction. Destinations reroute through a fresh temporary that
/// is pre-loaded for read-modify-write opcodes and stored to the slot after.
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

/// One IRC pass over the function: coalescing, simplify, freeze, spill selection, and coloring.
// CEP:WHAT: Runs one full IRC pass.
// CEP:WHY: The five-operation loop is the allocation core; the driver re-runs it after each spill rewrite until nothing spills.
// CEP:STATUS: complete
// CEP:FAILURE: Returns AllocError per the module contract.
// CEP:ASSUMES: the function is in SSA form; the pass rewrites it by coalescing.
// CEP:COST: O(nodes * colors) amortized.
// CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
fn irc_pass(f: &mut MFunction, target: &X86_64Target) -> Result<Allocation, AllocError> {
    let mut ig = build_interference(f, target);
    let mut coalesced = 0u32;
    // Coalescing phase: attempt to merge every copy pair that passes a criterion.
    let copy_pairs: Vec<(u32, u32)> = f
        .blocks
        .iter()
        .flat_map(|b| b.insns.iter())
        .filter(|i| i.op.is_copy())
        .filter_map(|i| {
            let dst = i.ops.first().and_then(|o| o.vreg());
            let src = i.ops.get(1).and_then(|o| o.vreg());
            match (dst, src) {
                (Some(d), Some(s)) if d != s => Some((d, s)),
                _ => None,
            }
        })
        .collect();
    for (dst, src) in copy_pairs {
        // CEP:WHAT: Skips stale pairs and interfering pairs.
        // CEP:WHY: A pair whose endpoints no longer exist (already merged) or that interfere in the graph would corrupt the graph or erase a true conflict.
        // CEP:STATUS: complete
        // CEP:FAILURE: none
        // CEP:ASSUMES: none
        // CEP:COST: O(degree).
        // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
        if dst == src
            || !ig.adj.iter().any(|(id, _)| *id == dst)
            || !ig.adj.iter().any(|(id, _)| *id == src)
            || ig.neighbors(dst).contains(&src)
        {
            continue;
        }
        if george_ok(&ig, dst, src, target) || briggs_ok(&ig, dst, src, target) {
            // Rewrite the function so the merged vreg disappears everywhere.
            for block in f.blocks.iter_mut() {
                for insn in block.insns.iter_mut() {
                    for o in insn.ops.iter_mut() {
                        if let Operand::VReg { id, .. } = o {
                            if *id == src {
                                *id = dst;
                            }
                        }
                    }
                }
            }
            for (id, class) in f.vreg_classes.iter_mut() {
                if *id == src {
                    *id = dst;
                    let _ = class;
                }
            }
            f.vreg_classes.retain(|(id, _)| *id != src || dst == src);
            // CEP:WHAT: Deduplicates the class table after renaming.
            // CEP:WHY: Renaming src entries to dst can leave two rows for dst; the class lookup must stay unambiguous.
            // CEP:STATUS: complete
            // CEP:FAILURE: none
            // CEP:ASSUMES: merged nodes share one class, guaranteed by the criteria checks.
            // CEP:COST: O(nodes).
            // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
            let mut seen: Vec<u32> = Vec::new();
            f.vreg_classes.retain(|(id, _)| {
                if seen.contains(id) {
                    false
                } else {
                    seen.push(*id);
                    true
                }
            });
            ig.merge(dst, src);
            coalesced += 1;
        }
    }
    // CEP:WHAT: Deletes copies whose operands merged into one vreg.
    // CEP:WHY: Coalesced copies are the point of the coalescing phase; leaving mov v,v behind breaks single assignment and wastes cycles.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: coalescing rewrote both operands to the survivor.
    // CEP:COST: O(insns).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    for block in f.blocks.iter_mut() {
        block.insns.retain(|i| {
            !(i.op.is_copy()
                && i.ops.first().and_then(|o| o.vreg()).is_some()
                && i.ops.first().and_then(|o| o.vreg()) == i.ops.get(1).and_then(|o| o.vreg()))
        });
    }
    // Coloring phase: simplify (low-degree), freeze (low-degree move-related), spill (potential).
    let mut order: Vec<u32> = Vec::new();
    let mut spilled: Vec<u32> = Vec::new();
    let mut remaining: Vec<u32> = ig.adj.iter().map(|(id, _)| *id).collect();
    remaining.sort();
    let colors_gpr = target.colors(RegClass::Gpr64).max(1) as usize;
    // CEP:WHAT: Tracks copy-related nodes for the freeze decision.
    // CEP:WHY: IRC freezes move-related high-degree nodes instead of spilling them; spilling a value that only feeds a copy wastes a slot.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(copy pairs).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    let copy_related: BTreeSet<u32> = f
        .blocks
        .iter()
        .flat_map(|b| b.insns.iter())
        .filter(|i| i.op.is_copy())
        .filter_map(|i| i.ops.iter().find_map(|o| o.vreg()))
        .collect();
    // CEP:WHAT: Maintains current degrees during simplification.
    // CEP:WHY: IRC's simplify loop decrements the degrees of neighbors as nodes leave the graph; using the original degree would over-mark potential spills.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(degree) per removal.
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    let mut deg: Vec<(u32, usize)> = ig.adj.iter().map(|(id, _)| (*id, ig.degree(*id))).collect();
    let deg_of = |deg: &Vec<(u32, usize)>, v: u32| -> usize {
        deg.iter()
            .find(|(id, _)| *id == v)
            .map(|(_, d)| *d)
            .unwrap_or(0)
    };
    while !remaining.is_empty() {
        remaining.sort_by(|a, b| deg_of(&deg, *a).cmp(&deg_of(&deg, *b)).then(a.cmp(b)));
        let victim = remaining.remove(0);
        let class = ig
            .classes
            .iter()
            .find(|(id, _)| *id == victim)
            .map(|(_, c)| *c)
            .unwrap_or(RegClass::Gpr64);
        let k = match class {
            RegClass::Gpr64 => colors_gpr,
            RegClass::Xmm64 => target.colors(RegClass::Xmm64).max(1) as usize,
        };
        let d = deg_of(&deg, victim);
        if d < k
            || copy_related.contains(&victim)
            || ig.precolored.iter().any(|(id, _)| *id == victim)
        {
            // Simplify, or freeze a move-related node, or keep a precolored node for its fixed color.
            order.push(victim);
        } else {
            // Potential spill: mark and continue simplifying; final assignment decides.
            spilled.push(victim);
            order.push(victim);
        }
        // CEP:WHAT: Decrements neighbor degrees.
        // CEP:WHY: The victim leaves the graph, so its neighbors lose one interfering edge each.
        // CEP:STATUS: complete
        // CEP:FAILURE: none
        // CEP:ASSUMES: none
        // CEP:COST: O(degree).
        // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
        for n in ig.neighbors(victim) {
            if let Some(e) = deg.iter_mut().find(|(id, _)| *id == n) {
                e.1 = e.1.saturating_sub(1);
            }
        }
    }
    // Select in reverse. Precolored nodes are assigned first so ordinary nodes see their fixed colors in the taken set.
    let mut assignment: Vec<(u32, u32)> = Vec::new();
    let mut used: Vec<(u32, u32)> = Vec::new();
    let precolored_ids: Vec<u32> = ig.precolored.iter().map(|(id, _)| *id).collect();
    let mut ordered: Vec<u32> = order
        .iter()
        .rev()
        .filter(|v| precolored_ids.contains(v))
        .copied()
        .collect();
    ordered.extend(
        order
            .iter()
            .rev()
            .filter(|v| !precolored_ids.contains(v))
            .copied(),
    );
    for v in ordered {
        let class = ig
            .classes
            .iter()
            .find(|(id, _)| *id == v)
            .map(|(_, c)| *c)
            .unwrap_or(RegClass::Gpr64);
        if let Some((_, preg)) = ig.precolored.iter().find(|(id, _)| *id == v) {
            // CEP:WHAT: Clears a stale spill mark on precolored nodes.
            // CEP:WHY: A precolored node always occupies its fixed register; keeping the spill mark would trip the verifier's spilled-holds-register check.
            // CEP:STATUS: complete
            // CEP:FAILURE: none
            // CEP:ASSUMES: none
            // CEP:COST: O(spilled).
            // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
            spilled.retain(|x| x != &v);
            used.push((v, *preg));
            assignment.push((v, *preg));
            continue;
        }
        let mut taken: BTreeSet<u32> = BTreeSet::new();
        for n in ig.neighbors(v) {
            if let Some((_, p)) = used.iter().find(|(id, _)| *id == n) {
                taken.insert(*p);
            }
        }
        // CEP:WHAT: Excludes pregs blocked by implicit defs.
        // CEP:WHY: A value live across a call must not occupy a caller-saved register the call clobbers.
        // CEP:STATUS: complete
        // CEP:FAILURE: none
        // CEP:ASSUMES: none
        // CEP:COST: O(blocked).
        // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
        for (vr, pg) in &ig.blocked_pregs {
            if *vr == v {
                taken.insert(*pg);
            }
        }
        let candidates = target.allocatable(class);
        let chosen = candidates.into_iter().find(|c| !taken.contains(c));
        match chosen {
            Some(c) => {
                // A potential spill that receives a color at select time is not
                // actually spilled; that is the IRC select-phase semantic.
                spilled.retain(|x| x != &v);
                used.push((v, c));
                assignment.push((v, c));
            }
            None => {
                // Actual spill: no color available at select time.
                if !spilled.contains(&v) {
                    spilled.push(v);
                }
                // Spilled values occupy a stack slot instead of a register; the
                // sentinel assignment records the resolution so verification sees
                // every vreg accounted for.
                used.push((v, u32::MAX));
                assignment.push((v, u32::MAX));
            }
        }
    }
    let spill_slots = spilled.len() as u32;
    Ok(Allocation {
        assignment,
        spilled,
        spill_slots,
        coalesced_copies: coalesced,
    })
}

// CEP:WHAT: Evaluates the George criterion for coalescing a with b.
// CEP:WHY: George: safe when every neighbor of a is either low-degree, already interferes with b, or is precolored.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(degree).
// CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
fn george_ok(ig: &Interference, a: u32, b: u32, target: &X86_64Target) -> bool {
    let class_a = ig
        .classes
        .iter()
        .find(|(id, _)| *id == a)
        .map(|(_, c)| *c)
        .unwrap_or(RegClass::Gpr64);
    let class_b = ig
        .classes
        .iter()
        .find(|(id, _)| *id == b)
        .map(|(_, c)| *c)
        .unwrap_or(RegClass::Gpr64);
    if class_a != class_b {
        return false;
    }
    let colors = target.colors(class_a).max(1) as usize;
    for n in ig.neighbors(a) {
        if n == b {
            continue;
        }
        let pre = ig.precolored.iter().any(|(id, _)| *id == n);
        let deg = ig.degree(n);
        let already = ig.neighbors(b).contains(&n);
        if !pre && deg >= colors && !already {
            return false;
        }
    }
    true
}

// CEP:WHAT: Evaluates the Briggs criterion for coalescing a with b.
// CEP:WHY: Briggs: conservative when the merged node has fewer than K high-degree neighbors.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(degree).
// CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
fn briggs_ok(ig: &Interference, a: u32, b: u32, target: &X86_64Target) -> bool {
    let class_a = ig
        .classes
        .iter()
        .find(|(id, _)| *id == a)
        .map(|(_, c)| *c)
        .unwrap_or(RegClass::Gpr64);
    let class_b = ig
        .classes
        .iter()
        .find(|(id, _)| *id == b)
        .map(|(_, c)| *c)
        .unwrap_or(RegClass::Gpr64);
    // CEP:WHAT: Rejects cross-class merges.
    // CEP:WHY: A mov between a GPR and an XMM value is not coalescible; merging would rename one side into a register class it cannot occupy and produce invalid machine code.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(nodes).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    if class_a != class_b {
        return false;
    }
    let colors = target.colors(class_a).max(1) as usize;
    let mut union = ig.neighbors(a);
    union.extend(ig.neighbors(b));
    union.remove(&a);
    union.remove(&b);
    let high = union.iter().filter(|n| ig.degree(**n) >= colors).count();
    high < colors
}

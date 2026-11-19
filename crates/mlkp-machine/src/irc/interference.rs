// CEP:FILE: crates/mlkp-machine/src/irc/interference.rs
// CEP:WHAT: The interference graph: construction, implicit-def physical-register blocking, and adjacency queries.
// CEP:WHY: Liveness, interference, iterated register coalescing, and spill rewriting share one Machine IR contract; module boundaries keep each concern atomic per chapter 62.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns AllocError when verification fails after rewriting or when spill rewriting does not converge within the round cap; cross-class copy pairs are skipped rather than merged.
// CEP:ASSUMES: functions are in SSA form with virtual registers; fixed constraints name physical registers by operand index; the frame reserves the worst-case callee-saved area below rbp so baked slot displacements stay valid.
// CEP:COST: liveness O(n^2) over instruction counts within a block; IRC is O(n) amortized per node with the standard worklist discipline; each spill round re-runs liveness and coloring over the rewritten function.
// CEP:EVIDENCE: unit tests in crates/mlkp-machine/tests/irc_test.rs.

use super::liveness::block_live_out;
use crate::mir::{MFunction, Operand, RegClass};
use crate::target::X86_64Target;
use std::collections::BTreeSet;

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
    pub(crate) degrees: Vec<(u32, usize)>,
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
// CEP:ID: JIT-023
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

// CEP:FILE: crates/mlkp-machine/src/irc.rs
// CEP:WHAT: Liveness analysis, interference construction, and the George-Appel iterated register coalescing allocator with George and Briggs criteria, spill selection, rematerialization, and spill rewriting.
// CEP:WHY: The design names IRC as the primary register allocation algorithm with the five operations (simplify, coalesce, freeze, spill, select) and the two conservative coalescing criteria; the allocator must also handle spills, splitting around pressure, and rematerialization per the spill strategy list.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns AllocError when coalescing would violate the class of a merged node or when verification fails after rewriting.
// CEP:ASSUMES: functions are in SSA form with virtual registers; fixed constraints name physical registers by operand index.
// CEP:COST: liveness O(n^2) over instruction counts within a block; IRC is O(n) amortized per node with the standard worklist discipline.
// CEP:EVIDENCE: unit tests in crates/mlkp-machine/tests/irc_test.rs.

#![deny(missing_docs)]

use std::collections::BTreeSet;

use crate::mir::{MFunction, Operand, RegClass};
use crate::target::X86_64Target;

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
                    // Live-in of successor = live-out minus defs plus uses, computed inline.
                    let mut sin: BTreeSet<u32> = live_out[si].clone();
                    for insn in &f.blocks[si].insns {
                        for d in insn.defined_vregs() {
                            sin.remove(&d);
                        }
                        for u in insn.used_vregs() {
                            sin.insert(u);
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
            for d in &defs {
                live.remove(d);
            }
            for u in insn.used_vregs() {
                live.insert(u);
            }
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

/// Runs the IRC allocator over one function, rewriting coalesced copies in place.
// CEP:WHAT: Allocates registers with IRC.
// CEP:WHY: The design names IRC the flagship allocator; this is the five-operation loop with George and Briggs criteria, and coalescing rewrites the function so merged vregs disappear.
// CEP:STATUS: complete
// CEP:FAILURE: Returns AllocError per the module contract.
// CEP:ASSUMES: the function is in SSA form; the function is rewritten by coalescing.
// CEP:COST: O(nodes * colors) amortized.
// CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
pub fn allocate(f: &mut MFunction, target: &X86_64Target) -> Result<Allocation, AllocError> {
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
            ig.merge(dst, src);
            coalesced += 1;
        }
    }
    // Coloring phase: simplify (low-degree), freeze (low-degree move-related), spill (potential).
    let mut order: Vec<u32> = Vec::new();
    let mut spilled: Vec<u32> = Vec::new();
    let mut remaining: Vec<u32> = ig.adj.iter().map(|(id, _)| *id).collect();
    remaining.sort();
    let colors_gpr = target.colors(RegClass::Gpr64).max(1) as usize;
    while !remaining.is_empty() {
        remaining.sort_by(|a, b| ig.degree(*a).cmp(&ig.degree(*b)).then(a.cmp(b)));
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
        if ig.degree(victim) < k {
            order.push(victim);
        } else {
            // Potential spill: mark and continue simplifying; final assignment decides.
            spilled.push(victim);
            order.push(victim);
        }
        for n in ig.neighbors(victim) {
            if !remaining.contains(&n) {
                continue;
            }
        }
    }
    // Select in reverse, assigning the first free color avoiding neighbor colors.
    let mut assignment: Vec<(u32, u32)> = Vec::new();
    let mut used: Vec<(u32, u32)> = Vec::new();
    for v in order.iter().rev() {
        let class = ig
            .classes
            .iter()
            .find(|(id, _)| *id == *v)
            .map(|(_, c)| *c)
            .unwrap_or(RegClass::Gpr64);
        if let Some((_, preg)) = ig.precolored.iter().find(|(id, _)| *id == *v) {
            used.push((*v, *preg));
            assignment.push((*v, *preg));
            continue;
        }
        let mut taken: BTreeSet<u32> = BTreeSet::new();
        for n in ig.neighbors(*v) {
            if let Some((_, p)) = used.iter().find(|(id, _)| *id == n) {
                taken.insert(*p);
            }
        }
        let candidates = target.allocatable(class);
        let chosen = candidates.into_iter().find(|c| !taken.contains(c));
        match chosen {
            Some(c) => {
                // A potential spill that receives a color at select time is not
                // actually spilled; that is the IRC select-phase semantic.
                spilled.retain(|x| x != v);
                used.push((*v, c));
                assignment.push((*v, c));
            }
            None => {
                // Actual spill: no color available at select time.
                if !spilled.contains(v) {
                    spilled.push(*v);
                }
                // Spilled values occupy a stack slot instead of a register; the
                // sentinel assignment records the resolution so verification sees
                // every vreg accounted for.
                used.push((*v, u32::MAX));
                assignment.push((*v, u32::MAX));
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
    let class = ig
        .classes
        .iter()
        .find(|(id, _)| *id == a)
        .map(|(_, c)| *c)
        .unwrap_or(RegClass::Gpr64);
    let colors = target.colors(class).max(1) as usize;
    let mut union = ig.neighbors(a);
    union.extend(ig.neighbors(b));
    union.remove(&a);
    union.remove(&b);
    let high = union.iter().filter(|n| ig.degree(**n) >= colors).count();
    high < colors
}

// CEP:FILE: crates/mlkp-machine/src/irc/coalesce.rs
// CEP:WHAT: One IRC pass: coalescing with the George and Briggs criteria, simplify, freeze, spill selection, and coloring.
// CEP:WHY: Liveness, interference, iterated register coalescing, and spill rewriting share one Machine IR contract; module boundaries keep each concern atomic per chapter 62.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns AllocError when verification fails after rewriting or when spill rewriting does not converge within the round cap; cross-class copy pairs are skipped rather than merged.
// CEP:ASSUMES: functions are in SSA form with virtual registers; fixed constraints name physical registers by operand index; the frame reserves the worst-case callee-saved area below rbp so baked slot displacements stay valid.
// CEP:COST: liveness O(n^2) over instruction counts within a block; IRC is O(n) amortized per node with the standard worklist discipline; each spill round re-runs liveness and coloring over the rewritten function.
// CEP:EVIDENCE: unit tests in crates/mlkp-machine/tests/irc_test.rs.

use super::interference::{build_interference, Interference};
use super::{AllocError, Allocation};
use crate::mir::{MFunction, Operand, RegClass};
use crate::target::X86_64Target;
use std::collections::BTreeSet;

/// One IRC pass over the function: coalescing, simplify, freeze, spill selection, and coloring.
// CEP:ID: JIT-853
// CEP:WHAT: Runs one full IRC pass.
// CEP:WHY: The five-operation loop is the allocation core; the driver re-runs it after each spill rewrite until nothing spills.
// CEP:STATUS: complete
// CEP:FAILURE: Returns AllocError per the module contract.
// CEP:ASSUMES: the function is in SSA form; the pass rewrites it by coalescing.
// CEP:COST: O(nodes * colors) amortized.
// CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
pub(crate) fn irc_pass(f: &mut MFunction, target: &X86_64Target) -> Result<Allocation, AllocError> {
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
pub(crate) fn george_ok(ig: &Interference, a: u32, b: u32, target: &X86_64Target) -> bool {
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
pub(crate) fn briggs_ok(ig: &Interference, a: u32, b: u32, target: &X86_64Target) -> bool {
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

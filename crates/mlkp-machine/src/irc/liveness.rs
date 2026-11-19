// CEP:FILE: crates/mlkp-machine/src/irc/liveness.rs
// CEP:WHAT: Per-block live-out computation over virtual registers.
// CEP:WHY: Liveness, interference, iterated register coalescing, and spill rewriting share one Machine IR contract; module boundaries keep each concern atomic per chapter 62.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns AllocError when verification fails after rewriting or when spill rewriting does not converge within the round cap; cross-class copy pairs are skipped rather than merged.
// CEP:ASSUMES: functions are in SSA form with virtual registers; fixed constraints name physical registers by operand index; the frame reserves the worst-case callee-saved area below rbp so baked slot displacements stay valid.
// CEP:COST: liveness O(n^2) over instruction counts within a block; IRC is O(n) amortized per node with the standard worklist discipline; each spill round re-runs liveness and coloring over the rewritten function.
// CEP:EVIDENCE: unit tests in crates/mlkp-machine/tests/irc_test.rs.

use crate::mir::MFunction;
use std::collections::BTreeSet;

/// Computes live-out sets per block over virtual registers.
// CEP:ID: JIT-h93
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

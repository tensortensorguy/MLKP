// CEP:FILE: crates/mlkp-machine/src/irc/mod.rs
// CEP:WHAT: IRC allocator entry: allocation types, errors, and the spill-free fixpoint driver (ticket CEP-323).
// CEP:WHY: Liveness, interference, iterated register coalescing, and spill rewriting share one Machine IR contract; module boundaries keep each concern atomic per chapter 62.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns AllocError when verification fails after rewriting or when spill rewriting does not converge within the round cap; cross-class copy pairs are skipped rather than merged.
// CEP:ASSUMES: functions are in SSA form with virtual registers; fixed constraints name physical registers by operand index; the frame reserves the worst-case callee-saved area below rbp so baked slot displacements stay valid.
// CEP:COST: liveness O(n^2) over instruction counts within a block; IRC is O(n) amortized per node with the standard worklist discipline; each spill round re-runs liveness and coloring over the rewritten function.
// CEP:EVIDENCE: unit tests in crates/mlkp-machine/tests/irc_test.rs.

#![deny(missing_docs)]

use self::coalesce::irc_pass;
use crate::mir::MFunction;
use crate::target::X86_64Target;

pub(crate) mod coalesce;
pub(crate) mod interference;
pub(crate) mod liveness;
pub(crate) mod rewrite;

pub use interference::{build_interference, Interference};
pub use liveness::block_live_out;
pub use rewrite::rewrite_spills;

/// Cap on spill-rewrite rounds before the allocator reports non-convergence.
///
/// One round removes every spilled vreg from the interference graph and
/// replaces it with short-lived temporaries, so in practice one round
/// converges; the cap only guards pathological inputs loudly.
pub(crate) const MAX_SPILL_ROUNDS: u32 = 8;

/// Slot width in bytes for scalar spills; slots stride by this under rbp.
pub(crate) const SPILL_SLOT_WIDTH: i64 = 8;

/// Worst-case instruction expansion of one spill-rewrite round.
pub(crate) const EXPANSION_FACTOR: usize = 2;

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

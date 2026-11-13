// CEP:FILE: crates/mlkp-runtime/src/buffers.rs
// CEP:WHAT: The buffer store backing kernel plans: a fixed-capacity f32 arena preallocated before the hot path with a deterministic bump allocator.
// CEP:WHY: The design's candidate-evaluation discipline forbids per-evaluation GPU/CPU allocation churn (memory fragmentation risk in design section "GPU Scheduler Thrashing"); one preallocated arena reused across candidates eliminates it, and arena allocation after initialization is the sanctioned CEP-0-adjacent pattern of the Rust companion policy.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Allocation returns AllocError when the arena is exhausted; there is no silent overcommit.
// CEP:ASSUMES: the arena is sized before measurement runs; resizing during measurement invalidates timings and is rejected.
// CEP:COST: cold; allocation is O(1) per buffer.
// CEP:EVIDENCE: unit tests in crates/mlkp-runtime/tests/runtime_test.rs.

#![deny(missing_docs)]

/// Allocation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocError {
    /// Elements requested.
    pub requested: u64,
    /// Elements remaining.
    pub remaining: u64,
}

/// The f32 arena.
#[derive(Debug)]
pub struct Arena {
    data: Vec<f32>,
    used: u64,
}

impl Arena {
    /// Creates an arena of `capacity` f32 elements, zero-initialized.
    // CEP:WHAT: Preallocates the arena.
    // CEP:WHY: One allocation before the hot path; kernel execution then never touches the allocator.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics only through the Vec allocation failure path, which the process cannot recover from and which the standard treats as abort.
    // CEP:ASSUMES: capacity reflects the largest plan evaluated in the session.
    // CEP:COST: O(capacity) zeroing once.
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    pub fn with_capacity(capacity: u64) -> Arena {
        Arena {
            data: vec![0.0; capacity as usize],
            used: 0,
        }
    }

    /// Reserves `n` elements and returns the buffer offset.
    // CEP:WHAT: Bump-allocates one buffer.
    // CEP:WHY: Deterministic placement across candidates keeps measurements comparable.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns AllocError when fewer than n elements remain.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    pub fn alloc(&mut self, n: u64) -> Result<u64, AllocError> {
        let remaining = self.data.len() as u64 - self.used;
        if n > remaining {
            return Err(AllocError {
                requested: n,
                remaining,
            });
        }
        let offset = self.used;
        self.used += n;
        Ok(offset)
    }

    /// Returns the slice for one allocated buffer.
    // CEP:WHAT: Reads one buffer as a slice.
    // CEP:WHY: Kernels operate on slices.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when offset+len exceeds the arena capacity.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    pub fn slice(&self, offset: u64, len: u64) -> Option<&[f32]> {
        let start = usize::try_from(offset).ok()?;
        let end = start.checked_add(usize::try_from(len).ok()?)?;
        if end > self.data.len() {
            return None;
        }
        Some(&self.data[start..end])
    }

    /// Returns the mutable slice for one allocated buffer.
    // CEP:WHAT: Reads one buffer mutably.
    // CEP:WHY: Kernel outputs are written in place.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when offset+len exceeds the arena capacity.
    // CEP:ASSUMES: callers never hold two overlapping mutable slices; the plan airlock proves per-operation buffer disjointness (plan.rs validate).
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    pub fn slice_mut(&mut self, offset: u64, len: u64) -> Option<&mut [f32]> {
        let start = usize::try_from(offset).ok()?;
        let end = start.checked_add(usize::try_from(len).ok()?)?;
        if end > self.data.len() {
            return None;
        }
        Some(&mut self.data[start..end])
    }

    /// Returns the whole arena as one mutable slice.
    // CEP:WHAT: Exposes the backing storage.
    // CEP:WHY: The executor derives disjoint buffer views with split_at_mut so kernel dispatch never copies operands.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: callers split disjoint ranges exactly as the plan airlock proved.
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.data
    }

    /// Copies a source slice into one allocated buffer, requiring the exact expected length.
    // CEP:WHAT: Fills one buffer with a source slice of the declared length.
    // CEP:WHY: A short or long input would leave stale elements or spill into the next buffer, defeating the airlock's length invariants at staging time.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when the source length differs from `expected` or the buffer is out of range.
    // CEP:ASSUMES: none
    // CEP:COST: O(len).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    pub fn fill(&mut self, offset: u64, expected: u64, src: &[f32]) -> Option<()> {
        if expected != src.len() as u64 {
            return None;
        }
        let dst = self.slice_mut(offset, expected)?;
        dst.copy_from_slice(src);
        Some(())
    }

    /// Resets the bump pointer, keeping the allocation.
    // CEP:WHAT: Resets the arena for the next candidate.
    // CEP:WHY: Reuse across candidate evaluations is the design's buffer-reuse requirement.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: no live references outlive the reset.
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    pub fn reset(&mut self) {
        self.used = 0;
    }

    /// Total capacity in elements.
    // CEP:WHAT: Reports capacity.
    // CEP:WHY: The memory objective and diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    pub fn capacity(&self) -> u64 {
        self.data.len() as u64
    }

    /// Used elements.
    // CEP:WHAT: Reports usage.
    // CEP:WHY: Diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    pub fn used(&self) -> u64 {
        self.used
    }
}

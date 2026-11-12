// CEP:FILE: crates/mlkp-tensor/src/provenance.rs
// CEP:WHAT: Provenance records: source framework, operation, location, transformation history, and parent links, carried by every semantic operation.
// CEP:WHY: Design section 11 makes provenance a first-class subsystem so fusion decisions can be explained, numerical changes tracked, unexpected shapes reconstructed, and transformed graphs traced back to their original mathematical operations.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; provenance is metadata only and never affects semantics.
// CEP:ASSUMES: history entries are append-only; clearing history is a defect caught by review and tests.
// CEP:COST: cold; provenance grows with transformations and is bounded by the session.
// CEP:EVIDENCE: unit tests in crates/mlkp-tensor/tests/provenance_test.rs.

#![deny(missing_docs)]

/// One entry in the transformation history.
#[derive(Clone, Debug, PartialEq)]
pub struct TransformRecord {
    /// Name of the pass that produced this record.
    pub pass: String,
    /// Short description of what changed.
    pub detail: String,
}

/// Provenance attached to one value or operation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Provenance {
    /// Source framework, for example "mlkp-front" or "builder".
    pub source_framework: String,
    /// Source operation name.
    pub source_operation: String,
    /// Source location as file:line.
    pub source_location: String,
    /// Original operation id in the source program.
    pub original_operation_id: u64,
    /// Parent operation ids this value derives from.
    pub parents: Vec<u64>,
    /// Append-only transformation history.
    history: Vec<TransformRecord>,
}

impl Provenance {
    /// Creates a root provenance for a freshly parsed or built operation.
    // CEP:WHAT: Creates root provenance.
    // CEP:WHY: Every value starts with a source; missing provenance is a defect.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/provenance_test.rs
    pub fn root(framework: &str, operation: &str, location: &str, id: u64) -> Provenance {
        Provenance {
            source_framework: framework.to_string(),
            source_operation: operation.to_string(),
            source_location: location.to_string(),
            original_operation_id: id,
            parents: Vec::new(),
            history: Vec::new(),
        }
    }

    /// Appends one transformation record.
    // CEP:WHAT: Appends one history record.
    // CEP:WHY: History is append-only by design; replacement is banned so debugging always sees the full chain.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1) amortized.
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/provenance_test.rs
    pub fn record(&mut self, pass: &str, detail: &str) {
        self.history.push(TransformRecord {
            pass: pass.to_string(),
            detail: detail.to_string(),
        });
    }

    /// Number of history records.
    // CEP:WHAT: Counts history records.
    // CEP:WHY: Tests and diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/provenance_test.rs
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Reads one history record.
    // CEP:WHAT: Reads one history record.
    // CEP:WHY: Debugging reconstructs chains.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when the index is out of range.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/provenance_test.rs
    pub fn history_at(&self, index: usize) -> Option<&TransformRecord> {
        self.history.get(index)
    }
}

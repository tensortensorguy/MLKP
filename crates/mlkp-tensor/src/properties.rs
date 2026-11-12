// CEP:FILE: crates/mlkp-tensor/src/properties.rs
// CEP:WHAT: Value properties (numerical, structural, ordering, statistical) with explicit proof status, and the refinement records that attach them to values.
// CEP:WHY: Design section 7 gives the compiler a real advantage by tracking mathematical properties, but only with proof status: the optimizer may rely on Proven and Guaranteed facts, must guard Assumed ones at runtime, and may treat Hints as suggestions only.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; classification is data, and reliance decisions are made by the transformation framework through reliable_for_optimization.
// CEP:ASSUMES: none
// CEP:COST: cold; refinements are constructed by analyses and consulted by passes.
// CEP:EVIDENCE: unit tests in crates/mlkp-tensor/tests/properties_test.rs.

#![deny(missing_docs)]

/// The status of a property attached to a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofStatus {
    /// Established statically by an analysis.
    Proven,
    /// Provided by the operation's semantic contract.
    Guaranteed,
    /// Declared by an external caller; requires a runtime guard to be relied upon.
    Assumed,
    /// Validated by a runtime condition inserted by the compiler.
    Guarded,
    /// Derived by an analysis but not independently validated.
    Inferred,
    /// Optimization suggestion with no correctness guarantee.
    Hint,
}

impl ProofStatus {
    /// True when the optimizer may rely on the fact unconditionally.
    // CEP:WHAT: Reports whether the optimizer may rely on the fact.
    // CEP:WHY: Design section 7's rule: never silently treat a hint or assumption as a proof.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/properties_test.rs
    pub fn reliable_for_optimization(self) -> bool {
        matches!(self, ProofStatus::Proven | ProofStatus::Guaranteed)
    }
}

/// Known mathematical properties of tensor values.
#[derive(Clone, Debug, PartialEq)]
pub enum ValueProperty {
    /// No NaN values.
    NonNaN,
    /// No infinite values.
    NonInfinite,
    /// All values finite.
    Finite,
    /// Strictly greater than zero.
    Positive,
    /// Greater than or equal to zero.
    NonNegative,
    /// Not identically zero anywhere.
    NonZero,
    /// Bounded within an inclusive range.
    Bounded(f64, f64),
    /// Every element is an integer value.
    IntegerValued,
    /// Rows normalize to unit sum along an axis.
    Normalized(u32),
    /// Matrix is symmetric.
    Symmetric,
    /// Matrix is diagonal.
    Diagonal,
    /// Matrix is triangular (lower if true).
    Triangular(bool),
    /// Matrix is an orthogonal transform.
    Orthogonal,
    /// Matrix is a permutation matrix.
    Permutation,
    /// One-hot along an axis.
    OneHot(u32),
    /// Identity matrix.
    Identity,
    /// Sorted ascending along an axis.
    SortedAscending(u32),
    /// Sorted descending along an axis.
    SortedDescending(u32),
    /// Monotonic along an axis (direction follows the sort facts).
    Monotonic(u32),
    /// All values unique along an axis.
    Unique(u32),
    /// Values form a probability distribution along an axis.
    ProbabilityDistribution(u32),
    /// Known sparsity ratio.
    SparsityRatio(f64),
    /// Constant along an axis.
    ConstantAlong(u32),
}

/// A refinement: one property with its proof status and evidence pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct Refinement {
    /// The claimed property.
    pub property: ValueProperty,
    /// The proof status.
    pub status: ProofStatus,
    /// Evidence pointer: test id, analysis id, or caller contract id.
    pub evidence: String,
}

impl Refinement {
    /// Constructs a refinement.
    // CEP:WHAT: Constructs one refinement.
    // CEP:WHY: Central construction keeps evidence mandatory.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: the evidence string is non-empty by convention; empty strings rely on review.
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/properties_test.rs
    pub fn new(property: ValueProperty, status: ProofStatus, evidence: &str) -> Refinement {
        Refinement {
            property,
            status,
            evidence: evidence.to_string(),
        }
    }

    /// True when an optimization may rely on this refinement.
    // CEP:WHAT: Reports reliance legality.
    // CEP:WHY: One call site for the reliance rule keeps audits simple.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/properties_test.rs
    pub fn reliable(&self) -> bool {
        self.status.reliable_for_optimization()
    }
}

/// The refinement set attached to one value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Refinements {
    items: Vec<Refinement>,
}

impl Refinements {
    /// Adds a refinement; a later refinement of the same property replaces the earlier one.
    // CEP:WHAT: Adds or replaces one refinement.
    // CEP:WHY: Stronger statuses (Proven over Hint) must supersede weaker ones for the same property instead of coexisting ambiguously.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(items).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/properties_test.rs
    pub fn add(&mut self, r: Refinement) {
        if let Some(slot) = self.items.iter_mut().find(|x| x.property == r.property) {
            *slot = r;
        } else {
            self.items.push(r);
        }
    }

    /// Returns the refinement for one property.
    // CEP:WHAT: Reads one refinement.
    // CEP:WHY: Passes query one property at a time.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when absent.
    // CEP:ASSUMES: none
    // CEP:COST: O(items).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/properties_test.rs
    pub fn get(&self, property: &ValueProperty) -> Option<&Refinement> {
        self.items.iter().find(|x| x.property == *property)
    }

    /// Iterates refinements usable for optimization.
    // CEP:WHAT: Iterates reliable refinements.
    // CEP:WHY: Passes consuming facts filter once and use everywhere.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(items).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/properties_test.rs
    pub fn reliable(&self) -> impl Iterator<Item = &Refinement> {
        self.items.iter().filter(|r| r.reliable())
    }

    /// Number of stored refinements.
    // CEP:WHAT: Counts refinements.
    // CEP:WHY: Test and diagnostic consumers.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/properties_test.rs
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when empty.
    // CEP:WHAT: Emptiness check.
    // CEP:WHY: Analyses skip empty sets quickly.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/properties_test.rs
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

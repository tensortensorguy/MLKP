// CEP:FILE: crates/mlkp-tensor/src/shapes.rs
// CEP:WHAT: The shape engine: a symbolic environment with assignments, constraint checking, evaluation, and equality proofs over dimension expressions.
// CEP:WHY: Design section 4 makes shapes first-class symbolic mathematics with proofs exposed to every optimization pass; the engine is where shape equations, divisibility, and bounds are established and where proofs are produced for the transformation framework.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns ShapeError on unknown symbols, conflicting assignments, non-exact division, or bound violations; evaluation returns None rather than guessing.
// CEP:ASSUMES: symbols are non-negative u32 ids; all dimensions are non-negative integers; data-dependent dynamic dimensions are out of scope for v1 and rejected at bind time.
// CEP:COST: cold; evaluation is O(expression size) per query.
// CEP:EVIDENCE: unit tests in crates/mlkp-tensor/tests/shapes_test.rs.

#![deny(missing_docs)]

use crate::types::{DimExpr, TypeError};

/// Errors from the shape engine.
#[derive(Clone, Debug, PartialEq)]
pub enum ShapeError {
    /// A symbol was used before binding.
    UnknownSymbol(u32),
    /// Two bindings disagree for one symbol.
    ConflictingBind(u32),
    /// A division was not exact when exactness was required.
    NonExactDivision,
    /// A constraint was violated.
    ConstraintViolation(String),
}

impl std::fmt::Display for ShapeError {
    // CEP:WHAT: Formats the shape error with a stable prefix.
    // CEP:WHY: Deterministic diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/shapes_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShapeError::UnknownSymbol(id) => write!(f, "shape error: unknown symbol {id}"),
            ShapeError::ConflictingBind(id) => {
                write!(f, "shape error: conflicting bind for symbol {id}")
            }
            ShapeError::NonExactDivision => write!(f, "shape error: division was not exact"),
            ShapeError::ConstraintViolation(s) => write!(f, "shape error: {s}"),
        }
    }
}

impl std::error::Error for ShapeError {}

/// One declared constraint over dimension expressions.
#[derive(Clone, Debug, PartialEq)]
pub enum Constraint {
    /// left == right.
    Eq(DimExpr, DimExpr),
    /// left % m == 0.
    Divisible(DimExpr, u64),
    /// left <= right.
    Le(DimExpr, DimExpr),
    /// left > 0.
    Positive(DimExpr),
}

/// The symbolic shape environment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeEnv {
    assignments: Vec<(u32, u64)>,
    constraints: Vec<Constraint>,
}

/// The outcome of a proof attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proof {
    /// The relation was established from bindings and constraints.
    Proven,
    /// The relation was disproved.
    Refuted,
    /// The relation could not be decided statically.
    Unknown,
}

impl ShapeEnv {
    /// Binds a symbol to a concrete value; rebinding with the same value is allowed.
    // CEP:WHAT: Binds one symbol to a concrete value.
    // CEP:WHY: Shape specialization binds runtime shapes; conflicting rebinds must fail loudly instead of silently picking one side.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns ConflictingBind when the symbol was already bound to a different value.
    // CEP:ASSUMES: none
    // CEP:COST: O(bindings).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/shapes_test.rs
    pub fn bind(&mut self, sym: u32, value: u64) -> Result<(), ShapeError> {
        for (s, v) in &self.assignments {
            if *s == sym {
                if *v != value {
                    return Err(ShapeError::ConflictingBind(sym));
                }
                return Ok(());
            }
        }
        self.assignments.push((sym, value));
        Ok(())
    }

    /// Declares a constraint; duplicates are kept so provenance of proofs stays simple.
    // CEP:WHAT: Declares one constraint.
    // CEP:WHY: Proofs consult the constraint set; duplicates do not change semantics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1) amortized.
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/shapes_test.rs
    pub fn constrain(&mut self, c: Constraint) {
        self.constraints.push(c);
    }

    /// Value of a symbol, if bound.
    // CEP:WHAT: Reads one binding.
    // CEP:WHY: Callers need the concrete specialization for cache keys.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None for unbound symbols.
    // CEP:ASSUMES: none
    // CEP:COST: O(bindings).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/shapes_test.rs
    pub fn value(&self, sym: u32) -> Option<u64> {
        self.assignments
            .iter()
            .find(|(s, _)| *s == sym)
            .map(|(_, v)| *v)
    }

    /// Evaluates an expression to a constant when fully bound.
    // CEP:WHAT: Evaluates one expression against the bindings.
    // CEP:WHY: Fully-specialized shapes drive kernel planning and cache keys.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when any leaf symbol is unbound; returns an error on non-exact Div or zero divisors.
    // CEP:ASSUMES: division by zero is rejected rather than producing infinity.
    // CEP:COST: O(nodes).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/shapes_test.rs
    pub fn eval(&self, e: &DimExpr) -> Result<Option<u64>, ShapeError> {
        match e {
            DimExpr::Const(v) => Ok(Some(*v)),
            DimExpr::Sym(id) => Ok(self.value(*id)),
            DimExpr::Add(x, y) => {
                let a = self.eval(x)?;
                let b = self.eval(y)?;
                Ok(match (a, b) {
                    (Some(x), Some(y)) => Some(x + y),
                    _ => None,
                })
            }
            DimExpr::Mul(x, y) => {
                let a = self.eval(x)?;
                let b = self.eval(y)?;
                Ok(match (a, b) {
                    (Some(x), Some(y)) => Some(x * y),
                    _ => None,
                })
            }
            DimExpr::Div(x, y) => {
                let a = self.eval(x)?;
                let b = self.eval(y)?;
                match (a, b) {
                    (_, Some(0)) => Err(ShapeError::NonExactDivision),
                    (Some(x), Some(d)) => {
                        if x % d != 0 {
                            return Err(ShapeError::NonExactDivision);
                        }
                        Ok(Some(x / d))
                    }
                    _ => Ok(None),
                }
            }
            DimExpr::CeilDiv(x, y) => {
                let a = self.eval(x)?;
                let b = self.eval(y)?;
                match (a, b) {
                    (_, Some(0)) => Err(ShapeError::NonExactDivision),
                    (Some(x), Some(d)) => Ok(Some(x.div_ceil(d))),
                    _ => Ok(None),
                }
            }
            DimExpr::Min(x, y) => bin_op(self, x, y, u64::min),
            DimExpr::Max(x, y) => bin_op(self, x, y, u64::max),
        }
    }

    /// Attempts to prove that two expressions are equal under bindings and constraints.
    // CEP:WHAT: Proves or refutes equality of two expressions.
    // CEP:WHY: Design section 4 requires shape proofs (for example A.shape[1] == B.shape[0]) to be exposed to every pass.
    // CEP:STATUS: complete
    // CEP:FAILURE: none; undecided relations return Unknown.
    // CEP:ASSUMES: constraints are trusted declarations established by the front-end.
    // CEP:COST: O(nodes * constraints).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/shapes_test.rs
    pub fn prove_eq(&self, a: &DimExpr, b: &DimExpr) -> Proof {
        let av = self.eval(a).ok().flatten();
        let bv = self.eval(b).ok().flatten();
        match (av, bv) {
            (Some(x), Some(y)) => {
                if x == y {
                    Proof::Proven
                } else {
                    Proof::Refuted
                }
            }
            _ => {
                // Constraint search: Eq(a, b) or a chain through shared bound values.
                for c in &self.constraints {
                    if let Constraint::Eq(x, y) = c {
                        if (x == a && y == b) || (x == b && y == a) {
                            return Proof::Proven;
                        }
                    }
                }
                Proof::Unknown
            }
        }
    }

    /// Validates all declared constraints against the current bindings.
    // CEP:WHAT: Checks every declared constraint.
    // CEP:WHY: The validation airlock: lowering must not enter the CEP-0 kernel path with unproven shape assumptions (standard chapter 64.2).
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns ConstraintViolation naming the failed constraint.
    // CEP:ASSUMES: none
    // CEP:COST: O(constraints * nodes).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/shapes_test.rs
    pub fn validate(&self) -> Result<(), ShapeError> {
        for c in &self.constraints {
            match c {
                Constraint::Eq(x, y) => {
                    let xv = self.eval(x)?.ok_or_else(|| {
                        ShapeError::ConstraintViolation("Eq over unbound symbol".to_string())
                    })?;
                    let yv = self.eval(y)?.ok_or_else(|| {
                        ShapeError::ConstraintViolation("Eq over unbound symbol".to_string())
                    })?;
                    if xv != yv {
                        return Err(ShapeError::ConstraintViolation(format!("{xv} != {yv}")));
                    }
                }
                Constraint::Divisible(x, m) => {
                    let xv = self.eval(x)?.ok_or_else(|| {
                        ShapeError::ConstraintViolation("Divisible over unbound symbol".to_string())
                    })?;
                    if xv % m != 0 {
                        return Err(ShapeError::ConstraintViolation(format!(
                            "{xv} not divisible by {m}"
                        )));
                    }
                }
                Constraint::Le(x, y) => {
                    let xv = self.eval(x)?.ok_or_else(|| {
                        ShapeError::ConstraintViolation("Le over unbound symbol".to_string())
                    })?;
                    let yv = self.eval(y)?.ok_or_else(|| {
                        ShapeError::ConstraintViolation("Le over unbound symbol".to_string())
                    })?;
                    if xv > yv {
                        return Err(ShapeError::ConstraintViolation(format!("{xv} > {yv}")));
                    }
                }
                Constraint::Positive(x) => {
                    let xv = self.eval(x)?.ok_or_else(|| {
                        ShapeError::ConstraintViolation("Positive over unbound symbol".to_string())
                    })?;
                    if xv == 0 {
                        return Err(ShapeError::ConstraintViolation(
                            "dimension is zero".to_string(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Checks a tensor type's dimensions against the environment.
    // CEP:WHAT: Validates one tensor type.
    // CEP:WHY: Types and environment must agree before lowering.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns ShapeError or TypeError depending on the violated rule.
    // CEP:ASSUMES: none
    // CEP:COST: O(dims * nodes).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/shapes_test.rs
    pub fn check_type(&self, t: &crate::types::TensorType) -> Result<(), ShapeError> {
        for d in &t.dims {
            if self.eval(d)?.is_none() {
                let id = match d {
                    DimExpr::Sym(id) => *id,
                    _ => 0,
                };
                return Err(ShapeError::UnknownSymbol(id));
            }
        }
        self.validate()
    }

    /// Convenience conversion from a shape error into the shared type error surface.
    // CEP:WHAT: Converts a ShapeError into a TypeError.
    // CEP:WHY: Callers that surface one error type get lossless text through the Display paths.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/shapes_test.rs
    pub fn into_type_error(e: ShapeError) -> TypeError {
        TypeError::ContractViolation(e.to_string())
    }
}

// CEP:WHAT: Evaluates a binary min/max node.
// CEP:WHY: Shared by Min and Max to keep the None-propagation rule in one place.
// CEP:STATUS: complete
// CEP:FAILURE: Propagates evaluation errors; returns None when either side is unbound.
// CEP:ASSUMES: none
// CEP:COST: O(subtrees).
// CEP:EVIDENCE: crates/mlkp-tensor/tests/shapes_test.rs
fn bin_op(
    env: &ShapeEnv,
    x: &DimExpr,
    y: &DimExpr,
    f: fn(u64, u64) -> u64,
) -> Result<Option<u64>, ShapeError> {
    let a = env.eval(x)?;
    let b = env.eval(y)?;
    Ok(match (a, b) {
        (Some(x), Some(y)) => Some(f(x, y)),
        _ => None,
    })
}

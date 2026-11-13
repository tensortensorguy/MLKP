// CEP:FILE: crates/mlkp-tensor/src/types.rs
// CEP:WHAT: Element types, numeric contracts as types, and composite tensor kinds for the semantic Tensor IR.
// CEP:WHY: The design's architectural boundary gives the Tensor IR ownership of mathematics only; these types carry precision, rank, and composite structure while physical layout, strides, and buffers remain forbidden here by design.md section 15.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Constructors return TypeError on zero-rank tensors and unknown element kinds; no panics.
// CEP:ASSUMES: element widths follow the IEEE and integer conventions named in the descriptions; quantized types carry their scale explicitly rather than assuming a calibration.
// CEP:COST: cold; type construction is O(1) and runs during lowering only.
// CEP:EVIDENCE: unit tests in crates/mlkp-tensor/tests/tensor_test.rs.

#![deny(missing_docs)]

use std::fmt;

/// Errors produced by the semantic type system.
#[derive(Clone, Debug, PartialEq)]
pub enum TypeError {
    /// A tensor was constructed with no dimensions.
    ZeroRank,
    /// An element-kind string was not recognized.
    UnknownElement(String),
    /// Two shapes that must match were not equal.
    ShapeMismatch(String),
    /// A numeric contract was internally inconsistent.
    ContractViolation(String),
}

impl fmt::Display for TypeError {
    // CEP:WHAT: Formats the type error with a stable prefix.
    // CEP:WHY: Diagnostics must be deterministic and greppable.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/tensor_test.rs
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeError::ZeroRank => write!(f, "type error: tensor has zero dimensions"),
            TypeError::UnknownElement(s) => write!(f, "type error: unknown element kind '{s}'"),
            TypeError::ShapeMismatch(s) => write!(f, "type error: {s}"),
            TypeError::ContractViolation(s) => write!(f, "type error: {s}"),
        }
    }
}

impl std::error::Error for TypeError {}

/// Semantic element kinds. Storage-independent: an f16 tensor accumulating in f32 is
/// expressed by the numeric contract, not by the element type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElemKind {
    /// IEEE binary16.
    F16,
    /// Brain float 16.
    Bf16,
    /// IEEE binary32.
    F32,
    /// IEEE binary64.
    F64,
    /// Signed 8-bit integer.
    I8,
    /// Signed 16-bit integer.
    I16,
    /// Signed 32-bit integer.
    I32,
    /// Signed 64-bit integer.
    I64,
    /// Unsigned 8-bit integer.
    U8,
    /// Unsigned 16-bit integer.
    U16,
    /// Unsigned 32-bit integer.
    U32,
    /// Unsigned 64-bit integer.
    U64,
    /// Boolean predicate.
    Bool,
    /// Complex with f32 components.
    Complex64,
    /// Quantized 8-bit integer with an explicit scale and zero point.
    QI8,
}

impl ElemKind {
    /// Byte width of one element.
    // CEP:WHAT: Returns the byte width of one element.
    // CEP:WHY: Memory estimation for the pruner needs element sizes without physical layout knowledge.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: complex64 occupies two f32 slots; booleans occupy one byte.
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/tensor_test.rs
    pub fn byte_width(self) -> u64 {
        match self {
            ElemKind::F16 | ElemKind::Bf16 => 2,
            ElemKind::F32 => 4,
            ElemKind::F64 => 8,
            ElemKind::I8 | ElemKind::U8 | ElemKind::QI8 | ElemKind::Bool => 1,
            ElemKind::I16 | ElemKind::U16 => 2,
            ElemKind::I32 | ElemKind::U32 => 4,
            ElemKind::I64 | ElemKind::U64 => 8,
            ElemKind::Complex64 => 8,
        }
    }

    /// True when the kind is floating point.
    // CEP:WHAT: Reports whether the kind is floating point.
    // CEP:WHY: Numeric contracts and rounding rules apply only to float domains.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/tensor_test.rs
    pub fn is_float(self) -> bool {
        matches!(
            self,
            ElemKind::F16 | ElemKind::Bf16 | ElemKind::F32 | ElemKind::F64 | ElemKind::Complex64
        )
    }

    /// Parses an element-kind name.
    // CEP:WHAT: Parses an element-kind name from the front-end.
    // CEP:WHY: The text front-end names types with these exact strings.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns TypeError::UnknownElement for unrecognized names.
    // CEP:ASSUMES: none
    // CEP:COST: O(names).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/tensor_test.rs
    pub fn parse(name: &str) -> Result<ElemKind, TypeError> {
        match name {
            "f16" => Ok(ElemKind::F16),
            "bf16" => Ok(ElemKind::Bf16),
            "f32" => Ok(ElemKind::F32),
            "f64" => Ok(ElemKind::F64),
            "i8" => Ok(ElemKind::I8),
            "i16" => Ok(ElemKind::I16),
            "i32" => Ok(ElemKind::I32),
            "i64" => Ok(ElemKind::I64),
            "u8" => Ok(ElemKind::U8),
            "u16" => Ok(ElemKind::U16),
            "u32" => Ok(ElemKind::U32),
            "u64" => Ok(ElemKind::U64),
            "bool" => Ok(ElemKind::Bool),
            "complex64" => Ok(ElemKind::Complex64),
            "qi8" => Ok(ElemKind::QI8),
            other => Err(TypeError::UnknownElement(other.to_string())),
        }
    }
}

/// Composite tensor kinds. These are semantic; the physical sparse encodings are the
/// heterogeneous IR's responsibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositeKind {
    /// Fully dense tensor.
    Dense,
    /// Semantically sparse; explicit zero knowledge is a refinement, not storage.
    Sparse,
    /// Block sparse with a semantic block shape carried in the sparsity refinement.
    BlockSparse,
    /// Variable-length rows.
    Ragged,
    /// Segments with known boundaries semantics.
    Segmented,
    /// Elementwise mask over another tensor.
    Masked,
    /// Nested tensor of tensors.
    Nested,
    /// Optional tensor: either present with its shape or absent.
    Optional,
}

/// A semantic tensor type: element kind plus one dimension per axis.
#[derive(Clone, Debug, PartialEq)]
pub struct TensorType {
    /// Element kind.
    pub elem: ElemKind,
    /// Composite kind.
    pub composite: CompositeKind,
    /// Dimensions; entries are symbolic dimension ids resolved by the shape engine.
    pub dims: Vec<DimExpr>,
}

impl TensorType {
    /// Constructs a dense tensor type, rejecting zero rank.
    // CEP:WHAT: Constructs a dense tensor type.
    // CEP:WHY: Zero-rank tensors are outside the supported semantic surface for v1 and must fail loudly rather than silently propagate.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns TypeError::ZeroRank for empty dimensions.
    // CEP:ASSUMES: none
    // CEP:COST: O(dims).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/tensor_test.rs
    pub fn dense(elem: ElemKind, dims: Vec<DimExpr>) -> Result<TensorType, TypeError> {
        if dims.is_empty() {
            return Err(TypeError::ZeroRank);
        }
        Ok(TensorType {
            elem,
            composite: CompositeKind::Dense,
            dims,
        })
    }

    /// Constructs a tensor type with an explicit composite kind.
    // CEP:WHAT: Constructs a tensor type with a composite kind.
    // CEP:WHY: Sparse and masked semantics are declared at the type level per design section 9.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns TypeError::ZeroRank for empty dimensions.
    // CEP:ASSUMES: none
    // CEP:COST: O(dims).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/tensor_test.rs
    pub fn composite(
        elem: ElemKind,
        composite: CompositeKind,
        dims: Vec<DimExpr>,
    ) -> Result<TensorType, TypeError> {
        if dims.is_empty() {
            return Err(TypeError::ZeroRank);
        }
        Ok(TensorType {
            elem,
            composite,
            dims,
        })
    }

    /// Rank of the tensor.
    // CEP:WHAT: Returns the rank.
    // CEP:WHY: Shape checks and indexing maps need the rank everywhere.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/tensor_test.rs
    pub fn rank(&self) -> usize {
        self.dims.len()
    }

    /// Structural hash seed over element kind, composite kind, and dimension expressions.
    // CEP:WHAT: Folds the type into a 64-bit structural fingerprint.
    // CEP:WHY: Subgraph hashing includes operand types; this is the single folding site.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(dims).
    // CEP:EVIDENCE: crates/mlkp-hir tests
    pub fn hash64(&self, seed: u64) -> u64 {
        let mut h = cep_hash::Hasher::seeded(seed);
        h.update_u64(match self.elem {
            ElemKind::F16 => 1,
            ElemKind::Bf16 => 2,
            ElemKind::F32 => 3,
            ElemKind::F64 => 4,
            ElemKind::I8 => 5,
            ElemKind::I16 => 6,
            ElemKind::I32 => 7,
            ElemKind::I64 => 8,
            ElemKind::U8 => 9,
            ElemKind::U16 => 10,
            ElemKind::U32 => 11,
            ElemKind::U64 => 12,
            ElemKind::Bool => 13,
            ElemKind::Complex64 => 14,
            ElemKind::QI8 => 15,
        });
        h.update_u64(match self.composite {
            CompositeKind::Dense => 1,
            CompositeKind::Sparse => 2,
            CompositeKind::BlockSparse => 3,
            CompositeKind::Ragged => 4,
            CompositeKind::Segmented => 5,
            CompositeKind::Masked => 6,
            CompositeKind::Nested => 7,
            CompositeKind::Optional => 8,
        });
        for d in &self.dims {
            d.hash_into(&mut h);
        }
        h.finish()
    }
}

/// A dimension expression: either a constant, a symbolic dimension, or a shape operation.
/// This is the semantic side of the shape engine's algebra.
#[derive(Clone, Debug, PartialEq)]
pub enum DimExpr {
    /// A known constant.
    Const(u64),
    /// A symbolic dimension bound in the shape environment.
    Sym(u32),
    /// Sum of two expressions.
    Add(Box<DimExpr>, Box<DimExpr>),
    /// Product of two expressions.
    Mul(Box<DimExpr>, Box<DimExpr>),
    /// Exact division; the shape engine rejects non-exact division when proving.
    Div(Box<DimExpr>, Box<DimExpr>),
    /// Ceiling division.
    CeilDiv(Box<DimExpr>, Box<DimExpr>),
    /// Minimum of two expressions.
    Min(Box<DimExpr>, Box<DimExpr>),
    /// Maximum of two expressions.
    Max(Box<DimExpr>, Box<DimExpr>),
}

impl DimExpr {
    /// Constant shorthand.
    // CEP:WHAT: Shorthand constructor for constants.
    // CEP:WHY: Call sites read better with DimExpr::c(64) than boxed variants.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/tensor_test.rs
    pub fn c(v: u64) -> DimExpr {
        DimExpr::Const(v)
    }

    /// Symbol shorthand.
    // CEP:WHAT: Shorthand constructor for symbols.
    // CEP:WHY: See the constant shorthand.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/tensor_test.rs
    pub fn s(id: u32) -> DimExpr {
        DimExpr::Sym(id)
    }

    /// Boxed addition (named to avoid confusion with `std::ops::Add`).
    // CEP:WHAT: Shorthand for addition.
    // CEP:WHY: Keeps call sites one line.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/tensor_test.rs
    pub fn add_expr(a: DimExpr, b: DimExpr) -> DimExpr {
        DimExpr::Add(Box::new(a), Box::new(b))
    }

    /// Boxed multiplication (named to avoid confusion with `std::ops::Mul`).
    // CEP:WHAT: Shorthand for multiplication.
    // CEP:WHY: Keeps call sites one line.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/tensor_test.rs
    pub fn mul_expr(a: DimExpr, b: DimExpr) -> DimExpr {
        DimExpr::Mul(Box::new(a), Box::new(b))
    }

    /// Boxed ceiling division.
    // CEP:WHAT: Shorthand for ceiling division.
    // CEP:WHY: Tiled shape math uses ceiling division pervasively.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/tensor_test.rs
    pub fn ceildiv(a: DimExpr, b: DimExpr) -> DimExpr {
        DimExpr::CeilDiv(Box::new(a), Box::new(b))
    }

    /// Folds the expression into a hasher.
    // CEP:WHAT: Folds the expression tree into a hasher.
    // CEP:WHY: Type fingerprints include dimension structure.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(nodes).
    // CEP:EVIDENCE: crates/mlkp-tensor/tests/tensor_test.rs
    pub fn hash_into(&self, h: &mut cep_hash::Hasher) {
        match self {
            DimExpr::Const(v) => {
                h.update_u64(1);
                h.update_u64(*v);
            }
            DimExpr::Sym(id) => {
                h.update_u64(2);
                h.update_u64(u64::from(*id));
            }
            DimExpr::Add(x, y) => bin_tag(h, 3, x, y),
            DimExpr::Mul(x, y) => bin_tag(h, 4, x, y),
            DimExpr::Div(x, y) => bin_tag(h, 5, x, y),
            DimExpr::CeilDiv(x, y) => bin_tag(h, 6, x, y),
            DimExpr::Min(x, y) => bin_tag(h, 7, x, y),
            DimExpr::Max(x, y) => bin_tag(h, 8, x, y),
        }
    }
}

// CEP:WHAT: Folds a binary dimension node.
// CEP:WHY: Shared by all binary variants to keep tags and order consistent.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(subtrees).
// CEP:EVIDENCE: crates/mlkp-tensor/tests/tensor_test.rs
fn bin_tag(h: &mut cep_hash::Hasher, tag: u64, x: &DimExpr, y: &DimExpr) {
    h.update_u64(tag);
    x.hash_into(h);
    y.hash_into(h);
}

// CEP:FILE: crates/mlkp-spirv/src/gmir.rs
// CEP:WHAT: The GPU Machine IR: kernel entry points, workgroup geometry, structured blocks with SSA virtual values, and the portable instruction vocabulary over global and workgroup storage.
// CEP:WHY: Design.md's GPU Machine IR table requires execution mapping, indexing, control flow, SSA values, memory spaces, effects, and optional capabilities represented explicitly, while carrying none of the Tensor IR's mathematical semantics and pretending no subgroup is a fixed-width warp.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; construction is total, the emitter validates before serialization.
// CEP:ASSUMES: static workgroup dimensions; only the conservative Vulkan core capability set.
// CEP:COST: cold; runs per kernel candidate.
// CEP:EVIDENCE: golden tests in crates/mlkp-spirv/tests/spirv_test.rs.

#![deny(missing_docs)]

/// SSA value ids inside one kernel (SPIR-V result ids are allocated from these).
pub type VId = u32;

/// GPU value types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GTy {
    /// 32-bit unsigned integer.
    U32,
    /// 32-bit signed integer.
    I32,
    /// 32-bit float.
    F32,
    /// 32-bit boolean.
    Bool,
    /// Void.
    Void,
}

/// GPU instruction opcodes.
#[derive(Clone, Debug, PartialEq)]
pub enum GOpc {
    /// Global invocation id along one axis.
    GlobalId(u32),
    /// Local invocation id along one axis.
    LocalId(u32),
    /// Load one f32 from a storage buffer at a dynamically computed index.
    LoadF32 {
        /// Descriptor binding of the buffer.
        binding: u32,
        /// Index value id.
        index: VId,
    },
    /// Store one f32 to a storage buffer.
    StoreF32 {
        /// Descriptor binding.
        binding: u32,
        /// Index value id.
        index: VId,
        /// Value id.
        value: VId,
    },
    /// f32 addition.
    FAdd(VId, VId),
    /// f32 subtraction.
    FSub(VId, VId),
    /// f32 multiplication.
    FMul(VId, VId),
    /// f32 division.
    FDiv(VId, VId),
    /// u32 addition.
    IAdd(VId, VId),
    /// u32 multiplication.
    IMul(VId, VId),
    /// u32 remainder.
    URem(VId, VId),
    /// u32 division.
    UDiv(VId, VId),
    /// Constant u32.
    ConstU32(u32),
    /// Constant f32.
    ConstF32(f32),
    /// f32 greater-than comparison.
    FGt(VId, VId),
    /// Branch to one of two blocks on a boolean condition.
    BranchCond {
        /// Condition value id.
        cond: VId,
        /// True target.
        true_label: String,
        /// False target.
        false_label: String,
    },
    /// Unconditional branch.
    Branch(String),
    /// Return from the kernel.
    Return,
    /// Workgroup barrier; only legal when every invocation reaches it.
    Barrier,
}

/// One GPU instruction: opcode plus assigned result id when it produces a value.
#[derive(Clone, Debug, PartialEq)]
pub struct GInsn {
    /// Opcode.
    pub opc: GOpc,
    /// Result value id when the instruction produces one.
    pub result: Option<VId>,
}

/// One structured block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GBlock {
    /// Block label.
    pub label: String,
    /// Instructions in order.
    pub insns: Vec<GInsn>,
}

/// One compute kernel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GKernel {
    /// Entry point name.
    pub name: String,
    /// Static workgroup dimensions (x, y, z).
    pub workgroup: [u32; 3],
    /// Blocks; the first is the entry.
    pub blocks: Vec<GBlock>,
    /// Descriptor bindings used: (binding, element count).
    pub bindings: Vec<(u32, u64)>,
}

impl GKernel {
    /// Adds one block.
    // CEP:WHAT: Adds one block.
    // CEP:WHY: Builders accumulate blocks in order.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: labels are unique.
    // CEP:COST: O(1) amortized.
    // CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
    pub fn push_block(&mut self, label: &str) {
        self.blocks.push(GBlock {
            label: label.to_string(),
            insns: Vec::new(),
        });
    }

    /// Appends one instruction to the last block.
    // CEP:WHAT: Appends one instruction.
    // CEP:WHY: Linear builders append to the current block.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: at least one block exists.
    // CEP:COST: O(1) amortized.
    // CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
    pub fn push(&mut self, opc: GOpc, result: Option<VId>) {
        if let Some(b) = self.blocks.last_mut() {
            b.insns.push(GInsn { opc, result });
        }
    }
}

/// Operand kinds used by the emitter's validation (derived from opcodes).
#[derive(Clone, Debug, PartialEq)]
pub enum GOperand {
    /// A value id.
    Value(VId),
    /// A literal u32.
    Literal(u32),
    /// A block label.
    Label(String),
    /// A binding index.
    Binding(u32),
}

// CEP:FILE: crates/mlkp-machine/src/mir.rs
// CEP:WHAT: The Machine IR model: functions of basic blocks of instructions with explicit operands, access modes, register constraints, and effects, plus the target-independent virtual-register form.
// CEP:WHY: The design's machine backend section requires instructions and their constraints represented precisely, with virtual registers typed by register class and effects explicit; the IRC allocator consumes exactly this model.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; construction is total and the verifier validates structural rules.
// CEP:ASSUMES: block and instruction ordering is the program order of a linear CFG with explicit branches.
// CEP:COST: cold; runs during backend compilation.
// CEP:EVIDENCE: unit tests in crates/mlkp-machine/tests/irc_test.rs.

#![deny(missing_docs)]

/// Machine opcodes for the x86-64-ish scalar subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opcode {
    /// Move register to register or immediate to register.
    Mov,
    /// Integer add.
    Add,
    /// Integer subtract.
    Sub,
    /// Integer multiply.
    IMul,
    /// Compare; defines flags.
    Cmp,
    /// Conditional jump on less-than flags.
    Jl,
    /// Conditional jump on greater-or-equal flags.
    Jge,
    /// Unconditional jump.
    Jmp,
    /// Call a function symbol.
    Call,
    /// Return.
    Ret,
    /// Load from memory [base + disp].
    Load,
    /// Store to memory [base + disp].
    Store,
    /// Floating add (scalar, SSE).
    AddSd,
    /// Floating multiply (scalar, SSE).
    MulSd,
}

impl Opcode {
    /// True when the instruction reads memory.
    // CEP:WHAT: Reports memory reads.
    // CEP:WHY: Effects feed the verifier and the scheduler.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn reads_memory(self) -> bool {
        matches!(self, Opcode::Load)
    }

    /// True when the instruction writes memory.
    // CEP:WHAT: Reports memory writes.
    // CEP:WHY: Effects feed the verifier and the scheduler.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn writes_memory(self) -> bool {
        matches!(self, Opcode::Store)
    }

    /// True when the instruction defines FLAGS.
    // CEP:WHAT: Reports flag definitions.
    // CEP:WHY: Cmp defines flags; conditional jumps read them, and the verifier checks ordering.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn defines_flags(self) -> bool {
        matches!(self, Opcode::Cmp)
    }

    /// True when the instruction ends control flow.
    // CEP:WHAT: Reports block terminator status.
    // CEP:WHY: The verifier requires each block to end in exactly one terminator.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn is_terminator(self) -> bool {
        matches!(self, Opcode::Jl | Opcode::Jge | Opcode::Jmp | Opcode::Ret)
    }

    /// True when the instruction is a copy the coalescer may remove.
    // CEP:WHAT: Reports copy status.
    // CEP:WHY: IRC coalescing eliminates removable copies.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn is_copy(self) -> bool {
        matches!(self, Opcode::Mov)
    }
}

/// Register classes of the x86-64-ish target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegClass {
    /// 64-bit general purpose.
    Gpr64,
    /// Scalar double-precision floating point.
    Xmm64,
}

/// One machine operand with its access mode.
#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    /// Virtual register (pre-allocation).
    VReg {
        /// Virtual register id.
        id: u32,
        /// Register class.
        class: RegClass,
        /// True when the operand is defined by this instruction.
        def: bool,
    },
    /// Physical register (post-allocation or fixed constraint).
    PReg {
        /// Physical register id within the target table.
        id: u32,
        /// Register class.
        class: RegClass,
        /// True when defined by this instruction.
        def: bool,
    },
    /// Immediate integer.
    Imm(i64),
    /// Memory reference [base + disp]; base is a virtual or physical register operand id space shared with VReg ids.
    Mem {
        /// Base register (virtual id when below the vreg boundary).
        base: u32,
        /// Displacement.
        disp: i64,
        /// True when the memory is read.
        reads: bool,
        /// True when the memory is written.
        writes: bool,
    },
    /// Block label reference.
    Block(String),
    /// Function symbol reference.
    Symbol(String),
}

impl Operand {
    /// Constructs a vreg use.
    // CEP:WHAT: Constructs a vreg use operand.
    // CEP:WHY: Test and builder ergonomics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn vreg_use(id: u32, class: RegClass) -> Operand {
        Operand::VReg {
            id,
            class,
            def: false,
        }
    }

    /// Constructs a vreg def.
    // CEP:WHAT: Constructs a vreg def operand.
    // CEP:WHY: Test and builder ergonomics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn vreg_def(id: u32, class: RegClass) -> Operand {
        Operand::VReg {
            id,
            class,
            def: true,
        }
    }

    /// Constructs an immediate-to-vreg move instruction.
    // CEP:WHAT: Constructs an immediate load move.
    // CEP:WHY: Rematerialization and test fixtures reuse this shape.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn imm_mov(dst: u32, value: i64) -> MInsn {
        MInsn::new(
            Opcode::Mov,
            vec![Operand::vreg_def(dst, RegClass::Gpr64), Operand::Imm(value)],
        )
    }

    /// The virtual register id when the operand is a vreg.
    // CEP:WHAT: Reads the virtual register id.
    // CEP:WHY: Liveness and interference consume vreg ids.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None for non-vreg operands.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn vreg(&self) -> Option<u32> {
        match self {
            Operand::VReg { id, .. } => Some(*id),
            _ => None,
        }
    }

    /// True when the operand is written by the instruction.
    // CEP:WHAT: Reports definition status.
    // CEP:WHY: Liveness separates uses from defs.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn is_def(&self) -> bool {
        match self {
            Operand::VReg { def, .. } | Operand::PReg { def, .. } => *def,
            Operand::Mem { writes, .. } => *writes,
            _ => false,
        }
    }

    /// True when the operand is read by the instruction.
    // CEP:WHAT: Reports use status.
    // CEP:WHY: Liveness separates uses from defs.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn is_use(&self) -> bool {
        match self {
            Operand::VReg { def, .. } | Operand::PReg { def, .. } => !*def,
            Operand::Mem { reads, .. } => *reads,
            _ => false,
        }
    }
}

/// One machine instruction.
#[derive(Clone, Debug, PartialEq)]
pub struct MInsn {
    /// Opcode.
    pub op: Opcode,
    /// Operands in order.
    pub ops: Vec<Operand>,
    /// Implicit physical register uses (for example RAX for division).
    pub implicit_uses: Vec<u32>,
    /// Implicit physical register definitions.
    pub implicit_defs: Vec<u32>,
    /// Fixed physical register requirement for the vreg operand at this index.
    pub fixed: Vec<(usize, u32)>,
}

impl MInsn {
    /// Builds a minimal instruction with no implicit effects.
    // CEP:WHAT: Builds a minimal instruction.
    // CEP:WHY: Most instructions have no implicit effects.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(operands).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn new(op: Opcode, ops: Vec<Operand>) -> MInsn {
        MInsn {
            op,
            ops,
            implicit_uses: Vec::new(),
            implicit_defs: Vec::new(),
            fixed: Vec::new(),
        }
    }

    /// Uses among the explicit vreg operands.
    // CEP:WHAT: Collects used vregs.
    // CEP:WHY: Liveness construction.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(operands).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn used_vregs(&self) -> Vec<u32> {
        self.ops
            .iter()
            .filter(|o| o.is_use())
            .filter_map(|o| o.vreg())
            .collect()
    }

    /// Defs among the explicit vreg operands.
    // CEP:WHAT: Collects defined vregs.
    // CEP:WHY: Liveness construction.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(operands).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn defined_vregs(&self) -> Vec<u32> {
        self.ops
            .iter()
            .filter(|o| o.is_def())
            .filter_map(|o| o.vreg())
            .collect()
    }
}

/// One basic block.
#[derive(Clone, Debug, PartialEq)]
pub struct MBlock {
    /// Block label.
    pub label: String,
    /// Instructions in program order.
    pub insns: Vec<MInsn>,
    /// Successor block labels.
    pub succs: Vec<String>,
}

/// One machine function in virtual-register (pre-allocation) or allocated form.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MFunction {
    /// Function name.
    pub name: String,
    /// Blocks in order; the first block is the entry.
    pub blocks: Vec<MBlock>,
    /// Virtual register class table by id.
    pub vreg_classes: Vec<(u32, RegClass)>,
}

impl MFunction {
    /// Adds one block.
    // CEP:WHAT: Appends one block.
    // CEP:WHY: Builders accumulate blocks in order.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: labels are unique.
    // CEP:COST: O(1) amortized.
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn push_block(&mut self, label: &str) {
        self.blocks.push(MBlock {
            label: label.to_string(),
            insns: Vec::new(),
            succs: Vec::new(),
        });
    }

    /// Appends one instruction to the last block.
    // CEP:WHAT: Appends one instruction.
    // CEP:WHY: Linear builders append to the current block.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: at least one block exists.
    // CEP:COST: O(1) amortized.
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn push_insn(&mut self, insn: MInsn) {
        if let Some(b) = self.blocks.last_mut() {
            b.insns.push(insn);
        }
    }
}

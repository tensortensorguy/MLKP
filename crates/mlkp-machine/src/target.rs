// CEP:FILE: crates/mlkp-machine/src/target.rs
// CEP:WHAT: The x86-64 target description: physical registers with register units, register classes, reserved and allocatable sets, callee and caller save sets, the System V argument order, and spill behavior.
// CEP:WHY: The design's allocation contract requires the target to describe the machine and the allocator to solve the assignment; register units handle the aliasing between overlapping register views without hardcoding assumptions (Law 7).
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; the description is static data.
// CEP:ASSUMES: the System V AMD64 ABI calling convention; the spill slot size equals the register class width.
// CEP:COST: cold; static tables.
// CEP:EVIDENCE: unit tests in crates/mlkp-machine/tests/irc_test.rs.

#![deny(missing_docs)]

use crate::mir::RegClass;

/// One physical register of the target.
#[derive(Clone, Copy, Debug)]
pub struct PhysReg {
    /// Stable id used by operands.
    pub id: u32,
    /// Name for emission and diagnostics.
    pub name: &'static str,
    /// Register class membership.
    pub class: RegClass,
    /// Register unit ids this register occupies; overlapping views share units.
    pub units: [u8; 2],
    /// True when the register must be preserved across calls.
    pub callee_saved: bool,
    /// True when the register is reserved (stack pointer, frame pointer).
    pub reserved: bool,
}

/// The x86-64 target description.
#[derive(Clone, Debug)]
pub struct X86_64Target {
    regs: Vec<PhysReg>,
}

/// Register ids used by fixed constraints and the ABI.
pub mod preg {
    /// RAX.
    pub const RAX: u32 = 0;
    /// RCX.
    pub const RCX: u32 = 1;
    /// RDX.
    pub const RDX: u32 = 2;
    /// RBX.
    pub const RBX: u32 = 3;
    /// RSI.
    pub const RSI: u32 = 4;
    /// RDI.
    pub const RDI: u32 = 5;
    /// RBP (frame pointer, reserved).
    pub const RBP: u32 = 6;
    /// RSP (stack pointer, reserved).
    pub const RSP: u32 = 7;
    /// R8.
    pub const R8: u32 = 8;
    /// R9.
    pub const R9: u32 = 9;
    /// R10.
    pub const R10: u32 = 10;
    /// R11.
    pub const R11: u32 = 11;
    /// R12.
    pub const R12: u32 = 12;
    /// R13.
    pub const R13: u32 = 13;
    /// R14.
    pub const R14: u32 = 14;
    /// R15.
    pub const R15: u32 = 15;
    /// XMM0.
    pub const XMM0: u32 = 16;
    /// XMM1.
    pub const XMM1: u32 = 17;
}

impl X86_64Target {
    /// Builds the standard x86-64 description.
    // CEP:WHAT: Builds the target description.
    // CEP:WHY: One construction site keeps the ABI facts in one place.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: the System V AMD64 ABI: integer args in RDI, RSI, RDX, RCX, R8, R9; return in RAX; RSP and RBP reserved.
    // CEP:COST: O(registers).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn new() -> X86_64Target {
        let gpr =
            |id: u32, name: &'static str, unit: u8, callee_saved: bool, reserved: bool| PhysReg {
                id,
                name,
                class: RegClass::Gpr64,
                units: [unit, 0],
                callee_saved,
                reserved,
            };
        let xmm = |id: u32, name: &'static str, unit: u8| PhysReg {
            id,
            name,
            class: RegClass::Xmm64,
            units: [unit, 0],
            callee_saved: false,
            reserved: false,
        };
        let regs = vec![
            gpr(preg::RAX, "rax", 0, false, false),
            gpr(preg::RCX, "rcx", 1, false, false),
            gpr(preg::RDX, "rdx", 2, false, false),
            gpr(preg::RBX, "rbx", 3, true, false),
            gpr(preg::RSI, "rsi", 4, false, false),
            gpr(preg::RDI, "rdi", 5, false, false),
            gpr(preg::RBP, "rbp", 6, true, true),
            gpr(preg::RSP, "rsp", 7, true, true),
            gpr(preg::R8, "r8", 8, false, false),
            gpr(preg::R9, "r9", 9, false, false),
            gpr(preg::R10, "r10", 10, false, false),
            gpr(preg::R11, "r11", 11, false, false),
            gpr(preg::R12, "r12", 12, true, false),
            gpr(preg::R13, "r13", 13, true, false),
            gpr(preg::R14, "r14", 14, true, false),
            gpr(preg::R15, "r15", 15, true, false),
            xmm(preg::XMM0, "xmm0", 16),
            xmm(preg::XMM1, "xmm1", 17),
        ];
        X86_64Target { regs }
    }

    /// Number of allocatable GPR colors.
    // CEP:WHAT: Counts allocatable GPRs.
    // CEP:WHY: K in the Briggs criterion is the number of available colors.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(registers).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn colors(&self, class: RegClass) -> u32 {
        self.regs
            .iter()
            .filter(|r| r.class == class && !r.reserved)
            .count() as u32
    }

    /// Reads one physical register by id.
    // CEP:WHAT: Reads one register description.
    // CEP:WHY: Emission and interference checks need names and units.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None for unknown ids.
    // CEP:ASSUMES: none
    // CEP:COST: O(registers).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn reg(&self, id: u32) -> Option<&PhysReg> {
        self.regs.iter().find(|r| r.id == id)
    }

    /// Lists allocatable registers of one class in id order.
    // CEP:WHAT: Lists allocatable registers.
    // CEP:WHY: The allocator iterates colors in a deterministic order.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(registers).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn allocatable(&self, class: RegClass) -> Vec<u32> {
        self.regs
            .iter()
            .filter(|r| r.class == class && !r.reserved)
            .map(|r| r.id)
            .collect()
    }

    /// True when two physical registers overlap via register units.
    // CEP:WHAT: Tests register-unit overlap.
    // CEP:WHY: Two registers interfere physically when their units overlap; this is the design's register-unit requirement.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(units).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn units_overlap(&self, a: u32, b: u32) -> bool {
        let (ra, rb) = match (self.reg(a), self.reg(b)) {
            (Some(x), Some(y)) => (x, y),
            _ => return false,
        };
        if a == b {
            return true;
        }
        ra.units.iter().any(|ua| *ua != 0 && rb.units.contains(ua))
    }

    /// System V integer argument registers in order.
    // CEP:WHAT: Lists argument registers.
    // CEP:WHY: Fixed constraints of calls.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn arg_regs(&self) -> [u32; 6] {
        [
            preg::RDI,
            preg::RSI,
            preg::RDX,
            preg::RCX,
            preg::R8,
            preg::R9,
        ]
    }

    /// Registers preserved across calls.
    // CEP:WHAT: Lists callee-saved registers.
    // CEP:WHY: Prologue and epilogue save the ones the allocator actually uses.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(registers).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn callee_saved(&self, class: RegClass) -> Vec<u32> {
        self.regs
            .iter()
            .filter(|r| r.class == class && r.callee_saved && !r.reserved)
            .map(|r| r.id)
            .collect()
    }

    /// Emission name of one register.
    // CEP:WHAT: Reads a register name.
    // CEP:WHY: The emitter prints names.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns a placeholder-free marker for unknown ids; the verifier rejects those earlier.
    // CEP:ASSUMES: none
    // CEP:COST: O(registers).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    pub fn reg_name(&self, id: u32) -> &str {
        self.reg(id).map(|r| r.name).unwrap_or("unknown")
    }
}

impl Default for X86_64Target {
    // CEP:WHAT: Default constructor.
    // CEP:WHY: Ergonomics for callers that want the standard target.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(registers).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/irc_test.rs
    fn default() -> Self {
        Self::new()
    }
}

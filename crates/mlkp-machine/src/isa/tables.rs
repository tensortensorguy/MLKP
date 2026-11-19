// CEP:FILE: crates/mlkp-machine/src/isa/tables.rs
// CEP:WHAT: The ISA-tier tables: mnemonic to opcode-shape bindings across all integer widths, the four-width register name tables, and the prefix and opcode constants the general decoder and the text assembler share.
// CEP:WHY: The general disassembler and the AT&T assembler must agree on one opcode map so text round-trips through bytes without a form-selection drift (ticket CEP-325).
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Table lookups return Option; callers reject loudly instead of guessing.
// CEP:ASSUMES: the tier covers the scalar integer families (moves, ALU, test, unary group, shifts, multiply, divide, stack, branches, no-ops) and the scalar SSE2 double set, in every operand shape and width.
// CEP:COST: cold; O(1) table entries.
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs.

/// Integer operand width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    /// Byte.
    B,
    /// Word.
    W,
    /// Doubleword.
    L,
    /// Quadword.
    Q,
}

impl Width {
    /// AT&T mnemonic suffix.
    // CEP:WHAT: Returns the suffix character.
    // CEP:WHY: Rendering and parsing share one suffix table.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
    pub fn suffix(self) -> &'static str {
        match self {
            Width::B => "b",
            Width::W => "w",
            Width::L => "l",
            Width::Q => "q",
        }
    }

    /// True when a 66 prefix is needed for this width.
    // CEP:WHAT: Reports the 66 prefix need.
    // CEP:WHY: Word operands switch the default 32-bit operand size.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
    pub fn needs_66(self) -> bool {
        matches!(self, Width::W)
    }

    /// True when REX.W is needed for this width.
    // CEP:WHAT: Reports the REX.W need.
    // CEP:WHY: Quadword operands set the 64-bit operand size.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
    pub fn needs_w(self) -> bool {
        matches!(self, Width::Q)
    }
}

/// ALU operation selector for the 00-3F map and the GRP1 immediates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alu {
    /// Add.
    Add,
    /// Or.
    Or,
    /// And.
    And,
    /// Sub.
    Sub,
    /// Xor.
    Xor,
    /// Compare.
    Cmp,
}

impl Alu {
    /// The base opcode of the operation's eight-byte map row: +0 byte MR,
    /// +1 wide MR, +2 byte RM, +3 wide RM, +4 byte AL-imm8, +5 wide AL-imm32.
    // CEP:WHAT: Returns the map base opcode.
    // CEP:WHY: The ALU map strides by operation; the low three bits select the form.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
    pub fn base(self) -> u8 {
        match self {
            Alu::Add => 0x00,
            Alu::Or => 0x08,
            Alu::And => 0x20,
            Alu::Sub => 0x28,
            Alu::Xor => 0x30,
            Alu::Cmp => 0x38,
        }
    }

    /// The ModRM /ext for the GRP1 immediate forms.
    // CEP:WHAT: Returns the group extension.
    // CEP:WHY: The immediate forms are GRP1 with the operation in the reg field.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
    pub fn ext(self) -> u8 {
        match self {
            Alu::Add => 0,
            Alu::Or => 1,
            Alu::And => 4,
            Alu::Sub => 5,
            Alu::Xor => 6,
            Alu::Cmp => 7,
        }
    }

    /// The accumulator short-form opcodes: (byte imm8, wide imm32).
    // CEP:WHAT: Returns the accumulator short-form opcodes.
    // CEP:WHY: GNU as and the encoder both prefer the short form when the
    // destination is the accumulator.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
    pub fn acc_short(self) -> (u8, u8) {
        match self {
            Alu::Add => (0x04, 0x05),
            Alu::Or => (0x0C, 0x0D),
            Alu::And => (0x24, 0x25),
            Alu::Sub => (0x2C, 0x2D),
            Alu::Xor => (0x34, 0x35),
            Alu::Cmp => (0x3C, 0x3D),
        }
    }

    /// Resolves an ALU operation from the operation's base opcode (opc & 0xF8).
    // CEP:WHAT: Resolves an ALU op from a base opcode.
    // CEP:WHY: The decoder maps the 00-3F family back to the operation.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None for opcodes outside the six operations.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
    pub fn from_base(base: u8) -> Option<Alu> {
        match base {
            0x00 => Some(Alu::Add),
            0x08 => Some(Alu::Or),
            0x20 => Some(Alu::And),
            0x28 => Some(Alu::Sub),
            0x30 => Some(Alu::Xor),
            0x38 => Some(Alu::Cmp),
            _ => None,
        }
    }

    /// Resolves an ALU operation from the GRP1 reg-field extension.
    // CEP:WHAT: Resolves an ALU op from the GRP1 /ext.
    // CEP:WHY: The immediate forms carry the operation in the reg field.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None for adc and sbb (outside the tier).
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
    pub fn from_ext_low(ext: u32) -> Option<Alu> {
        match ext {
            0 => Some(Alu::Add),
            1 => Some(Alu::Or),
            4 => Some(Alu::And),
            5 => Some(Alu::Sub),
            6 => Some(Alu::Xor),
            7 => Some(Alu::Cmp),
            _ => None,
        }
    }
}

/// The GRP3 (F6/F7) unary extensions the tier accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grp3 {
    /// Test immediate (/0).
    Test,
    /// Not (/2).
    Not,
    /// Neg (/3).
    Neg,
    /// Unsigned multiply (/4).
    Mul,
    /// Signed multiply, one operand (/5).
    Imul,
    /// Unsigned divide (/6).
    Div,
    /// Signed divide (/7).
    Idiv,
}

impl Grp3 {
    /// The /ext number.
    // CEP:WHAT: Returns the group extension.
    // CEP:WHY: Encoding and decoding share the extension numbers.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
    pub fn ext(self) -> u8 {
        match self {
            Grp3::Test => 0,
            Grp3::Not => 2,
            Grp3::Neg => 3,
            Grp3::Mul => 4,
            Grp3::Imul => 5,
            Grp3::Div => 6,
            Grp3::Idiv => 7,
        }
    }

    /// Resolves an extension number to the operation.
    // CEP:WHAT: Resolves a GRP3 extension.
    // CEP:WHY: The decoder maps the /ext field back to the operation.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None for /1 and /x beyond the table.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
    pub fn from_ext(ext: u8) -> Option<Grp3> {
        match ext {
            0 => Some(Grp3::Test),
            2 => Some(Grp3::Not),
            3 => Some(Grp3::Neg),
            4 => Some(Grp3::Mul),
            5 => Some(Grp3::Imul),
            6 => Some(Grp3::Div),
            7 => Some(Grp3::Idiv),
            _ => None,
        }
    }
}

/// GPR names for every width, indexed by hardware number (0..15).
pub(crate) const NAMES_B: [&str; 16] = [
    "al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil", "r8b", "r9b", "r10b", "r11b", "r12b",
    "r13b", "r14b", "r15b",
];
/// Word names.
pub(crate) const NAMES_W: [&str; 16] = [
    "ax", "cx", "dx", "bx", "sp", "bp", "si", "di", "r8w", "r9w", "r10w", "r11w", "r12w", "r13w",
    "r14w", "r15w",
];
/// Dword names.
pub(crate) const NAMES_L: [&str; 16] = [
    "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi", "r8d", "r9d", "r10d", "r11d", "r12d",
    "r13d", "r14d", "r15d",
];
/// Quad names.
pub(crate) const NAMES_Q: [&str; 16] = [
    "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13",
    "r14", "r15",
];

/// Returns the register name for a hardware number at a width.
// CEP:WHAT: Names a register at a width.
// CEP:WHY: Rendering and parsing share one name table so text round-trips.
// CEP:STATUS: complete
// CEP:FAILURE: Panics only for numbers above 15; the ISA defines exactly 16 GPRs.
// CEP:ASSUMES: hardware numbers 0..=15.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub fn reg_name(hw: u32, w: Width) -> &'static str {
    match w {
        Width::B => NAMES_B[hw as usize],
        Width::W => NAMES_W[hw as usize],
        Width::L => NAMES_L[hw as usize],
        Width::Q => NAMES_Q[hw as usize],
    }
}

/// Resolves a register name to (hardware number, width).
// CEP:WHAT: Parses a register name.
// CEP:WHY: The assembler maps AT&T names back to hardware numbers.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None for unknown names.
// CEP:ASSUMES: none
// CEP:COST: O(table).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub fn reg_hw(name: &str) -> Option<(u32, Width)> {
    let name = name.strip_prefix('%').unwrap_or(name);
    let tables = [
        (NAMES_Q.as_slice(), Width::Q),
        (NAMES_L.as_slice(), Width::L),
        (NAMES_W.as_slice(), Width::W),
        (NAMES_B.as_slice(), Width::B),
    ];
    for (table, w) in tables {
        if let Some(idx) = table.iter().position(|n| *n == name) {
            return Some((idx as u32, w));
        }
    }
    None
}

/// The sixteen branch conditions in opcode order (0x0 ..= 0xF).
pub(crate) const COND_NAMES: [&str; 16] = [
    "o", "no", "b", "ae", "e", "ne", "be", "a", "s", "ns", "p", "np", "l", "ge", "le", "g",
];

/// Resolves an AT&T condition mnemonic to its opcode number.
// CEP:WHAT: Resolves a condition mnemonic.
// CEP:WHY: Jcc families are one opcode number across rel8 and rel32.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None for non-branch mnemonics.
// CEP:ASSUMES: none
// CEP:COST: O(table).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub fn cond_number(mn: &str) -> Option<u8> {
    COND_NAMES.iter().position(|n| *n == mn).map(|v| v as u8)
}

/// Scalar SSE2 double mnemonics with their (prefix, opcode) pairs.
#[allow(dead_code)]
pub(crate) mod sse {
    /// F2 prefix.
    pub const F2: u8 = 0xF2;
    /// 66 prefix.
    pub const P66: u8 = 0x66;
    /// Escape.
    pub const ESC: u8 = 0x0F;

    /// (mnemonic, prefix, opcode).
    pub const BINOPS: [(&str, u8, u8); 7] = [
        ("addsd", F2, 0x58),
        ("mulsd", F2, 0x59),
        ("subsd", F2, 0x5C),
        ("divsd", F2, 0x5E),
        ("sqrtsd", F2, 0x51),
        ("ucomisd", P66, 0x2E),
        ("comisd", P66, 0x2F),
    ];
    /// movsd load opcode (also the reg-reg move).
    pub const MOVSD_LD: u8 = 0x10;
    /// movsd store opcode.
    pub const MOVSD_ST: u8 = 0x11;
    /// movapd load (reg-reg).
    pub const MOVAPD_LD: u8 = 0x28;
    /// movapd store.
    pub const MOVAPD_ST: u8 = 0x29;
    /// cvtsi2sd.
    pub const CVTSI2SD: u8 = 0x2A;
    /// cvttsd2si.
    pub const CVTTSD2SI: u8 = 0x2C;
}

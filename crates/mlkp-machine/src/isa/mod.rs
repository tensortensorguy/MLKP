// CEP:FILE: crates/mlkp-machine/src/isa/mod.rs
// CEP:WHAT: The general ISA-tier pair: a decoder that turns arbitrary x86-64 machine bytes of the scalar tier into structured instructions, an AT&T renderer for them, and an assembler that parses AT&T text back to bytes with labels, a branch-size fixpoint, and relocations (ticket CEP-325).
// CEP:WHY: The ticket CEP-322 decoder only accepted the encoder's emitted subset; the standard's machine tier ends at a complete encoding surface, so the pair completes it: every operand shape and width of the tier's families, with loud rejection outside the tier.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns IsaError for truncated input, encodings outside the tier (VEX, EVEX, x87, string ops, segment or lock prefixes, rings), malformed text, unknown labels, or an unstable branch fixpoint; nothing is guessed.
// CEP:ASSUMES: the tier is the scalar integer families (move, ALU, test, unary group, shifts, multiply, divide, stack, branch, no-op) plus the scalar SSE2 double set, in every width and operand shape; encodings the tier does not define are rejected, never interpreted.
// CEP:COST: cold; O(bytes) decode, O(insns) assemble per fixpoint round.
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs.

#![deny(missing_docs)]

/// Opcode, prefix, and name tables.
pub mod tables;

use tables::{Alu, Width};

/// One decoded or assembled operand, in hardware numbering.
#[derive(Clone, Debug, PartialEq)]
pub enum IsaOp {
    /// A general-purpose register by hardware number (0..15).
    Gpr(u32),
    /// An xmm register by index (0..15).
    Xmm(u32),
    /// An immediate.
    Imm(i64),
    /// A memory operand: base and index hardware numbers (None when absent),
    /// scale 1/2/4/8, displacement; `rip` marks RIP-relative addressing.
    Mem {
        /// Base register hardware number.
        base: Option<u32>,
        /// Index register hardware number.
        index: Option<u32>,
        /// Scale (1, 2, 4, or 8).
        scale: u8,
        /// Displacement.
        disp: i64,
        /// True for RIP-relative addressing.
        rip: bool,
    },
    /// A label or symbol reference (assembler input only).
    Sym(String),
}

/// A tier mnemonic with its width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsaMn {
    /// mov (all widths, all immediate forms, movabs for the 64-bit imm64).
    Mov,
    /// movzx (0F B6/B7); the payload is the source width.
    MovZx(Width),
    /// movsx (0F BE/BF); the payload is the source width.
    MovSx(Width),
    /// movsxd (0F 63).
    MovSxd,
    /// lea.
    Lea,
    /// The six ALU operations.
    Alu(Alu),
    /// test.
    Test,
    /// not.
    Not,
    /// neg.
    Neg,
    /// mul (one operand, unsigned).
    Mul,
    /// imul (one operand).
    Imul,
    /// div (one operand, unsigned).
    Div,
    /// idiv.
    Idiv,
    /// cqo / cdq (width Q/L).
    Cqo,
    /// shl.
    Shl,
    /// shr.
    Shr,
    /// sar.
    Sar,
    /// imul r, r/m (two operand).
    Imul2,
    /// imul imm, r/m, r (three operand).
    Imul3,
    /// push (register or immediate).
    Push,
    /// pop.
    Pop,
    /// inc (FE/FF /0).
    Inc,
    /// dec (FE/FF /1).
    Dec,
    /// call rel32.
    Call,
    /// jmp.
    Jmp,
    /// jcc with the condition number 0..15.
    Jcc(u8),
    /// ret.
    Ret,
    /// nop (0x90).
    Nop,
    /// movsd (also the xmm reg-reg move).
    MovSd,
    /// movapd.
    MovApd,
    /// addsd.
    AddSd,
    /// subsd.
    SubSd,
    /// mulsd.
    MulSd,
    /// divsd.
    DivSd,
    /// sqrtsd.
    SqrtSd,
    /// ucomisd.
    Ucomisd,
    /// comisd.
    Comisd,
    /// cvtsi2sd.
    CvtSi2Sd,
    /// cvttsd2si.
    Cvttsd2Si,
}

/// One instruction of the tier: mnemonic, width, and operands in AT&T order
/// (source first for two-operand forms, destination last).
#[derive(Clone, Debug, PartialEq)]
pub struct IsaInsn {
    /// Mnemonic.
    pub mn: IsaMn,
    /// Integer operand width (ignored for the SSE double set).
    pub width: Width,
    /// Operands in AT&T order.
    pub ops: Vec<IsaOp>,
    /// Absolute branch target for decoded branches (byte offset from the
    /// instruction end); the assembler stores None and uses Sym instead.
    pub target: Option<i64>,
}

/// Failure modes of the general pair.
#[derive(Clone, Debug, PartialEq)]
pub enum IsaError {
    /// The byte stream ended inside one instruction at this offset.
    Truncated(u32),
    /// The encoding at this offset is outside the tier.
    OutsideTier(u32),
    /// The text failed to parse at this line with this reason.
    Parse(usize, String),
    /// A label or symbol is unknown.
    UnknownLabel(String),
    /// The branch-size fixpoint did not stabilize.
    FixpointUnstable,
}

impl std::fmt::Display for IsaError {
    // CEP:WHAT: Formats the ISA error.
    // CEP:WHY: Deterministic diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IsaError::Truncated(o) => write!(f, "isa error: truncated at {o}"),
            IsaError::OutsideTier(o) => {
                write!(f, "isa error: encoding outside the tier at {o}")
            }
            IsaError::Parse(l, r) => write!(f, "isa error: line {l}: {r}"),
            IsaError::UnknownLabel(s) => write!(f, "isa error: unknown label {s}"),
            IsaError::FixpointUnstable => {
                write!(f, "isa error: branch size fixpoint unstable")
            }
        }
    }
}

/// One relocation the assembler produced: the rel32 or disp32 field at `offset`
/// refers to `symbol` with the PC-relative `addend`.
#[derive(Clone, Debug, PartialEq)]
pub struct IsaReloc {
    /// Byte offset of the relocation field.
    pub offset: u32,
    /// Referenced symbol.
    pub symbol: String,
    /// Addend (-4 for a rel32 call or jump, 0 for other forms).
    pub addend: i64,
}

/// The assembled artifact: bytes, relocations in program order, and label offsets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IsaObject {
    /// Encoded bytes.
    pub text: Vec<u8>,
    /// Relocations in program order.
    pub relocs: Vec<IsaReloc>,
    /// Label byte offsets in program order.
    pub labels: Vec<(u32, String)>,
}

/// Assembled output of one text program.
pub mod assemble;
/// General byte-level decoder and its AT&T renderer.
pub mod decode;
mod escape;
mod render;

/// Assembles AT&T text into bytes with labels and relocations.
pub use assemble::assemble;
/// Decodes a byte slice into tier instructions.
pub use decode::decode_bytes;
/// Renders one instruction as the emitter would.
pub use render::render_insn;

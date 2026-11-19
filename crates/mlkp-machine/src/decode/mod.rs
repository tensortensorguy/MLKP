// CEP:FILE: crates/mlkp-machine/src/decode/mod.rs
// CEP:WHAT: CodeObject disassembly entry: decoded instruction types, the decode walk, branch resolution, and the deterministic AT&T text rendering.
// CEP:WHY: The decoder is the mechanical bytes-versus-text equivalence check of ticket CEP-322; one shared structural grammar keeps decoding identical across the CodeObject path and the general path.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module DecodeError variants; encodings outside the ISA tier are rejected loudly, never guessed at.
// CEP:ASSUMES: the bytes came from encode::compile for the CodeObject path; prefixes arrive in canonical order.
// CEP:COST: cold; O(insns).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs.

pub(crate) mod insn;
pub(crate) mod reader;
pub(crate) mod render;
pub(crate) mod sse;
pub(crate) mod tables;

use self::insn::decode_one;
use self::reader::Reader;
use self::render::render;
use crate::encode::CodeObject;
use crate::mir::Opcode;

/// Decoder failure modes.
#[derive(Clone, Debug, PartialEq)]
pub enum DecodeError {
    /// The byte stream ended inside one instruction.
    Truncated(u32),
    /// The byte sequence at this offset is not in the encodable subset.
    UnknownEncoding(u32),
    /// A branch or internal call resolved to an offset that is no label boundary.
    BadBranchTarget(u32),
}

impl std::fmt::Display for DecodeError {
    // CEP:WHAT: Formats the decode error.
    // CEP:WHY: Deterministic diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Truncated(o) => write!(f, "decode error: truncated at {o}"),
            DecodeError::UnknownEncoding(o) => {
                write!(f, "decode error: encoding outside the subset at {o}")
            }
            DecodeError::BadBranchTarget(o) => {
                write!(f, "decode error: branch lands at {o} which is no label")
            }
        }
    }
}

/// The kind of a decoded instruction: a Machine-IR opcode or a prologue or epilogue register move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DecKind {
    /// A regular instruction with a Machine-IR opcode.
    Insn(Opcode),
    /// Push of the register in `dst`.
    Push,
    /// Pop of the register in `dst`.
    Pop,
}

/// One decoded destination location: a register, a memory operand, or nothing.
#[derive(Clone, Debug, PartialEq)]
pub enum DecDst {
    /// Physical register id from the target table (xmm ids offset by 16).
    Reg(u32),
    /// Memory destination [base + disp] for read-modify-write forms.
    Mem {
        /// Base register id.
        base: u32,
        /// Displacement.
        disp: i64,
    },
    /// No destination operand.
    None,
}

/// One decoded instruction at a known offset.
#[derive(Clone, Debug, PartialEq)]
pub struct DecInsn {
    /// Instruction kind.
    pub kind: DecKind,
    /// Byte offset in the text section.
    pub offset: u32,
    /// Destination location (register, memory, or none).
    pub dst: DecDst,
    /// Operand 1 when present.
    pub src: Option<DecSrc>,
    /// Secondary source operand (the true source of a three-operand multiply).
    pub src2: Option<DecSrc>,
    /// Resolved branch or call target: a label for internal targets, a symbol for external calls.
    pub target: Option<String>,
}

impl DecKind {
    /// The Machine-IR opcode of a regular instruction.
    // CEP:WHAT: Unwraps the instruction kind.
    // CEP:WHY: The renderer keys mnemonic selection off the opcode.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics for Push and Pop, which the renderer handles separately.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
    pub fn into_op(self) -> Opcode {
        match self {
            DecKind::Insn(op) => op,
            _ => panic!("into_op() on a push or pop"),
        }
    }
}

/// One decoded source operand.
#[derive(Clone, Debug, PartialEq)]
pub enum DecSrc {
    /// Physical register id from the target table (xmm ids offset by 16).
    Reg(u32),
    /// Immediate value.
    Imm(i64),
    /// Memory [base + disp].
    Mem {
        /// Base register id.
        base: u32,
        /// Displacement.
        disp: i64,
    },
}

/// Decodes the whole text section into instructions with offsets.
// CEP:WHAT: Decodes a CodeObject.
// CEP:WHY: Evidence hashing and the bytes-versus-text equivalence check.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module errors.
// CEP:ASSUMES: labels cover every branch and internal call target.
// CEP:COST: O(insns).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs.
pub fn decode_insns(co: &CodeObject) -> Result<Vec<DecInsn>, DecodeError> {
    let mut out = Vec::new();
    let mut r = Reader {
        bytes: &co.text,
        pos: 0,
    };
    while r.pos < co.text.len() {
        let start = r.pos as u32;
        let insn = decode_one(&mut r, co, start)?;
        out.push(insn);
    }
    Ok(out)
}

/// Resolves a relative branch to its label.
// CEP:WHAT: Resolves branch targets.
// CEP:WHY: Branches are relative in bytes and symbolic in the IR.
// CEP:STATUS: complete
// CEP:FAILURE: Returns BadBranchTarget when the absolute offset is no label.
// CEP:ASSUMES: none
// CEP:COST: O(labels).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn resolve_branch(
    co: &CodeObject,
    from_end_of_insn: i64,
) -> Result<String, DecodeError> {
    let abs = (from_end_of_insn) as u32;
    co.labels
        .iter()
        .find(|(o, _)| *o == abs)
        .map(|(_, l)| l.clone())
        .ok_or(DecodeError::BadBranchTarget(abs))
}

/// Disassembles one CodeObject into assembly text matching emit() minus its header comments.
// CEP:ID: JIT-x59
// CEP:WHAT: Disassembles one CodeObject.
// CEP:WHY: The standard's disassembly-hash evidence and the mechanical bytes-versus-text equivalence check.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module errors.
// CEP:ASSUMES: labels cover every branch and internal call target; every label offset coincides with an instruction start or the epilogue.
// CEP:COST: O(insns + labels).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs.
pub fn disassemble(
    co: &CodeObject,
    function: &str,
    target: &crate::target::X86_64Target,
) -> Result<String, DecodeError> {
    let insns = decode_insns(co)?;
    let mut out = String::new();
    out.push_str(".text\n");
    out.push_str(&format!(".globl {function}\n{function}:\n"));
    let mut labels = co.labels.clone();
    labels.sort_by_key(|(o, _)| *o);
    let mut li = 0usize;
    for insn in &insns {
        while li < labels.len() && labels[li].0 == insn.offset {
            let (_, name) = &labels[li];
            out.push_str(&format!(".L{name}:\n"));
            li += 1;
        }
        out.push_str(&render(insn, target));
    }
    // Remaining labels sit at or after the final instruction (the epilogue label
    // lands exactly at the ret-bearing epilogue start, already printed above
    // when it coincides; anything left is printed in order).
    while li < labels.len() {
        let (_, name) = &labels[li];
        out.push_str(&format!(".L{name}:\n"));
        li += 1;
    }
    // emit() closes the function with a blank line; match it byte for byte.
    out.push('\n');
    Ok(out)
}

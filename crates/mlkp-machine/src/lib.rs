// CEP:FILE: crates/mlkp-machine/src/lib.rs
// CEP:WHAT: Crate facade of the machine backend: the Machine IR model, the x86-64 target description, liveness and interference, the IRC allocator, assembly emission with allocation verification, byte-level encoding with relocations, a subset disassembler, and ELF64 object emission (ticket CEP-322).
// CEP:WHY: Design.md's machine backend section orders these subsystems explicitly (instruction model, target description, register classes, liveness, interference, IRC, spill management, verification, emission, encoding, relocations, object files); this crate follows that order file by file.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Allocation, encoding, and emission return typed errors; the verifiers return every violation.
// CEP:ASSUMES: the System V AMD64 ABI.
// CEP:COST: cold; runs during backend compilation.
// CEP:EVIDENCE: cargo test -p mlkp-machine.

//! MLKP machine backend.

#![deny(missing_docs)]

/// Disassembler for the encoded subset.
pub mod decode;
/// Assembly emission and allocation verification.
pub mod emit;
/// Byte-level x86-64 encoding with relocations.
pub mod encode;
/// Liveness, interference, and the IRC allocator.
pub mod irc;
/// The general ISA-tier pair: decoder, renderer, assembler (ticket CEP-325).
pub mod isa;
/// The Machine IR model.
pub mod mir;
/// ELF64 relocatable object emission with evidence hashes.
pub mod object;
/// The x86-64 target description.
pub mod target;

pub use decode::{decode_insns, disassemble, DecDst, DecInsn, DecKind, DecSrc, DecodeError};
pub use emit::{emit, verify_allocation, EmitError};
pub use encode::{compile, encode, CodeObject, EncodeError, Reloc};
pub use irc::{
    allocate, block_live_out, build_interference, rewrite_spills, AllocError, Allocation,
    Interference, SpillRewrite,
};
pub use isa::{
    decode_bytes as isa_decode_bytes, render_insn as isa_render_insn, IsaError, IsaInsn, IsaObject,
};
pub use mir::{MBlock, MFunction, MInsn, Opcode, Operand, RegClass};
pub use object::{emit_object, emit_with_evidence, ObjectEvidence};
pub use target::{preg, X86_64Target};

// CEP:FILE: crates/mlkp-machine/src/lib.rs
// CEP:WHAT: Crate facade of the machine backend: the Machine IR model, the x86-64 target description, liveness and interference, the IRC allocator, and assembly emission with allocation verification.
// CEP:WHY: Design.md's machine backend section orders these subsystems explicitly (instruction model, target description, register classes, liveness, interference, IRC, spill management, verification, emission); this crate follows that order file by file.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Allocation and emission return typed errors; the verifier returns every violation.
// CEP:ASSUMES: the System V AMD64 ABI.
// CEP:COST: cold; runs during backend compilation.
// CEP:EVIDENCE: cargo test -p mlkp-machine.

//! MLKP machine backend.

#![deny(missing_docs)]

/// Assembly emission and allocation verification.
pub mod emit;
/// Liveness, interference, and the IRC allocator.
pub mod irc;
/// The Machine IR model.
pub mod mir;
/// The x86-64 target description.
pub mod target;

pub use emit::{emit, verify_allocation, EmitError};
pub use irc::{allocate, block_live_out, build_interference, AllocError, Allocation, Interference};
pub use mir::{MBlock, MFunction, MInsn, Opcode, Operand, RegClass};
pub use target::{preg, X86_64Target};

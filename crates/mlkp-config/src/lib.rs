// CEP:FILE: crates/mlkp-config/src/lib.rs
// CEP:WHAT: Crate facade that exposes the repository-level config directory as two modules.
// CEP:WHY: Chapter 32.11 requires configuration constants to live in the config directory; the #[path] include keeps the files in their mandated location while making them an ordinary compile-time-checked Rust module.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Compilation fails if the included files violate their compile-time assertions.
// CEP:ASSUMES: the crate lives two levels below the repository root; the include path is relative to this file's directory.
// CEP:COST: compile-time only.
// CEP:EVIDENCE: cargo build -p mlkp-config executes the compile-time assertions.

//! Named target and limit constants for the whole compiler. All values live in
//! the repository `config/` directory per standard chapter 32.11.
#![deny(missing_docs)]

/// Target hardware constants (cache line, page size, endianness, pointer width).
#[path = "../../../config/target.rs"]
pub mod target;

/// Pipeline, autotuner, and runtime limit constants.
#[path = "../../../config/limits.rs"]
pub mod limits;

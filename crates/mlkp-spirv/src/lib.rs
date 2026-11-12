// CEP:FILE: crates/mlkp-spirv/src/lib.rs
// CEP:WHAT: Crate facade of the GPU backend's code-generation tier: the GPU Machine IR vocabulary, the deterministic native SPIR-V binary emitter, the module verifier, and the kernel manifest that defines the host/device ABI.
// CEP:WHY: Design.md's GPU backend decision gives the compiler ownership through SPIR-V generation with no external compiler dependency; the driver owns target-specific lowering, so this crate's contract is verified, deterministic modules plus an explicit manifest instead of reverse-engineering.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Emission returns EmitError on invalid control flow or unlowerable ops; the module verifier rejects invalid output before it reaches a driver.
// CEP:ASSUMES: Vulkan Compute with SPIR-V 1.3 (Vulkan 1.1 baseline); only the conservative core capability set is emitted.
// CEP:COST: cold; emission runs per kernel candidate.
// CEP:EVIDENCE: cargo test -p mlkp-spirv with golden word tests.

//! MLKP native SPIR-V emitter.

#![deny(missing_docs)]

/// The binary emitter and the module verifier.
pub mod emit;
/// The GPU Machine IR vocabulary and kernel model.
pub mod gmir;
/// The checked-in SPIR-V grammar subset.
pub mod grammar;
/// The kernel manifest: the host/device ABI contract.
pub mod manifest;

pub use emit::{emit_module, verify_module, EmitError, SpirvModule};
pub use gmir::{GBlock, GInsn, GKernel, GOpc, GOperand, GTy};
pub use manifest::{KernelBinding, KernelManifest, WorkgroupDims};

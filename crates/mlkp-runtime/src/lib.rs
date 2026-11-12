// CEP:FILE: crates/mlkp-runtime/src/lib.rs
// CEP:WHAT: Crate facade of the execution layer: CEP-0 kernels, kernel plans with the validation airlock, the buffer arena, plan execution, the device probe, and the measurement discipline.
// CEP:WHY: The design's autotuner measures real executions of real schedules; this crate is the only place execution cost is produced, and it separates the cold shell and airlock from the cycle-exact kernel file.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Plan and execution errors are typed; the kernel file itself never fails.
// CEP:ASSUMES: none
// CEP:COST: cold shell plus measured hot kernels.
// CEP:EVIDENCE: cargo test -p mlkp-runtime and benches/artifacts/.

//! MLKP runtime: plans, buffers, kernels, probe, timing.

#![deny(missing_docs)]

/// The preallocated buffer arena.
pub mod buffers;
/// Plan execution shell.
pub mod exec;
/// The cycle-exact kernel file.
pub mod kernels;
/// Kernel plans and the schedule configuration.
pub mod plan;
/// The device probe.
pub mod probe;
/// The measurement discipline.
pub mod timing;

pub use buffers::{AllocError, Arena};
pub use exec::{execute, ExecError};
pub use kernels::{EwBinary, EwUnary, ReduceKind, Tile};
pub use plan::{KernelPlan, PassConfig, PlanError, PlanOp};
pub use probe::{probe, DeviceProbe};
pub use timing::{measure, Measurement};

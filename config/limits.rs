// CEP:FILE: config/limits.rs
// CEP:WHAT: Named limit, threshold, and tuning constants for the compiler pipeline, the autotuner, and the runtime.
// CEP:WHY: The design document names budgets (30 second first compile, 5 second recompile), measurement disciplines (warmup, repetition), and pruner behavior (similarity epsilon); naming them in one configuration file keeps the design and the code synchronized and gives every knob a single auditable home.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Compile-time assertions fire if limits are mutually inconsistent; no runtime failure path.
// CEP:ASSUMES: Defaults are chosen for a workstation-class host; the CLI exposes overrides and every override flows through these named types.
// CEP:COST: compile-time only.
// CEP:EVIDENCE: design.md sections VI, VII, and XII define the budget and measurement requirements these constants implement.

/// Maximum accepted front-end source file size in bytes.
pub const MAX_FRONTEND_FILE_BYTES: usize = 1024 * 1024;

/// Maximum statements per front-end function.
pub const MAX_FRONTEND_STATEMENTS: usize = 4096;

/// Maximum identifier length accepted by the front-end parser.
pub const MAX_FRONTEND_IDENT_BYTES: usize = 64;

/// Execution count at which the profiler declares a path hot and triggers the JIT pipeline.
pub const HOT_PATH_EXEC_COUNT: u64 = 8;

/// Default wall-clock budget for a cold autotuning session, in milliseconds.
pub const COLD_TIME_BUDGET_MS: u64 = 30_000;

/// Default wall-clock budget for a warm-started recompilation, in milliseconds.
pub const WARM_TIME_BUDGET_MS: u64 = 5_000;

/// Fraction of the total budget allocated to Phase 1 broad sampling.
pub const PHASE1_BUDGET_FRACTION: f64 = 0.25;

/// Fraction of the total budget allocated to Phase 2 Bayesian optimization.
pub const PHASE2_BUDGET_FRACTION: f64 = 0.40;

/// Fraction of the total budget allocated to Phase 3 evolutionary refinement.
pub const PHASE3_BUDGET_FRACTION: f64 = 0.25;

/// Phase 4 receives the remaining budget after phases 1 through 3.
pub const PHASE4_BUDGET_FRACTION_REST: f64 = 0.10;

/// Phase 1 probe count: the diverse sample evaluated before model-guided phases.
pub const PHASE1_PROBE_COUNT: usize = 20;

/// Phase 2 batch size ranked and evaluated per acquisition round.
pub const PHASE2_BATCH_SIZE: usize = 8;

/// Multiplier on the cost model's residual standard deviation used as the
/// lower-confidence-bound exploration bonus of the Phase 2 acquisition.
pub const PHASE2_EXPLORATION_BONUS: f64 = 1.0;

/// Mutant count generated around the incumbent in Phase 3.
pub const PHASE3_MUTANT_COUNT: usize = 24;

/// Candidates re-measured with the extended run count in Phase 4.
pub const PHASE4_TOP_COUNT: usize = 5;

/// Warmup runs discarded from each measurement series.
pub const MEASUREMENT_WARMUP_RUNS: u32 = 3;

/// Timed runs per candidate during search phases.
pub const MEASUREMENT_RUNS: u32 = 9;

/// Timed runs per candidate during the final polish phase.
pub const POLISH_MEASUREMENT_RUNS: u32 = 60;

/// Confidence level used by the final-polish t-test.
pub const POLISH_CONFIDENCE_LEVEL: f64 = 0.95;

/// Relative tolerance for the shadow correctness check against the interpreter baseline.
pub const CORRECTNESS_REL_TOLERANCE: f64 = 1e-4;

/// Absolute tolerance for the shadow correctness check.
pub const CORRECTNESS_ABS_TOLERANCE: f64 = 1e-3;

/// Similarity distance below which a candidate inherits an evaluated neighbor's interpolated result.
pub const PRUNER_SIMILARITY_EPSILON: f64 = 0.05;

/// Quantile of cost-model predictions below which candidates are pruned as hopeless.
pub const COST_MODEL_PRUNE_QUANTILE: f64 = 0.85;

/// Minimum Pearson correlation the cost model must maintain before it may prune.
pub const COST_MODEL_MIN_CORRELATION: f64 = 0.6;

/// Minimum evaluated samples before the cost model is trusted for pruning.
pub const COST_MODEL_MIN_SAMPLES: usize = 24;

/// Drift threshold on mean measured latency ratio that triggers re-autotuning.
pub const PGO_DRIFT_RATIO: f64 = 1.25;

/// Hard cap on concurrent evaluation threads.
pub const MAX_EVAL_THREADS: usize = 16;

/// Hard cap on candidate evaluations per autotuning session.
pub const MAX_SESSION_CANDIDATES: usize = 400;

/// Early-stop margin: a candidate slower than the incumbent by this factor is aborted mid-measurement.
pub const EARLY_STOP_FACTOR: f64 = 2.0;

/// Maximum machine functions per compilation unit passed to the register allocator.
pub const MAX_MIR_FUNCTIONS: usize = 64;

/// Maximum SPIR-V id bound accepted by the module verifier.
pub const MAX_SPIRV_ID_BOUND: u32 = 1_000_000;

/// Workspace preallocation for the runtime arena, in bytes.
pub const ARENA_DEFAULT_BYTES: usize = 256 * 1024 * 1024;

/// Workspace preallocation for candidate evaluation arenas, in f32 elements.
/// Sized for the largest candidate plan of the benchmark problems.
pub const ARENA_EVAL_ELEMENTS: u64 = 64 * 1024 * 1024;

const _: () = assert!(
    PHASE1_BUDGET_FRACTION
        + PHASE2_BUDGET_FRACTION
        + PHASE3_BUDGET_FRACTION
        + PHASE4_BUDGET_FRACTION_REST
        <= 1.0
);
const _: () = assert!(MEASUREMENT_WARMUP_RUNS < MEASUREMENT_RUNS);
const _: () = assert!(MAX_EVAL_THREADS >= 1);
const _: () = assert!(HOT_PATH_EXEC_COUNT >= 1);

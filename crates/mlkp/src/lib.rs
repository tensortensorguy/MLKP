// CEP:FILE: crates/mlkp/src/lib.rs
// CEP:WHAT: The MLKP compiler facade: loads a program through the front-end, verifies and passes the heterogeneous graph, runs the JIT autotune lifecycle, and installs the winning plan for execution.
// CEP:WHY: The design's pipeline (Tensor IR, heterogeneous IR, plans) is one product through this facade; the CLI and the integration tests exercise exactly this path.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns CompilerError with the pipeline stage that failed.
// CEP:ASSUMES: none
// CEP:COST: cold compilation bounded by budget; execution is the measured hot path.
// CEP:EVIDENCE: integration test in crates/mlkp/tests/lifecycle_test.rs.

//! MLKP compiler facade.

#![deny(missing_docs)]

/// Compilation failure with the pipeline stage named.
#[derive(Clone, Debug, PartialEq)]
pub enum CompilerError {
    /// The front-end rejected the source.
    Parse(String),
    /// The heterogeneous graph failed verification.
    Verify(String),
    /// The autotune lifecycle failed.
    Compile(String),
}

impl std::fmt::Display for CompilerError {
    // CEP:WHAT: Formats the compiler error.
    // CEP:WHY: Deterministic diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompilerError::Parse(s) => write!(f, "parse: {s}"),
            CompilerError::Verify(s) => write!(f, "verify: {s}"),
            CompilerError::Compile(s) => write!(f, "compile: {s}"),
        }
    }
}

impl std::error::Error for CompilerError {}

/// A compiled program ready to execute.
#[derive(Clone, Debug)]
pub struct Compiled {
    /// The program name.
    pub name: String,
    /// The winning plan.
    pub plan: mlkp_runtime::KernelPlan,
    /// The winning config hash.
    pub config_hash: u64,
    /// The measured winner latency.
    pub latency_ns: u64,
    /// Whether the compilation was served from cache.
    pub from_cache: bool,
    /// Buffer staging offsets: input a, input b, output.
    pub staging: (u64, u64, u64),
}

/// Compiles one program end to end: parse, verify, autotune.
// CEP:WHAT: Compiles one program.
// CEP:WHY: The single entry point for the CLI and the integration suite; the autotuner applies each candidate's pass pipeline over the verified source graph, so pass selection is a real search dimension.
// CEP:STATUS: complete
// CEP:FAILURE: Returns CompilerError per the pipeline stage.
// CEP:ASSUMES: none
// CEP:COST: bounded by the autotune budget.
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
pub fn compile(
    source: &str,
    pgo: &mut mlkp_autotune::PgoStore,
    budget: std::time::Duration,
    inputs: (Vec<f32>, Vec<f32>),
) -> Result<Compiled, CompilerError> {
    let program =
        mlkp_front::parse_program(source).map_err(|e| CompilerError::Parse(e.to_string()))?;
    mlkp_hir::verify(&program.graph).map_err(|errs| {
        CompilerError::Verify(
            errs.into_iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    // The subgraph hash identifies the verified source graph; the autotuner's
    // per-candidate pass pipelines transform it further for each schedule.
    let subgraph_hash = program.graph.hash64(0x4d_4c_4b_50);
    let entry = mlkp_autotune::autotune(&program.graph, subgraph_hash, pgo, budget, inputs)
        .map_err(|e| CompilerError::Compile(e.to_string()))?;
    // Staging offsets come from the plan's own offset table: inputs are buffers 0 and 1,
    // and the output is the destination of the last scheduled op.
    let lens = &entry.plan.buffer_lens;
    let a_off = entry.plan.offset_of(0);
    let b_off = entry.plan.offset_of(1);
    let out_id = entry
        .plan
        .ops
        .last()
        .map(|op| match op {
            mlkp_runtime::PlanOp::Unary { dst, .. }
            | mlkp_runtime::PlanOp::Binary { dst, .. }
            | mlkp_runtime::PlanOp::Reduce { dst, .. }
            | mlkp_runtime::PlanOp::Softmax { dst, .. }
            | mlkp_runtime::PlanOp::MatMul { c: dst, .. } => *dst,
        })
        .unwrap_or(0);
    let out_off = entry.plan.offset_of(out_id);
    let _ = lens;
    Ok(Compiled {
        name: program.name,
        plan: entry.plan,
        config_hash: entry.config_hash,
        latency_ns: entry.latency_ns,
        from_cache: entry.from_cache,
        staging: (a_off, b_off, out_off),
    })
}

/// Executes a compiled program on prepared inputs and returns the output.
// CEP:WHAT: Executes one compiled program.
// CEP:WHY: The installed fast path the JIT produces.
// CEP:STATUS: complete
// CEP:FAILURE: Returns CompilerError::Compile on execution failure.
// CEP:ASSUMES: the arena capacity covers the plan.
// CEP:COST: the measured hot path.
// CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
pub fn execute(compiled: &Compiled, a: &[f32], b: &[f32]) -> Result<Vec<f32>, CompilerError> {
    let total: u64 = compiled.plan.buffer_lens.iter().sum();
    let mut arena = mlkp_runtime::Arena::with_capacity(total);
    let (a_off, b_off, out_off) = compiled.staging;
    let a_expect = compiled.plan.buffer_lens.first().copied().unwrap_or(0);
    let b_expect = compiled.plan.buffer_lens.get(1).copied().unwrap_or(0);
    // CEP:WHAT: Stages inputs with exact length checks.
    // CEP:WHY: A short or long input would leave stale elements or spill into the next buffer, defeating the airlock's length invariants at staging time.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns CompilerError::Compile on length or range mismatch.
    // CEP:ASSUMES: the first two buffers are the inputs, matching the staging tuple produced by compile.
    // CEP:COST: O(elements).
    // CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
    arena
        .fill(a_off, a_expect, a)
        .ok_or_else(|| CompilerError::Compile("stage a length mismatch".to_string()))?;
    arena
        .fill(b_off, b_expect, b)
        .ok_or_else(|| CompilerError::Compile("stage b length mismatch".to_string()))?;
    mlkp_runtime::execute(&compiled.plan, &mut arena)
        .map_err(|e| CompilerError::Compile(e.to_string()))?;
    let out_id = compiled
        .plan
        .ops
        .last()
        .map(|op| match op {
            mlkp_runtime::PlanOp::Unary { dst, .. }
            | mlkp_runtime::PlanOp::Binary { dst, .. }
            | mlkp_runtime::PlanOp::Reduce { dst, .. }
            | mlkp_runtime::PlanOp::Softmax { dst, .. }
            | mlkp_runtime::PlanOp::MatMul { c: dst, .. } => *dst,
        })
        .unwrap_or(0);
    // CEP:WHAT: Reads the output buffer with its planned length.
    // CEP:WHY: Falling back to the input length would silently return a wrong-sized result when the output buffer id has no table entry.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns CompilerError::Compile when the output buffer has no planned length or the slice is unavailable.
    // CEP:ASSUMES: none
    // CEP:COST: O(elements).
    // CEP:EVIDENCE: crates/mlkp/tests/lifecycle_test.rs
    let out_len = compiled
        .plan
        .buffer_lens
        .get(out_id as usize)
        .copied()
        .ok_or_else(|| CompilerError::Compile("output buffer has no planned length".to_string()))?;
    arena
        .slice(out_off, out_len)
        .map(|s| s.to_vec())
        .ok_or_else(|| CompilerError::Compile("output slice unavailable".to_string()))
}

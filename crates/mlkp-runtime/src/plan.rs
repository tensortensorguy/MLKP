// CEP:FILE: crates/mlkp-runtime/src/plan.rs
// CEP:WHAT: Kernel plans: the scheduled form of a subgraph, plus the pass configuration that parameterizes lowering, and the validation airlock that proves plan invariants before execution.
// CEP:WHY: The autotuner searches over compiler schedules; the plan is the compilation result those schedules produce, and the airlock enforces standard chapter 64.2 by proving every kernel invariant once, outside the hot path.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Lowering returns PlanError on arity or shape mismatches; the airlock returns PlanError listing every violated invariant.
// CEP:ASSUMES: the input graph verifies and shapes are fully concrete at lowering time.
// CEP:COST: cold; lowering runs per candidate.
// CEP:EVIDENCE: unit tests in crates/mlkp-runtime/tests/plan_test.rs.

#![deny(missing_docs)]

use mlkp_hir::{Device, Graph, Op};
use mlkp_tensor::DimExpr;

use crate::kernels::{EwBinary, EwUnary, ReduceKind, Tile};

/// Lowering and execution errors.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanError {
    /// A node had an operand arity mismatch.
    Arity(String),
    /// A shape was not concrete at lowering time.
    UnconstrainedShape(String),
    /// The airlock found violated invariants.
    Airlock(String),
    /// A device was unavailable in the current probe.
    DeviceUnavailable(String),
}

impl std::fmt::Display for PlanError {
    // CEP:WHAT: Formats the plan error.
    // CEP:WHY: Deterministic diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::Arity(s) => write!(f, "plan error: {s}"),
            PlanError::UnconstrainedShape(s) => write!(f, "plan error: {s}"),
            PlanError::Airlock(s) => write!(f, "plan airlock: {s}"),
            PlanError::DeviceUnavailable(s) => write!(f, "plan error: {s}"),
        }
    }
}

impl std::error::Error for PlanError {}

/// The schedule configuration the search space produces: the concrete values of the
/// PL practice dimensions that reach the backend.
#[derive(Clone, Debug, PartialEq)]
pub struct PassConfig {
    /// Matrix multiply tile sizes.
    pub tile: Tile,
    /// Outer-dimension thread chunking for big parallel work.
    pub threads: u32,
    /// Loop ordering over the matmul triple: 0 = i,j,k, 1 = i,k,j.
    pub loop_order: u32,
    /// Whether the fusion pass merged elementwise chains (affects plan structure only).
    pub fused_elementwise: bool,
    /// Whether the layout pass applied column-major B (transposed access pattern).
    pub transpose_b: bool,
}

impl Default for PassConfig {
    // CEP:WHAT: Returns the baseline schedule.
    // CEP:WHY: The search seeds include the default so fallbacks are always legal.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
    fn default() -> Self {
        PassConfig {
            tile: Tile::NAIVE,
            threads: 1,
            loop_order: 0,
            fused_elementwise: true,
            transpose_b: false,
        }
    }
}

/// One scheduled kernel invocation over buffer ids.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanOp {
    /// Unary elementwise over full buffers.
    Unary {
        /// Operation.
        op: EwUnary,
        /// Source buffer.
        src: u32,
        /// Destination buffer.
        dst: u32,
        /// Element count.
        n: usize,
    },
    /// Binary elementwise over full buffers.
    Binary {
        /// Operation.
        op: EwBinary,
        /// Left buffer.
        a: u32,
        /// Right buffer.
        b: u32,
        /// Destination buffer.
        dst: u32,
        /// Element count.
        n: usize,
    },
    /// Last-axis reduction.
    Reduce {
        /// Combiner.
        op: ReduceKind,
        /// Source buffer [outer, red].
        src: u32,
        /// Destination buffer [outer].
        dst: u32,
        /// Outer extent.
        outer: usize,
        /// Reduction extent.
        red: usize,
    },
    /// Stable softmax over the last axis.
    Softmax {
        /// Source buffer.
        src: u32,
        /// Destination buffer.
        dst: u32,
        /// Outer extent.
        outer: usize,
        /// Reduction extent.
        red: usize,
    },
    /// Tiled matrix multiply.
    MatMul {
        /// Left buffer [m, k].
        a: u32,
        /// Right buffer [k, n].
        b: u32,
        /// Destination buffer [m, n].
        c: u32,
        /// Rows.
        m: usize,
        /// Columns.
        n: usize,
        /// Depth.
        k: usize,
        /// Tile schedule.
        tile: Tile,
    },
}

/// A scheduled kernel plan over a set of buffers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KernelPlan {
    /// Scheduled operations in execution order.
    pub ops: Vec<PlanOp>,
    /// Number of buffers the plan owns.
    pub buffers: u32,
    /// Buffer element counts by id.
    pub buffer_lens: Vec<u64>,
    /// Buffer arena offsets by id (element offsets, prefix sums of lens).
    pub buffer_offsets: Vec<u64>,
    /// The schedule configuration that produced this plan.
    pub config: PassConfig,
}

impl KernelPlan {
    /// Arena element offset of one buffer id.
    // CEP:WHAT: Maps a buffer id to its arena offset.
    // CEP:WHY: The executor slices buffers by offset, never by id.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns 0 for out-of-range ids; the airlock rejects those plans first.
    // CEP:ASSUMES: none
    // CEP:COST: O(buffers).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
    pub fn offset_of(&self, id: u32) -> u64 {
        self.buffer_offsets.get(id as usize).copied().unwrap_or(0)
    }

    /// Total elements across all buffers.
    // CEP:WHAT: Sums buffer element counts.
    // CEP:WHY: The arena allocates from the total; memory objectives consume it.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(buffers).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
    pub fn total_elements(&self) -> u64 {
        self.buffer_lens.iter().sum()
    }

    /// Memory footprint in bytes at 4 bytes per element.
    // CEP:WHAT: Computes the plan memory footprint.
    // CEP:WHY: The Pareto front tracks memory; this is its primary number.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: f32 storage is 4 bytes.
    // CEP:COST: O(buffers).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
    pub fn memory_bytes(&self) -> u64 {
        self.total_elements() * 4
    }
}

/// Lowers a verified, shape-concrete graph into a kernel plan under one schedule.
// CEP:WHAT: Lowers one graph into a kernel plan.
// CEP:WHY: This is the backend compilation step the design's Stage 5 names; the config parameterization is what the search space varies.
// CEP:STATUS: complete
// CEP:FAILURE: Returns PlanError on arity mismatch, non-concrete shapes, or unavailable device placements.
// CEP:ASSUMES: the graph passes the verifier first.
// CEP:COST: O(nodes).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
pub fn lower(graph: &Graph, config: &PassConfig) -> Result<KernelPlan, PlanError> {
    let mut plan = KernelPlan {
        ops: Vec::new(),
        buffers: 0,
        buffer_lens: Vec::new(),
        buffer_offsets: Vec::new(),
        config: config.clone(),
    };
    let mut node_buf: Vec<(u32, u32)> = Vec::new();
    // Inputs are not materialized by the plan; they alias caller-provided buffers 0..inputs.
    for id in &graph.inputs {
        let n = graph
            .node(*id)
            .ok_or_else(|| PlanError::Arity(format!("missing input {id}")))?;
        let len = concrete_len(&n.ty)?;
        let buf = plan.buffers;
        plan.buffers += 1;
        plan.buffer_lens.push(len);
        node_buf.push((*id, buf));
    }
    for n in graph.iter() {
        if matches!(n.op, Op::Input) {
            continue;
        }
        if n.device != Device::Cpu(0) {
            return Err(PlanError::DeviceUnavailable(format!(
                "node {} placed on {:?}; the CPU plan rejects it",
                n.id, n.device
            )));
        }
        let buf = plan.buffers;
        plan.buffers += 1;
        let len = concrete_len(&n.ty)?;
        plan.buffer_lens.push(len);
        let src_of = |id: u32| -> Result<u32, PlanError> {
            node_buf
                .iter()
                .find(|(nid, _)| *nid == id)
                .map(|(_, b)| *b)
                .ok_or_else(|| PlanError::Arity(format!("operand {id} not lowered")))
        };
        match &n.op {
            Op::Input => {
                continue;
            }
            Op::Const(_) => {
                // Constants are materialized at buffer fill time by the runtime.
                node_buf.push((n.id, buf));
            }
            Op::Map(_) => {
                let src =
                    src_of(*n.inputs.first().ok_or_else(|| {
                        PlanError::Arity(format!("map node {} lacks input", n.id))
                    })?)?;
                plan.ops.push(PlanOp::Unary {
                    op: map_to_unary(n)?,
                    src,
                    dst: buf,
                    n: len as usize,
                });
                node_buf.push((n.id, buf));
            }
            Op::Reduce { op, axes } => {
                let src = src_of(*n.inputs.first().ok_or_else(|| {
                    PlanError::Arity(format!("reduce node {} lacks input", n.id))
                })?)?;
                let dims = concrete_dims(&n.ty)?;
                let outer: usize = dims
                    .iter()
                    .try_fold(1u64, |acc, d| acc.checked_mul(*d))
                    .ok_or_else(|| {
                        PlanError::UnconstrainedShape(
                            "reduce extent overflows during lowering".to_string(),
                        )
                    })? as usize;
                let red = reduction_extent(&n.inputs[0], graph)? as usize;
                let kind = match op {
                    mlkp_tensor::ReduceOp::Sum => ReduceKind::Sum,
                    mlkp_tensor::ReduceOp::Max => ReduceKind::Max,
                    mlkp_tensor::ReduceOp::Min => {
                        return Err(PlanError::Arity(
                            "min reduce unsupported in v1 plan".to_string(),
                        ))
                    }
                };
                // CEP:WHAT: Rejects reductions over any axis but the last.
                // CEP:WHY: The plan vocabulary only schedules last-axis reductions; silently reducing the wrong axis would produce wrong numbers that pass the length airlock whenever extents coincide.
                // CEP:STATUS: complete
                // CEP:FAILURE: Returns PlanError::Arity for non-last-axis reductions.
                // CEP:ASSUMES: none
                // CEP:COST: O(axes).
                // CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
                let src_rank = graph
                    .node(n.inputs[0])
                    .map(|m| m.ty.dims.len())
                    .ok_or_else(|| PlanError::Arity(format!("missing operand {}", n.inputs[0])))?;
                let expected = vec![src_rank.saturating_sub(1) as u32];
                if *axes != expected {
                    return Err(PlanError::Arity(format!(
                        "reduce node {} must reduce the last axis {expected:?}; got {axes:?}",
                        n.id
                    )));
                }
                plan.ops.push(PlanOp::Reduce {
                    op: kind,
                    src,
                    dst: buf,
                    outer,
                    red,
                });
                node_buf.push((n.id, buf));
            }
            Op::Contract => {
                if n.inputs.len() != 2 {
                    return Err(PlanError::Arity(format!(
                        "contract node {} needs two operands",
                        n.id
                    )));
                }
                let a = src_of(n.inputs[0])?;
                let b = src_of(n.inputs[1])?;
                let dims = concrete_dims(&n.ty)?;
                if dims.len() != 2 {
                    return Err(PlanError::Arity(format!(
                        "contract node {} must produce a rank-2 result",
                        n.id
                    )));
                }
                let m = dims[0] as usize;
                let nn = dims[1] as usize;
                let k = reduction_extent(&n.inputs[0], graph)? as usize;
                plan.ops.push(PlanOp::MatMul {
                    a,
                    b,
                    c: buf,
                    m,
                    n: nn,
                    k,
                    tile: config.tile,
                });
                node_buf.push((n.id, buf));
            }
            Op::Softmax { .. } => {
                let src = src_of(*n.inputs.first().ok_or_else(|| {
                    PlanError::Arity(format!("softmax node {} lacks input", n.id))
                })?)?;
                let dims = concrete_dims(&n.ty)?;
                let red = dims[dims.len() - 1] as usize;
                let outer = dims[..dims.len() - 1]
                    .iter()
                    .try_fold(1u64, |acc, d| acc.checked_mul(*d))
                    .ok_or_else(|| {
                        PlanError::UnconstrainedShape(
                            "softmax extent overflows during lowering".to_string(),
                        )
                    })? as usize;
                plan.ops.push(PlanOp::Softmax {
                    src,
                    dst: buf,
                    outer,
                    red,
                });
                node_buf.push((n.id, buf));
            }
            Op::Broadcast { .. } | Op::Permute { .. } | Op::Reshape { .. } => {
                return Err(PlanError::Arity(format!(
                    "node {} uses an op the CPU plan does not schedule in v1",
                    n.id
                )));
            }
        }
    }
    // Prefix-sum offsets: buffer i starts at the sum of lens[0..i].
    let mut acc = 0u64;
    for l in &plan.buffer_lens {
        plan.buffer_offsets.push(acc);
        acc += *l;
    }
    Ok(plan)
}

// CEP:WHAT: Maps a HIR map op to its unary kernel.
// CEP:WHY: The plan vocabulary is smaller than the graph vocabulary; binary maps appear only through fused chains in later versions.
// CEP:STATUS: complete
// CEP:FAILURE: Returns PlanError::Arity for map kinds the plan does not recognize; the front-end rejects them and the algebraic pass removes the rest, so a hand-built graph fails loudly instead of silently executing the wrong kernel.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
fn map_to_unary(n: &mlkp_hir::Node) -> Result<EwUnary, PlanError> {
    match &n.op {
        Op::Map(mlkp_tensor::MapFn::Relu) => Ok(EwUnary::Relu),
        Op::Map(mlkp_tensor::MapFn::Exp) => Ok(EwUnary::Exp),
        Op::Map(mlkp_tensor::MapFn::Neg) => Ok(EwUnary::Neg),
        Op::Map(mlkp_tensor::MapFn::Sigmoid) => Ok(EwUnary::Sigmoid),
        other => Err(PlanError::Arity(format!(
            "map node {} uses an op the plan cannot schedule: {other:?}",
            n.id
        ))),
    }
}

// CEP:WHAT: Computes the concrete element count of one type.
// CEP:WHY: Buffers are sized by element counts.
// CEP:STATUS: complete
// CEP:FAILURE: Returns UnconstrainedShape when any dimension is symbolic, the rank is zero, or the product is zero or overflows.
// CEP:ASSUMES: none
// CEP:COST: O(dims).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
fn concrete_len(ty: &mlkp_tensor::TensorType) -> Result<u64, PlanError> {
    let dims = concrete_dims(ty)?;
    let product = dims
        .iter()
        .try_fold(1u64, |acc, d| acc.checked_mul(*d))
        .ok_or_else(|| {
            PlanError::UnconstrainedShape("element count overflows during lowering".to_string())
        })?;
    if product == 0 {
        return Err(PlanError::UnconstrainedShape(
            "zero-length tensors are not schedulable".to_string(),
        ));
    }
    Ok(product)
}

// CEP:WHAT: Computes concrete dimensions of one type.
// CEP:WHY: Kernel extents need plain integers.
// CEP:STATUS: complete
// CEP:FAILURE: Returns UnconstrainedShape for symbolic dimensions.
// CEP:ASSUMES: none
// CEP:COST: O(dims).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
fn concrete_dims(ty: &mlkp_tensor::TensorType) -> Result<Vec<u64>, PlanError> {
    let mut out = Vec::new();
    for d in &ty.dims {
        match d {
            DimExpr::Const(c) => out.push(*c),
            other => {
                return Err(PlanError::UnconstrainedShape(format!(
                    "dimension {other:?} is not concrete at lowering time"
                )))
            }
        }
    }
    if out.is_empty() {
        return Err(PlanError::UnconstrainedShape(
            "rank-0 types are not schedulable".to_string(),
        ));
    }
    Ok(out)
}

// CEP:WHAT: Computes the reduction extent of one operand's last dimension.
// CEP:WHY: The matmul depth and reduce extents come from the operand shape.
// CEP:STATUS: complete
// CEP:FAILURE: Returns UnconstrainedShape for symbolic dimensions or empty shapes.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
fn reduction_extent(input: &u32, graph: &Graph) -> Result<u64, PlanError> {
    let node = graph
        .node(*input)
        .ok_or_else(|| PlanError::Arity(format!("missing operand {input}")))?;
    let dims = concrete_dims(&node.ty)?;
    dims.last()
        .copied()
        .ok_or_else(|| PlanError::UnconstrainedShape("empty operand shape".to_string()))
}

impl KernelPlan {
    /// The validation airlock: proves every kernel invariant before execution.
    // CEP:WHAT: Proves plan invariants before the hot path runs.
    // CEP:WHY: Standard chapter 64.2: validate once at the airlock so kernels carry debug asserts only; this is the single enforcement point.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns PlanError::Airlock listing every violated invariant.
    // CEP:ASSUMES: none
    // CEP:COST: O(plan size).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
    pub fn validate(&self) -> Result<(), PlanError> {
        let mut errors: Vec<String> = Vec::new();
        // CEP:WHAT: Proves buffer table consistency.
        // CEP:WHY: A plan whose offsets disagree with its lengths can silently alias buffers or index far outside the arena; both states must fail here instead of in a kernel.
        // CEP:STATUS: complete
        // CEP:FAILURE: Appends an airlock error per violated invariant.
        // CEP:ASSUMES: none
        // CEP:COST: O(buffers).
        // CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
        if self.buffer_lens.len() != self.buffers as usize
            || self.buffer_offsets.len() != self.buffers as usize
        {
            errors.push("buffer table length does not match the buffer count".to_string());
        } else {
            let mut prev_end: u64 = 0;
            for i in 0..self.buffers as usize {
                let off = self.buffer_offsets[i];
                if i == 0 && off != 0 {
                    errors.push(format!("buffer 0 offset is {off}, expected 0"));
                }
                if off < prev_end {
                    errors.push(format!(
                        "buffer {i} overlaps its predecessor: offset {off} < {prev_end}"
                    ));
                }
                match off.checked_add(self.buffer_lens[i]) {
                    Some(end) => prev_end = end,
                    None => {
                        errors.push(format!("buffer {i} range overflows the element space"));
                        prev_end = u64::MAX;
                    }
                }
            }
        }
        for op in &self.ops {
            match op {
                PlanOp::Unary { src, dst, n, .. } => {
                    if !len_matches(self, *src, *n as u64) {
                        errors.push(format!("unary src {src} length != {n}"));
                    }
                    if !len_matches(self, *dst, *n as u64) {
                        errors.push(format!("unary dst {dst} length != {n}"));
                    }
                }
                PlanOp::Binary { a, b, dst, n, .. } => {
                    for id in [*a, *b, *dst] {
                        if !len_matches(self, id, *n as u64) {
                            errors.push(format!("binary buffer {id} length != {n}"));
                        }
                    }
                }
                PlanOp::Reduce {
                    src,
                    dst,
                    outer,
                    red,
                    ..
                } => {
                    if !len_matches(self, *src, (*outer * *red) as u64) {
                        errors.push(format!("reduce src {src} length != outer*red"));
                    }
                    if !len_matches(self, *dst, *outer as u64) {
                        errors.push(format!("reduce dst {dst} length != outer"));
                    }
                }
                PlanOp::Softmax {
                    src,
                    dst,
                    outer,
                    red,
                } => {
                    if !len_matches(self, *src, (*outer * *red) as u64) {
                        errors.push(format!("softmax src {src} length != outer*red"));
                    }
                    if !len_matches(self, *dst, (*outer * *red) as u64) {
                        errors.push(format!("softmax dst {dst} length != outer*red"));
                    }
                }
                PlanOp::MatMul {
                    a,
                    b,
                    c,
                    m,
                    n,
                    k,
                    tile,
                } => {
                    if !len_matches(self, *a, (*m * *k) as u64) {
                        errors.push(format!("matmul a {a} length != m*k"));
                    }
                    if !len_matches(self, *b, (*k * *n) as u64) {
                        errors.push(format!("matmul b {b} length != k*n"));
                    }
                    if !len_matches(self, *c, (*m * *n) as u64) {
                        errors.push(format!("matmul c {c} length != m*n"));
                    }
                    if tile.bm == 0 || tile.bn == 0 || tile.bk == 0 {
                        errors.push("matmul tile has a zero extent".to_string());
                    }
                    // CEP:WHAT: Enforces the power-of-two tile contract.
                    // CEP:WHY: The kernel contract states tiles are powers of two; oversized tiles are clamped in-kernel, but a non-power-of-two tile would make the measurement claim false.
                    // CEP:STATUS: complete
                    // CEP:FAILURE: Appends an airlock error for non-power-of-two tiles.
                    // CEP:ASSUMES: none
                    // CEP:COST: O(1).
                    // CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
                    if !tile.bm.is_power_of_two()
                        || !tile.bn.is_power_of_two()
                        || !tile.bk.is_power_of_two()
                    {
                        errors.push("matmul tile extents must be powers of two".to_string());
                    }
                }
            }
        }
        // CEP:WHAT: Proves per-op buffer disjointness.
        // CEP:WHY: Two overlapping ranges inside one op would read and write the same bytes concurrently; the plan must schedule disjoint buffers so kernels carry no aliasing logic.
        // CEP:STATUS: complete
        // CEP:FAILURE: Appends an airlock error per overlapping op.
        // CEP:ASSUMES: the buffer table check above passed, so offsets are meaningful.
        // CEP:COST: O(buffers per op).
        // CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
        for op in &self.ops {
            let ranges = op_ranges(self, op);
            for i in 0..ranges.len() {
                for j in (i + 1)..ranges.len() {
                    let (id_a, off_a, len_a) = ranges[i];
                    let (id_b, off_b, len_b) = ranges[j];
                    let a_end = off_a.saturating_add(len_a);
                    let b_end = off_b.saturating_add(len_b);
                    let overlaps = off_a < b_end && off_b < a_end;
                    if overlaps {
                        errors.push(format!("op buffers {id_a} and {id_b} overlap in the arena"));
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(PlanError::Airlock(errors.join("; ")))
        }
    }
}

// CEP:WHAT: Lists the arena ranges one op touches.
// CEP:WHY: The disjointness airlock needs the concrete (id, offset, length) triples per op.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: buffer ids are in range for the table.
// CEP:COST: O(buffers per op).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
fn op_ranges(plan: &KernelPlan, op: &PlanOp) -> Vec<(u32, u64, u64)> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<(u32, u64, u64)>, id: u32, len: u64| {
        out.push((id, plan.offset_of(id), len));
    };
    match op {
        PlanOp::Unary { src, dst, n, .. } => {
            push(&mut out, *src, *n as u64);
            push(&mut out, *dst, *n as u64);
        }
        PlanOp::Binary { a, b, dst, n, .. } => {
            push(&mut out, *a, *n as u64);
            push(&mut out, *b, *n as u64);
            push(&mut out, *dst, *n as u64);
        }
        PlanOp::Reduce {
            src,
            dst,
            outer,
            red,
            ..
        } => {
            push(&mut out, *src, (*outer * *red) as u64);
            push(&mut out, *dst, *outer as u64);
        }
        PlanOp::Softmax {
            src,
            dst,
            outer,
            red,
        } => {
            push(&mut out, *src, (*outer * *red) as u64);
            push(&mut out, *dst, (*outer * *red) as u64);
        }
        PlanOp::MatMul {
            a, b, c, m, n, k, ..
        } => {
            push(&mut out, *a, (*m * *k) as u64);
            push(&mut out, *b, (*k * *n) as u64);
            push(&mut out, *c, (*m * *n) as u64);
        }
    }
    out
}

// CEP:WHAT: Tests one buffer length against an expectation.
// CEP:WHY: The airlock checks every buffer reference.
// CEP:STATUS: complete
// CEP:FAILURE: Returns false for out-of-range ids.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/plan_test.rs
fn len_matches(plan: &KernelPlan, id: u32, len: u64) -> bool {
    plan.buffer_lens.get(id as usize).is_some_and(|l| *l == len)
}

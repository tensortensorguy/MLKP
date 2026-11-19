// CEP:FILE: crates/mlkp-runtime/src/plan.rs
// CEP:WHAT: Kernel plans: the scheduled form of a subgraph, plus the pass configuration that parameterizes lowering, and the validation airlock that proves plan invariants before execution.
// CEP:WHY: The autotuner searches over compiler schedules; the plan is the compilation result those schedules produce, and the airlock enforces standard chapter 64.2 by proving every kernel invariant once, outside the hot path.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Lowering returns PlanError on arity or shape mismatches; the airlock returns PlanError listing every violated invariant.
// CEP:ASSUMES: the input graph verifies and shapes are fully concrete at lowering time.
// CEP:COST: cold; lowering runs per candidate.
// CEP:EVIDENCE: unit tests in crates/mlkp-runtime/tests/runtime_test.rs.

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
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
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
    /// Whether the buffer-reuse pass assigns dead buffer ranges to new buffers (liveness-based arena compaction).
    pub buffer_reuse: bool,
}

impl Default for PassConfig {
    // CEP:WHAT: Returns the baseline schedule.
    // CEP:WHY: The search seeds include the default so fallbacks are always legal.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    fn default() -> Self {
        PassConfig {
            tile: Tile::NAIVE,
            threads: 1,
            loop_order: 0,
            fused_elementwise: true,
            transpose_b: false,
            buffer_reuse: true,
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
        /// Source buffer `[outer, red]`.
        src: u32,
        /// Destination buffer `[outer]`.
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
        /// Loop order over the matmul triple: 0 = i,j,k, 1 = i,k,j.
        loop_order: u32,
        /// Strided-B access pattern (column-major B reads).
        transpose_b: bool,
    },
}

impl PlanOp {
    /// Buffer ids the operation reads and writes.
    // CEP:WHAT: Lists the buffer ids of one operation.
    // CEP:WHY: The airlock proves per-operation disjointness so the executor can hand out disjoint mutable slices without copies.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    pub fn buffer_ids(&self) -> Vec<u32> {
        match self {
            PlanOp::Unary { src, dst, .. } => vec![*src, *dst],
            PlanOp::Binary { a, b, dst, .. } => vec![*a, *b, *dst],
            PlanOp::Reduce { src, dst, .. } | PlanOp::Softmax { src, dst, .. } => {
                vec![*src, *dst]
            }
            PlanOp::MatMul { a, b, c, .. } => vec![*a, *b, *c],
        }
    }
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
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
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
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    pub fn total_elements(&self) -> u64 {
        self.buffer_offsets
            .iter()
            .zip(self.buffer_lens.iter())
            .map(|(o, l)| o + l)
            .max()
            .unwrap_or(0)
    }

    /// Memory footprint in bytes at 4 bytes per element.
    // CEP:WHAT: Computes the plan memory footprint.
    // CEP:WHY: The Pareto front tracks memory; this is its primary number.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: f32 storage is 4 bytes.
    // CEP:COST: O(buffers).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    pub fn memory_bytes(&self) -> u64 {
        self.total_elements() * 4
    }
}

/// Lowers a verified, shape-concrete graph into a kernel plan under one schedule.
// CEP:ID: JIT-t26
// CEP:WHAT: Lowers one graph into a kernel plan.
// CEP:WHY: This is the backend compilation step the design's Stage 5 names; the config parameterization is what the search space varies.
// CEP:STATUS: complete
// CEP:FAILURE: Returns PlanError on arity mismatch, non-concrete shapes, or unavailable device placements.
// CEP:ASSUMES: the graph passes the verifier first.
// CEP:COST: O(nodes).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
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
                let outer: usize = dims.iter().product::<u64>() as usize;
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
                // The v1 plan vocabulary schedules last-axis reductions only; any
                // other axis set fails loudly instead of silently mis-shaping the
                // kernel extents.
                let last = (dims.len().saturating_sub(1)) as u32;
                if !(axes.len() == 1 && axes[0] == last) {
                    return Err(PlanError::Arity(format!(
                        "reduce node {} reduces axes {axes:?}; the plan schedules only axis {last}",
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
                    loop_order: config.loop_order,
                    transpose_b: config.transpose_b,
                });
                node_buf.push((n.id, buf));
            }
            Op::Softmax { axis } => {
                let src = src_of(*n.inputs.first().ok_or_else(|| {
                    PlanError::Arity(format!("softmax node {} lacks input", n.id))
                })?)?;
                let dims = concrete_dims(&n.ty)?;
                let last = dims.len().checked_sub(1).ok_or_else(|| {
                    PlanError::UnconstrainedShape(format!(
                        "softmax node {} normalizes a rank-0 tensor",
                        n.id
                    ))
                })?;
                if *axis != last as u32 {
                    return Err(PlanError::Arity(format!(
                        "softmax node {} declares axis {axis}; the plan normalizes axis {last}",
                        n.id
                    )));
                }
                let red = dims[last] as usize;
                let outer = dims[..last].iter().product::<u64>() as usize;
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
    if config.buffer_reuse {
        assign_offsets_with_reuse(&mut plan, graph);
    } else {
        // Prefix-sum offsets: buffer i starts at the sum of lens[0..i].
        let mut acc = 0u64;
        for l in &plan.buffer_lens {
            plan.buffer_offsets.push(acc);
            acc += *l;
        }
    }
    Ok(plan)
}

// CEP:ID: JIT-f13
// CEP:WHAT: The buffer-reuse pass: computes each buffer's first-write and last-read time from the fused execution schedule (what the dispatcher really touches, including epilogue writes at the matmul's index and chain steps that skip intermediates), then assigns arena ranges so a buffer whose readers are all done shares its range with a later buffer, appending only when no freed range fits.
// CEP:WHY: The memory objective is the arena footprint; dead intermediates dominate it on deep chains, and sharing their ranges compacts the arena without any timed-window copies. Planning against the raw plan ops would collide with fused steps, which write consumer destinations one or more ops early.
// CEP:STATUS: complete
// CEP:FAILURE: none; liveness comes from the dispatcher's own grouping (exec::fused_touches), and ranges are shared only when the previous buffer's last read is strictly before the new buffer's first write, so the executor's per-step disjoint views stay disjoint.
// CEP:ASSUMES: fused_touches mirrors the dispatcher exactly; inputs and declared outputs stay live for the whole execution (their ranges are never reused as targets); every non-input buffer is written by at most one step.
// CEP:COST: O(steps + buffers^2) worst case for the first-fit scan; buffers in the hundreds.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
fn assign_offsets_with_reuse(plan: &mut KernelPlan, graph: &mlkp_hir::Graph) {
    let nb = plan.buffers as usize;
    // First write per buffer (step index; inputs count as written at index 0 by
    // the caller) and last read per buffer (None = never touched by any step).
    let mut first_write = vec![usize::MAX; nb];
    let mut last_read: Vec<Option<usize>> = vec![None; nb];
    for id in &graph.inputs {
        if (*id as usize) < nb {
            first_write[*id as usize] = 0;
            last_read[*id as usize] = Some(usize::MAX);
        }
    }
    for id in &graph.outputs {
        if (*id as usize) < nb {
            last_read[*id as usize] = Some(usize::MAX);
        }
    }
    let mut touched = vec![false; nb];
    for step in crate::exec::fused_touches(plan) {
        for w in &step.writes {
            if (*w as usize) < nb {
                first_write[*w as usize] = first_write[*w as usize].min(step.start);
                touched[*w as usize] = true;
            }
        }
        for r in &step.reads {
            if (*r as usize) < nb {
                let cur = last_read[*r as usize];
                last_read[*r as usize] = Some(cur.map_or(step.start, |v| v.max(step.start)));
                touched[*r as usize] = true;
            }
        }
    }
    // Inputs keep their ranges reserved (defensive: callers may re-read them
    // between executions); a buffer never read frees right after its own write.
    let free_at = |b: usize| -> usize {
        match last_read[b] {
            Some(v) => v,
            None => first_write[b],
        }
    };
    // Assign in first-write order (ties by id): free what is dead, then
    // first-fit from the free list, appending only when nothing fits.
    let mut offsets = vec![0u64; nb];
    let mut offsets_set = vec![false; nb];
    let mut order: Vec<usize> = (0..nb).filter(|&b| touched[b]).collect();
    order.sort_by_key(|&b| (first_write[b], b));
    let mut free: Vec<(u64, u64)> = Vec::new();
    let mut assigned = vec![false; nb];
    let mut freed = vec![false; nb];
    let mut top = 0u64;
    for &b in &order {
        let w = if first_write[b] == usize::MAX {
            0
        } else {
            first_write[b]
        };
        for x in 0..nb {
            if assigned[x] && !freed[x] && free_at(x) < w {
                freed[x] = true;
                free.push((offsets[x], plan.buffer_lens[x]));
            }
        }
        let len = plan.buffer_lens[b];
        let mut placed = false;
        for slot in free.iter_mut() {
            if slot.1 >= len {
                offsets[b] = slot.0;
                if slot.1 == len {
                    let start = slot.0;
                    let pos = free
                        .iter()
                        .position(|s| s.0 == start)
                        .expect("slot present");
                    free.remove(pos);
                } else {
                    slot.0 += len;
                    slot.1 -= len;
                }
                placed = true;
                break;
            }
        }
        if !placed {
            offsets[b] = top;
            top += len;
        }
        assigned[b] = true;
    }
    // Buffers no fused step touches (chain intermediates and skipped matmul
    // outputs) are inaccessible memory at runtime, but the airlock still checks
    // their plan-op operand ranges. Park each one inside a touched range that
    // no co-occurring buffer's ASSIGNED range overlaps — touched ranges are
    // time-shared, so the check is by range, not by buffer identity — keeping
    // parked ranges pairwise disjoint. A buffer with no room appends at the top.
    let mut parked: Vec<(u64, u64)> = Vec::new();
    for b in 0..nb {
        if touched[b] {
            continue;
        }
        let len = plan.buffer_lens[b];
        // The assigned ranges of every buffer this one co-occurs with in a plan
        // op; only buffers already placed (touched, or parked earlier in id
        // order) have final ranges — later parkers check this one symmetrically.
        let mut forbidden: Vec<(u64, u64)> = Vec::new();
        for op in &plan.ops {
            let ids = op.buffer_ids();
            if ids.contains(&(b as u32)) {
                for id in ids {
                    let x = id as usize;
                    if id != b as u32
                        && x < nb
                        && (touched[x] || (x < b && !touched[x] && offsets_set[x]))
                    {
                        forbidden.push((offsets[x], offsets[x] + plan.buffer_lens[x]));
                    }
                }
            }
        }
        let len_b = plan.buffer_lens[b];
        let mut placed = false;
        'hosts: for x in 0..nb {
            if !touched[x] {
                continue;
            }
            let (hx, lx) = (offsets[x], plan.buffer_lens[x]);
            if lx < len {
                continue;
            }
            let steps = (lx - len) / len_b.max(1) + 1;
            for k in 0..steps {
                let cand = hx + k * len_b.max(1);
                let range = (cand, cand + len);
                if parked.iter().any(|p| p.0 < range.1 && range.0 < p.1)
                    || forbidden.iter().any(|f| f.0 < range.1 && range.0 < f.1)
                {
                    continue;
                }
                offsets[b] = cand;
                parked.push(range);
                offsets_set[b] = true;
                placed = true;
                break 'hosts;
            }
        }
        if !placed {
            offsets[b] = top;
            top += len;
            offsets_set[b] = true;
        }
    }
    plan.buffer_offsets = offsets;
}

// CEP:WHAT: Maps a HIR map op to its unary kernel.
// CEP:WHY: The plan vocabulary is smaller than the graph vocabulary; unmapped kinds must fail loudly, never silently change the computation.
// CEP:STATUS: complete
// CEP:FAILURE: Returns PlanError::Arity for map kinds the plan cannot schedule; the front-end rejects binary arities and the algebraic pass removes Id nodes before lowering.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
fn map_to_unary(n: &mlkp_hir::Node) -> Result<EwUnary, PlanError> {
    match &n.op {
        Op::Map(mlkp_tensor::MapFn::Relu) => Ok(EwUnary::Relu),
        Op::Map(mlkp_tensor::MapFn::Exp) => Ok(EwUnary::Exp),
        Op::Map(mlkp_tensor::MapFn::Neg) => Ok(EwUnary::Neg),
        Op::Map(mlkp_tensor::MapFn::Sigmoid) => Ok(EwUnary::Sigmoid),
        other => Err(PlanError::Arity(format!(
            "node {} uses the map {other:?}; the CPU plan schedules only relu, exp, neg, sigmoid",
            n.id
        ))),
    }
}

// CEP:WHAT: Computes the concrete element count of one type.
// CEP:WHY: Buffers are sized by element counts.
// CEP:STATUS: complete
// CEP:FAILURE: Returns UnconstrainedShape when any dimension is symbolic.
// CEP:ASSUMES: none
// CEP:COST: O(dims).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
fn concrete_len(ty: &mlkp_tensor::TensorType) -> Result<u64, PlanError> {
    let dims = concrete_dims(ty)?;
    Ok(dims.iter().product::<u64>().max(1))
}

// CEP:WHAT: Computes concrete dimensions of one type.
// CEP:WHY: Kernel extents need plain integers.
// CEP:STATUS: complete
// CEP:FAILURE: Returns UnconstrainedShape for symbolic dimensions.
// CEP:ASSUMES: none
// CEP:COST: O(dims).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
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
    Ok(out)
}

// CEP:WHAT: Computes the reduction extent of one operand's last dimension.
// CEP:WHY: The matmul depth and reduce extents come from the operand shape.
// CEP:STATUS: complete
// CEP:FAILURE: Returns UnconstrainedShape for symbolic dimensions or empty shapes.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
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
    // CEP:ID: JIT-084
    // CEP:WHAT: Proves plan invariants before the hot path runs.
    // CEP:WHY: Standard chapter 64.2: validate once at the airlock so kernels carry debug asserts only; this is the single enforcement point, covering extents, tile legality, and per-operation buffer disjointness.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns PlanError::Airlock listing every violated invariant.
    // CEP:ASSUMES: none
    // CEP:COST: O(plan size).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
    pub fn validate(&self) -> Result<(), PlanError> {
        let mut errors: Vec<String> = Vec::new();
        // CEP:WHAT: Proves the schedule configuration is executable.
        // CEP:WHY: The kernels select behavior from the config fields; a loop order or thread count outside the executable set would make the measured cost claim false.
        // CEP:STATUS: complete
        // CEP:FAILURE: Appends an airlock error per invalid config field.
        // CEP:ASSUMES: none
        // CEP:COST: O(1).
        // CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
        if self.config.loop_order > 1 {
            errors.push(format!(
                "config loop_order {} is outside the executable set {{0, 1}}",
                self.config.loop_order
            ));
        }
        if self.config.threads == 0 {
            errors.push("config threads must be at least 1".to_string());
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
                    ..
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
                    if !is_power_of_two(tile.bm)
                        || !is_power_of_two(tile.bn)
                        || !is_power_of_two(tile.bk)
                    {
                        errors.push("matmul tile is not a power of two".to_string());
                    }
                }
            }
            // Per-operation buffer disjointness: the executor hands out disjoint
            // mutable slices, so overlapping references inside one operation are a
            // plan defect, not a runtime event.
            let mut ranges: Vec<(u64, u64)> = Vec::new();
            for id in op.buffer_ids() {
                let off = self.offset_of(id);
                let len = self.buffer_lens.get(id as usize).copied().unwrap_or(0);
                ranges.push((off, off + len));
            }
            for i in 0..ranges.len() {
                for j in (i + 1)..ranges.len() {
                    if ranges_overlap(ranges[i], ranges[j]) {
                        errors.push(format!(
                            "operation over buffers {:?} has overlapping ranges",
                            op.buffer_ids()
                        ));
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

// CEP:WHAT: Tests whether one extent is a positive power of two.
// CEP:WHY: The matmul kernel's blocked loop and the tile contract require power-of-two tiles; the airlock is the single enforcement point.
// CEP:STATUS: complete
// CEP:FAILURE: Returns false for zero.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
fn is_power_of_two(x: usize) -> bool {
    x != 0 && (x & (x - 1)) == 0
}

// CEP:WHAT: Tests one buffer length against an expectation.
// CEP:WHY: The airlock checks every buffer reference.
// CEP:STATUS: complete
// CEP:FAILURE: Returns false for out-of-range ids.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
fn len_matches(plan: &KernelPlan, id: u32, len: u64) -> bool {
    plan.buffer_lens.get(id as usize).is_some_and(|l| *l == len)
}

// CEP:WHAT: Tests whether two element ranges overlap.
// CEP:WHY: The airlock proves per-operation disjointness with this predicate.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/runtime_test.rs
fn ranges_overlap(a: (u64, u64), b: (u64, u64)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

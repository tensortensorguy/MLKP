// CEP:FILE: crates/mlkp-runtime/examples/exec_profile.rs
// CEP:WHAT: The execution profile harness: times the full 64-op plan end to end and the isolated kernel stages (scalar tiled matmul, elementwise unary passes, plan validation) so optimization work targets the real split, not guesses.
// CEP:WHY: The design's measurement discipline applies to the optimizer itself: the 10x execution target must be attacked where the time actually is, and the before/after split is recorded in benches/artifacts.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect on parse, lowering, or execution failure.
// CEP:ASSUMES: run under release (cargo run --release) or the numbers are meaningless.
// CEP:COST: a few seconds; the harness itself is cold-tool.
// CEP:EVIDENCE: cargo run --release -p mlkp-runtime --example exec_profile

//! Run with: cargo run --release -p mlkp-runtime --example exec_profile

#![deny(missing_docs)]

use std::hint::black_box;
use std::time::Instant;

use mlkp_runtime::{execute, Arena, PassConfig};

/// The same 64-op program the pipeline benchmark uses.
// CEP:ID: JIT-e24
// CEP:WHAT: Generates the 64-op mixed source program.
// CEP:WHY: The profile must measure the same op mix the pipeline benchmark executes so their numbers are comparable.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the front-end accepts the generated matmul and map chain on f32[64,64].
// CEP:COST: O(ops).
// CEP:EVIDENCE: cargo run --release -p mlkp-runtime --example exec_profile
fn program() -> String {
    let mut s = String::from("fn @p64(%a: f32[64,64], %b: f32[64,64]) -> f32[64,64] {\n");
    let mut prev = "%a".to_string();
    let mut other = "%b".to_string();
    for i in 0..64 {
        match i % 5 {
            0 => {
                s.push_str(&format!("  %t{i} = matmul({prev}, {other})\n"));
                other = "%b".to_string();
            }
            1 => s.push_str(&format!("  %t{i} = relu({prev})\n")),
            2 => s.push_str(&format!("  %t{i} = sigmoid({prev})\n")),
            3 => s.push_str(&format!("  %t{i} = exp({prev})\n")),
            _ => s.push_str(&format!("  %t{i} = neg({prev})\n")),
        }
        prev = format!("%t{i}");
    }
    s.push_str(&format!("  return {prev}\n}}\n"));
    s
}

/// Median over runs.
// CEP:ID: JIT-e36
// CEP:WHAT: Times one closure over repeated runs and returns the median in microseconds.
// CEP:WHY: A median over repeated runs with warmup discards cold-start noise and scheduler spikes; the profile's numbers must be stable enough to optimize against.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the closure's work dominates the timing overhead.
// CEP:COST: O(iters) executions.
// CEP:EVIDENCE: cargo run --release -p mlkp-runtime --example exec_profile
fn median_of<F: FnMut()>(mut f: F, iters: usize) -> f64 {
    let mut v = Vec::new();
    for _ in 0..3 {
        f();
    }
    for _ in 0..iters {
        let t = Instant::now();
        f();
        v.push(t.elapsed().as_secs_f64() * 1e6);
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

// CEP:ID: JIT-e35
// CEP:WHAT: Runs the execution profile: lowers the 64-op plan, times the full execution and the isolated kernel stages, and prints the split.
// CEP:WHY: The optimizer must attack the real time distribution, not guesses; this harness recorded the 93-percent matmul split that motivated the CEP-327 fast path.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect on parse, lowering, or execution failure.
// CEP:ASSUMES: run under release (cargo run --release) or the numbers are meaningless.
// CEP:COST: a few seconds; the harness itself is cold-tool.
// CEP:EVIDENCE: cargo run --release -p mlkp-runtime --example exec_profile and benches/artifacts/exec_fastpath.md.
fn main() {
    let prog = mlkp_front::parse_program(&program()).expect("parses");
    let plan = mlkp_runtime::plan::lower(&prog.graph, &PassConfig::default()).expect("lowers");
    println!("plan: {} ops, {} buffers", plan.ops.len(), plan.buffers);
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for op in &plan.ops {
        let k = match op {
            mlkp_runtime::PlanOp::MatMul { .. } => "matmul",
            mlkp_runtime::PlanOp::Unary { op, .. } => match op {
                mlkp_runtime::EwUnary::Relu => "relu",
                mlkp_runtime::EwUnary::Exp => "exp",
                mlkp_runtime::EwUnary::Neg => "neg",
                mlkp_runtime::EwUnary::Sigmoid => "sigmoid",
            },
            _ => "other",
        };
        *counts.entry(k.to_string()).or_default() += 1;
    }
    println!("op mix: {:?}", counts);

    let mut arena = Arena::with_capacity(
        plan.buffer_offsets.last().unwrap_or(&0) + plan.buffer_lens.last().unwrap_or(&0),
    );
    for (i, len) in plan.buffer_lens.iter().enumerate() {
        let off = plan.buffer_offsets[i];
        if arena.alloc(*len).is_err() {
            panic!("arena alloc failed");
        }
        let _ = off;
    }
    // Fill buffers 0 and 1 (the inputs); everything else is scratch.
    let a: Vec<f32> = (0..4096).map(|i| (i % 17) as f32 * 0.03).collect();
    let b: Vec<f32> = (0..4096).map(|i| (i % 13) as f32 * 0.02 - 0.1).collect();
    arena.fill(plan.offset_of(0), 4096, &a).expect("fill a");
    arena.fill(plan.offset_of(1), 4096, &b).expect("fill b");

    let full = median_of(
        || {
            execute(black_box(&plan), &mut arena).expect("executes");
        },
        60,
    );
    println!(
        "full 64-op exec:        {:>9.1} us  ({:.1} us/op)",
        full,
        full / 64.0
    );

    let val = median_of(
        || {
            black_box(plan.validate()).expect("validates");
        },
        200,
    );
    println!("plan.validate():        {:>9.1} us", val);

    let mm = median_of(
        || {
            let (s0, l0) = (plan.offset_of(0) as usize, 4096);
            let (s1, l1) = (plan.offset_of(1) as usize, 4096);
            let (s2, l2) = (plan.offset_of(2) as usize, 4096);
            let data = arena.as_mut_slice();
            // Disjoint by construction of the plan.
            let (av, bv, cv) = split3(data, s0, l0, s1, l1, s2, l2);
            mlkp_runtime::kernels::matmul_tiled(
                black_box(av),
                black_box(bv),
                cv,
                64,
                64,
                64,
                mlkp_runtime::Tile::NAIVE,
            );
        },
        60,
    );
    println!(
        "matmul 64^3 (engine):     {:>9.1} us  (x13 = {:.1} us)",
        mm,
        mm * 13.0
    );

    let n = 4096usize;
    let src: Vec<f32> = (0..n).map(|i| ((i % 64) as f32 - 31.5) / 32.0).collect();
    let mut dst = vec![0.0f32; n];
    let ex = median_of(
        || {
            mlkp_runtime::kernels::ew_unary(mlkp_runtime::EwUnary::Exp, black_box(&src), &mut dst);
        },
        200,
    );
    println!(
        "ew exp 4096 (engine):     {:>9.3} us  (x13 = {:.1} us)",
        ex,
        ex * 13.0
    );
    let sg = median_of(
        || {
            mlkp_runtime::kernels::ew_unary(
                mlkp_runtime::EwUnary::Sigmoid,
                black_box(&src),
                &mut dst,
            );
        },
        200,
    );
    println!(
        "ew sigmoid 4096 (engine): {:>9.3} us  (x13 = {:.1} us)",
        sg,
        sg * 13.0
    );
    let rl = median_of(
        || {
            mlkp_runtime::kernels::ew_unary(mlkp_runtime::EwUnary::Relu, black_box(&src), &mut dst);
        },
        200,
    );
    println!("ew relu 4096 (engine):    {:>9.3} us", rl);
}

// CEP:ID: JIT-p01
// CEP:WHAT: Splits one arena slice into three disjoint views.
// CEP:WHY: The profile needs the same view derivation the dispatcher uses without reaching into its private helpers.
// CEP:STATUS: complete
// CEP:FAILURE: Panics on overlap or out-of-range; the profile constructs disjoint ranges.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: cargo run --release -p mlkp-runtime --example exec_profile
fn split3(
    data: &mut [f32],
    s0: usize,
    n0: usize,
    s1: usize,
    n1: usize,
    s2: usize,
    n2: usize,
) -> (&mut [f32], &mut [f32], &mut [f32]) {
    assert!(s0 + n0 <= s1 && s1 + n1 <= s2 && s2 + n2 <= data.len());
    let (x, rest) = data.split_at_mut(s1);
    let (y, z) = rest.split_at_mut(s2 - s1);
    (&mut x[s0..s0 + n0], &mut y[..n1], &mut z[..n2])
}

// CEP:FILE: crates/mlkp/benches/pipeline.rs
// CEP:WHAT: The pipeline benchmark suite: front-end parsing, heterogeneous verification, cold and warm facade compilation under a fixed autotune budget, the installed execution fast path, and the ISA tier's assemble, decode, and render stages, each timed with a std-only median harness (no external dependencies).
// CEP:WHY: The design's cold-versus-warm split and the ISA tier's cold-tool claim are quantitative: these numbers are the evidence that warm compilation is cache-bound, execution is linear in the op count, and the tier tools stay in cold-tool territory.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect on pipeline errors; the suite measures, it does not verify correctness (that is the test suites' job).
// CEP:ASSUMES: runs under the bench (release) profile; wall-clock timing on a shared sandbox is indicative, not certified hardware.
// CEP:COST: the suite itself takes a few seconds; each scenario warms up before timing.
// CEP:EVIDENCE: cargo bench -p mlkp --bench pipeline; results recorded in doc/benchmarks.md.

//! MLKP pipeline benchmark. Run with: cargo bench -p mlkp --bench pipeline

#![deny(missing_docs)]

use std::hint::black_box;
use std::time::{Duration, Instant};

use mlkp_autotune::PgoStore;

/// Runs one scenario: warmup, then per-iteration timing, then median.
// CEP:ID: JIT-b01
// CEP:WHAT: Times one scenario.
// CEP:WHY: A median over repeated runs with a warmup discards cold-start noise and one-off scheduler spikes.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the closure's work dominates the timing overhead.
// CEP:COST: O(iters) scenario executions.
// CEP:EVIDENCE: cargo bench -p mlkp --bench pipeline
fn time_it<F: FnMut(usize)>(name: &str, iters: usize, mut f: F) -> Duration {
    for i in 0..3 {
        f(i); // warmup
    }
    let mut samples = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        f(i);
        samples.push(t.elapsed());
    }
    samples.sort();
    let median = samples[samples.len() / 2];
    let min = samples[0];
    println!(
        "  {:<34} {:>10.3?} median  {:>10.3?} min  ({} iters)",
        name, median, min, iters
    );
    median
}

/// Builds one MLKP source program with the requested op-chain length.
// CEP:ID: JIT-b02
// CEP:WHAT: Generates a source program.
// CEP:WHY: Parse, compile, and execute scenarios all scale on one generator so their numbers are comparable per op.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the front-end accepts matmul, relu, exp, neg, sigmoid, and softmax on f32[64,64] chains.
// CEP:COST: O(ops).
// CEP:EVIDENCE: cargo bench -p mlkp --bench pipeline
fn make_program(name: &str, ops: usize) -> String {
    let mut s = format!("fn @{name}(%a: f32[64,64], %b: f32[64,64]) -> f32[64,64] {{\n");
    let mut prev = "%a".to_string();
    let mut other = "%b".to_string();
    for i in 0..ops {
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

/// The ISA conformance corpus: the same program the tier tests run through the gas differential.
// CEP:ID: JIT-b03
// CEP:WHAT: Supplies the ISA corpus.
// CEP:WHY: The tier benchmark must measure the same instruction mix the tests verify, every family and form.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold.
// CEP:EVIDENCE: cargo bench -p mlkp --bench pipeline
fn isa_corpus() -> &'static str {
    include_str!("../../mlkp-machine/tests/isa_corpus.inc")
}

/// The main harness: runs every scenario and prints the results table.
// CEP:ID: JIT-b04
// CEP:WHAT: Runs the benchmark suite.
// CEP:WHY: One binary with a stable scenario order makes successive runs on the same host comparable.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect on pipeline errors.
// CEP:ASSUMES: none
// CEP:COST: a few seconds end to end.
// CEP:EVIDENCE: cargo bench -p mlkp --bench pipeline
fn main() {
    println!(
        "MLKP pipeline benchmark (bench profile, {} threads)\n",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    );
    let mut pgo = PgoStore::in_memory();

    // --- Front end and verification -------------------------------------------------
    println!("front end");
    let src8 = make_program("p8", 8);
    let src64 = make_program("p64", 64);
    let src256 = make_program("p256", 256);
    time_it("parse 8-op program", 50, |_| {
        black_box(mlkp_front::parse_program(black_box(&src8))).expect("parses");
    });
    time_it("parse 64-op program", 50, |_| {
        black_box(mlkp_front::parse_program(black_box(&src64))).expect("parses");
    });
    time_it("parse 256-op program", 50, |_| {
        black_box(mlkp_front::parse_program(black_box(&src256))).expect("parses");
    });
    let prog256 = mlkp_front::parse_program(&src256).expect("parses");
    time_it("verify 256-op graph", 50, |_| {
        black_box(mlkp_hir::verify(black_box(&prog256.graph))).expect("verifies");
    });

    // --- Compilation: cold (fresh PGO store) and warm (shared store) ----------------
    println!("\ncompilation (autotune budget 30 ms)");
    time_it("compile 8-op cold", 5, |_| {
        let mut fresh = PgoStore::in_memory();
        black_box(mlkp::compile(
            &src8,
            &mut fresh,
            Duration::from_millis(30),
            &[vec![0.5f32; 4096], vec![0.25f32; 4096]],
        ))
        .expect("compiles");
    });
    time_it("compile 64-op cold", 5, |_| {
        let mut fresh = PgoStore::in_memory();
        black_box(mlkp::compile(
            &src64,
            &mut fresh,
            Duration::from_millis(30),
            &[vec![0.5f32; 4096], vec![0.25f32; 4096]],
        ))
        .expect("compiles");
    });
    let mut warm = PgoStore::in_memory();
    mlkp::compile(
        &src64,
        &mut warm,
        Duration::from_millis(30),
        &[vec![0.5f32; 4096], vec![0.25f32; 4096]],
    )
    .expect("warms");
    time_it("compile 64-op warm (cache hit)", 20, |_| {
        black_box(mlkp::compile(
            &src64,
            &mut warm,
            Duration::from_millis(30),
            &[vec![0.5f32; 4096], vec![0.25f32; 4096]],
        ))
        .expect("compiles");
    });

    // --- Execution: the installed fast path ------------------------------------------
    println!("\nexecution (64-op plan, f32[64,64] buffers)");
    let compiled = mlkp::compile(
        &src64,
        &mut pgo,
        Duration::from_millis(30),
        &[vec![0.5f32; 4096], vec![0.25f32; 4096]],
    )
    .expect("compiles");
    let a = vec![0.5f32; 4096];
    let b = vec![0.25f32; 4096];
    let out = mlkp::execute(&compiled, &[&a, &b]).expect("executes");
    println!(
        "  {:<34} output[0]={:+.6} len={}",
        "sample output",
        out[0],
        out.len()
    );
    let t = time_it("execute 64-op plan", 200, |_| {
        black_box(mlkp::execute(black_box(&compiled), black_box(&[&a, &b]))).expect("executes");
    });
    println!(
        "  {:<34} {:.1} ops/exec, {:.1} ns/op effective",
        "throughput",
        64.0,
        t.as_nanos() as f64 / 64.0
    );

    // --- ISA tier: assemble, decode, render, round trip ------------------------------
    println!("\nISA tier (conformance corpus)");
    let corpus = isa_corpus();
    let insns = corpus
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.ends_with(':') && l.trim() != ".text")
        .count();
    let obj = mlkp_machine::isa_assemble(corpus).expect("assembles");
    println!(
        "  {:<34} {} bytes, {} instructions",
        "corpus shape",
        obj.text.len(),
        insns
    );
    time_it("assemble corpus (one pass)", 200, |_| {
        black_box(mlkp_machine::isa_assemble(black_box(corpus))).expect("assembles");
    });
    time_it("decode corpus bytes", 200, |_| {
        black_box(mlkp_machine::isa_decode_bytes(black_box(&obj.text))).expect("decodes");
    });
    let decoded = mlkp_machine::isa_decode_bytes(&obj.text).expect("decodes");
    time_it("render corpus", 200, |_| {
        for insn in black_box(&decoded) {
            black_box(mlkp_machine::isa_render_insn(insn));
        }
    });
    time_it("full text round trip", 100, |_| {
        let o = mlkp_machine::isa_assemble(black_box(corpus)).expect("assembles");
        let d = mlkp_machine::isa_decode_bytes(&o.text).expect("decodes");
        for insn in black_box(&d) {
            black_box(mlkp_machine::isa_render_insn(insn));
        }
    });
    println!("\ndone");
}

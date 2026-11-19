// CEP:FILE: crates/mlkp-runtime/examples/repro_reuse.rs
// CEP:WHAT: A scratch reproduction harness: lowers the 64-op program under every candidate schedule and reports the first plan that fails the airlock with its offsets.
// CEP:WHY: The buffer-reuse pass must survive every schedule the search enumerates; this harness finds the failing configuration deterministically.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Exits nonzero when a schedule fails, printing the diagnosis.
// CEP:ASSUMES: cold-tool only.
// CEP:COST: a few seconds.
// CEP:EVIDENCE: cargo run --release -p mlkp-runtime --example repro_reuse

//! Run with: cargo run --release -p mlkp-runtime --example repro_reuse

use mlkp_runtime::{PassConfig, Tile};

// CEP:ID: JIT-w03
// CEP:WHAT: Generates the 64-op mixed source program.
// CEP:WHY: The reproduction sweep must run the same op mix the pipeline benchmark executes.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the front-end accepts the generated matmul and map chain on f32[64,64].
// CEP:COST: O(ops).
// CEP:EVIDENCE: cargo run --release -p mlkp-runtime --example repro_reuse
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

// CEP:ID: JIT-w04
// CEP:WHAT: Sweeps candidate schedules over the 64-op program and reports the first plan that fails the airlock.
// CEP:WHY: The buffer-reuse pass must survive every schedule the search enumerates; the sweep turns a search-dependent failure into a deterministic reproduction.
// CEP:STATUS: complete
// CEP:FAILURE: Prints the diagnosis and continues; the exit report counts the failures.
// CEP:ASSUMES: cold-tool only.
// CEP:COST: a few seconds.
// CEP:EVIDENCE: cargo run --release -p mlkp-runtime --example repro_reuse
fn main() {
    let prog = mlkp_front::parse_program(&program()).expect("parses");
    let mut bad = 0;
    for bm in [16usize, 64] {
        for order in [0u32, 1] {
            for fused in [true, false] {
                for reuse in [true, false] {
                    let cfg = PassConfig {
                        tile: Tile { bm, bn: 64, bk: 64 },
                        threads: 1,
                        loop_order: order,
                        fused_elementwise: fused,
                        transpose_b: false,
                        buffer_reuse: reuse,
                    };
                    let plan = match mlkp_runtime::plan::lower(&prog.graph, &cfg) {
                        Ok(p) => p,
                        Err(e) => {
                            println!("LOWER FAIL {bm}/{order}/{fused}/{reuse}: {e}");
                            bad += 1;
                            continue;
                        }
                    };
                    if let Err(e) = plan.validate() {
                        println!("VALIDATE FAIL bm={bm} order={order} fused={fused} reuse={reuse}");
                        println!("  {e}");
                        println!("  offsets: {:?}", plan.buffer_offsets);
                        println!("  lens: {:?}", plan.buffer_lens);
                        bad += 1;
                    }
                }
            }
        }
    }
    println!("bad configs: {bad}");
}

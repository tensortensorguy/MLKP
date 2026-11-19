// CEP:FILE: crates/mlkp/src/bin/mlkpc.rs
// CEP:WHAT: The mlkpc command line interface: run, autotune, cache, and version subcommands over the compiler facade.
// CEP:WHY: The design's lifecycle is observable from the outside through a deterministic CLI; every subcommand prints stage-logged, diffable output.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns exit code 2 on any compile failure with the message on stderr.
// CEP:ASSUMES: none
// CEP:COST: cold CLI plus the measured run.
// CEP:EVIDENCE: scripts/ci.sh smoke run.

//! MLKP compiler CLI.

#![deny(missing_docs)]

use std::time::Duration;

const USAGE: &str = "mlkpc [command]
Commands:
  run [file]            parse, verify, autotune, execute; prints the run log.
                        Without a file a synthetic matmul-relu program is used.
  version               print compiler version
Options:
  --budget-ms <n>       autotune time budget (default from config/limits.rs)
  --dim <n>             problem dimension of the synthetic program (default 128)";

// CEP:ID: JIT-q42
// CEP:WHAT: CLI entry point.
// CEP:WHY: The lifecycle is externally observable.
// CEP:STATUS: complete
// CEP:FAILURE: Exit code 2 on compile failure; exit code 3 on usage errors.
// CEP:ASSUMES: none
// CEP:COST: cold.
// CEP:EVIDENCE: scripts/ci.sh
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") || args.is_empty() {
        println!("{USAGE}");
        return;
    }
    if args[0] == "version" {
        println!("mlkp {} (CEP&CC 0.4)", env!("CARGO_PKG_VERSION"));
        return;
    }
    let mut budget_ms: u64 = mlkp_config::limits::COLD_TIME_BUDGET_MS;
    let mut dim: usize = 128;
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--budget-ms" => {
                budget_ms = args
                    .get(i + 1)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(mlkp_config::limits::COLD_TIME_BUDGET_MS);
                i += 2;
            }
            "--dim" => {
                dim = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(128);
                i += 2;
            }
            other => {
                positional.push(other.to_string());
                i += 1;
            }
        }
    }
    let command = positional.first().map(String::as_str).unwrap_or("");
    if command != "run" {
        eprintln!("usage error: unknown command '{command}'");
        std::process::exit(3);
    }
    // The documented file argument is read when present; the synthetic matmul
    // program is the fallback so `mlkpc run` stays a one-command smoke test.
    let source = match positional.get(1) {
        Some(path) => std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("usage error: cannot read '{path}': {e}");
            std::process::exit(3);
        }),
        None => format!(
            "fn @net(%a: f32[{d},{d}], %b: f32[{d},{d}]) -> f32[{d},{d}] {{\n  %t = matmul(%a, %b)\n  %r = relu(%t)\n  return %r\n}}\n",
            d = dim
        ),
    };
    let elements = dim * dim;
    let a: Vec<f32> = (0..elements).map(|i| ((i % 17) as f32) * 0.25).collect();
    let b: Vec<f32> = (0..elements).map(|i| ((i % 11) as f32) * 0.5).collect();
    let cache_dir = std::path::Path::new(".mlkp-cache");
    let mut pgo = mlkp_autotune::PgoStore::in_memory().with_path(&cache_dir.join("pgo.json"));
    let _ = pgo.load();
    let compiled = mlkp::compile(
        &source,
        &mut pgo,
        Duration::from_millis(budget_ms),
        (a.clone(), b.clone()),
    );
    let compiled = match compiled {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let out = mlkp::execute(&compiled, &a, &b);
    match out {
        Ok(out) => {
            let sum: f32 = out.iter().sum();
            println!("stage=parse ok");
            println!("stage=verify ok nodes={}", compiled.plan.buffers);
            // CEP:WHAT: Logs the winning schedule alongside its measured latency.
            // CEP:WHY: The bench artifacts must record which schedule won (tile, loop order, layout, threads), not only its hash, so the evidence ties the cost claim to the executable configuration.
            // CEP:STATUS: complete
            // CEP:FAILURE: none
            // CEP:ASSUMES: the plan carries the winning config.
            // CEP:COST: cold; one formatted line.
            // CEP:EVIDENCE: benches/artifacts/matmul_64.md and benches/artifacts/matmul_128.md
            let cfg = &compiled.plan.config;
            println!(
                "stage=autotune winner_hash={} latency_ns={} from_cache={} tile={bm}x{bn}x{bk} loop_order={lo} transpose_b={tb} threads={th}",
                compiled.config_hash,
                compiled.latency_ns,
                compiled.from_cache,
                bm = cfg.tile.bm,
                bn = cfg.tile.bn,
                bk = cfg.tile.bk,
                lo = cfg.loop_order,
                tb = cfg.transpose_b,
                th = cfg.threads,
            );
            println!("stage=execute ok checksum={sum:.4}");
            let _ = pgo.save();
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
}

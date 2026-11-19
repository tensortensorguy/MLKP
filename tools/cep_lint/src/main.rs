// CEP:FILE: tools/cep_lint/src/main.rs
// CEP:WHAT: Command-line entry point implementing the invocation contract of chapter 50.1: scan, self-test, help, and version with the configured exit codes.
// CEP:WHY: The tool is the mechanical enforcement gate; its CLI behavior, environment override, and exit codes are part of the repository contract, not implementation details.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: Exit code 2 with a CEP-LINT-BOOT message on bootstrap failure; exit code 3 on usage errors; exit code 1 on failing findings or failed self-test scenarios.
// CEP:ASSUMES: The default configuration path is .cep/cep_lint.json relative to the working directory unless CEP_LINT_CONFIG is set.
// CEP:COST: cold; proportional to scanned input size.
// CEP:EVIDENCE: cep_lint --self-test in scripts/ci.sh; integration tests in tools/cep_lint/tests/manifest_test.rs.

use std::path::PathBuf;
use std::process::ExitCode;

use cep_lint::config::{self, Config};
use cep_lint::report;
use cep_lint::{selftest, TOOL_VERSION};

const USAGE: &str = "cep_lint [options] <files or directories>
Options:
  --config, -c <path>  lint configuration JSON (default .cep/cep_lint.json; env CEP_LINT_CONFIG)
  --self-test, -t      run the self-test manifest and exit
  --verify-hunt        verify the signed Hunt and HUNTER receipts and exit
  --public-key <seed-file> <out.pub>
                       derive the Ed25519 public key from a 32-byte seed file (CI tooling)
  --sign-receipt <payload.json> <seed-file> <out.json>
                       sign one receipt payload with a 32-byte seed file (CI tooling)
  --help, -h           show usage
  --version, -v        print tool and standard versions";

// CEP:ID: CS-119
// CEP:WHAT: CLI entry point.
// CEP:WHY: Enforces the exit-code table of chapter 50.1.
// CEP:STATUS: complete
// CEP:FAILURE: See the module-level CEP:FAILURE; all paths return a defined exit code.
// CEP:ASSUMES: none
// CEP:COST: cold; O(input).
// CEP:EVIDENCE: scripts/ci.sh
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut config_path: Option<PathBuf> = None;
    let mut self_test = false;
    let mut verify_hunt = false;
    let mut sign_receipt: Option<(String, String, String)> = None;
    let mut public_key: Option<(String, String)> = None;
    let mut help = false;
    let mut version = false;
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--config" | "-c" => {
                if i + 1 >= args.len() {
                    eprintln!("usage error: --config requires a path");
                    return usage_exit();
                }
                config_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--self-test" | "-t" => {
                self_test = true;
                i += 1;
            }
            "--public-key" => {
                if i + 2 >= args.len() {
                    eprintln!("usage error: --public-key requires seed file and output");
                    return usage_exit();
                }
                public_key = Some((args[i + 1].clone(), args[i + 2].clone()));
                i += 3;
            }
            "--verify-hunt" => {
                verify_hunt = true;
                i += 1;
            }
            "--sign-receipt" => {
                if i + 3 >= args.len() {
                    eprintln!(
                        "usage error: --sign-receipt requires payload, seed file, and output"
                    );
                    return usage_exit();
                }
                sign_receipt = Some((
                    args[i + 1].clone(),
                    args[i + 2].clone(),
                    args[i + 3].clone(),
                ));
                i += 4;
            }
            "--help" | "-h" => {
                help = true;
                i += 1;
            }
            "--version" | "-v" => {
                version = true;
                i += 1;
            }
            other => {
                if other.starts_with('-') {
                    eprintln!("usage error: unknown option {other}");
                    return usage_exit();
                }
                paths.push(PathBuf::from(other));
                i += 1;
            }
        }
    }
    if help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let load_result = load_config(config_path.as_deref());
    let config = match load_result {
        Ok(c) => c,
        Err(e) => {
            if version {
                println!("cep_lint {TOOL_VERSION} (standard version unavailable)");
                return ExitCode::SUCCESS;
            }
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    if version {
        println!(
            "cep_lint {TOOL_VERSION} ({} {})",
            config.standard_name, config.standard_version
        );
        return ExitCode::SUCCESS;
    }
    if let Some((seed_path, out_path)) = public_key {
        let seed_raw = match std::fs::read(&seed_path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("CEP-LINT-BOOT-1: unreadable seed {seed_path}: {e}");
                return ExitCode::from(2);
            }
        };
        let pk = cep_lint::ed25519::public_from_seed(&seed_raw);
        if let Err(e) = std::fs::write(
            out_path,
            format!("{}\n", cep_lint::receipts::hex_encode(&pk)),
        ) {
            eprintln!("CEP-LINT-BOOT-1: cannot write public key: {e}");
            return ExitCode::from(2);
        }
        return ExitCode::SUCCESS;
    }
    if let Some((payload_path, seed_path, out_path)) = sign_receipt {
        let payload = match std::fs::read_to_string(&payload_path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("CEP-LINT-BOOT-1: unreadable payload {payload_path}: {e}");
                return ExitCode::from(2);
            }
        };
        let seed_raw = match std::fs::read(&seed_path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("CEP-LINT-BOOT-1: unreadable seed {seed_path}: {e}");
                return ExitCode::from(2);
            }
        };
        let receipt = match cep_lint::receipts::sign_payload(&payload, &seed_raw) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("CEP-LINT-BOOT-4: {e}");
                return ExitCode::from(2);
            }
        };
        if let Err(e) = std::fs::write(out_path, receipt) {
            eprintln!("CEP-LINT-BOOT-1: cannot write receipt: {e}");
            return ExitCode::from(2);
        }
        return ExitCode::SUCCESS;
    }
    if verify_hunt {
        let findings = cep_lint::run_verify_hunt(&config);
        report::print(&findings);
        return ExitCode::from(report::exit_code(&findings, &config));
    }
    if self_test {
        let manifest_text = match std::fs::read_to_string(&config.self_test_manifest) {
            Ok(t) => t,
            Err(e) => {
                eprintln!(
                    "CEP-LINT-BOOT-1: unreadable self-test manifest {}: {e}",
                    config.self_test_manifest.display()
                );
                return ExitCode::from(2);
            }
        };
        let base = config
            .self_test_manifest
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        match selftest::run_manifest(&manifest_text, &base) {
            Ok(results) => {
                let ok = selftest::print_results(&results);
                if ok {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(1)
                }
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(2)
            }
        }
    } else if paths.is_empty() {
        eprintln!("usage error: no input files or directories");
        usage_exit()
    } else {
        match cep_lint::run_scan(&paths, &config) {
            Ok(findings) => {
                report::print(&findings);
                ExitCode::from(report::exit_code(&findings, &config))
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(2)
            }
        }
    }
}

// CEP:WHAT: Resolves the configuration path from the flag, environment, or default.
// CEP:WHY: Chapter 50.1 defines the default and the CEP_LINT_CONFIG override.
// CEP:STATUS: complete
// CEP:FAILURE: Propagates bootstrap errors.
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
fn load_config(explicit: Option<&std::path::Path>) -> Result<Config, config::BootError> {
    let path: PathBuf = explicit
        .map(|p| p.to_path_buf())
        .or_else(|| std::env::var("CEP_LINT_CONFIG").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(".cep/cep_lint.json"));
    config::load(&path)
}

// CEP:WHAT: Returns the usage exit code.
// CEP:WHY: Exit code 3 is defined for command-line usage errors.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: scripts/ci.sh
fn usage_exit() -> ExitCode {
    ExitCode::from(3)
}

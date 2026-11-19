// CEP:FILE: tools/cep_lint/src/lib.rs
// CEP:WHAT: Library surface of the cep_lint enforcement tool, including the top-level scan driver that merges document-level and file-level findings.
// CEP:WHY: The binary and the test suites share one driver so the self-test scenarios exercise exactly the production code path.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: Bootstrap failures surface as Err(BootError); everything else becomes findings and an exit code.
// CEP:ASSUMES: none
// CEP:COST: cold; proportional to scanned input size.
// CEP:EVIDENCE: cargo test -p cep_lint.

//! CEP&CC chapter 50 enforcement tool. Mechanism only; every pattern, threshold,
//! severity, message, and option lives in `.cep/cep_lint.json`.

#![deny(missing_docs)]

/// Check primitives (mechanism layer).
pub mod checks;
/// Configuration loading and bootstrap errors.
pub mod config;
/// In-tree Ed25519 for receipt signatures.
pub mod ed25519;
/// Minimal glob matcher for file_globs.
pub mod glob;
/// Rust-aware lexical scanner.
pub mod lex;
/// In-tree backtracking regex engine.
pub mod re;
/// Signed receipt verification for chapters 65 and 67.
pub mod receipts;
/// Finding records, ordering, and exit decisions.
pub mod report;
/// Deterministic file collection and scan driver.
pub mod scan;
/// Self-test manifest engine.
pub mod selftest;
/// FIPS 180-4 SHA-512.
pub mod sha512;

use std::path::PathBuf;

use crate::config::Config;
use crate::report::Finding;

/// Tool version reported by --version.
pub const TOOL_VERSION: &str = "0.1.0";

/// Runs the invocation-mode receipt verification (chapters 65 and 67).
// CEP:WHAT: Runs the --verify-hunt receipt gate.
// CEP:WHY: Commits touching cycle-exact code must carry cryptographically signed hunt evidence.
// CEP:STATUS: complete
// CEP:FAILURE: Returns findings for missing, tampered, or insufficient receipts.
// CEP:ASSUMES: none
// CEP:COST: cold; one signature verification per configured receipt rule.
// CEP:EVIDENCE: tools/cep_lint/tests/receipts_test.rs
pub fn run_verify_hunt(config: &Config) -> Vec<Finding> {
    let mut findings = Vec::new();
    for rule in &config.rules {
        if rule.check == "commit_hunt_manifest" || rule.check == "hunter_manifest" {
            findings.extend(receipts::run_receipt_rule(rule, &config.base_dir));
        }
    }
    report::sort(&mut findings);
    findings
}

// CEP:WHAT: Scans the given paths under the given configuration and returns sorted findings.
// CEP:WHY: One driver guarantees that binary runs, self-tests, and CI use identical semantics.
// CEP:STATUS: complete
// CEP:FAILURE: Returns Err(BootError) when the standard document cannot be read for DOC-SYNC.
// CEP:ASSUMES: paths exist.
// CEP:COST: cold; O(total input size).
// CEP:EVIDENCE: cargo test -p cep_lint.
/// Scans the given paths under the configuration; returns sorted findings.
pub fn run_scan(paths: &[PathBuf], config: &Config) -> Result<Vec<Finding>, config::BootError> {
    let mut findings: Vec<Finding> = Vec::new();
    // Document-level DOC-SYNC runs once, before file scans.
    if let Some(params) = &config.doc_sync {
        let doc_text = std::fs::read_to_string(&params.doc_path).map_err(|e| {
            config::BootError::new(
                1,
                format!(
                    "unreadable standard document {}: {e}",
                    params.doc_path.display()
                ),
            )
        })?;
        let severity = config
            .rules
            .iter()
            .find(|r| r.name == "CEP-LINT-DOC-SYNC")
            .map(|r| r.severity)
            .unwrap_or(1);
        for mut f in checks::doc_sync::run_doc_sync(params, config, &doc_text, &params.doc_path) {
            f.severity = severity;
            findings.push(f);
        }
    }
    let files = scan::collect_files(paths, &config.file_globs);
    for path in &files {
        let display = path.display().to_string();
        match std::fs::read_to_string(path) {
            Ok(text) => scan::scan_text(&display, &text, config, &mut findings),
            Err(e) => findings.push(Finding::new(
                1,
                "CEP-LINT-FILE-HEADER",
                &display,
                1,
                1,
                format!("unreadable source file: {e}"),
            )),
        }
    }
    run_rosetta_phase(config, &mut findings)?;
    report::sort(&mut findings);
    Ok(findings)
}

// CEP:WHAT: Runs the tree-level Rosetta checks once per configured rosetta rule.
// CEP:WHY: Registry and heading contracts span the documentation tree, so they run at scan level like DOC-SYNC rather than per source file.
// CEP:STATUS: complete
// CEP:FAILURE: Returns Err(BootError) when a registry-mode rule cannot read its registry.
// CEP:ASSUMES: none
// CEP:COST: cold; O(documentation tree size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn run_rosetta_phase(
    config: &Config,
    findings: &mut Vec<Finding>,
) -> Result<(), config::BootError> {
    for (rule, params) in &config.rosetta {
        let mut produced = checks::rosetta::run_rosetta(rule, params)?;
        for f in &mut produced {
            f.severity = rule.severity;
        }
        findings.extend(produced);
    }
    Ok(())
}

// CEP:WHAT: Runs the lint over in-memory files without touching the filesystem.
// CEP:WHY: The self-test manifest supplies fixture files as strings and asserts exact finding counts.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(fixture size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs.
/// Scans in-memory fixture files; used by the self-test engine.
pub fn run_scan_memory(
    files: &[(String, String)],
    config: &Config,
    include_doc_sync: bool,
) -> Result<Vec<Finding>, config::BootError> {
    let mut findings: Vec<Finding> = Vec::new();
    if include_doc_sync {
        if let Some(params) = &config.doc_sync {
            let doc_text = std::fs::read_to_string(&params.doc_path).map_err(|e| {
                config::BootError::new(
                    1,
                    format!(
                        "unreadable standard document {}: {e}",
                        params.doc_path.display()
                    ),
                )
            })?;
            let severity = config
                .rules
                .iter()
                .find(|r| r.name == "CEP-LINT-DOC-SYNC")
                .map(|r| r.severity)
                .unwrap_or(1);
            for mut f in checks::doc_sync::run_doc_sync(params, config, &doc_text, &params.doc_path)
            {
                f.severity = severity;
                findings.push(f);
            }
        }
    }
    for (name, text) in files {
        scan::scan_text(name, text, config, &mut findings);
    }
    run_rosetta_phase(config, &mut findings)?;
    report::sort(&mut findings);
    Ok(findings)
}

// CEP:FILE: tools/cep_lint/src/report.rs
// CEP:WHAT: The Finding record, deterministic report sorting, and the exit-code decision.
// CEP:WHY: Chapter 50 defines issue ordering (file, line, rule, message) and fail_on severity mapping; both live here and nowhere else.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: none; reporting cannot fail.
// CEP:ASSUMES: none
// CEP:COST: cold; O(findings log findings) for sorting.
// CEP:EVIDENCE: integration tests in tools/cep_lint/tests/manifest_test.rs.

use crate::config::Config;

/// One lint finding.
#[derive(Debug, Clone, PartialEq)]
/// One lint finding.
pub struct Finding {
    /// Severity 0 through 3.
    pub severity: u8,
    /// Rule that produced the finding.
    pub rule: String,
    /// File path as given.
    pub file: String,
    /// One-based line.
    pub line: usize,
    /// One-based column.
    pub col: usize,
    /// Resolved message.
    pub message: String,
}

impl Finding {
    // CEP:WHAT: Constructs a finding.
    // CEP:WHY: Central construction keeps the record shape uniform.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; allocations for the strings.
    // CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
    /// Constructs a finding.
    pub fn new(
        severity: u8,
        rule: &str,
        file: &str,
        line: usize,
        col: usize,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            rule: rule.to_string(),
            file: file.to_string(),
            line,
            col,
            message: message.into(),
        }
    }
}

// CEP:WHAT: Sorts findings by file, line, rule, and message as chapter 50 mandates.
// CEP:WHY: Determinism of output ordering is an exit-code contract.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(n log n).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
/// Sorts findings by file, line, rule, and message as chapter 50 mandates.
pub fn sort(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        (&a.file, a.line, &a.rule, &a.message).cmp(&(&b.file, b.line, &b.rule, &b.message))
    });
}

// CEP:WHAT: Prints all findings and the summary line to stdout.
// CEP:WHY: The tool writes its full result set in one deterministic stream.
// CEP:STATUS: complete
// CEP:FAILURE: none; stdout write errors are ignored because findings are the product, not the stream.
// CEP:ASSUMES: none
// CEP:COST: cold; O(findings).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
/// Prints all findings and the summary line to stdout.
pub fn print(findings: &[Finding]) {
    for f in findings {
        println!(
            "severity {} {} {}:{}:{} {}",
            f.severity, f.rule, f.file, f.line, f.col, f.message
        );
    }
    println!("{} findings", findings.len());
}

// CEP:WHAT: Decides the process exit code from the fail_on severities.
// CEP:WHY: Exit code 1 must map to findings at the configured severities only.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(findings).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
/// Decides the process exit code from the fail_on severities.
pub fn exit_code(findings: &[Finding], config: &Config) -> u8 {
    let failing = findings
        .iter()
        .any(|f| config.fail_on.contains(&f.severity));
    if failing {
        config.exit_codes[1]
    } else {
        config.exit_codes[0]
    }
}

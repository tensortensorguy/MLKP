// CEP:FILE: tools/cep_lint/src/scan.rs
// CEP:WHAT: Deterministic file collection and the per-file scan driver that dispatches configured rules to check primitives.
// CEP:WHY: Chapter 50 requires sorted traversal and sorted findings so output never depends on filesystem iteration order.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: Unreadable individual files produce a finding, not a crash, because a file that cannot be linted is itself a defect.
// CEP:ASSUMES: Input paths exist; enforced by the CLI before this module runs.
// CEP:COST: cold; O(total input size).
// CEP:EVIDENCE: integration tests in tools/cep_lint/tests/manifest_test.rs.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::glob;
use crate::lex;
use crate::report::Finding;

/// Maximum accepted file size for a first-party source file.
const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;

/// Collects first-party source files under the given paths, sorted, filtered by globs.
// CEP:WHAT: Collects first-party files under the given paths, sorted and deduplicated.
// CEP:WHY: Deterministic traversal is an exit-code contract; directory iteration order must never leak into output.
// CEP:STATUS: complete
// CEP:FAILURE: none; unreadable directories are skipped.
// CEP:ASSUMES: none
// CEP:COST: cold; O(files).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn collect_files(paths: &[PathBuf], globs: &[String]) -> Vec<PathBuf> {
    // CEP:WHAT: Anchors glob matching to the process working directory.
    // CEP:WHY: Matching against the walked root made file selection depend on the invocation path (cep_lint tools vs cep_lint . collected different sets); a scan that silently collects zero files is fail-open.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: the process CWD is the repository root in CI; explicit paths outside the CWD match only when the configured globs name them absolutely.
    // CEP:COST: O(files).
    // CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
    let cwd = std::env::current_dir().ok();
    let mut out = Vec::new();
    for p in paths {
        if p.is_file() {
            if glob_is_first_party(&cwd_relative(p, cwd.as_deref()), globs) {
                out.push(p.clone());
            }
            continue;
        }
        walk_dir(p, globs, &mut out, cwd.as_deref());
    }
    out.sort();
    out.dedup();
    out
}

// CEP:WHAT: Normalizes one path to a forward-slash string relative to the CWD when possible.
// CEP:WHY: Glob patterns are written against forward-slash component paths; backslash separators and invocation-dependent prefixes must not change the match result.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(path length).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn cwd_relative(path: &std::path::Path, cwd: Option<&std::path::Path>) -> String {
    let stripped = match cwd {
        Some(c) => path.strip_prefix(c).unwrap_or(path),
        None => path,
    };
    stripped.to_str().unwrap_or_default().replace('\\', "/")
}

// CEP:WHAT: Recursively walks one directory in sorted order.
// CEP:WHY: Deterministic traversal is a chapter 50 exit-code contract.
// CEP:STATUS: complete
// CEP:FAILURE: Unreadable subdirectories are skipped silently; a directory that cannot be read cannot produce findings either way.
// CEP:ASSUMES: recursion depth is bounded by the filesystem depth.
// CEP:COST: cold; O(files).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn walk_dir(dir: &Path, globs: &[String], out: &mut Vec<PathBuf>, cwd: Option<&Path>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    let mut names: Vec<(PathBuf, bool)> = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        let is_dir = path.is_dir();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        if name == ".git" || name == "target" || name == "quarantine" {
            continue;
        }
        names.push((path, is_dir));
    }
    names.sort();
    for (path, is_dir) in names {
        if is_dir {
            walk_dir(&path, globs, out, cwd);
        } else if glob_is_first_party(&cwd_relative(&path, cwd), globs) {
            out.push(path.clone());
        }
    }
}

// CEP:WHAT: Tests whether a file is first-party under the configured globs.
// CEP:WHY: Explicitly given files must match the first-party set to be linted.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(globs).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn glob_is_first_party(candidate: &str, globs: &[String]) -> bool {
    globs.iter().any(|g| glob::matches(candidate, g))
}

/// Runs every configured rule over one file's text.
// CEP:WHAT: Runs every configured rule over one file's text.
// CEP:WHY: One per-file driver keeps rule dispatch identical for binary runs and self-tests.
// CEP:STATUS: complete
// CEP:FAILURE: Oversized files produce a finding rather than a crash.
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size * rules).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn scan_text(file: &str, text: &str, config: &Config, findings: &mut Vec<Finding>) {
    if text.len() > MAX_FILE_BYTES {
        findings.push(Finding::new(
            1,
            "CEP-LINT-FILE-HEADER",
            file,
            1,
            1,
            "source file exceeds the maximum accepted size",
        ));
        return;
    }
    let lexed = lex::lex(text);
    for rule in &config.rules {
        let found = crate::checks::run(rule, file, &lexed);
        findings.extend(found);
    }
}

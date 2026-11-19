// CEP:FILE: tools/cep_lint/src/checks/topology.rs
// CEP:WHAT: The file_topology_policy primitive: per-class line-count limits and the junk-drawer file-name ban over scanned source files.
// CEP:WHY: Chapter 62 fixes hard file-size limits per declared CEP class and bans util/helper/misc/common file names; rule CEP-LINT-ATOMIC-FILE carries the limits and the banned names in configuration.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: none; a file without a declared class falls back to the configured default limit, and a name check never aborts the run.
// CEP:ASSUMES: the CEP:CLASS field, when present, sits in the file-header comment block; LOC is the mechanical proxy for chapter 62.1.2's "lines of code", counted as non-blank, non-comment-only lines.
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs.

use crate::checks::{
    cep_field, comment_blocks, header_block, param_str, param_strs, param_u64, template,
};
use crate::config::RuleConfig;
use crate::lex::Lexed;
use crate::report::Finding;

/// Runs the file topology policy over one file.
// CEP:ID: CS-141
// CEP:WHAT: Checks one file's line count and file name against the configured class limits and banned names.
// CEP:WHY: Chapter 62.5 mandates mechanical limits; the limit selection follows the file's declared class and the default limit covers files without one.
// CEP:STATUS: complete
// CEP:FAILURE: Produces findings for oversized files and junk-drawer names; never aborts the run.
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn file_topology_policy(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    let mut out = Vec::new();
    let class_field = param_str(rule, "class_field", "CEP:CLASS");
    let default_limit = param_u64(rule, "default_limit", 800);
    let blocks = comment_blocks(lexed);
    let class = header_block(&blocks)
        .and_then(|b| cep_field(&b.text, &class_field))
        .unwrap_or_default();
    let limits = rule
        .params
        .get("line_limits")
        .and_then(|v| v.as_obj())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|(k, v)| v.as_u64().map(|n| (k.clone(), n)))
                .collect::<Vec<(String, u64)>>()
        })
        .unwrap_or_default();
    let limit = limits
        .iter()
        .find(|(k, _)| *k == class)
        .map(|(_, n)| *n)
        .unwrap_or(default_limit);
    // LOC is the standard's own unit (chapter 62.1.2 says "lines of code (LOC)"):
    // non-blank, non-comment-only lines, so mandated documentation never inflates
    // the count of a genuinely atomic file.
    let lines = lexed
        .lines
        .iter()
        .filter(|l| !l.code.trim().is_empty())
        .count() as u64;
    if lines > limit {
        out.push(Finding::new(
            rule.severity,
            &rule.name,
            file,
            1,
            1,
            template(
                &rule.message,
                &[(
                    "detail",
                    format!("file has {lines} lines of code; class {class} limit is {limit}"),
                )],
            ),
        ));
    }
    for junk in param_strs(rule, "junk_names") {
        if stem_of(file).contains(&junk) {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                file,
                1,
                1,
                template(
                    &rule.message,
                    &[(
                        "detail",
                        format!("file name contains banned junk-drawer token {junk}"),
                    )],
                ),
            ));
        }
    }
    out
}

// CEP:WHAT: Extracts the file stem (final component without extension) from a display path.
// CEP:WHY: The junk-drawer ban applies to the file name, not the directory path.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the whole input when no separator or extension exists.
// CEP:ASSUMES: forward-slash display paths.
// CEP:COST: O(path length).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn stem_of(file: &str) -> String {
    let base = file.rsplit('/').next().unwrap_or(file);
    match base.rfind('.') {
        Some(pos) if pos > 0 => base[..pos].to_string(),
        _ => base.to_string(),
    }
}

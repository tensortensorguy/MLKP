// CEP:FILE: tools/cep_lint/src/checks/mod.rs
// CEP:WHAT: Check primitive dispatch and shared helpers for comment blocks, CEP field extraction, and message templating.
// CEP:WHY: Chapter 50 mandates that the engine contains mechanism only, so every rule is a primitive plus configuration data, and the shared block and field helpers keep interpretation identical across primitives.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: none; a check returns findings or an empty set, never aborts the run.
// CEP:ASSUMES: none
// CEP:COST: cold; proportional to scanned input size.
// CEP:EVIDENCE: integration tests in tools/cep_lint/tests/manifest_test.rs.

/// Banned-token policy for cycle-exact classes.
pub mod banned;
/// DOC-SYNC document/configuration coverage check.
pub mod doc_sync;
/// Function extraction and function-scoped rules.
pub mod func_policy;
/// Macro prefix policy.
pub mod macros;
/// Numeric literal policy.
pub mod numeric;
/// Region-scoped pattern checks.
pub mod regex_checks;
/// Comment block schema checks.
pub mod schema;

use crate::config::RuleConfig;
use crate::lex::Lexed;
use crate::lex::LineInfo;
use crate::report::Finding;

/// One maximal run of full-line comments.
#[derive(Debug, Clone)]
pub struct CommentBlock {
    /// Zero-based index of the first line of the block.
    pub start_line: usize,
    /// Zero-based index one past the last line of the block.
    pub end_line: usize,
    /// Joined comment text (without `//` markers).
    pub text: String,
}

/// Extracts maximal runs of comment-only lines in source order.
// CEP:WHAT: Extracts maximal runs of full-line comment blocks in source order.
// CEP:WHY: Header and function blocks are located by one shared scanner so field parsing is identical across rules.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn comment_blocks(lexed: &Lexed) -> Vec<CommentBlock> {
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lexed.lines.len() {
        if is_comment_only(&lexed.lines[i]) {
            let start = i;
            let mut text = String::new();
            while i < lexed.lines.len() && is_comment_only(&lexed.lines[i]) {
                if let Some(c) = &lexed.lines[i].comment {
                    text.push_str(c);
                    text.push('\n');
                }
                i += 1;
            }
            blocks.push(CommentBlock {
                start_line: start,
                end_line: i,
                text,
            });
        } else {
            i += 1;
        }
    }
    blocks
}

// CEP:WHAT: Tests whether a line is a full-line comment.
// CEP:WHY: Block extraction needs one shared predicate.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(line length).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn is_comment_only(line: &LineInfo) -> bool {
    // CEP:WHAT: Counts empty comment lines as part of a comment block.
    // CEP:WHY: A bare // divider inside a header block is a visual separator; splitting the block there would report fields that follow it as missing.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
    line.comment.is_some() && line.code.trim().is_empty()
}

/// Returns the first comment block when it starts at line 1 (the file header).
pub fn header_block(blocks: &[CommentBlock]) -> Option<&CommentBlock> {
    blocks.first().filter(|b| b.start_line == 0)
}

/// Reads one CEP field value from a comment block text; accepts the field name with or without the CEP: prefix.
// CEP:WHAT: Reads one CEP field value from a comment block text.
// CEP:WHY: Header and function blocks are parsed with one shared field grammar; configuration may name fields with or without the CEP prefix.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None when the field is absent.
// CEP:ASSUMES: none
// CEP:COST: cold; O(block text).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn cep_field(block_text: &str, field: &str) -> Option<String> {
    let name = field.strip_prefix("CEP:").unwrap_or(field);
    let prefix = format!("CEP:{name}:");
    for line in block_text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(&prefix) {
            return Some(rest.trim().to_string());
        }
    }
    None
}

/// Extracts every `CEP:FIELD: value` pair from a text, in order.
pub fn cep_fields(block_text: &str) -> Vec<(String, String)> {
    block_text.lines().filter_map(cep_field_any).collect()
}

/// Parses one line as a `CEP:FIELD: value` pair.
// CEP:WHAT: Parses one line as a CEP field pair.
// CEP:WHY: Field-agnostic checks (status values, stub requirements) must not hardcode field lists.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None for lines that are not field lines.
// CEP:ASSUMES: none
// CEP:COST: cold; O(line length).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn cep_field_any(line_text: &str) -> Option<(String, String)> {
    let t = line_text.trim();
    if !t.starts_with("CEP:") {
        return None;
    }
    let rest = &t[4..];
    let (name, value) = rest.split_once(':')?;
    Some((name.trim().to_string(), value.trim().to_string()))
}

/// Substitutes `{key}` slots in a configured message template.
// CEP:WHAT: Substitutes key slots in a configured message template.
// CEP:WHY: Message wording lives in configuration; the engine substitutes finding-specific values without hardcoding text.
// CEP:STATUS: complete
// CEP:FAILURE: none; unknown placeholders are left verbatim for review.
// CEP:ASSUMES: none
// CEP:COST: cold; O(template length * pairs).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn template(msg: &str, pairs: &[(&str, String)]) -> String {
    let mut out = msg.to_string();
    for (k, v) in pairs {
        let needle = format!("{{{k}}}");
        out = out.replace(&needle, v);
    }
    out
}

/// Numeric parameter accessor with default.
pub fn param_u64(rule: &RuleConfig, key: &str, default: u64) -> u64 {
    rule.params
        .get(key)
        .and_then(|v| v.as_u64())
        .unwrap_or(default)
}

/// String parameter accessor with default.
pub fn param_str(rule: &RuleConfig, key: &str, default: &str) -> String {
    rule.params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default.to_string())
}

/// String list parameter accessor.
pub fn param_strs(rule: &RuleConfig, key: &str) -> Vec<String> {
    rule.params
        .get(key)
        .and_then(|v| v.as_arr())
        .map(|items| {
            items
                .iter()
                .filter_map(|s| s.as_str().map(|x| x.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Dispatches one rule to its check primitive by configured name.
// CEP:WHAT: Dispatches one rule to its check primitive by configured name.
// CEP:WHY: The engine contains mechanism only; the primitive name selects the mechanism and the parameters carry the policy.
// CEP:STATUS: complete
// CEP:FAILURE: Unknown primitive names are rejected at config load; this arm keeps the match exhaustive.
// CEP:ASSUMES: none
// CEP:COST: cold; O(rule count * file size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn run(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    match rule.check.as_str() {
        "comment_block_schema" => schema::comment_block_schema(rule, file, lexed),
        "function_policy" => func_policy::function_policy(rule, file, lexed),
        "comment_regex" => regex_checks::comment_regex(rule, file, lexed),
        "string_literal_regex" => regex_checks::string_literal_regex(rule, file, lexed),
        "flat_code_regex" => regex_checks::flat_code_regex(rule, file, lexed),
        "numeric_literal_policy" => numeric::numeric_literal_policy(rule, file, lexed),
        "macro_prefix_policy" => macros::macro_prefix_policy(rule, file, lexed),
        "banned_token_policy" => banned::banned_token_policy(rule, file, lexed),
        "commit_hunt_manifest" => Vec::new(),
        "hunter_manifest" => Vec::new(),
        // Document-level rule; the scan driver invokes doc_sync::run_doc_sync separately.
        "doc_rule_coverage" => Vec::new(),
        _ => Vec::new(),
    }
}

// CEP:FILE: tools/cep_lint/src/checks/regex_checks.rs
// CEP:WHAT: The pattern-scoped check primitives: comment_regex, string_literal_regex, and flat_code_regex.
// CEP:WHY: The comment-scoped rules (work markers, declared statuses, commented-out code), string-literal rules, and whole-file structural rules all run configured patterns over one lexical region, so one primitive per region keeps behavior uniform.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: none; invalid patterns are rejected at config load (bootstrap error 8).
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size) per pattern.
// CEP:EVIDENCE: integration tests in tools/cep_lint/tests/manifest_test.rs.

use crate::checks::{param_str, template};
use crate::config::RuleConfig;
use crate::lex::Lexed;
use crate::re;
use crate::report::Finding;

/// Applies a configured pattern to comment text with an optional suppression pattern.
// CEP:ID: CS-r54
// CEP:WHAT: Applies a configured pattern to comment text with an optional suppression pattern.
// CEP:WHY: The comment-scoped rules (work markers, declared statuses, commented-out code) share one region and one semantics.
// CEP:STATUS: complete
// CEP:FAILURE: none; invalid patterns are rejected at load.
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn comment_regex(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    let pattern = param_str(rule, "pattern", "");
    let icase = rule
        .params
        .get("icase")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let suppression = rule
        .params
        .get("suppression_pattern")
        .and_then(|v| v.as_str());
    let compiled = match re::compile(&pattern, icase) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let suppress_re = suppression.and_then(|s| re::compile(s, icase).ok());
    let mut out = Vec::new();
    for line in &lexed.lines {
        let Some(comment) = &line.comment else {
            continue;
        };
        let chars: Vec<char> = comment.chars().collect();
        if compiled.is_match(&chars) {
            if let Some(s) = &suppress_re {
                if s.is_match(&chars) {
                    continue;
                }
            }
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                file,
                line.line_no,
                1,
                template(&rule.message, &[("line", comment.trim().to_string())]),
            ));
        }
    }
    out
}

/// Applies a configured pattern to recorded string literal contents.
// CEP:WHAT: Applies a configured pattern to recorded string literal contents.
// CEP:WHY: Marker text must fail loudly rather than leak into runtime messages.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(literals).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn string_literal_regex(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    let pattern = param_str(rule, "pattern", "");
    let icase = rule
        .params
        .get("icase")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let compiled = match re::compile(&pattern, icase) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for lit in &lexed.strings {
        let chars: Vec<char> = lit.content.chars().collect();
        if compiled.is_match(&chars) {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                file,
                lit.line,
                1,
                template(
                    &rule.message,
                    &[("line", format!("string literal {:?}", lit.content))],
                ),
            ));
        }
    }
    out
}

/// Applies a configured pattern to the flattened masked code with optional name validation.
// CEP:ID: CS-v51
// CEP:WHAT: Applies a configured pattern to the flattened masked code with optional capture-based name validation.
// CEP:WHY: Whole-file structural rules (EMPTY-CATCH, EMPTY-BRANCH, NAMING-TYPE) need cross-line scanning with positions mapped back to source lines.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: when name_pattern and line_group are configured, a match is a finding only if the captured name violates the name pattern.
// CEP:COST: cold; O(flat size) per pattern.
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn flat_code_regex(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    let pattern = param_str(rule, "pattern", "");
    let icase = rule
        .params
        .get("icase")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let compiled = match re::compile(&pattern, icase) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let name_group = rule
        .params
        .get("line_group")
        .and_then(|v| v.as_u64())
        .map(|g| (g as usize).saturating_sub(1));
    let name_re = rule
        .params
        .get("name_pattern")
        .and_then(|v| v.as_str())
        .and_then(|p| re::compile(p, false).ok());
    let mut flat = String::new();
    let mut line_starts: Vec<usize> = Vec::new();
    for line in &lexed.lines {
        line_starts.push(flat.len());
        flat.push_str(&line.code);
        flat.push('\n');
    }
    let chars: Vec<char> = flat.chars().collect();
    let mut out = Vec::new();
    let mut search_from = 0usize;
    while let Some(m) = find_from(&compiled, &chars, search_from) {
        let (start, end) = m.span();
        let mut detail = String::new();
        // CEP:WHAT: Treats a non-participating name capture as a violation.
        // CEP:WHY: Defaulting to accepted when the capture group is absent would let optional groups silently suppress findings; absence of a name is exactly what the rule flags.
        // CEP:STATUS: complete
        // CEP:FAILURE: none
        // CEP:ASSUMES: none
        // CEP:COST: O(1).
        // CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
        let mut violated = true;
        if let (Some(idx), Some(name_rx)) = (name_group, &name_re) {
            if let Some(group) = m.group_chars(&chars, idx) {
                let name: String = group.iter().copied().collect();
                detail = name.clone();
                let gchars: Vec<char> = name.chars().collect();
                if name_rx.is_match(&gchars) {
                    violated = false;
                }
            }
        }
        if violated {
            let (line_no, col) = offset_to_line_col(&line_starts, start);
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                file,
                line_no,
                col,
                template(&rule.message, &[("line", detail)]),
            ));
        }
        search_from = if end > start { end } else { start + 1 };
        if search_from >= chars.len() {
            break;
        }
    }
    out
}

// CEP:WHAT: Finds all non-overlapping matches starting at or after `from`.
// CEP:WHY: The engine's find() reports only the first match; this helper iterates with absolute offsets.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; bounded per position by the engine step budget.
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn find_from(regex: &re::Regex, chars: &[char], from: usize) -> Option<re::Match> {
    if from > chars.len() {
        return None;
    }
    let slice = &chars[from..];
    let m = regex.find(slice)?;
    let (s, e) = m.span();
    let groups = m
        .groups
        .iter()
        .map(|g| g.map(|(gs, ge)| (gs + from, ge + from)))
        .collect();
    Some(re::Match {
        start: s + from,
        end: e + from,
        groups,
    })
}

// CEP:WHAT: Maps a flat-text char offset to a one-based line and column.
// CEP:WHY: Findings must carry source positions.
// CEP:STATUS: complete
// CEP:FAILURE: Returns (1, 1) for offsets before the first line start.
// CEP:ASSUMES: none
// CEP:COST: cold; O(lines) per call.
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn offset_to_line_col(line_starts: &[usize], off: usize) -> (usize, usize) {
    let mut line_no = 1usize;
    for (idx, start) in line_starts.iter().enumerate() {
        if *start <= off {
            line_no = idx + 1;
        } else {
            break;
        }
    }
    let start = line_starts.get(line_no - 1).copied().unwrap_or(0);
    (line_no, off.saturating_sub(start) + 1)
}

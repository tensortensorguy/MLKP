// CEP:FILE: tools/cep_lint/src/checks/numeric.rs
// CEP:WHAT: The numeric_literal_policy primitive: numeric literals outside the allow-list must sit in sanctioned statement contexts.
// CEP:WHY: Law 7 bans silent magic numbers; section 50.3 defines the sanctioned forms as const or static declarations and enum contexts, evaluated over the joined statement text bounded to twelve lines.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: Value comparison is numeric so 0x0 equals 0; tuple field indices (identifier-dot-digit) are not literals and are skipped.
// CEP:COST: cold; O(file size) per file.
// CEP:EVIDENCE: integration tests in tools/cep_lint/tests/manifest_test.rs.

use crate::checks::{param_strs, template};
use crate::config::RuleConfig;
use crate::lex::Lexed;
use crate::re;
use crate::report::Finding;

/// Maximum lines spanned by one joined statement text.
const STATEMENT_WINDOW: usize = 12;

/// Runs the magic-number rule over one scanned file.
// CEP:WHAT: Runs the magic-number rule over one scanned file.
// CEP:WHY: Law 7 bans silent magic numbers; literals outside the allow-list must sit in sanctioned statement contexts evaluated over the joined statement text.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn numeric_literal_policy(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    let mut out = Vec::new();
    let allowed: Vec<f64> = rule
        .params
        .get("allowed_values")
        .and_then(|v| v.as_arr())
        .map(|items| items.iter().filter_map(|x| x.as_f64()).collect())
        .unwrap_or_default();
    let exempt_patterns = param_strs(rule, "exempt_contexts");
    let exempt_res: Vec<re::Regex> = exempt_patterns
        .iter()
        .filter_map(|p| re::compile(p, false).ok())
        .collect();
    for (li, line) in lexed.lines.iter().enumerate() {
        let code: Vec<char> = line.code.chars().collect();
        for (col, ch) in code.iter().copied().enumerate() {
            let next = code.get(col + 1).copied();
            let is_start =
                ch.is_ascii_digit() || (ch == '.' && next.is_some_and(|c| c.is_ascii_digit()));
            if !is_start {
                continue;
            }
            let prev = code.get(col.wrapping_sub(1)).copied().unwrap_or(' ');
            if prev.is_ascii_alphanumeric() || prev == '_' {
                continue; // part of an identifier or suffix such as u32
            }
            if ch == '.' && (col == 0 || prev == '.' || tuple_field_index(&code, col)) {
                continue;
            }
            let Some((text, _len)) = literal_value(&code, col) else {
                continue;
            };
            let value = parse_value(&text);
            if value.is_some_and(|v| allowed.iter().any(|a| (a - v).abs() < f64::EPSILON)) {
                continue;
            }
            let stmt = statement_text(lexed, li, col);
            let stmt_chars: Vec<char> = stmt.chars().collect();
            if exempt_res.iter().any(|r| r.is_match(&stmt_chars)) {
                continue;
            }
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                file,
                line.line_no,
                col + 1,
                template(&rule.message, &[("value", text)]),
            ));
        }
    }
    out
}

// CEP:WHAT: Detects a tuple field access `ident.0`.
// CEP:WHY: Rust tuple indices are digit tokens but not numeric literals.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(window) backwards scan.
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn tuple_field_index(code: &[char], dot: usize) -> bool {
    if dot == 0 {
        return false;
    }
    if code[dot - 1].is_ascii_alphanumeric() || code[dot - 1] == '_' {
        let k = dot + 1;
        return match code.get(k) {
            Some(c) if c.is_ascii_digit() => {
                let next = code.get(k + 1).copied();
                !next.is_some_and(|n| n.is_ascii_digit())
            }
            _ => false,
        };
    }
    false
}

// CEP:WHAT: Extracts the literal token starting at col with prefix, digits, underscores, and suffix.
// CEP:WHY: The token text is needed for numeric comparison and reporting.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None when no digits follow the start position.
// CEP:ASSUMES: none
// CEP:COST: cold; O(token length).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn literal_value(code: &[char], start: usize) -> Option<(String, usize)> {
    let mut k = start;
    let mut text = String::new();
    if code[k] == '.' {
        text.push('.');
        k += 1;
    }
    if code.get(k) == Some(&'0') && matches!(code.get(k + 1), Some('x') | Some('b') | Some('o')) {
        text.push('0');
        text.push(code[k + 1]);
        k += 2;
    }
    let mut saw_digit = false;
    while k < code.len() {
        let c = code[k];
        if c.is_ascii_digit() || c == '_' {
            saw_digit = true;
            text.push(c);
            k += 1;
        } else if !saw_digit && c == '.' {
            text.push(c);
            k += 1;
        } else {
            break;
        }
    }
    if !saw_digit {
        return None;
    }
    if matches!(code.get(k), Some('e') | Some('E')) {
        text.push(code[k]);
        k += 1;
        if matches!(code.get(k), Some('+') | Some('-')) {
            text.push(code[k]);
            k += 1;
        }
        while k < code.len() && (code[k].is_ascii_digit() || code[k] == '_') {
            text.push(code[k]);
            k += 1;
        }
    }
    Some((text, k - start))
}

// CEP:WHAT: Converts a literal token to a numeric value for allow-list comparison.
// CEP:WHY: Value comparison is numeric per chapter 50.3.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None for tokens that are not plain numbers.
// CEP:ASSUMES: none
// CEP:COST: cold; O(token length).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn parse_value(text: &str) -> Option<f64> {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    cleaned.parse::<f64>().ok()
}

// CEP:WHAT: Builds the joined statement text around an occurrence, bounded to twelve lines.
// CEP:WHY: Section 50.3 defines the exemption context as the joined statement text.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(window * line length).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn statement_text(lexed: &Lexed, line_idx: usize, col: usize) -> String {
    let mut start_line = line_idx;
    let mut start_col = 0usize;
    let mut back = 0usize;
    loop {
        if back >= STATEMENT_WINDOW || start_line == 0 {
            break;
        }
        let line = &lexed.lines[start_line];
        let chars: Vec<char> = line.code.chars().collect();
        let upto = if back == 0 {
            col.min(chars.len())
        } else {
            chars.len()
        };
        if let Some(pos) = chars[..upto]
            .iter()
            .rposition(|c| *c == ';' || *c == '{' || *c == '}')
        {
            start_col = pos + 1;
            break;
        }
        start_line -= 1;
        back += 1;
        start_col = 0;
    }
    let mut end_line = line_idx;
    let mut end_col = usize::MAX;
    let mut fwd = 0usize;
    loop {
        if fwd >= STATEMENT_WINDOW || end_line >= lexed.lines.len() {
            break;
        }
        let line = &lexed.lines[end_line];
        let chars: Vec<char> = line.code.chars().collect();
        let from = if fwd == 0 { col.min(chars.len()) } else { 0 };
        if let Some(pos) = chars[from..]
            .iter()
            .position(|c| *c == ';' || *c == '{' || *c == '}')
        {
            end_col = from + pos;
            break;
        }
        end_line += 1;
        fwd += 1;
        end_col = usize::MAX;
    }
    let mut out = String::new();
    let last = end_line.min(lexed.lines.len().saturating_sub(1));
    for (k, line) in lexed.lines[start_line..=last].iter().enumerate() {
        let k = start_line + k;
        let chars: Vec<char> = line.code.chars().collect();
        let s = if k == start_line {
            start_col.min(chars.len())
        } else {
            0
        };
        let e = if k == end_line && end_col != usize::MAX {
            end_col.min(chars.len())
        } else {
            chars.len()
        };
        out.push_str(&chars[s..e].iter().collect::<String>());
        out.push(' ');
    }
    out
}

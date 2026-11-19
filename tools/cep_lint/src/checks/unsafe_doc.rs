// CEP:FILE: tools/cep_lint/src/checks/unsafe_doc.rs
// CEP:WHAT: The unsafe_doc_policy primitive: every code occurrence of the unsafe keyword must be documented by a CEP:UNSAFE field in the enclosing function's comment block or the file header.
// CEP:WHY: Chapters 22.7 and 25.7 require a documented invariant on every unsafe construct; rule CEP-LINT-UNSAFE-DOC makes the documentation mechanical.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: none; occurrences outside any function check the file header only.
// CEP:ASSUMES: masked code lines carry no comments or string literals, so only real keyword occurrences match; the word-boundary test is manual to keep the check free of regex step budgets.
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs.

use crate::checks::{cep_field, comment_blocks, header_block, param_strs, template};
use crate::config::RuleConfig;
use crate::glob;
use crate::lex::Lexed;
use crate::re;
use crate::report::Finding;

/// Runs the unsafe documentation policy over one file.
// CEP:ID: CS-931
// CEP:WHAT: Flags unsafe keyword occurrences that lack a CEP:UNSAFE field nearby.
// CEP:WHY: Each occurrence must be covered by the enclosing function's comment block or by the file header; the nearest preceding comment block is the mechanical proxy for "enclosing".
// CEP:STATUS: complete
// CEP:FAILURE: Produces one finding per undocumented occurrence; never aborts the run.
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn unsafe_doc_policy(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    let mut out = Vec::new();
    for exempt in param_strs(rule, "exempt_paths") {
        if glob::matches(file, &exempt) {
            return out;
        }
    }
    let blocks = comment_blocks(lexed);
    let header = header_block(&blocks);
    let functions = crate::checks::func_policy::extract_functions(lexed);
    let gap = r"^\s*(#.*)?$";
    let gap_re = match re::compile(gap, false) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for (idx, line) in lexed.lines.iter().enumerate() {
        if !has_unsafe_keyword(&line.code) {
            continue;
        }
        let enclosing = functions
            .iter()
            .find(|f| f.body_start <= idx && idx <= f.body_end);
        let block = match enclosing {
            Some(f) => crate::checks::func_policy::find_preceding_block(&blocks, f, lexed, &gap_re),
            None => None,
        };
        let in_function_block = block
            .map(|b| cep_field(&b.text, "CEP:UNSAFE").is_some())
            .unwrap_or(false);
        let in_body = enclosing
            .map(|f| body_has_unsafe_field(lexed, f.body_start, f.body_end))
            .unwrap_or(false);
        let in_header = header
            .map(|b| cep_field(&b.text, "CEP:UNSAFE").is_some())
            .unwrap_or(false);
        if !in_function_block && !in_body && !in_header {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                file,
                idx + 1,
                1,
                template(
                    &rule.message,
                    &[(
                        "detail",
                        "unsafe keyword occurrence lacks a CEP:UNSAFE field in the enclosing block or file header"
                            .to_string(),
                    )],
                ),
            ));
        }
    }
    out
}

// CEP:WHAT: Scans one function body's comments for a CEP:UNSAFE field.
// CEP:WHY: Chapter 22.7 and 25.7 document unsafe blocks with fields placed on or inside the construct; a field anywhere in the enclosing function satisfies the mandate.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: line.comment carries full-line and trailing comments with the marker stripped.
// CEP:COST: O(body size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn body_has_unsafe_field(lexed: &Lexed, start: usize, end: usize) -> bool {
    lexed.lines[start..=end.min(lexed.lines.len() - 1)]
        .iter()
        .filter_map(|l| l.comment.as_ref())
        .any(|c| {
            crate::checks::cep_field_any(c)
                .map(|(n, _)| n == "UNSAFE")
                .unwrap_or(false)
        })
}

// CEP:WHAT: Tests one masked code line for a word-bounded unsafe keyword.
// CEP:WHY: Comments and strings are already masked; the boundary test prevents matching identifiers that merely contain the word.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(line length).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn has_unsafe_keyword(code: &str) -> bool {
    let chars: Vec<char> = code.chars().collect();
    let needle = ['u', 'n', 's', 'a', 'f', 'e'];
    let mut k = 0;
    while k + needle.len() <= chars.len() {
        if chars[k..k + needle.len()] == needle {
            let before_ok = k == 0 || !(chars[k - 1].is_alphanumeric() || chars[k - 1] == '_');
            let after_idx = k + needle.len();
            let after_ok = after_idx == chars.len()
                || !(chars[after_idx].is_alphanumeric() || chars[after_idx] == '_');
            if before_ok && after_ok {
                return true;
            }
        }
        k += 1;
    }
    false
}

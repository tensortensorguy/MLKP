// CEP:FILE: tools/cep_lint/src/checks/banned.rs
// CEP:WHAT: The banned_token_policy primitive: files whose declared class is in the configured set must not contain the configured tokens in code.
// CEP:WHY: Rule CEP-LINT-HOT-BANNED exterminates allocation and dispatch constructs from cycle-exact hot code; tokens are matched after comment and string removal per chapter 50.3.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: The class is read from the file header block; files without headers are other rules' findings.
// CEP:COST: cold; O(file size * tokens).
// CEP:EVIDENCE: integration tests in tools/cep_lint/tests/manifest_test.rs.

use crate::checks::{cep_field, comment_blocks, header_block, param_str, param_strs, template};
use crate::config::RuleConfig;
use crate::lex::Lexed;
use crate::report::Finding;

/// Runs the banned-token rule over one scanned file.
// CEP:WHAT: Runs the banned-token rule over one scanned file.
// CEP:WHY: Cycle-exact classes must be free of allocation and dispatch constructs; the class is read from the file header and only matching classes are token-scanned.
// CEP:STATUS: complete
// CEP:FAILURE: none; tokens found produce findings, missing headers are other rules' findings.
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size * tokens).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn banned_token_policy(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    let mut out = Vec::new();
    let applies = param_strs(rule, "applies_to_classes");
    let class_field = param_str(rule, "class_field", "CEP:CLASS");
    let tokens = param_strs(rule, "tokens");
    let blocks = comment_blocks(lexed);
    let Some(header) = header_block(&blocks) else {
        return out;
    };
    let Some(class) = cep_field(&header.text, &class_field) else {
        return out;
    };
    if !applies.contains(&class) {
        return out;
    }
    for token in &tokens {
        for line in &lexed.lines {
            let mut from = 0usize;
            while let Some(pos) = line.code[from..].find(token.as_str()) {
                let byte_col = from + pos;
                let col = line.code[..byte_col].chars().count() + 1;
                out.push(Finding::new(
                    rule.severity,
                    &rule.name,
                    file,
                    line.line_no,
                    col,
                    template(&rule.message, &[("token", token.clone())]),
                ));
                from = byte_col + token.len();
                if from >= line.code.len() {
                    break;
                }
            }
        }
    }
    out
}

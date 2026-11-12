// CEP:FILE: tools/cep_lint/src/checks/macros.rs
// CEP:WHAT: The macro_prefix_policy primitive: macro definitions must carry the sanctioned CEP_ prefix or guard pattern.
// CEP:WHY: Rule CEP-LINT-MACRO-PREFIX; in Rust sources macro_rules! definitions are the object-like macro analog of #define and obey the same name patterns per the 0.3 applicability note.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: integration tests in tools/cep_lint/tests/manifest_test.rs.

use crate::checks::{param_str, template};
use crate::config::RuleConfig;
use crate::lex::Lexed;
use crate::re;
use crate::report::Finding;

/// Runs the macro-prefix rule over one scanned file.
// CEP:WHAT: Runs the macro-prefix rule over one scanned file.
// CEP:WHY: Macro definitions must carry the sanctioned prefix so generated control flow stays auditable.
// CEP:STATUS: complete
// CEP:FAILURE: none; invalid configured patterns are rejected at load.
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn macro_prefix_policy(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    let mut out = Vec::new();
    let name_pattern = param_str(rule, "name_pattern", "^CEP_[A-Z0-9_]+$");
    let name_re = match re::compile(&name_pattern, false) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for line in &lexed.lines {
        let code: Vec<char> = line.code.chars().collect();
        if let Some(name) = macro_name_rust(&code) {
            let nchars: Vec<char> = name.chars().collect();
            if !name_re.is_match(&nchars) {
                out.push(Finding::new(
                    rule.severity,
                    &rule.name,
                    file,
                    line.line_no,
                    1,
                    template(&rule.message, &[("name", name)]),
                ));
            }
        }
    }
    out
}

// CEP:WHAT: Extracts the name after `macro_rules!`.
// CEP:WHY: The Rust macro definition form needs name extraction for the prefix check.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None when the line does not define a macro.
// CEP:ASSUMES: none
// CEP:COST: cold; O(line length).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn macro_name_rust(code: &[char]) -> Option<String> {
    const NEEDLE: [char; 12] = ['m', 'a', 'c', 'r', 'o', '_', 'r', 'u', 'l', 'e', 's', '!'];
    for k in 0..code.len() {
        if code.len() >= k + NEEDLE.len() && code[k..k + NEEDLE.len()] == NEEDLE {
            let mut m = k + NEEDLE.len();
            while m < code.len() && (code[m] == ' ' || code[m] == '\t') {
                m += 1;
            }
            let start = m;
            while m < code.len() && (code[m].is_alphanumeric() || code[m] == '_') {
                m += 1;
            }
            if m > start {
                return Some(code[start..m].iter().collect());
            }
        }
    }
    None
}

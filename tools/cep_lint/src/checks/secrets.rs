// CEP:FILE: tools/cep_lint/src/checks/secrets.rs
// CEP:WHAT: The secret_probe_policy primitive: lexical probing of source lines for secret-shaped content such as credential assignments, private-key headers, and cloud access-key prefixes.
// CEP:WHY: Chapters 10.14, 22.13, and 34.3 ban secrets in first-party sources; rule CEP-LINT-SECRET-SCAN carries the patterns and exemptions in configuration.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: none; an invalid pattern is rejected at configuration load, so matching never silently degrades.
// CEP:ASSUMES: probing is lexical only; obfuscated secrets require the review stage of chapter 34.3. Line-oriented probing covers comments, string literals, and code uniformly because a secret-shaped assignment is a defect in any region.
// CEP:COST: cold; O(lines * patterns).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs.

use crate::checks::{param_str, param_strs, template};
use crate::config::RuleConfig;
use crate::glob;
use crate::lex::Lexed;
use crate::re;
use crate::report::Finding;

/// Runs the secret probe over one file.
// CEP:WHAT: Probes each raw source line against the configured secret patterns.
// CEP:WHY: A secret-shaped assignment is a defect in comments, string literals, and code alike, so the probe reads the raw text rather than a masked region.
// CEP:STATUS: complete
// CEP:FAILURE: Produces one finding per non-suppressed match; never aborts the run.
// CEP:ASSUMES: none
// CEP:COST: cold; O(lines * patterns).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn secret_probe_policy(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    let mut out = Vec::new();
    for exempt in param_strs(rule, "exempt_paths") {
        if glob::matches(file, &exempt) {
            return out;
        }
    }
    let suppression = param_str(rule, "suppression_pattern", "");
    let suppress_re = if suppression.is_empty() {
        None
    } else {
        re::compile(&suppression, false).ok()
    };
    for pattern in param_strs(rule, "patterns") {
        let compiled = match re::compile(&pattern, false) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for (idx, line) in lexed.lines.iter().enumerate() {
            let chars: Vec<char> = line.raw.chars().collect();
            if let Some(sup) = &suppress_re {
                if sup.is_match(&chars) {
                    continue;
                }
            }
            if let Some(m) = compiled.find(&chars) {
                out.push(Finding::new(
                    rule.severity,
                    &rule.name,
                    file,
                    idx + 1,
                    m.start + 1,
                    template(
                        &rule.message,
                        &[("detail", format!("secret-shaped content matches {pattern}"))],
                    ),
                ));
            }
        }
    }
    out
}

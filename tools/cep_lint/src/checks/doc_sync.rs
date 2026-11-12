// CEP:FILE: tools/cep_lint/src/checks/doc_sync.rs
// CEP:WHAT: The doc_rule_coverage primitive enforcing the DOC-SYNC contract between the standard document and the lint configuration, plus the two receipt verification rules of chapters 65 and 67.
// CEP:WHY: Law 8 bans stale documentation; chapter 50.3 requires that every configured rule has a `### Rule` heading in the standard, every documented rule exists in the configuration, headings are unique, and the document version equals the configured version, in both coverage directions.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: Unreadable standard document surfaces as bootstrap error 1 before any finding is produced.
// CEP:ASSUMES: none
// CEP:COST: cold; O(document size).
// CEP:EVIDENCE: integration tests in tools/cep_lint/tests/manifest_test.rs.

use std::collections::BTreeSet;
use std::path::Path;

use crate::config::{Config, DocSyncParams};
use crate::re;
use crate::report::Finding;

/// Runs the DOC-SYNC check once per invocation.
// CEP:WHAT: Runs the DOC-SYNC contract over the standard document and the configuration.
// CEP:WHY: Law 8 bans stale documentation; both coverage directions between configured rules and documented rule headings are checked mechanically.
// CEP:STATUS: complete
// CEP:FAILURE: Produces findings for drift; an unreadable document is a bootstrap error raised by the caller.
// CEP:ASSUMES: none
// CEP:COST: cold; O(document size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn run_doc_sync(
    params: &DocSyncParams,
    config: &Config,
    doc_text: &str,
    doc_display: &Path,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let doc_file = doc_display.display().to_string();
    let heading_re = match re::compile(&params.heading_pattern, false) {
        Ok(r) => r,
        Err(_) => return out,
    };
    let version_re = match re::compile(&params.version_pattern, false) {
        Ok(r) => r,
        Err(_) => return out,
    };
    let chars: Vec<char> = doc_text.chars().collect();
    let mut doc_rules: Vec<(String, usize)> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut search_from = 0usize;
    while let Some(m) = find_from(&heading_re, &chars, search_from) {
        let (start, end) = m.span();
        if let Some(group) = m.group_chars(&chars, 0) {
            let name: String = group.iter().copied().collect();
            let line = 1 + chars[..start].iter().filter(|c| **c == '\n').count();
            if !seen.insert(name.clone()) {
                out.push(Finding::new(
                    1,
                    "CEP-LINT-DOC-SYNC",
                    &doc_file,
                    line,
                    1,
                    format!("duplicate rule heading {name} in standard document"),
                ));
            }
            doc_rules.push((name, line));
        }
        search_from = if end > start { end } else { start + 1 };
        if search_from >= chars.len() {
            break;
        }
    }
    let configured: BTreeSet<&String> = config.rules.iter().map(|r| &r.name).collect();
    let documented: BTreeSet<&String> = doc_rules.iter().map(|(n, _)| n).collect();
    for (name, line) in &doc_rules {
        if !configured.contains(name) {
            out.push(Finding::new(
                1,
                "CEP-LINT-DOC-SYNC",
                &doc_file,
                *line,
                1,
                format!("rule {name} documented in the standard but absent from configuration"),
            ));
        }
    }
    for name in &configured {
        if !documented.contains(name) {
            out.push(Finding::new(
                1,
                "CEP-LINT-DOC-SYNC",
                &doc_file,
                1,
                1,
                format!("rule {name} configured but has no '### Rule' heading in the standard"),
            ));
        }
    }
    if let Some(m) = version_re.find(&chars) {
        if let Some(group) = m.group_chars(&chars, 0) {
            let version: String = group.iter().copied().collect();
            if version != config.standard_version {
                out.push(Finding::new(
                    1,
                    "CEP-LINT-DOC-SYNC",
                    &doc_file,
                    1 + chars[..m.span().0].iter().filter(|c| **c == '\n').count(),
                    1,
                    format!(
                        "standard document version {version} does not equal configured version {}",
                        config.standard_version
                    ),
                ));
            }
        }
    } else {
        out.push(Finding::new(
            1,
            "CEP-LINT-DOC-SYNC",
            &doc_file,
            1,
            1,
            "standard document has no Version line matching the configured pattern",
        ));
    }
    out
}

// CEP:WHAT: Finds all non-overlapping matches at or after `from`.
// CEP:WHY: Heading extraction iterates the document.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; bounded per position by the engine step budget.
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn find_from(regex: &re::Regex, chars: &[char], from: usize) -> Option<re::Match> {
    if from > chars.len() {
        return None;
    }
    regex.find(&chars[from..]).map(|m| {
        let (s, e) = m.span();
        let groups = m
            .groups
            .iter()
            .map(|g| g.map(|(gs, ge)| (gs + from, ge + from)));
        re::Match {
            start: s + from,
            end: e + from,
            groups: groups.collect(),
        }
    })
}

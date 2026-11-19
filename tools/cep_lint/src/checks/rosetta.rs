// CEP:FILE: tools/cep_lint/src/checks/rosetta.rs
// CEP:WHAT: The rosetta_policy primitive: registry/documentation-tree agreement in both directions, the dual-tier heading mandate, and the silent-failure checklist mandate over the documentation tree.
// CEP:WHY: Chapter 68 mandates that every registered function has a Markdown file, every file corresponds to a registry entry, and every file carries the tier and checklist sections; rules CEP-LINT-DOC-FILE, CEP-LINT-DOC-TIERS, and CEP-LINT-DOC-INQUISITION share this primitive in three modes.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: A missing registry file is a bootstrap error (code 1) when the registry mode is configured, per chapter 50.3; an invalid registry document is a bootstrap error (code 2).
// CEP:ASSUMES: the registry document is strict JSON shaped {"functions": [{"id", "file", "line"}]}; documentation files live under docs_prefix as DOMAIN/ID.md; headings are compared against trimmed lines.
// CEP:COST: cold; O(documentation tree size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs.

use std::path::{Path, PathBuf};

use crate::config::{BootError, RosettaParams, RuleConfig};
use crate::report::Finding;

/// Runs one rosetta mode over the documentation tree.
// CEP:WHAT: Runs one rosetta mode: registry agreement, tier headings, or checklist presence.
// CEP:WHY: The three rules of chapter 68.6.1 share one tree walk and one sorted, deterministic output order.
// CEP:STATUS: complete
// CEP:FAILURE: Returns Err for bootstrap-level failures (unreadable or malformed registry); findings otherwise.
// CEP:ASSUMES: none
// CEP:COST: cold; O(tree size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn run_rosetta(rule: &RuleConfig, params: &RosettaParams) -> Result<Vec<Finding>, BootError> {
    match rule.mode.as_str() {
        "registry" => registry_mode(rule, params),
        "tiers" => heading_mode(
            rule,
            params,
            &[&params.tier1_heading, &params.tier2_heading],
        ),
        "inquisition" => heading_mode(rule, params, &[&params.checklist_heading]),
        _ => Ok(Vec::new()),
    }
}

// CEP:ID: CS-s19
// CEP:WHAT: Checks registry entries against the documentation tree and the tree against the registry.
// CEP:WHY: Chapter 68.2.3 requires both directions so a stale registry and an orphan file are both defects.
// CEP:STATUS: complete
// CEP:FAILURE: Missing or malformed registry surfaces as a bootstrap error.
// CEP:ASSUMES: none
// CEP:COST: cold; O(tree size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn registry_mode(rule: &RuleConfig, params: &RosettaParams) -> Result<Vec<Finding>, BootError> {
    let text = std::fs::read_to_string(&params.registry_path).map_err(|e| {
        BootError::new(
            1,
            format!(
                "unreadable function registry {}: {e}",
                params.registry_path.display()
            ),
        )
    })?;
    let value = cep_json::parse(&text)
        .map_err(|e| BootError::new(2, format!("registry parse failure: {e}")))?;
    let entries = value
        .get("functions")
        .and_then(|v| v.as_arr())
        .ok_or_else(|| BootError::new(4, "registry missing key: functions"))?;
    let mut out = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    for entry in entries {
        let id = entry
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| BootError::new(4, "registry entry missing id"))?
            .to_string();
        let (domain, _) = split_id(&id);
        // Chapter 68.6 names each file with the full id: docs/functions/DOMAIN/ID.md.
        let path = params.docs_prefix.join(&domain).join(format!("{id}.md"));
        if !path.is_file() {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                &params.registry_path.display().to_string(),
                1,
                1,
                format!(
                    "registry entry {id} has no documentation file at {}",
                    path.display()
                ),
            ));
        }
        ids.push(id);
    }
    let mut seen: Vec<String> = Vec::new();
    for id in &ids {
        if seen.contains(id) {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                &params.registry_path.display().to_string(),
                1,
                1,
                format!("registry id {id} is duplicated"),
            ));
        }
        seen.push(id.clone());
    }
    let registered: Vec<&String> = ids.iter().collect();
    for (id, path) in collect_markdown(&params.docs_prefix) {
        if !registered.contains(&&id) {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                &params.registry_path.display().to_string(),
                1,
                1,
                format!(
                    "documentation file {} for {id} is absent from the registry",
                    path.display()
                ),
            ));
        }
    }
    Ok(out)
}

// CEP:WHAT: Checks mandated headings over every documentation file.
// CEP:WHY: Tier and checklist presence are line-level contracts; a trimmed-line comparison keeps the check insensitive to indentation.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(tree size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn heading_mode(
    rule: &RuleConfig,
    params: &RosettaParams,
    headings: &[&str],
) -> Result<Vec<Finding>, BootError> {
    let mut out = Vec::new();
    for (id, path) in collect_markdown(&params.docs_prefix) {
        let text = std::fs::read_to_string(&path).map_err(|e| {
            BootError::new(
                1,
                format!("unreadable documentation file {}: {e}", path.display()),
            )
        })?;
        for heading in headings {
            let present = text.lines().any(|l| l.trim() == *heading);
            if !present {
                out.push(Finding::new(
                    rule.severity,
                    &rule.name,
                    &path.display().to_string(),
                    1,
                    1,
                    format!("documentation file for {id} lacks the heading '{heading}'"),
                ));
            }
        }
    }
    Ok(out)
}

// CEP:WHAT: Splits one CEP function id into its domain and identifier parts.
// CEP:WHY: The tree layout places documentation under the domain directory.
// CEP:STATUS: complete
// CEP:FAILURE: A malformed id yields empty parts and the path check reports the miss.
// CEP:ASSUMES: ids are formatted DOMAIN-ID per chapter 68.2.1.
// CEP:COST: O(id length).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn split_id(id: &str) -> (String, String) {
    match id.split_once('-') {
        Some((d, r)) => (d.to_string(), r.to_string()),
        None => (String::new(), id.to_string()),
    }
}

// CEP:WHAT: Collects all Markdown files under a prefix in sorted order as (id, path) pairs.
// CEP:WHY: Sorted traversal keeps findings independent of filesystem order.
// CEP:STATUS: complete
// CEP:FAILURE: Unreadable directories are skipped; the id is the file stem.
// CEP:ASSUMES: none
// CEP:COST: cold; O(tree size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn collect_markdown(prefix: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    walk(prefix, &mut out);
    out.sort();
    out
}

// CEP:WHAT: Recursively walks one directory collecting Markdown files.
// CEP:WHY: Shared tree walk for all rosetta modes.
// CEP:STATUS: complete
// CEP:FAILURE: Unreadable subdirectories are skipped.
// CEP:ASSUMES: recursion depth is bounded by the filesystem depth.
// CEP:COST: cold; O(tree size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn walk(dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    let mut names: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    names.sort();
    for path in names {
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            out.push((stem, path));
        }
    }
}

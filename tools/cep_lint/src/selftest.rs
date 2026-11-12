// CEP:FILE: tools/cep_lint/src/selftest.rs
// CEP:WHAT: The self-test engine for tools/cep_lint/tests/manifest.json, running per-rule violating and compliant scenarios with exact expected finding counts.
// CEP:WHY: Chapter 50.6 requires that the tool verifies itself against a manifest shipping one violating and one compliant scenario per configured rule before any change ships.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: A failed scenario is reported and fails the run; malformed manifests surface as bootstrap errors.
// CEP:ASSUMES: Inline scenario configurations resolve relative paths against the manifest file's directory.
// CEP:COST: cold; O(manifest size).
// CEP:EVIDENCE: cep_lint --self-test gate in scripts/ci.sh.

use std::path::Path;

use cep_json::Json;

use crate::config::{self, Config};
use crate::report::Finding;

/// One scenario outcome.
#[derive(Debug, Clone)]
pub struct ScenarioResult {
    /// Scenario name.
    pub name: String,
    /// Whether the expected finding count matched exactly.
    pub passed: bool,
    /// Observed total findings for the scenario's rule set.
    pub observed: usize,
    /// Expected count from the manifest.
    pub expected: usize,
}

/// Runs every scenario in the manifest and returns the outcomes.
// CEP:WHAT: Runs every scenario in the self-test manifest.
// CEP:WHY: Chapter 50.6 requires one violating and one compliant scenario per configured rule with exact counts.
// CEP:STATUS: complete
// CEP:FAILURE: Returns a bootstrap error for malformed manifest data.
// CEP:ASSUMES: none
// CEP:COST: cold; O(manifest size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn run_manifest(
    manifest_text: &str,
    base_dir: &Path,
) -> Result<Vec<ScenarioResult>, config::BootError> {
    let manifest = cep_json::parse(manifest_text)
        .map_err(|e| config::BootError::new(2, format!("manifest parse failure: {e}")))?;
    let scenarios = manifest
        .get("scenarios")
        .and_then(|s| s.as_arr())
        .ok_or_else(|| config::BootError::new(4, "missing key: scenarios"))?;
    let mut results = Vec::new();
    for scenario in scenarios {
        results.push(run_scenario(scenario, base_dir)?);
    }
    Ok(results)
}

// CEP:WHAT: Runs one scenario: builds its configuration, scans its in-memory files, and compares finding counts.
// CEP:WHY: Exact expected counts catch both under-reporting and over-reporting, including cross-rule leakage from fixtures.
// CEP:STATUS: complete
// CEP:FAILURE: Returns a bootstrap error only for malformed scenario data.
// CEP:ASSUMES: none
// CEP:COST: cold; O(fixture size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn run_scenario(scenario: &Json, base_dir: &Path) -> Result<ScenarioResult, config::BootError> {
    let name = scenario
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed")
        .to_string();
    let expect = scenario
        .get("expect_findings")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let rule_filter: Vec<String> = scenario
        .get("rules")
        .and_then(|v| v.as_arr())
        .map(|items| {
            items
                .iter()
                .filter_map(|r| r.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let files: Vec<(String, String)> = scenario
        .get("files")
        .and_then(|v| v.as_obj())
        .map(|members| {
            members
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let config: Config = match scenario.get("config") {
        Some(inline) => config::from_value(inline, base_dir)?,
        None => config::load(&base_dir.join("../../../.cep/cep_lint.json"))?,
    };
    let include_doc_sync = files.is_empty();
    let findings = if scenario.get("invoke").and_then(|v| v.as_str()) == Some("verify_hunt") {
        crate::run_verify_hunt(&config)
    } else {
        crate::run_scan_memory(&files, &config, include_doc_sync)?
    };
    let observed = if rule_filter.iter().any(|r| r == "*") || rule_filter.is_empty() {
        findings.len()
    } else {
        findings
            .iter()
            .filter(|f| rule_filter.contains(&f.rule))
            .count()
    };
    let passed = observed == expect;
    Ok(ScenarioResult {
        name,
        passed,
        observed,
        expected: expect,
    })
}

/// Prints scenario outcomes in deterministic order and returns the aggregate pass state.
// CEP:WHAT: Prints scenario outcomes in deterministic order with an aggregate pass state.
// CEP:WHY: CI needs a single unambiguous self-test verdict.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(scenarios).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn print_results(results: &[ScenarioResult]) -> bool {
    const PASS: &str = "PASS";
    const FAIL: &str = "FAIL";
    let mut all = true;
    for r in results {
        if !r.passed {
            all = false;
        }
        let state = if r.passed { PASS } else { FAIL };
        println!(
            "{state} {} observed={} expected={}",
            r.name, r.observed, r.expected
        );
    }
    println!(
        "{}/{} scenarios passed",
        results.iter().filter(|r| r.passed).count(),
        results.len()
    );
    all
}

/// Collects the sorted distinct rule names of a finding set.
// CEP:WHAT: Collects the sorted distinct rule names of a finding set.
// CEP:WHY: Tests assert the exact rule set of a scenario, not only counts.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(findings log findings).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
pub fn finding_rules(findings: &[Finding]) -> Vec<String> {
    let mut names: Vec<String> = findings.iter().map(|f| f.rule.clone()).collect();
    names.sort();
    names.dedup();
    names
}

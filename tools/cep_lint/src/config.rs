// CEP:FILE: tools/cep_lint/src/config.rs
// CEP:WHAT: Typed loader and schema validator for .cep/cep_lint.json with the chapter 50 bootstrap error codes.
// CEP:WHY: The engine must contain mechanism only, so every pattern, threshold, severity, message, and option is parsed and validated here before any check runs; malformed configuration must abort with a coded bootstrap error instead of a silent fallback.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: Returns BootError with codes 1 through 8 on unreadable files, malformed JSON, schema violations, unknown check primitives, out-of-range severities, or invalid patterns.
// CEP:ASSUMES: Relative paths inside the configuration resolve against the configuration file's directory.
// CEP:COST: cold; O(config size).
// CEP:EVIDENCE: unit tests in tools/cep_lint/tests/config_test.rs.

use std::path::{Path, PathBuf};

use cep_json::Json;

use crate::re;

/// A bootstrap failure before or during configuration loading.
#[derive(Debug, Clone)]
pub struct BootError {
    /// Bootstrap code 1 through 8 per chapter 50.4.
    pub code: u8,
    /// Human-readable detail.
    pub message: String,
}

impl BootError {
    /// Constructs a bootstrap error with a code in 1..=8.
    // CEP:WHAT: Constructs a bootstrap error with a code.
    // CEP:WHY: Exit code 2 output must be exactly `CEP-LINT-BOOT-<n>: detail`.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: code is in 1..=8; enforced by the constructor's range check.
    // CEP:COST: cold; one allocation.
    // CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
    pub fn new(code: u8, message: impl Into<String>) -> Self {
        debug_assert!((1..=8).contains(&code));
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for BootError {
    // CEP:WHAT: Formats the bootstrap error with its CEP-LINT-BOOT prefix.
    // CEP:WHY: The exit code contract of chapter 50.1 requires this exact prefix.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CEP-LINT-BOOT-{}: {}", self.code, self.message)
    }
}

/// One configured rule with its mechanism selection and parameters.
#[derive(Debug, Clone)]
pub struct RuleConfig {
    /// Rule name, e.g. CEP-LINT-FILE-HEADER.
    pub name: String,
    /// Severity 0 through 3.
    pub severity: u8,
    /// Check primitive name selected from the catalog.
    pub check: String,
    /// Optional mode within the primitive.
    pub mode: String,
    /// Message template with {key} slots.
    pub message: String,
    /// Raw parameter object.
    pub params: Json,
}

/// Loaded lint configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Standard name, e.g. CEP&CC.
    pub standard_name: String,
    /// Standard version, e.g. 0.4.
    pub standard_version: String,
    /// First-party file glob patterns.
    pub file_globs: Vec<String>,
    /// Severities that fail the run.
    pub fail_on: Vec<u8>,
    /// Exit code mapping: ok, findings, bootstrap, usage.
    pub exit_codes: [u8; 4],
    /// DOC-SYNC parameters when the rule is configured.
    pub doc_sync: Option<DocSyncParams>,
    /// Rosetta rule parameters paired with their rules, in declaration order.
    pub rosetta: Vec<(RuleConfig, RosettaParams)>,
    /// Path of the self-test manifest.
    pub self_test_manifest: PathBuf,
    /// Configured rules in declaration order.
    pub rules: Vec<RuleConfig>,
    /// Directory the configuration was loaded from; base for relative paths.
    pub base_dir: PathBuf,
}

/// DOC-SYNC rule parameters.
#[derive(Debug, Clone)]
pub struct DocSyncParams {
    /// Path of the standard document.
    pub doc_path: PathBuf,
    /// Pattern extracting rule headings.
    pub heading_pattern: String,
    /// Pattern extracting the version value.
    pub version_pattern: String,
}

/// Rosetta rule parameters, resolved against the configuration directory at load time.
#[derive(Debug, Clone)]
pub struct RosettaParams {
    /// Path of the registry manifest (registry mode).
    pub registry_path: PathBuf,
    /// Root of the function documentation tree (all modes).
    pub docs_prefix: PathBuf,
    /// Required Tier 1 heading.
    pub tier1_heading: String,
    /// Required Tier 2 heading.
    pub tier2_heading: String,
    /// Required silent-failure checklist heading.
    pub checklist_heading: String,
}

/// Known check primitive names; anything else is bootstrap error 6.
const KNOWN_CHECKS: [&str; 13] = [
    "comment_block_schema",
    "function_policy",
    "comment_regex",
    "string_literal_regex",
    "numeric_literal_policy",
    "macro_prefix_policy",
    "banned_token_policy",
    "commit_hunt_manifest",
    "hunter_manifest",
    "file_topology_policy",
    "secret_probe_policy",
    "unsafe_doc_policy",
    "rosetta_policy",
];

/// Additional mechanism names beyond the parameterized checks.
const KNOWN_CHECKS_EXTRA: [&str; 2] = ["flat_code_regex", "doc_rule_coverage"];

/// Loads and validates a configuration from `path`.
// CEP:WHAT: Loads and validates the lint configuration file.
// CEP:WHY: Bootstrap failures must be coded and precise; the loader is the single schema enforcement point.
// CEP:STATUS: complete
// CEP:FAILURE: Returns BootError codes 1 and 2 for unreadable files and parse failures.
// CEP:ASSUMES: none
// CEP:COST: cold; O(config size).
// CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
pub fn load(path: &Path) -> Result<Config, BootError> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        BootError::new(
            1,
            format!("unreadable configuration {}: {e}", path.display()),
        )
    })?;
    let value = cep_json::parse(&text).map_err(|e| {
        BootError::new(
            2,
            format!("config parse failure: {} at line {}", e.message, e.line + 1),
        )
    })?;
    from_value(&value, path.parent().unwrap_or(Path::new(".")))
}

/// Builds a typed Config from a parsed JSON tree.
// CEP:ID: CS-r94
// CEP:WHAT: Builds a typed Config from a parsed JSON tree.
// CEP:WHY: Schema validation is centralized so every rule sees an identical parameter contract.
// CEP:STATUS: complete
// CEP:FAILURE: Returns BootError codes 4, 5, 6, 7, or 8 for missing keys, wrong types, unknown primitives, bad severities, or invalid patterns.
// CEP:ASSUMES: none
// CEP:COST: cold; O(config size).
// CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
pub fn from_value(value: &Json, base_dir: &Path) -> Result<Config, BootError> {
    let standard = value
        .get("standard")
        .ok_or_else(|| BootError::new(4, "missing key: standard"))?;
    let standard_name = require_str(standard, "name", "standard.name")?.to_string();
    let standard_version = require_str(standard, "version", "standard.version")?.to_string();

    let globs = value
        .get("file_globs")
        .and_then(|g| g.as_arr())
        .ok_or_else(|| BootError::new(4, "missing key: file_globs"))?;
    let file_globs: Vec<String> = globs
        .iter()
        .map(|g| {
            g.as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| BootError::new(5, "wrong type: file_globs element"))
        })
        .collect::<Result<_, _>>()?;

    let fail_on = value
        .get("fail_on")
        .and_then(|f| f.as_arr())
        .ok_or_else(|| BootError::new(4, "missing key: fail_on"))?;
    let fail_on: Vec<u8> = fail_on
        .iter()
        .map(|s| {
            let v = s
                .as_u64()
                .ok_or_else(|| BootError::new(5, "wrong type: fail_on element"))?;
            // CEP:WHAT: Range-checks the severity before any narrowing cast.
            // CEP:WHY: Casting u64 to u8 first would silently wrap 259 into 3 and accept an out-of-range severity.
            // CEP:STATUS: complete
            // CEP:FAILURE: Returns BootError code 7.
            // CEP:ASSUMES: none
            // CEP:COST: O(1).
            // CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
            if v > 3 {
                return Err(BootError::new(
                    7,
                    format!("severity {v} out of range 0..=3"),
                ));
            }
            check_severity(v as u8)
        })
        .collect::<Result<_, _>>()?;

    let exits = value
        .get("exit_codes")
        .ok_or_else(|| BootError::new(4, "missing key: exit_codes"))?;
    let exit_codes = [
        require_exit(exits, "ok", "exit_codes.ok")?,
        require_exit(exits, "findings", "exit_codes.findings")?,
        require_exit(exits, "bootstrap", "exit_codes.bootstrap")?,
        require_exit(exits, "usage", "exit_codes.usage")?,
    ];

    let manifest = value
        .get("self_test_manifest")
        .and_then(|m| m.as_str())
        .ok_or_else(|| BootError::new(4, "missing key: self_test_manifest"))?;
    let self_test_manifest = base_dir.join(manifest);

    let rules_value = value
        .get("rules")
        .and_then(|r| r.as_obj())
        .ok_or_else(|| BootError::new(4, "missing key: rules"))?;
    let mut rules = Vec::new();
    let mut doc_sync = None;
    let mut rosetta = Vec::new();
    for (name, body) in rules_value {
        let severity_v = require_u64(body, "severity", &format!("rules.{name}.severity"))?;
        // CEP:WHAT: Range-checks rule severities before the narrowing cast.
        // CEP:WHY: A truncated cast would accept 259 as severity 3 and silently change the rule's failure behavior.
        // CEP:STATUS: complete
        // CEP:FAILURE: Returns BootError code 7.
        // CEP:ASSUMES: none
        // CEP:COST: O(1).
        // CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
        if severity_v > 3 {
            return Err(BootError::new(
                7,
                format!("severity {severity_v} out of range 0..=3 for rule {name}"),
            ));
        }
        let severity = check_severity(severity_v as u8)?;
        let check = require_str(body, "check", &format!("rules.{name}.check"))?.to_string();
        if !KNOWN_CHECKS.contains(&check.as_str()) && !KNOWN_CHECKS_EXTRA.contains(&check.as_str())
        {
            return Err(BootError::new(
                6,
                format!("unknown check primitive '{check}' for rule {name}"),
            ));
        }
        let mode = body
            .get("mode")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let message = require_str(body, "message", &format!("rules.{name}.message"))?.to_string();
        // CEP:WHAT: Validates the params object type and every pattern-valued param.
        // CEP:WHY: A mistyped params object would fall back to defaults at check time; an invalid pattern would silently disable a severity-1 rule because the check primitives discard compile errors. Both are fail-open paths the schema must close.
        // CEP:STATUS: complete
        // CEP:FAILURE: Returns BootError code 5 for non-object params and code 8 for invalid patterns.
        // CEP:ASSUMES: the PATTERN_PARAMS table lists the pattern-valued params per check.
        // CEP:COST: O(params).
        // CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
        let params = body.get("params").cloned().unwrap_or(Json::Obj(Vec::new()));
        if body.get("params").is_some() && params.as_obj().is_none() {
            return Err(BootError::new(
                5,
                format!("wrong type: rules.{name}.params must be an object"),
            ));
        }
        if let Some(obj) = params.as_obj() {
            const PATTERN_PARAMS: [&str; 6] = [
                "pattern",
                "suppression_pattern",
                "name_pattern",
                "gap_pattern",
                "match_pattern",
                "heading_pattern",
            ];
            for (k, v) in obj {
                if PATTERN_PARAMS.contains(&k.as_str()) {
                    if let Some(p) = v.as_str() {
                        validate_pattern(p).map_err(|e| {
                            BootError::new(8, format!("rules.{name}.params.{k}: {e}"))
                        })?;
                    }
                }
                // CEP:WHAT: Validates pattern-list params element-wise.
                // CEP:WHY: exempt_contexts and patterns are arrays of regexes; a single invalid entry would silently disable matching for that context.
                // CEP:STATUS: complete
                // CEP:FAILURE: Returns BootError code 8 for an invalid pattern.
                // CEP:ASSUMES: other array params (tokens, statuses, fields) are literal strings, not patterns.
                // CEP:COST: O(elements).
                // CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
                if k == "exempt_contexts" || k == "patterns" {
                    if let Some(arr) = v.as_arr() {
                        for (i, el) in arr.iter().enumerate() {
                            if let Some(p) = el.as_str() {
                                validate_pattern(p).map_err(|e| {
                                    BootError::new(8, format!("rules.{name}.params.{k}[{i}]: {e}"))
                                })?;
                            }
                        }
                    }
                }
            }
        }
        if name == "CEP-LINT-DOC-SYNC" {
            let doc_path = require_str(&params, "doc_path", "doc_sync.doc_path")?.to_string();
            let heading_pattern =
                require_str(&params, "heading_pattern", "doc_sync.heading_pattern")?.to_string();
            let version_pattern =
                require_str(&params, "version_pattern", "doc_sync.version_pattern")?.to_string();
            validate_pattern(&heading_pattern)
                .map_err(|e| BootError::new(8, format!("doc_sync.heading_pattern: {e}")))?;
            validate_pattern(&version_pattern)
                .map_err(|e| BootError::new(8, format!("doc_sync.version_pattern: {e}")))?;
            doc_sync = Some(DocSyncParams {
                doc_path: base_dir.join(doc_path),
                heading_pattern,
                version_pattern,
            });
        }
        // CEP:WHAT: Resolves Rosetta rule parameters against the configuration directory.
        // CEP:WHY: Registry and documentation paths are relative to the configuration so CI and self-test fixtures resolve identically; heading defaults carry the standard's normative section names.
        // CEP:STATUS: complete
        // CEP:FAILURE: Registry mode without registry_path is bootstrap error 4.
        // CEP:ASSUMES: none
        // CEP:COST: O(1) per rosetta rule.
        // CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
        if check == "rosetta_policy" {
            let docs_prefix_v = params
                .get("docs_prefix")
                .and_then(|v| v.as_str())
                .unwrap_or("docs/functions");
            let registry_path = if mode == "registry" {
                base_dir.join(require_str(
                    &params,
                    "registry_path",
                    "rules.rosetta.registry_path",
                )?)
            } else {
                base_dir.join("docs/registry.json")
            };
            let heading = |key: &str, default: &str| -> String {
                params
                    .get(key)
                    .and_then(|v| v.as_str())
                    .unwrap_or(default)
                    .to_string()
            };
            rosetta.push((
                RuleConfig {
                    name: name.clone(),
                    severity,
                    check: check.clone(),
                    mode: mode.clone(),
                    message: message.clone(),
                    params: params.clone(),
                },
                RosettaParams {
                    registry_path,
                    docs_prefix: base_dir.join(docs_prefix_v),
                    tier1_heading: heading("tier1_heading", "## Tier 1: Beginner"),
                    tier2_heading: heading("tier2_heading", "## Tier 2: Silicon Reality"),
                    checklist_heading: heading("checklist_heading", "## Silent Failure Checklist"),
                },
            ));
        }
        rules.push(RuleConfig {
            name: name.clone(),
            severity,
            check,
            mode,
            message,
            params,
        });
    }
    if rules.is_empty() {
        return Err(BootError::new(4, "missing key: rules is empty"));
    }
    Ok(Config {
        standard_name,
        standard_version,
        file_globs,
        fail_on,
        exit_codes,
        doc_sync,
        rosetta,
        self_test_manifest,
        rules,
        base_dir: base_dir.to_path_buf(),
    })
}

// CEP:WHAT: Reads a required string field.
// CEP:WHY: Schema violations must be reported with the exact missing key path.
// CEP:STATUS: complete
// CEP:FAILURE: Returns BootError code 4 or 5.
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
fn require_str<'a>(obj: &'a Json, key: &str, path: &str) -> Result<&'a str, BootError> {
    let v = obj
        .get(key)
        .ok_or_else(|| BootError::new(4, format!("missing key: {path}")))?;
    v.as_str()
        .ok_or_else(|| BootError::new(5, format!("wrong type: {path} must be a string")))
}

// CEP:WHAT: Reads a required unsigned field.
// CEP:WHY: Numeric options need exact integer typing.
// CEP:STATUS: complete
// CEP:FAILURE: Returns BootError code 4 or 5.
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
fn require_u64(obj: &Json, key: &str, path: &str) -> Result<u64, BootError> {
    let v = obj
        .get(key)
        .ok_or_else(|| BootError::new(4, format!("missing key: {path}")))?;
    v.as_u64().ok_or_else(|| {
        BootError::new(
            5,
            format!("wrong type: {path} must be a non-negative integer"),
        )
    })
}

// CEP:WHAT: Validates that a severity is in range 0 through 3.
// CEP:WHY: Bootstrap error 7 is defined for out-of-range severities.
// CEP:STATUS: complete
// CEP:FAILURE: Returns BootError code 7.
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
fn check_severity(v: u8) -> Result<u8, BootError> {
    if v > 3 {
        Err(BootError::new(
            7,
            format!("severity {v} out of range 0..=3"),
        ))
    } else {
        Ok(v)
    }
}

/// Validates a pattern by compiling it with the in-tree regex engine.
// CEP:WHAT: Validates a pattern by compiling it with the in-tree regex engine.
// CEP:WHY: Bootstrap error 8 is defined for invalid regular expressions.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the compile error message.
// CEP:ASSUMES: none
// CEP:COST: cold; O(pattern size).
// CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
pub fn validate_pattern(pattern: &str) -> Result<(), String> {
    re::compile(pattern, false).map(|_| ())
}

// CEP:WHAT: Reads a required exit code in 0..=255.
// CEP:WHY: Exit codes are u8 at the process boundary; a wrapped value would make the configured contract and the observed exit disagree.
// CEP:STATUS: complete
// CEP:FAILURE: Returns BootError code 5 for out-of-range values.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/config_test.rs
fn require_exit(obj: &Json, key: &str, path: &str) -> Result<u8, BootError> {
    let v = require_u64(obj, key, path)?;
    if v > 255 {
        return Err(BootError::new(
            5,
            format!("wrong type: {path} must be in 0..=255"),
        ));
    }
    Ok(v as u8)
}

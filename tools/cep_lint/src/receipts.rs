// CEP:FILE: tools/cep_lint/src/receipts.rs
// CEP:WHAT: Invocation-mode verification of the signed Hunt and HUNTER receipts of chapters 65 and 67.
// CEP:WHY: The Bloodhound and HUNTER protocols do not trust the developer's word; the linter verifies an Ed25519-signed receipt covering fuzz executions, differential inputs, divergences, and unproven symbolic paths before a commit that touches cycle-exact code may merge.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: A missing, malformed, unsigned, or threshold-violating receipt produces severity findings; unreadable public keys produce findings; malformed public key hex produces findings.
// CEP:ASSUMES: The receipt payload is canonically serialized with cep_json before signing and before verification; the git-level cross-reference of modified files against the diff is a documented limitation handled by the signed targets list (see doc/cep_lint.md).
// CEP:COST: cold; one signature verification per receipt.
// CEP:EVIDENCE: unit tests in tools/cep_lint/tests/receipts_test.rs and cep_lint --self-test scenarios.

use std::path::Path;

use cep_json::Json;

use crate::config::RuleConfig;
use crate::ed25519;
use crate::report::Finding;

/// Verifies one invocation-mode receipt rule and returns its findings.
// CEP:WHAT: Verifies one signed receipt rule end to end.
// CEP:WHY: Chapters 65 and 67 demand mechanical, cryptographic enforcement of hunt evidence.
// CEP:STATUS: complete
// CEP:FAILURE: Produces findings; never panics on any file content.
// CEP:ASSUMES: none
// CEP:COST: cold; O(receipt size) plus one signature verification.
// CEP:EVIDENCE: tools/cep_lint/tests/receipts_test.rs
pub fn run_receipt_rule(rule: &RuleConfig, base_dir: &Path) -> Vec<Finding> {
    let receipt_path = crate::checks::param_str(rule, "receipt_path", "");
    let key_path = crate::checks::param_str(rule, "public_key_path", "");
    let mut out = Vec::new();
    if receipt_path.is_empty() || key_path.is_empty() {
        out.push(Finding::new(
            rule.severity,
            &rule.name,
            "receipt-config",
            1,
            1,
            "receipt rule is missing receipt_path or public_key_path parameters",
        ));
        return out;
    }
    let receipt_full = base_dir.join(&receipt_path);
    let key_full = base_dir.join(&key_path);
    let receipt_text = match std::fs::read_to_string(&receipt_full) {
        Ok(t) => t,
        Err(e) => {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                &receipt_full.display().to_string(),
                1,
                1,
                format!("missing or unreadable receipt: {e}"),
            ));
            return out;
        }
    };
    let key_text = match std::fs::read_to_string(&key_full) {
        Ok(t) => t,
        Err(e) => {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                &key_full.display().to_string(),
                1,
                1,
                format!("missing or unreadable public key: {e}"),
            ));
            return out;
        }
    };
    let mut key_bytes = [0u8; 32];
    if !hex_decode_32(key_text.trim(), &mut key_bytes) {
        out.push(Finding::new(
            rule.severity,
            &rule.name,
            &key_full.display().to_string(),
            1,
            1,
            "public key is not 64 hex characters",
        ));
        return out;
    }
    verify_receipt_text(rule, &receipt_text, &key_bytes, base_dir, &mut out);
    out
}

/// Verifies one receipt text against a parsed public key.
// CEP:WHAT: Verifies one receipt text against a parsed public key.
// CEP:WHY: Splitting text and key lets the self-test exercise the same path with in-memory fixtures.
// CEP:STATUS: complete
// CEP:FAILURE: Appends findings for malformed JSON, missing fields, signature failures, threshold violations, and unknown function references.
// CEP:ASSUMES: none
// CEP:COST: cold; O(receipt size).
// CEP:EVIDENCE: tools/cep_lint/tests/receipts_test.rs
pub fn verify_receipt_text(
    rule: &RuleConfig,
    receipt_text: &str,
    key: &[u8; 32],
    base_dir: &Path,
    out: &mut Vec<Finding>,
) {
    let parsed = match cep_json::parse(receipt_text) {
        Ok(v) => v,
        Err(e) => {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                "receipt",
                1,
                1,
                format!("receipt is not valid JSON: {e}"),
            ));
            return;
        }
    };
    let Some(payload) = parsed.get("payload") else {
        out.push(Finding::new(
            rule.severity,
            &rule.name,
            "receipt",
            1,
            1,
            "receipt has no payload member",
        ));
        return;
    };
    let Some(sig_hex) = parsed.get("signature").and_then(|s| s.as_str()) else {
        out.push(Finding::new(
            rule.severity,
            &rule.name,
            "receipt",
            1,
            1,
            "receipt has no signature member",
        ));
        return;
    };
    let mut sig = [0u8; 64];
    if !hex_decode_64(sig_hex.trim(), &mut sig) {
        out.push(Finding::new(
            rule.severity,
            &rule.name,
            "receipt",
            1,
            1,
            "signature is not 128 hex characters",
        ));
        return;
    }
    let canonical = cep_json::write::to_string_compact(payload);
    if !ed25519::verify(key, canonical.as_bytes(), &sig) {
        out.push(Finding::new(
            rule.severity,
            &rule.name,
            "receipt",
            1,
            1,
            "receipt signature verification failed",
        ));
        return;
    }
    check_thresholds(rule, payload, base_dir, out);
}

// CEP:WHAT: Appends one threshold finding when a numeric payload condition fails.
// CEP:WHY: All receipt thresholds share one failure-message shape.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/receipts_test.rs
fn threshold_finding(
    rule: &RuleConfig,
    out: &mut Vec<Finding>,
    ok: bool,
    observed: Option<u64>,
    field: &str,
    requirement: String,
) {
    if ok {
        return;
    }
    let detail = match observed {
        Some(v) => format!("{field} {v} violates requirement {requirement}"),
        None => format!("payload lacks {field}"),
    };
    out.push(Finding::new(
        rule.severity,
        &rule.name,
        "receipt",
        1,
        1,
        detail,
    ));
}

// CEP:WHAT: Checks the rule-specific thresholds and function references of a verified payload.
// CEP:WHY: A signed receipt with insufficient evidence must still fail the gate.
// CEP:STATUS: complete
// CEP:FAILURE: Appends one finding per violated threshold or unknown function reference.
// CEP:ASSUMES: none
// CEP:COST: cold; O(payload size + referenced files).
// CEP:EVIDENCE: tools/cep_lint/tests/receipts_test.rs
fn check_thresholds(rule: &RuleConfig, payload: &Json, base_dir: &Path, out: &mut Vec<Finding>) {
    let is_hunt = rule.check == "commit_hunt_manifest";
    let min_fuzz = crate::checks::param_u64(rule, "min_fuzz_executions", 100_000);
    let min_diff = crate::checks::param_u64(rule, "min_diff_inputs", 10_000);
    if is_hunt {
        let fuzz = payload.get("fuzz_executions").and_then(|v| v.as_u64());
        threshold_finding(
            rule,
            out,
            fuzz.is_some_and(|v| v >= min_fuzz),
            fuzz,
            "fuzz_executions",
            format!("at least {min_fuzz}"),
        );
        let diff = payload.get("diff_inputs").and_then(|v| v.as_u64());
        threshold_finding(
            rule,
            out,
            diff.is_some_and(|v| v >= min_diff),
            diff,
            "diff_inputs",
            format!("at least {min_diff}"),
        );
        let divergences = payload.get("divergences").and_then(|v| v.as_u64());
        threshold_finding(
            rule,
            out,
            divergences == Some(0),
            divergences,
            "divergences",
            "zero".to_string(),
        );
    } else {
        let unproven = payload.get("unproven_paths").and_then(|v| v.as_u64());
        threshold_finding(
            rule,
            out,
            unproven == Some(0),
            unproven,
            "unproven_paths",
            "zero".to_string(),
        );
    }
    let list_field = if is_hunt {
        "targets"
    } else {
        "functions_analyzed"
    };
    let repo_root = base_dir.join(crate::checks::param_str(rule, "repo_root", ".."));
    if let Some(items) = payload.get(list_field).and_then(|v| v.as_arr()) {
        for item in items {
            let file = item.get("file").and_then(|f| f.as_str()).unwrap_or("");
            if file.is_empty() {
                out.push(Finding::new(
                    rule.severity,
                    &rule.name,
                    "receipt",
                    1,
                    1,
                    format!("{list_field} entry lacks file"),
                ));
                continue;
            }
            let full = repo_root.join(file);
            let Ok(text) = std::fs::read_to_string(&full) else {
                out.push(Finding::new(
                    rule.severity,
                    &rule.name,
                    "receipt",
                    1,
                    1,
                    format!("receipt references unreadable file {file}"),
                ));
                continue;
            };
            let lexed = crate::lex::lex(&text);
            let functions = crate::checks::func_policy::extract_functions(&lexed);
            if let Some(fns) = item.get("functions").and_then(|f| f.as_arr()) {
                for f in fns {
                    let name = f.as_str().unwrap_or("");
                    if !name.is_empty() && !functions.iter().any(|g| g.name == name) {
                        out.push(Finding::new(
                            rule.severity,
                            &rule.name,
                            "receipt",
                            1,
                            1,
                            format!("receipt references unknown function {name} in {file}"),
                        ));
                    }
                }
            }
        }
    }
}

// CEP:WHAT: Decodes 32 bytes from hex.
// CEP:WHY: Public keys are stored as hex text.
// CEP:STATUS: complete
// CEP:FAILURE: Returns false for wrong length or non-hex characters.
// CEP:ASSUMES: none
// CEP:COST: cold; O(64).
// CEP:EVIDENCE: tools/cep_lint/tests/receipts_test.rs
fn hex_decode_32(text: &str, out: &mut [u8; 32]) -> bool {
    let mut bytes = [0u8; 32];
    if !hex_decode(text, &mut bytes) {
        return false;
    }
    out.copy_from_slice(&bytes);
    true
}

// CEP:WHAT: Decodes 64 bytes from hex.
// CEP:WHY: Signatures are stored as hex text.
// CEP:STATUS: complete
// CEP:FAILURE: Returns false for wrong length or non-hex characters.
// CEP:ASSUMES: none
// CEP:COST: cold; O(128).
// CEP:EVIDENCE: tools/cep_lint/tests/receipts_test.rs
fn hex_decode_64(text: &str, out: &mut [u8; 64]) -> bool {
    let mut bytes = [0u8; 64];
    if !hex_decode(text, &mut bytes) {
        return false;
    }
    out.copy_from_slice(&bytes);
    true
}

// CEP:WHAT: Decodes a hex string into a byte buffer of the exact expected length.
// CEP:WHY: Shared hex decoding for keys and signatures.
// CEP:STATUS: complete
// CEP:FAILURE: Returns false on odd length, wrong length, or invalid digits.
// CEP:ASSUMES: none
// CEP:COST: cold; O(text length).
// CEP:EVIDENCE: tools/cep_lint/tests/receipts_test.rs
fn hex_decode(text: &str, out: &mut [u8]) -> bool {
    if text.len() != out.len() * 2 {
        return false;
    }
    let hex_val = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    let raw = text.as_bytes();
    for i in 0..out.len() {
        let hi = hex_val(raw[i * 2]);
        let lo = hex_val(raw[i * 2 + 1]);
        match (hi, lo) {
            (Some(h), Some(l)) => out[i] = (h << 4) | l,
            _ => return false,
        }
    }
    true
}

/// Encodes bytes as lowercase hex.
// CEP:WHAT: Encodes bytes as lowercase hex.
// CEP:WHY: The receipt signer and key generator emit hex.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(bytes).
// CEP:EVIDENCE: tools/cep_lint/tests/receipts_test.rs
pub fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0F) as usize] as char);
    }
    out
}

/// Signs a receipt payload JSON text with a seed and writes the wrapped receipt JSON text.
// CEP:WHAT: Signs one canonical receipt payload.
// CEP:WHY: CI and the test fixtures produce receipts with the same signer the verifier trusts.
// CEP:STATUS: complete
// CEP:FAILURE: Returns an error message for malformed payloads.
// CEP:ASSUMES: the payload text is a valid JSON object.
// CEP:COST: cold; one signature.
// CEP:EVIDENCE: tools/cep_lint/tests/receipts_test.rs
pub fn sign_payload(payload_text: &str, seed: &[u8]) -> Result<String, String> {
    let payload = cep_json::parse(payload_text).map_err(|e| format!("payload parse: {e}"))?;
    let canonical = cep_json::write::to_string_compact(&payload);
    let sig = ed25519::sign(seed, canonical.as_bytes());
    let receipt = Json::Obj(vec![
        ("payload".to_string(), payload),
        ("signature".to_string(), Json::Str(hex_encode(&sig))),
    ]);
    Ok(cep_json::write::to_string_pretty(&receipt))
}

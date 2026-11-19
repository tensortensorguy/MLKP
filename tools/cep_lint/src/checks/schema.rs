// CEP:FILE: tools/cep_lint/src/checks/schema.rs
// CEP:WHAT: The comment_block_schema primitive: file-header mode, status-value mode, and stub-requirements mode.
// CEP:WHY: Rules CEP-LINT-FILE-HEADER, CEP-LINT-STATUS-VALUE, and CEP-LINT-STUB-DECL share one block-parsing mechanism with different configured parameters and modes.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: none; missing structures produce findings rather than aborts.
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: integration tests in tools/cep_lint/tests/manifest_test.rs.

use crate::checks::{
    cep_field, cep_fields, comment_blocks, header_block, param_str, param_strs, param_u64, template,
};
use crate::config::RuleConfig;
use crate::lex::Lexed;
use crate::report::Finding;

/// Entry point for the comment_block_schema primitive.
pub fn comment_block_schema(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    match rule.mode.as_str() {
        "file_header" => file_header_mode(rule, file, lexed),
        "status_value" => status_value_mode(rule, file, lexed),
        "stub_requirements" => stub_requirements_mode(rule, file, lexed),
        _ => Vec::new(),
    }
}

// CEP:ID: CS-281
// CEP:WHAT: Validates the first contiguous full-line comment block as a file header.
// CEP:WHY: Section 35 requires CEP:FILE, WHAT, WHY, CLASS, STATUS, FAILURE, ASSUMES, COST, EVIDENCE at the top of every first-party file, with a declared conformance class.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn file_header_mode(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    let mut out = Vec::new();
    let blocks = comment_blocks(lexed);
    let max_start = param_u64(rule, "max_start_line", 1) as usize;
    let required = param_strs(rule, "required_fields");
    let class_field = param_str(rule, "class_field", "CEP:CLASS");
    let allowed_classes = param_strs(rule, "allowed_classes");
    let Some(header) = header_block(&blocks) else {
        out.push(Finding::new(
            rule.severity,
            &rule.name,
            file,
            1,
            1,
            template(
                &rule.message,
                &[("detail", "no comment block starts the file".to_string())],
            ),
        ));
        return out;
    };
    if header.start_line + 1 > max_start {
        out.push(Finding::new(
            rule.severity,
            &rule.name,
            file,
            1,
            1,
            template(
                &rule.message,
                &[(
                    "detail",
                    format!("header block starts at line {}", header.start_line + 1),
                )],
            ),
        ));
        return out;
    }
    for field in &required {
        if cep_field(&header.text, field).is_none() {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                file,
                header.start_line + 1,
                1,
                template(
                    &rule.message,
                    &[("detail", format!("missing field {field}"))],
                ),
            ));
        }
    }
    if let Some(class) = cep_field(&header.text, &class_field) {
        if !allowed_classes.contains(&class) {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                file,
                header.start_line + 1,
                1,
                template(
                    &rule.message,
                    &[(
                        "detail",
                        format!("declared class '{class}' is not a defined conformance class"),
                    )],
                ),
            ));
        }
    }
    out
}

// CEP:WHAT: Validates every CEP:STATUS value in the file against the allowed set.
// CEP:WHY: Section 10.5 defines exactly four statuses; any other value is a defect.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn status_value_mode(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    let mut out = Vec::new();
    let status_field = param_str(rule, "status_field", "STATUS");
    let allowed = param_strs(rule, "allowed_values");
    for line in &lexed.lines {
        let Some(comment) = &line.comment else {
            continue;
        };
        for (field, value) in cep_fields(comment) {
            if field == status_field && !allowed.contains(&value) {
                out.push(Finding::new(
                    rule.severity,
                    &rule.name,
                    file,
                    line.line_no,
                    1,
                    template(
                        &rule.message,
                        &[("detail", format!("status '{value}' is not allowed"))],
                    ),
                ));
            }
        }
    }
    out
}

// CEP:WHAT: Requires CEP:OWNER and CEP:TICKET in blocks that declare stub, partial, or placeholder status.
// CEP:WHY: Section 10.10 bans anonymous stubs; unowned partial work cannot be tracked.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: tools/cep_lint/tests/manifest_test.rs
fn stub_requirements_mode(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    let mut out = Vec::new();
    let status_field = param_str(rule, "status_field", "STATUS");
    let statuses = param_strs(rule, "statuses");
    let required = param_strs(rule, "required_fields");
    let status_lookup = format!("CEP:{status_field}");
    for block in comment_blocks(lexed) {
        let status = cep_field(&block.text, &status_lookup).unwrap_or_default();
        if statuses.contains(&status) {
            for field in &required {
                if cep_field(&block.text, field).is_none() {
                    out.push(Finding::new(
                        rule.severity,
                        &rule.name,
                        file,
                        block.start_line + 1,
                        1,
                        template(
                            &rule.message,
                            &[("detail", format!("{status} block is missing {field}"))],
                        ),
                    ));
                }
            }
        }
    }
    out
}

// CEP:FILE: tools/cep_lint/src/checks/func_policy.rs
// CEP:WHAT: The function_policy primitive: block mode (required CEP function blocks), stub-body mode, naming mode, and constant-return mode over lexically extracted Rust function definitions.
// CEP:WHY: Rules CEP-LINT-FUNC-BLOCK, CEP-LINT-STUB-BODY, CEP-LINT-NAMING-FUNC, and CEP-LINT-CONSTANT-RETURN share one function extractor whose statement counting is the mechanical approximation of nontriviality defined in chapter 50.3.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: none; unparseable constructs are skipped rather than reported (chapter 50.5 documents extractor limits).
// CEP:ASSUMES: Rust source layout; the first `{` after a function name opens its body (Rust parameters and where clauses contain no braces).
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: unit tests in tools/cep_lint/tests/func_policy_test.rs.

use crate::checks::{cep_field, comment_blocks, param_str, param_strs, template, CommentBlock};
use crate::config::RuleConfig;
use crate::lex::Lexed;
use crate::re;
use crate::report::Finding;

/// One extracted function definition.
#[derive(Debug, Clone)]
pub struct Function {
    /// Function name.
    pub name: String,
    /// Zero-based line of the `fn` keyword.
    pub sig_line: usize,
    /// Zero-based first line of the body.
    pub body_start: usize,
    /// Zero-based last line of the body (inclusive).
    pub body_end: usize,
    /// Masked body text (comments and strings removed).
    pub body_text: String,
    /// Mechanical statement count: semicolons plus a trailing expression.
    pub stmts: usize,
}

/// Dispatches one function_policy rule mode over extracted functions.
// CEP:WHAT: Dispatches one function_policy rule mode over extracted functions.
// CEP:WHY: Four rules share one extractor so statement counting and block association stay identical across modes.
// CEP:STATUS: complete
// CEP:FAILURE: none; unparseable constructs are skipped per chapter 50.5.
// CEP:ASSUMES: none
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: tools/cep_lint/tests/func_policy_test.rs
pub fn function_policy(rule: &RuleConfig, file: &str, lexed: &Lexed) -> Vec<Finding> {
    let functions = extract_functions(lexed);
    match rule.mode.as_str() {
        "block" => block_mode(rule, file, lexed, &functions),
        "stub_body" => stub_body_mode(rule, file, lexed, &functions),
        "naming" => naming_mode(rule, file, &functions),
        "constant_return" => constant_return_mode(rule, file, &functions),
        "id" => id_mode(rule, file, lexed, &functions),
        _ => Vec::new(),
    }
}

/// Extracts function definitions with statement counts from masked code.
// CEP:ID: CS-b38
// CEP:WHAT: Extracts function definitions with statement counts from masked code.
// CEP:WHY: Four rules share the extractor; one implementation keeps their semantics identical.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the first `{` after the signature opens the body; `fn` in pointer types has no identifier and is skipped.
// CEP:COST: cold; O(file size).
// CEP:EVIDENCE: tools/cep_lint/tests/func_policy_test.rs
pub fn extract_functions(lexed: &Lexed) -> Vec<Function> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lexed.lines.len() {
        let line = &lexed.lines[i];
        let Some(name_span) = find_fn_name(line) else {
            i += 1;
            continue;
        };
        let mut j = i;
        let mut found_open = None;
        'outer: while j < lexed.lines.len() {
            for (off, ch) in lexed.lines[j].code.char_indices() {
                if j == i && off <= name_span.1 {
                    continue;
                }
                if ch == ';' {
                    break 'outer;
                }
                if ch == '{' {
                    found_open = Some((j, off));
                    break 'outer;
                }
            }
            j += 1;
        }
        let Some((open_line, open_off)) = found_open else {
            i += 1;
            continue;
        };
        let mut depth = 0usize;
        let mut end_line = open_line;
        let mut end_off = 0usize;
        'scan: while end_line < lexed.lines.len() {
            let start_off = if end_line == open_line { open_off } else { 0 };
            for (off, ch) in lexed.lines[end_line].code.char_indices().skip(start_off) {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        end_off = off;
                        break 'scan;
                    }
                }
            }
            end_line += 1;
        }
        if depth != 0 {
            i += 1;
            continue;
        }
        let mut body_text = String::new();
        let lines_slice = &lexed.lines[open_line..=end_line.min(lexed.lines.len() - 1)];
        for (k_rel, l) in lines_slice.iter().enumerate() {
            let k = open_line + k_rel;
            let code = &l.code;
            let s_idx = if k == open_line { open_off + 1 } else { 0 };
            let e_idx = if k == end_line { end_off } else { code.len() };
            body_text.push_str(&code[s_idx.min(e_idx)..e_idx]);
            body_text.push('\n');
        }
        let stmts = count_statements(&body_text);
        out.push(Function {
            name: name_span.2,
            sig_line: i,
            body_start: open_line,
            body_end: end_line,
            body_text,
            stmts,
        });
        i = end_line + 1;
    }
    out
}

// CEP:ID: CS-n90
// CEP:WHAT: Finds `fn NAME` in one masked line, returning (start_col, end_col, name).
// CEP:WHY: The extractor needs the name and its column for findings.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None when the line has no function definition.
// CEP:ASSUMES: none
// CEP:COST: cold; O(line length).
// CEP:EVIDENCE: tools/cep_lint/tests/func_policy_test.rs
fn find_fn_name(line: &crate::lex::LineInfo) -> Option<(usize, usize, String)> {
    let code = &line.code;
    let chars: Vec<char> = code.chars().collect();
    let mut k = 0;
    while k + 2 < chars.len() {
        if chars[k] == 'f' && chars[k + 1] == 'n' {
            let before_ok = k == 0 || !(chars[k - 1].is_alphanumeric() || chars[k - 1] == '_');
            if before_ok && (chars[k + 2] == ' ' || chars[k + 2] == '\t') {
                let mut m = k + 2;
                while m < chars.len() && (chars[m] == ' ' || chars[m] == '\t') {
                    m += 1;
                }
                let name_start = m;
                while m < chars.len() && (chars[m].is_alphanumeric() || chars[m] == '_') {
                    m += 1;
                }
                if m > name_start {
                    let name: String = chars[name_start..m].iter().collect();
                    return Some((k, m, name));
                }
            }
        }
        k += 1;
    }
    None
}

// CEP:WHAT: Counts statements in a masked body: semicolons plus a trailing expression.
// CEP:WHY: Chapter 50 approximates nontriviality as two or more statements.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: strings and comments are already masked out of the body text.
// CEP:COST: cold; O(body size).
// CEP:EVIDENCE: tools/cep_lint/tests/func_policy_test.rs
fn count_statements(body_text: &str) -> usize {
    let semis = body_text.matches(';').count();
    let after_last = match body_text.rfind(';') {
        Some(pos) => &body_text[pos + 1..],
        None => body_text,
    };
    let tail = after_last
        .lines()
        .map(|l| l.trim().trim_matches(|c| c == '}' || c == '{'))
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .count();
    semis + (tail > 0) as usize
}

// CEP:ID: CS-f53
// CEP:WHAT: Block mode: requires a CEP comment block before nontrivial functions.
// CEP:WHY: Rule CEP-LINT-FUNC-BLOCK; attributes and blank lines may sit between block and signature per the gap pattern.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(functions * blocks).
// CEP:EVIDENCE: tools/cep_lint/tests/func_policy_test.rs
fn block_mode(
    rule: &RuleConfig,
    file: &str,
    lexed: &Lexed,
    functions: &[Function],
) -> Vec<Finding> {
    let mut out = Vec::new();
    let required = param_strs(rule, "required_fields");
    let min_stmts = crate::checks::param_u64(rule, "min_body_statements", 2) as usize;
    let gap = param_str(rule, "gap_pattern", r"^\s*(#.*)?$");
    let gap_re = match re::compile(&gap, false) {
        Ok(r) => r,
        Err(_) => return out,
    };
    let blocks = comment_blocks(lexed);
    for function in functions {
        if function.stmts < min_stmts {
            continue;
        }
        let accepted = find_preceding_block(&blocks, function, lexed, &gap_re);
        let Some(block) = accepted else {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                file,
                function.sig_line + 1,
                1,
                template(
                    &rule.message,
                    &[(
                        "detail",
                        format!("function {} lacks a CEP block", function.name),
                    )],
                ),
            ));
            continue;
        };
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
                        &[(
                            "detail",
                            format!("function {} block is missing {field}", function.name),
                        )],
                    ),
                ));
            }
        }
    }
    out
}

/// Locates the comment block preceding a function, enforcing the gap pattern and rejecting the file header.
// CEP:ID: CS-t98
// CEP:WHAT: Locates the comment block preceding a function.
// CEP:WHY: Chapter 50.3: only attributes may separate block and definition; header blocks are never function blocks. Shared with the unsafe-documentation check so "enclosing block" has one meaning.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None when no acceptable block exists.
// CEP:ASSUMES: none
// CEP:COST: cold; O(blocks near the function).
// CEP:EVIDENCE: tools/cep_lint/tests/func_policy_test.rs
pub(crate) fn find_preceding_block(
    blocks: &[CommentBlock],
    function: &Function,
    lexed: &Lexed,
    gap_re: &re::Regex,
) -> Option<CommentBlock> {
    let mut k = function.sig_line;
    while k > 0 {
        k -= 1;
        let line = &lexed.lines[k];
        let text = &line.raw;
        let chars: Vec<char> = text.chars().collect();
        let is_blank_or_gap = text.trim().is_empty() || gap_re.is_match(&chars);
        if is_blank_or_gap {
            continue;
        }
        if line.code.trim().is_empty() && line.comment.is_some() {
            for block in blocks {
                if block.start_line <= k && k < block.end_line {
                    if block.start_line == 0 {
                        return None; // File header is never a function block.
                    }
                    return Some(block.clone());
                }
            }
            return None;
        }
        return None;
    }
    None
}

// CEP:ID: CS-745
// CEP:WHAT: Stub-body mode: flags empty function bodies without a declared stub status.
// CEP:WHY: Rule CEP-LINT-STUB-BODY; declared stubs must carry OWNER and TICKET per STUB-DECL.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(functions).
// CEP:EVIDENCE: tools/cep_lint/tests/func_policy_test.rs
fn stub_body_mode(
    rule: &RuleConfig,
    file: &str,
    lexed: &Lexed,
    functions: &[Function],
) -> Vec<Finding> {
    let mut out = Vec::new();
    let suppressed = param_strs(rule, "suppress_statuses");
    let status_field = param_str(rule, "suppress_status_field", "STATUS");
    let gap = param_str(rule, "gap_pattern", r"^\s*(#.*)?$");
    let gap_re = match re::compile(&gap, false) {
        Ok(r) => r,
        Err(_) => return out,
    };
    let blocks = comment_blocks(lexed);
    for function in functions {
        let is_empty = function.body_text.trim().is_empty();
        if !is_empty {
            continue;
        }
        let status = find_preceding_block(&blocks, function, lexed, &gap_re)
            .and_then(|block| {
                let lookup = format!("CEP:{status_field}");
                cep_field(&block.text, &lookup)
            })
            .unwrap_or_default();
        if !suppressed.contains(&status) {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                file,
                function.sig_line + 1,
                1,
                template(
                    &rule.message,
                    &[(
                        "detail",
                        format!(
                            "empty body of {} without declared stub status",
                            function.name
                        ),
                    )],
                ),
            ));
        }
    }
    out
}

// CEP:WHAT: Naming mode: validates function names against the configured pattern.
// CEP:WHY: Rule CEP-LINT-NAMING-FUNC requires snake_case identifiers.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(functions).
// CEP:EVIDENCE: tools/cep_lint/tests/func_policy_test.rs
fn naming_mode(rule: &RuleConfig, file: &str, functions: &[Function]) -> Vec<Finding> {
    let mut out = Vec::new();
    let pattern = param_str(rule, "name_pattern", "^[a-z_][a-z0-9_]*$");
    let name_re = match re::compile(&pattern, false) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for function in functions {
        let chars: Vec<char> = function.name.chars().collect();
        if !name_re.is_match(&chars) {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                file,
                function.sig_line + 1,
                1,
                template(&rule.message, &[("name", function.name.clone())]),
            ));
        }
    }
    out
}

// CEP:WHAT: Constant-return mode: flags bodies that return a constant without computing.
// CEP:WHY: Rule CEP-LINT-CONSTANT-RETURN; the result must derive from inputs.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(functions).
// CEP:EVIDENCE: tools/cep_lint/tests/func_policy_test.rs
fn constant_return_mode(rule: &RuleConfig, file: &str, functions: &[Function]) -> Vec<Finding> {
    let mut out = Vec::new();
    let pattern = param_str(rule, "pattern", r"^(return\s+)?(true|false|nullptr)\s*;?$");
    let re_match = match re::compile(&pattern, false) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for function in functions {
        let stripped: String = function
            .body_text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<&str>>()
            .join(" ");
        let chars: Vec<char> = stripped.chars().collect();
        if re_match.is_match(&chars) {
            out.push(Finding::new(
                rule.severity,
                &rule.name,
                file,
                function.sig_line + 1,
                1,
                template(&rule.message, &[("name", function.name.clone())]),
            ));
        }
    }
    out
}

// CEP:ID: CS-645
// CEP:WHAT: Id mode: requires a well-formed CEP:ID declaration on functions above the statement threshold.
// CEP:WHY: Rule CEP-LINT-DOC-ID is the mechanical form of chapter 68.6.1's mandate that functions over ten statements declare a Rosetta identifier; the id must match the configured DOMAIN-id pattern.
// CEP:STATUS: complete
// CEP:FAILURE: none; unparseable constructs are skipped per chapter 50.5.
// CEP:ASSUMES: the statement count comes from the shared extractor so block and id modes agree on nontriviality.
// CEP:COST: cold; O(functions * blocks).
// CEP:EVIDENCE: tools/cep_lint/tests/func_policy_test.rs
fn id_mode(rule: &RuleConfig, file: &str, lexed: &Lexed, functions: &[Function]) -> Vec<Finding> {
    let mut out = Vec::new();
    let max = crate::checks::param_u64(rule, "max_id_statements", 10) as usize;
    let pattern = param_str(
        rule,
        "id_pattern",
        "^(JIT|IR|LEX|FE|MEM|NET|GPU|HOT|CS)-[a-z0-9]+$",
    );
    let id_re = match re::compile(&pattern, false) {
        Ok(r) => r,
        Err(_) => return out,
    };
    let gap = param_str(rule, "gap_pattern", r"^\s*(#.*)?$");
    let gap_re = match re::compile(&gap, false) {
        Ok(r) => r,
        Err(_) => return out,
    };
    let blocks = comment_blocks(lexed);
    for function in functions {
        if function.stmts <= max {
            continue;
        }
        let declared = find_preceding_block(&blocks, function, lexed, &gap_re)
            .and_then(|block| cep_field(&block.text, "CEP:ID"));
        match declared {
            None => out.push(Finding::new(
                rule.severity,
                &rule.name,
                file,
                function.sig_line + 1,
                1,
                template(
                    &rule.message,
                    &[(
                        "detail",
                        format!(
                            "function {} has {} statements and declares no CEP:ID",
                            function.name, function.stmts
                        ),
                    )],
                ),
            )),
            Some(id) => {
                let chars: Vec<char> = id.chars().collect();
                if !id_re.is_match(&chars) {
                    out.push(Finding::new(
                        rule.severity,
                        &rule.name,
                        file,
                        function.sig_line + 1,
                        1,
                        template(
                            &rule.message,
                            &[(
                                "detail",
                                format!(
                                    "function {} declares malformed CEP:ID '{id}'",
                                    function.name
                                ),
                            )],
                        ),
                    ));
                }
            }
        }
    }
    out
}

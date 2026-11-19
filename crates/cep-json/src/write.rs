// CEP:FILE: crates/cep-json/src/write.rs
// CEP:WHAT: Deterministic JSON serializer with two-space indentation and insertion-order members.
// CEP:WHY: Cache files and fixtures must be byte-identical across runs to satisfy the reproducibility gates, so the writer never sorts, never depends on locale, and always escapes identically.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; serialization of an in-memory value tree cannot fail, and non-finite floats are clamped to null by contract.
// CEP:ASSUMES: JSON has no representation for NaN or infinity; the writer emits null for those values, and callers must not rely on the round trip of non-finite numbers.
// CEP:COST: cold; O(size of value tree) with one output String allocation.
// CEP:EVIDENCE: unit tests in crates/cep-json/tests/write_test.rs.

use crate::Json;

// CEP:WHAT: Serializes one value with two-space indentation and a trailing newline.
// CEP:WHY: Cache files and fixtures must be diffable and stable.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: non-finite floats serialize as null by contract.
// CEP:COST: cold; O(value size).
// CEP:EVIDENCE: crates/cep-json/tests/write_test.rs
/// Serializes a value with two-space indentation and a trailing newline.
pub fn to_string_pretty(value: &Json) -> String {
    let mut out = String::new();
    write_value(&mut out, value, 0);
    out.push('\n');
    out
}

/// Serializes a value on a single line without whitespace.
// CEP:WHAT: Serializes one value on a single line without whitespace.
// CEP:WHY: Canonical signing and hashing inputs need minimal bytes.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(value size).
// CEP:EVIDENCE: crates/cep-json/tests/write_test.rs
pub fn to_string_compact(value: &Json) -> String {
    let mut out = String::new();
    write_compact(&mut out, value);
    out
}

// CEP:ID: CS-l68
// CEP:WHAT: Recursive pretty writer with depth-based indentation.
// CEP:WHY: Human-diffable cache files make regressions reviewable.
// CEP:STATUS: complete
// CEP:FAILURE: none; String writes do not fail.
// CEP:ASSUMES: nesting depth is bounded by the value tree shape; cache values are shallow by construction.
// CEP:COST: cold; O(size of value tree).
// CEP:EVIDENCE: unit test crates/cep-json/tests/write_test.rs
fn write_value(out: &mut String, value: &Json, depth: usize) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(true) => out.push_str("true"),
        Json::Bool(false) => out.push_str("false"),
        Json::Num(n) => out.push_str(&format_number(*n)),
        Json::Str(s) => write_string(out, s),
        Json::Arr(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                indent(out, depth + 1);
                write_value(out, item, depth + 1);
                if i + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, depth);
            out.push(']');
        }
        Json::Obj(members) => {
            if members.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for (i, (key, val)) in members.iter().enumerate() {
                indent(out, depth + 1);
                write_string(out, key);
                out.push_str(": ");
                write_value(out, val, depth + 1);
                if i + 1 < members.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, depth);
            out.push('}');
        }
    }
}

// CEP:WHAT: Recursive compact writer with no whitespace.
// CEP:WHY: Hash-sensitive consumers want minimal canonical bytes.
// CEP:STATUS: complete
// CEP:FAILURE: none; String writes do not fail.
// CEP:ASSUMES: none
// CEP:COST: cold; O(size of value tree).
// CEP:EVIDENCE: unit test crates/cep-json/tests/write_test.rs
fn write_compact(out: &mut String, value: &Json) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(true) => out.push_str("true"),
        Json::Bool(false) => out.push_str("false"),
        Json::Num(n) => out.push_str(&format_number(*n)),
        Json::Str(s) => write_string(out, s),
        Json::Arr(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_compact(out, item);
            }
            out.push(']');
        }
        Json::Obj(members) => {
            out.push('{');
            for (i, (key, val)) in members.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(out, key);
                out.push(':');
                write_compact(out, val);
            }
            out.push('}');
        }
    }
}

// CEP:WHAT: Emits indentation spaces for one depth level.
// CEP:WHY: Indentation is two spaces per level by contract.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(2 * depth).
// CEP:EVIDENCE: unit test crates/cep-json/tests/write_test.rs
fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

// CEP:WHAT: Formats one f64 as an integer when exact, otherwise as a minimal float.
// CEP:WHY: Integer-looking numbers must round trip without exponent noise.
// CEP:STATUS: complete
// CEP:FAILURE: Emits null for non-finite values; JSON cannot carry them.
// CEP:ASSUMES: integers up to 2^53 are exactly representable in f64; enforced by range check.
// CEP:COST: cold; one formatting call.
// CEP:EVIDENCE: unit test crates/cep-json/tests/write_test.rs
fn format_number(n: f64) -> String {
    if !n.is_finite() {
        return "null".to_string();
    }
    if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_992.0 && n.abs() < 1e21 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

// CEP:WHAT: Writes one escaped JSON string.
// CEP:WHY: Control bytes, quotes, and backslashes must be escaped; other bytes pass through as UTF-8.
// CEP:STATUS: complete
// CEP:FAILURE: none; escapes the two mandatory bytes and control characters explicitly.
// CEP:ASSUMES: input is valid UTF-8 (the value tree only stores Rust Strings).
// CEP:COST: cold; O(size of string).
// CEP:EVIDENCE: unit test crates/cep-json/tests/write_test.rs
fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

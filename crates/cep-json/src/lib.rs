// CEP:FILE: crates/cep-json/src/lib.rs
// CEP:WHAT: Public API of the strict deterministic JSON value model, parser, and writer.
// CEP:WHY: Tooling and cache layers need JSON without external dependencies so the supply chain stays empty and builds stay reproducible; objects preserve insertion order because nondeterministic iteration is banned by CEP&CC Law 1.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Parsing returns ParseError with file, line, column, and reason on any malformed input; no panics, no recovery.
// CEP:ASSUMES: Input is valid UTF-8 text; enforced by the caller which reads files as String.
// CEP:COST: cold; parsing is O(n) in input bytes and runs in configuration loading and cache I/O only.
// CEP:EVIDENCE: unit tests in crates/cep-json/tests/parse_test.rs and crates/cep-json/tests/write_test.rs.

//! Strict JSON for CEP&CC tooling.
//!
//! The parser is a recursive-descent reader over bytes with a hard depth
//! bound (no unbounded recursion), strict duplicate-key rejection, strict
//! trailing-comma rejection, and line/column-accurate errors. Objects keep
//! insertion order so every traversal is deterministic.

#![deny(missing_docs)]

/// Parses JSON text (re-exported parser entry point).
pub mod parse;
/// Serializes JSON values deterministically.
pub mod write;

pub use parse::{parse, ParseError};
pub use write::{to_string_compact, to_string_pretty};

/// Maximum nesting depth accepted by the parser.
pub const MAX_DEPTH: usize = 128;

/// A JSON value. Object members keep insertion order.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    /// The JSON `null` literal.
    Null,
    /// A JSON boolean.
    Bool(bool),
    /// A JSON number stored as f64.
    Num(f64),
    /// A JSON string.
    Str(String),
    /// A JSON array.
    Arr(Vec<Json>),
    /// A JSON object with ordered members.
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// Looks up an object member by key; returns None for non-objects.
    // CEP:WHAT: Looks up an object member by key.
    // CEP:WHY: Callers need deterministic first-match lookup on ordered objects instead of hash iteration.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None for missing key or non-object value.
    // CEP:ASSUMES: none
    // CEP:COST: O(members); cold path only.
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    // CEP:WHAT: Interprets the value as a UTF-8 string slice.
    // CEP:WHY: Configuration fields are strings; this accessor centralizes the type check.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when the value is not a string.
    // CEP:ASSUMES: none
    // CEP:COST: O(1); cold path only.
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    /// Interprets the value as a UTF-8 string slice.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    // CEP:WHAT: Interprets the value as an f64 number.
    // CEP:WHY: Numeric thresholds and severities are stored as JSON numbers.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when the value is not a number.
    // CEP:ASSUMES: none
    // CEP:COST: O(1); cold path only.
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    /// Interprets the value as an f64 number.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }

    // CEP:WHAT: Interprets the value as an unsigned 64-bit integer.
    // CEP:WHY: Counts, sizes, and severity codes must be exact non-negative integers.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None for negative, fractional, or out-of-range numbers.
    // CEP:ASSUMES: f64 exactly represents integers up to 2^53; the type system cannot enforce this, so the range check below rejects anything larger.
    // CEP:COST: O(1); cold path only.
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    /// Interprets the value as an unsigned 64-bit integer; range-checked.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Num(n) if n.fract() == 0.0 && *n >= 0.0 && *n <= 9_007_199_254_740_992.0 => {
                Some(*n as u64)
            }
            _ => None,
        }
    }

    // CEP:WHAT: Interprets the value as a signed 64-bit integer.
    // CEP:WHY: Some fields (offsets, deltas) are signed.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None for fractional or out-of-range numbers.
    // CEP:ASSUMES: f64 exactly represents integers up to 2^53 in either direction; the range check rejects anything larger.
    // CEP:COST: O(1); cold path only.
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    /// Interprets the value as a signed 64-bit integer; range-checked.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Num(n) if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_992.0 => {
                Some(*n as i64)
            }
            _ => None,
        }
    }

    // CEP:WHAT: Interprets the value as a boolean.
    // CEP:WHY: Feature flags are JSON booleans.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when the value is not a boolean.
    // CEP:ASSUMES: none
    // CEP:COST: O(1); cold path only.
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    /// Interprets the value as a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    // CEP:WHAT: Interprets the value as an array reference.
    // CEP:WHY: Lists of globs, rules, and fixtures are JSON arrays.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when the value is not an array.
    // CEP:ASSUMES: none
    // CEP:COST: O(1); cold path only.
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    /// Interprets the value as an array reference.
    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(items) => Some(items),
            _ => None,
        }
    }

    // CEP:WHAT: Interprets the value as an object member slice.
    // CEP:WHY: Rule catalogs iterate ordered members during DOC-SYNC checks.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when the value is not an object.
    // CEP:ASSUMES: none
    // CEP:COST: O(1); cold path only.
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    /// Interprets the value as an ordered object member slice.
    pub fn as_obj(&self) -> Option<&[(String, Json)]> {
        match self {
            Json::Obj(members) => Some(members),
            _ => None,
        }
    }
}

// CEP:FILE: crates/cep-json/src/parse.rs
// CEP:WHAT: Strict recursive-descent JSON parser with line and column accurate errors and a hard depth bound.
// CEP:WHY: Bootstrap error code 2 of the lint tool requires file, line, and column on parse failure, and unbounded recursion is banned by CEP&CC Law 6, so the parser walks bytes with explicit position tracking and a depth limit.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns ParseError on malformed syntax, duplicate keys, depth overflow, or invalid escapes; never panics on any byte input.
// CEP:ASSUMES: Input length fits in usize; enforced by the platform memory model.
// CEP:COST: cold; O(n) time over the input, O(depth) stack, O(n) allocations for the value tree.
// CEP:EVIDENCE: unit tests in crates/cep-json/tests/parse_test.rs.

use crate::Json;

/// A parse failure with source position.
#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    /// Zero-based line of the failure.
    pub line: usize,
    /// Zero-based column of the failure.
    pub col: usize,
    /// Failure reason.
    pub message: String,
}

impl std::fmt::Display for ParseError {
    // CEP:WHAT: Formats the error as line:column: message.
    // CEP:WHY: Bootstrap diagnostics must be deterministic and machine-scannable.
    // CEP:STATUS: complete
    // CEP:FAILURE: none; formatting a short string cannot fail.
    // CEP:ASSUMES: none
    // CEP:COST: cold; one allocation for the formatted string.
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: usize,
    col: usize,
}

// CEP:WHAT: Parses one JSON document with strict grammar and position-accurate errors.
// CEP:WHY: Config and manifest loaders need one strict entry point with no silent recovery.
// CEP:STATUS: complete
// CEP:FAILURE: Returns ParseError with line and column on any malformed input.
// CEP:ASSUMES: input is valid UTF-8 text.
// CEP:COST: cold; O(n) over the input.
// CEP:EVIDENCE: crates/cep-json/tests/parse_test.rs
/// Parses `text` as a single JSON document.
/// Trailing whitespace is allowed; trailing content is an error.
pub fn parse(text: &str) -> Result<Json, ParseError> {
    let mut p = Parser {
        bytes: text.as_bytes(),
        pos: 0,
        line: 0,
        col: 0,
    };
    p.skip_ws();
    let value = p.value(0)?;
    p.skip_ws();
    if p.pos < p.bytes.len() {
        return Err(p.err("trailing content after JSON document"));
    }
    Ok(value)
}

impl<'a> Parser<'a> {
    // CEP:WHAT: Builds a positioned error at the current byte.
    // CEP:WHY: All failure paths must report the same position convention.
    // CEP:STATUS: complete
    // CEP:FAILURE: none; constructs the error value without side effects.
    // CEP:ASSUMES: none
    // CEP:COST: cold; one allocation for the message.
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    fn err(&self, msg: &str) -> ParseError {
        ParseError {
            line: self.line,
            col: self.col,
            message: msg.to_string(),
        }
    }

    // CEP:WHAT: Advances the cursor over one byte, maintaining line and column counters.
    // CEP:WHY: Column-accurate errors need the position of the failing byte, not the byte after it.
    // CEP:STATUS: complete
    // CEP:FAILURE: none; caller guarantees the cursor is inside the input.
    // CEP:ASSUMES: caller checked `pos < bytes.len()` immediately above; enforced by debug assertion.
    // CEP:COST: cold; O(1).
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    fn bump(&mut self) {
        debug_assert!(self.pos < self.bytes.len());
        let b = self.bytes[self.pos];
        self.pos += 1;
        if b == b'\n' {
            self.line += 1;
            self.col = 0;
        } else {
            self.col += 1;
        }
    }

    // CEP:WHAT: Returns the current byte without consuming it.
    // CEP:WHY: Lookahead keeps the grammar readable.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None at end of input.
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(1).
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    // CEP:WHAT: Skips spaces, tabs, carriage returns, and newlines.
    // CEP:WHY: JSON whitespace is exactly these four bytes.
    // CEP:STATUS: complete
    // CEP:FAILURE: none; stops at any other byte or end of input.
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(n) over the whitespace run.
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                self.bump();
            } else {
                break;
            }
        }
    }

    // CEP:WHAT: Parses one JSON value at the given nesting depth.
    // CEP:WHY: Depth threading enforces the hard recursion bound of Law 6.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns ParseError on any malformed value or depth overflow.
    // CEP:ASSUMES: depth <= MAX_DEPTH is checked on entry.
    // CEP:COST: cold; proportional to the value's byte size.
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    fn value(&mut self, depth: usize) -> Result<Json, ParseError> {
        if depth > crate::MAX_DEPTH {
            return Err(self.err("nesting depth exceeds limit"));
        }
        match self.peek() {
            None => Err(self.err("unexpected end of input")),
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b) if b == b'-' || b.is_ascii_digit() => self.number(),
            Some(_) => Err(self.err("unexpected byte")),
        }
    }

    // CEP:WHAT: Consumes an expected keyword literal.
    // CEP:WHY: true, false, and null are the three JSON keywords.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns ParseError when the keyword is misspelled or truncated.
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(keyword length).
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    fn literal(&mut self, word: &str, value: Json) -> Result<Json, ParseError> {
        for expected in word.bytes() {
            match self.peek() {
                Some(b) if b == expected => self.bump(),
                _ => return Err(self.err("invalid literal")),
            }
        }
        Ok(value)
    }

    // CEP:WHAT: Parses a JSON object with ordered members and duplicate-key rejection.
    // CEP:WHY: Duplicate keys make configuration ambiguous; the standard bans silent assumptions, so ambiguity is an error.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns ParseError on malformed syntax or duplicate keys.
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(size of object).
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    fn object(&mut self, depth: usize) -> Result<Json, ParseError> {
        self.bump();
        let mut members: Vec<(String, Json)> = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.bump();
            return Ok(Json::Obj(members));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(self.err("expected object key string"));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(self.err("expected ':' after object key"));
            }
            self.bump();
            self.skip_ws();
            let value = self.value(depth + 1)?;
            if members.iter().any(|(k, _)| *k == key) {
                return Err(self.err("duplicate object key"));
            }
            members.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.bump(),
                Some(b'}') => {
                    self.bump();
                    return Ok(Json::Obj(members));
                }
                _ => return Err(self.err("expected ',' or '}' in object")),
            }
        }
    }

    // CEP:WHAT: Parses a JSON array with strict comma placement.
    // CEP:WHY: Trailing commas are rejected to keep the grammar strict.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns ParseError on malformed syntax.
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(size of array).
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    fn array(&mut self, depth: usize) -> Result<Json, ParseError> {
        self.bump();
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.bump();
            return Ok(Json::Arr(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value(depth + 1)?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.bump(),
                Some(b']') => {
                    self.bump();
                    return Ok(Json::Arr(items));
                }
                _ => return Err(self.err("expected ',' or ']' in array")),
            }
        }
    }

    // CEP:WHAT: Parses a JSON string with full escape handling including surrogate pairs.
    // CEP:WHY: Configuration paths and messages may contain escapes; control bytes are rejected so no invalid UTF-8 enters the value tree.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns ParseError on unterminated strings, raw control bytes, or invalid escapes.
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(size of string).
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    fn string(&mut self) -> Result<String, ParseError> {
        if self.peek() != Some(b'"') {
            return Err(self.err("expected string"));
        }
        self.bump();
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => {
                    self.bump();
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.bump();
                    let esc = match self.peek() {
                        None => return Err(self.err("unterminated escape")),
                        Some(e) => e,
                    };
                    self.bump();
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let ch = if (0xD800..=0xDBFF).contains(&hi) {
                                if self.peek() != Some(b'\\') {
                                    return Err(self.err("lone high surrogate"));
                                }
                                self.bump();
                                if self.peek() != Some(b'u') {
                                    return Err(self.err("lone high surrogate"));
                                }
                                self.bump();
                                let lo = self.hex4()?;
                                if !(0xDC00..=0xDFFF).contains(&lo) {
                                    return Err(self.err("invalid low surrogate"));
                                }
                                let combined = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                                match char::from_u32(combined) {
                                    Some(c) => c,
                                    None => return Err(self.err("invalid surrogate pair")),
                                }
                            } else if (0xDC00..=0xDFFF).contains(&hi) {
                                return Err(self.err("lone low surrogate"));
                            } else {
                                match char::from_u32(hi) {
                                    Some(c) => c,
                                    None => return Err(self.err("invalid unicode escape")),
                                }
                            };
                            out.push(ch);
                        }
                        _ => return Err(self.err("invalid escape byte")),
                    }
                }
                Some(b) if b < 0x20 => return Err(self.err("raw control byte in string")),
                Some(_) => {
                    let start = self.pos;
                    while let Some(b) = self.peek() {
                        if b == b'"' || b == b'\\' || b < 0x20 {
                            break;
                        }
                        self.bump();
                    }
                    let chunk = std::str::from_utf8(&self.bytes[start..self.pos])
                        .map_err(|_| self.err("invalid UTF-8 in string"))?;
                    out.push_str(chunk);
                }
            }
        }
    }

    // CEP:WHAT: Reads exactly four hex digits as a u32.
    // CEP:WHY: The unicode escape grammar requires four digits.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns ParseError when fewer than four valid hex digits remain.
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(4).
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    fn hex4(&mut self) -> Result<u32, ParseError> {
        let mut v: u32 = 0;
        for _ in 0..4 {
            let b = match self.peek() {
                Some(b) => b,
                None => return Err(self.err("unterminated unicode escape")),
            };
            let digit = match b {
                b'0'..=b'9' => u32::from(b - b'0'),
                b'a'..=b'f' => u32::from(b - b'a') + 10,
                b'A'..=b'F' => u32::from(b - b'A') + 10,
                _ => return Err(self.err("invalid hex digit")),
            };
            self.bump();
            v = v * 16 + digit;
        }
        Ok(v)
    }

    // CEP:WHAT: Parses a JSON number with strict grammar and f64 semantics.
    // CEP:WHY: JSON numbers are decimal; leading zeros, lone minus, and hex are invalid.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns ParseError on malformed numbers or values outside f64 range.
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(size of number).
    // CEP:EVIDENCE: unit test crates/cep-json/tests/parse_test.rs
    fn number(&mut self) -> Result<Json, ParseError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.bump();
        }
        match self.peek() {
            Some(b'0') => {
                self.bump();
                if let Some(b) = self.peek() {
                    if b.is_ascii_digit() {
                        return Err(self.err("leading zero in number"));
                    }
                }
            }
            Some(b) if b.is_ascii_digit() => {
                while let Some(b) = self.peek() {
                    if b.is_ascii_digit() {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
            _ => return Err(self.err("invalid number")),
        }
        if self.peek() == Some(b'.') {
            self.bump();
            if !matches!(self.peek(), Some(b) if b.is_ascii_digit()) {
                return Err(self.err("missing digits after decimal point"));
            }
            while let Some(b) = self.peek() {
                if b.is_ascii_digit() {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.bump();
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.bump();
            }
            if !matches!(self.peek(), Some(b) if b.is_ascii_digit()) {
                return Err(self.err("missing digits in exponent"));
            }
            while let Some(b) = self.peek() {
                if b.is_ascii_digit() {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| self.err("invalid UTF-8 in number"))?;
        let n: f64 = text
            .parse()
            .map_err(|_| self.err("number out of f64 range"))?;
        Ok(Json::Num(n))
    }
}

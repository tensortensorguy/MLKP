// CEP:FILE: tools/cep_lint/src/lex.rs
// CEP:WHAT: A Rust-aware lexical scanner that separates code from comments and string literals while preserving byte positions.
// CEP:WHY: Chapter 50 rules must match tokens after comment and string removal and must report exact file and line positions, so masking replaces every masked byte with a space instead of deleting it.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: none; the scanner accepts any byte input and never panics; malformed constructs degrade to code text rather than truncation.
// CEP:ASSUMES: Input is valid UTF-8; the caller reads source files as String. Char-literal recognition uses a bounded same-line window (Rust char literals cannot span lines), and lifetimes are left as code.
// CEP:COST: cold; O(n) over the file with one masked buffer allocation per file.
// CEP:EVIDENCE: unit tests in tools/cep_lint/tests/lex_test.rs.

/// One scanned source line.
#[derive(Debug, Clone)]
pub struct LineInfo {
    /// One-based line number.
    pub line_no: usize,
    /// Original line text without the newline.
    pub raw: String,
    /// Masked line: comments and string/char contents replaced by spaces, same length as raw.
    pub code: String,
    /// Comment text on this line (original characters), if any.
    pub comment: Option<String>,
}

/// A string literal occurrence recorded for string-level checks.
#[derive(Debug, Clone)]
pub struct StringLit {
    /// One-based line of the opening delimiter.
    pub line: usize,
    /// Raw literal content without delimiters.
    pub content: String,
}

/// Scan result for one source file.
#[derive(Debug, Clone)]
pub struct Lexed {
    /// Per-line scan info, one entry per source line.
    pub lines: Vec<LineInfo>,
    /// String literals in source order.
    pub strings: Vec<StringLit>,
}

/// A comment content range in original char coordinates.
struct CommentSpan {
    start: usize,
    end: usize,
}

/// Scans Rust source text into masked lines plus comment and string records.
// CEP:WHAT: Scans one source file into masked lines, comment regions, and string literals.
// CEP:WHY: All region-scoped rules depend on exact code/comment/string separation with preserved positions.
// CEP:STATUS: complete
// CEP:FAILURE: none; malformed constructs degrade to code text.
// CEP:ASSUMES: input is valid UTF-8.
// CEP:COST: cold; O(n).
// CEP:EVIDENCE: tools/cep_lint/tests/lex_test.rs
pub fn lex(text: &str) -> Lexed {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut masked = chars.clone();
    let mut comments: Vec<CommentSpan> = Vec::new();
    let mut strings: Vec<StringLit> = Vec::new();
    let mut i = 0;
    let mut line = 0;
    while i < n {
        let c = chars[i];
        if c == '/' && i + 1 < n && chars[i + 1] == '/' {
            let content_start = i + 2;
            let mut j = i;
            while j < n && chars[j] != '\n' {
                j += 1;
            }
            mask_range(&mut masked, i, j);
            comments.push(CommentSpan {
                start: content_start.min(j),
                end: j,
            });
            i = j;
        } else if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            let mut j = i + 2;
            while j + 1 < n && !(chars[j] == '*' && chars[j + 1] == '/') {
                if chars[j] == '\n' {
                    line += 1;
                }
                j += 1;
            }
            let content_start = i + 2;
            let content_end = j.min(n);
            let close_end = (j + 2).min(n);
            mask_range(&mut masked, i, close_end);
            comments.push(CommentSpan {
                start: content_start,
                end: content_end,
            });
            i = close_end;
        } else if c == '"' {
            let start_line = line;
            let open = i;
            let mut j = i + 1;
            while j < n {
                let t = chars[j];
                if t == '\\' && j + 1 < n {
                    j += 2;
                    continue;
                }
                if t == '"' || t == '\n' {
                    break;
                }
                j += 1;
            }
            let content_end = j.min(n);
            let close = if j < n && chars[j] == '"' { j + 1 } else { j };
            mask_range(&mut masked, open, close);
            strings.push(StringLit {
                line: start_line + 1,
                content: chars[open + 1..content_end].iter().collect(),
            });
            i = close;
        } else if c == 'r'
            && i + 1 < n
            && (chars[i + 1] == '"' || chars[i + 1] == '#')
            && raw_prefix_ok(&chars, i)
        {
            let start_line = line;
            let open = i;
            let mut hashes = 0usize;
            let mut j = i + 1;
            while j < n && chars[j] == '#' {
                hashes += 1;
                j += 1;
            }
            if j < n && chars[j] == '"' {
                let content_start = j + 1;
                j += 1;
                let mut done = false;
                while j < n && !done {
                    if chars[j] == '"' {
                        let mut k = j + 1;
                        let mut seen = 0usize;
                        while k < n && seen < hashes && chars[k] == '#' {
                            seen += 1;
                            k += 1;
                        }
                        if seen == hashes {
                            let content_end = j;
                            mask_range(&mut masked, open, k);
                            strings.push(StringLit {
                                line: start_line + 1,
                                content: chars[content_start..content_end].iter().collect(),
                            });
                            j = k;
                            done = true;
                        }
                    }
                    if !done {
                        if chars[j] == '\n' {
                            line += 1;
                        }
                        j += 1;
                    }
                }
                i = j;
            } else {
                i += 1;
            }
        } else if c == '\'' {
            match char_literal_len(&chars, i) {
                Some(len) => {
                    mask_range(&mut masked, i, (i + len).min(n));
                    i += len;
                }
                None => {
                    i += 1;
                }
            }
        } else {
            if c == '\n' {
                line += 1;
            }
            i += 1;
        }
    }
    build_lines(text, &chars, &masked, &comments, strings)
}

// CEP:WHAT: Overwrites one char range with spaces.
// CEP:WHY: Masking preserves column positions for finding reports.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: range is within buffer bounds; enforced by clamping below.
// CEP:COST: cold; O(range length).
// CEP:EVIDENCE: tools/cep_lint/tests/lex_test.rs
fn mask_range(masked: &mut [char], start: usize, end: usize) {
    let end = end.min(masked.len());
    let start = start.min(end);
    for m in masked[start..end].iter_mut() {
        *m = ' ';
    }
}

// CEP:WHAT: Decides whether the r at position i starts a raw string.
// CEP:WHY: An r preceded by an identifier char is part of a name, unless it is the b or c byte-string prefix.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/lex_test.rs
fn raw_prefix_ok(chars: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = chars[i - 1];
    !(prev.is_ascii_alphanumeric() || prev == '_') || prev == 'b' || prev == 'c'
}

// CEP:WHAT: Measures a char literal starting at the quote, or None for a lifetime.
// CEP:WHY: Lifetimes and char literals share the quote; misclassification would corrupt masking.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: char literals close on the same source line.
// CEP:COST: cold; O(1) bounded window.
// CEP:EVIDENCE: tools/cep_lint/tests/lex_test.rs
fn char_literal_len(chars: &[char], i: usize) -> Option<usize> {
    let n = chars.len();
    let window = 12;
    let end = (i + window).min(n);
    if i + 1 >= n {
        return None;
    }
    if chars[i + 1] == '\\' {
        let mut j = i + 1;
        while j < end {
            if chars[j] == '\'' {
                return Some(j + 1 - i);
            }
            j += 1;
        }
        None
    } else if i + 2 < end && chars[i + 2] == '\'' && chars[i + 1] != '\n' {
        Some(3)
    } else {
        None
    }
}

// CEP:WHAT: Splits original and masked text into parallel per-line records with comment text.
// CEP:WHY: Checks consume per-line code and comment text.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(n).
// CEP:EVIDENCE: tools/cep_lint/tests/lex_test.rs
fn build_lines(
    text: &str,
    chars: &[char],
    masked: &[char],
    comments: &[CommentSpan],
    strings: Vec<StringLit>,
) -> Lexed {
    let raw_lines: Vec<&str> = text.split('\n').collect();
    let mut comment_lines: Vec<Option<String>> = vec![None; raw_lines.len()];
    for span in comments {
        let mut cur_line = line_idx_at(chars, span.start);
        let mut buf = String::new();
        for ch in &chars[span.start..span.end.min(chars.len())] {
            if *ch == '\n' {
                append_comment(&mut comment_lines, cur_line, &buf);
                cur_line += 1;
                buf.clear();
            } else {
                buf.push(*ch);
            }
        }
        append_comment(&mut comment_lines, cur_line, &buf);
    }
    let mut lines = Vec::new();
    let mut off = 0usize;
    for (idx, raw) in raw_lines.iter().enumerate() {
        let len = raw.chars().count();
        let end = (off + len).min(masked.len());
        let code: String = masked[off..end].iter().collect();
        lines.push(LineInfo {
            line_no: idx + 1,
            raw: (*raw).to_string(),
            code,
            comment: comment_lines[idx].clone(),
        });
        off = end + 1;
    }
    Lexed { lines, strings }
}

// CEP:WHAT: Computes the zero-based line index of a char offset.
// CEP:WHY: Comment spans must be attributed to their source lines.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(offset) over the file; comment spans are few.
// CEP:EVIDENCE: tools/cep_lint/tests/lex_test.rs
fn line_idx_at(chars: &[char], off: usize) -> usize {
    chars[..off.min(chars.len())]
        .iter()
        .filter(|c| **c == '\n')
        .count()
}

// CEP:WHAT: Appends comment text to a line's comment buffer.
// CEP:WHY: One line can carry several comment fragments.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(fragment length).
// CEP:EVIDENCE: tools/cep_lint/tests/lex_test.rs
fn append_comment(comment_lines: &mut [Option<String>], line: usize, buf: &str) {
    if line < comment_lines.len() && !buf.trim().is_empty() {
        match comment_lines[line].as_mut() {
            Some(existing) => existing.push_str(buf),
            None => comment_lines[line] = Some(buf.to_string()),
        }
    }
}

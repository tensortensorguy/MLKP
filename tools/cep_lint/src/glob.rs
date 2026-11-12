// CEP:FILE: tools/cep_lint/src/glob.rs
// CEP:WHAT: Minimal glob matcher supporting the **, *, and ? wildcards over path components.
// CEP:WHY: file_globs select the first-party source dialects; a tiny matcher avoids a dependency while keeping traversal deterministic.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: none; unmatched patterns simply return false.
// CEP:ASSUMES: Paths use the platform separator; matching is done on normalized forward-slash components.
// CEP:COST: cold; O(components * pattern segments).
// CEP:EVIDENCE: unit tests in tools/cep_lint/tests/glob_test.rs.

/// Returns true when `path` matches the glob pattern.
// CEP:WHAT: Matches one path against one glob pattern with **, *, and ?.
// CEP:WHY: file_globs select the first-party dialects without a dependency.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(components * segments).
// CEP:EVIDENCE: tools/cep_lint/tests/glob_test.rs
pub fn matches(path: &str, pattern: &str) -> bool {
    let path_parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let pat_parts: Vec<&str> = pattern.split('/').filter(|p| !p.is_empty()).collect();
    match_from(&path_parts, &pat_parts)
}

// CEP:WHAT: Recursive component matcher for glob patterns.
// CEP:WHY: `**` spans multiple components while `*` spans one.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: recursion depth is bounded by the pattern length, which configuration controls.
// CEP:COST: cold; O(p * q).
// CEP:EVIDENCE: tools/cep_lint/tests/glob_test.rs
fn match_from(path: &[&str], pat: &[&str]) -> bool {
    match pat.split_first() {
        None => path.is_empty(),
        Some((seg, rest)) => {
            if *seg == "**" {
                if rest.is_empty() {
                    return true;
                }
                for skip in 0..=path.len() {
                    if match_from(&path[skip..], rest) {
                        return true;
                    }
                }
                false
            } else {
                match path.split_first() {
                    None => false,
                    Some((first, prest)) => seg_match(seg, first) && match_from(prest, rest),
                }
            }
        }
    }
}

// CEP:WHAT: Matches one path component against one pattern segment with * and ?.
// CEP:WHY: Component-level wildcards keep `*` from crossing directory boundaries.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(len(seg) * len(name)) backtracking.
// CEP:EVIDENCE: tools/cep_lint/tests/glob_test.rs
fn seg_match(seg: &str, name: &str) -> bool {
    let s: Vec<char> = seg.chars().collect();
    let t: Vec<char> = name.chars().collect();
    fn go(s: &[char], t: &[char]) -> bool {
        match s.split_first() {
            None => t.is_empty(),
            Some((c, rest)) => match c {
                '*' => {
                    for skip in 0..=t.len() {
                        if go(rest, &t[skip..]) {
                            return true;
                        }
                    }
                    false
                }
                '?' => !t.is_empty() && go(rest, &t[1..]),
                c => !t.is_empty() && t[0] == *c && go(rest, &t[1..]),
            },
        }
    }
    go(&s, &t)
}

// CEP:FILE: tools/cep_lint/src/re.rs
// CEP:WHAT: A small backtracking regular expression engine with captures, anchors, classes, alternation, and a bounded step budget.
// CEP:WHY: Chapter 50 stores lint patterns in configuration as regular expressions and the repository bans third-party dependencies, so the engine must exist in-tree; the step budget enforces Law 6 by refusing pathological patterns to run unbounded.
// CEP:CLASS: CEP-2
// CEP:STATUS: complete
// CEP:FAILURE: Compile returns an error string on unsupported syntax; matching returns None if the step budget is exhausted (treated as no match, and the budget is sized far above any legitimate pattern in the configuration).
// CEP:ASSUMES: Case folding is ASCII-only; the ^ and $ anchors bind to line boundaries (multiline semantics) because lint patterns are line oriented; lazy quantifiers are not supported by design (configuration patterns avoid them).
// CEP:COST: cold; matching is exponential in the worst case but bounded by STEP_BUDGET steps per start position.
// CEP:EVIDENCE: unit tests in tools/cep_lint/tests/re_test.rs.

/// Hard cap on backtracking steps for one match attempt.
pub const STEP_BUDGET: u32 = 1_000_000;

/// A compiled pattern.
#[derive(Debug)]
pub struct Regex {
    /// Root AST node.
    root: Node,
    /// ASCII case-fold flag.
    case_fold: bool,
}

#[derive(Debug, Clone)]
enum Node {
    Char(char),
    Any,
    Class {
        neg: bool,
        items: Vec<ClassItem>,
    },
    Start,
    End,
    WordB,
    Group(usize, Box<Node>),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: u32,
        max: Option<u32>,
    },
}

#[derive(Debug, Clone)]
enum ClassItem {
    Ch(char),
    Range(char, char),
    Digit,
    NonDigit,
    Word,
    NonWord,
    Space,
    NonSpace,
}

/// One successful match with capture group spans (char indices over the scanned buffer).
#[derive(Debug, Clone)]
pub struct Match {
    /// Char index of the first matched char.
    pub start: usize,
    /// Char index one past the last matched char.
    pub end: usize,
    /// Span of each capture group; explicit group 1 is index 0 of this vector.
    pub groups: Vec<Option<(usize, usize)>>,
}

impl Match {
    /// Returns capture group i (0-based over explicit groups) as a char slice.
    // CEP:WHAT: Returns capture group i as a char slice over the scanned buffer.
    // CEP:WHY: Capture extraction must not allocate; char-index spans index directly into the caller's char buffer.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when the group did not participate in the match.
    // CEP:ASSUMES: chars is the char vector the match was computed on.
    // CEP:COST: cold; O(group length).
    // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
    pub fn group_chars<'a>(&self, chars: &'a [char], i: usize) -> Option<&'a [char]> {
        self.groups
            .get(i)
            .and_then(|g| g.map(|(s, e)| &chars[s..e]))
    }

    /// Returns the char index span of the whole match.
    // CEP:WHAT: Returns the char index span of the whole match.
    // CEP:WHY: Findings need the start column of a match for deterministic reporting.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(1).
    // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
    pub fn span(&self) -> (usize, usize) {
        (self.start, self.end)
    }
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    ngroups: usize,
}

/// Compiles a pattern. Supported syntax: literals, `.`, character classes with ranges and
/// `\d \D \w \W \s \S`, escapes, `^ $ \b`, groups `()` and `(?:)`, alternation `|`,
/// quantifiers `* + ? {m} {m,} {m,n}`.
// CEP:WHAT: Compiles one pattern into the engine AST.
// CEP:WHY: Configuration patterns must be validated once at load with precise errors.
// CEP:STATUS: complete
// CEP:FAILURE: Returns Err with position on unsupported syntax.
// CEP:ASSUMES: anchors are line-anchored; ASCII-only case folding.
// CEP:COST: cold; O(pattern size).
// CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
pub fn compile(pattern: &str, case_fold: bool) -> Result<Regex, String> {
    let mut p = Parser {
        chars: pattern.chars().collect(),
        pos: 0,
        ngroups: 0,
    };
    let root = p.alt()?;
    if p.pos != p.chars.len() {
        return Err(format!("unexpected '{}' at {}", p.chars[p.pos], p.pos));
    }
    Ok(Regex { root, case_fold })
}

impl Regex {
    /// Searches for the first match anywhere in the char buffer.
    // CEP:WHAT: Searches for the first match anywhere in the char buffer.
    // CEP:WHY: Rule checks scan lines and flattened documents for patterns.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None when no match exists or the step budget is exhausted.
    // CEP:ASSUMES: none
    // CEP:COST: cold; bounded by STEP_BUDGET per start position.
    // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
    pub fn find(&self, text: &[char]) -> Option<Match> {
        // CEP:WHAT: Resets the step budget at every start position.
        // CEP:WHY: The documented contract bounds each start position's work; a cumulative counter would exhaust the budget on early failed starts and silently miss legitimate matches late in the input.
        // CEP:STATUS: complete
        // CEP:FAILURE: Returns None when a single start position exhausts the budget.
        // CEP:ASSUMES: none
        // CEP:COST: O(starts * budget).
        // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
        let steps = std::cell::Cell::new(0u32);
        for start in 0..=text.len() {
            steps.set(0);
            let mut caps: Vec<Option<(usize, usize)>> = vec![None; self.group_count()];
            if let Some(end) = match_node(
                &self.root,
                text,
                start,
                &mut caps,
                &steps,
                self.case_fold,
                &|p, _c| Some(p),
            ) {
                return Some(Match {
                    start,
                    end,
                    groups: caps,
                });
            }
        }
        None
    }

    /// Reports whether the pattern matches anywhere.
    // CEP:WHAT: Reports whether the pattern matches anywhere.
    // CEP:WHY: Most rule checks only need existence.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns false when no match exists or the budget is exhausted.
    // CEP:ASSUMES: none
    // CEP:COST: cold; bounded by STEP_BUDGET per start position.
    // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
    pub fn is_match(&self, text: &[char]) -> bool {
        self.find(text).is_some()
    }

    // CEP:WHAT: Counts explicit capture groups in the AST.
    // CEP:WHY: Capture vector size must be known before matching.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(nodes).
    // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
    fn group_count(&self) -> usize {
        fn count(n: &Node) -> usize {
            match n {
                Node::Group(_, b) => 1 + count(b),
                Node::Concat(v) | Node::Alt(v) => v.iter().map(count).sum(),
                Node::Repeat { node: b, .. } => count(b),
                _ => 0,
            }
        }
        count(&self.root)
    }
}

type Cont<'k> = &'k dyn Fn(usize, &mut Vec<Option<(usize, usize)>>) -> Option<usize>;

// CEP:ID: CS-a49
// CEP:WHAT: Matches one AST node at a position with continuation-passing backtracking.
// CEP:WHY: The node dispatcher is the core of the matcher; anchors, classes, groups, and repeats route through it.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None on mismatch or step-budget exhaustion.
// CEP:ASSUMES: none
// CEP:COST: cold; bounded by STEP_BUDGET.
// CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
#[allow(clippy::too_many_arguments)]
fn match_node(
    node: &Node,
    text: &[char],
    pos: usize,
    caps: &mut Vec<Option<(usize, usize)>>,
    steps: &std::cell::Cell<u32>,
    fold: bool,
    k: Cont<'_>,
) -> Option<usize> {
    let s = steps.get();
    if s >= STEP_BUDGET {
        return None;
    }
    steps.set(s + 1);
    match node {
        Node::Char(c) => {
            let t = text.get(pos).copied()?;
            let ok = if fold {
                t.eq_ignore_ascii_case(c)
            } else {
                t == *c
            };
            if ok {
                k(pos + 1, caps)
            } else {
                None
            }
        }
        Node::Any => {
            let t = text.get(pos).copied()?;
            if t == '\n' {
                None
            } else {
                k(pos + 1, caps)
            }
        }
        Node::Class { neg, items } => {
            let t = text.get(pos).copied()?;
            let mut inside = class_matches(items, t, fold);
            if *neg {
                inside = !inside;
            }
            if inside {
                k(pos + 1, caps)
            } else {
                None
            }
        }
        Node::Start => {
            // Line-anchored: start of text or first char after a newline.
            if pos == 0 || text[pos - 1] == '\n' {
                k(pos, caps)
            } else {
                None
            }
        }
        Node::End => {
            // Line-anchored: end of text or position of a newline.
            if pos == text.len() || text[pos] == '\n' {
                k(pos, caps)
            } else {
                None
            }
        }
        Node::WordB => {
            let before = pos > 0 && is_word(text[pos - 1]);
            let after = pos < text.len() && is_word(text[pos]);
            if before != after {
                k(pos, caps)
            } else {
                None
            }
        }
        Node::Group(idx, inner) => {
            let start = pos;
            let my = *idx - 1;
            let saved = caps[my];
            let r = match_node(inner, text, pos, caps, steps, fold, &|p, c| {
                let prev = c[my];
                c[my] = Some((start, p));
                if k(p, c).is_some() {
                    Some(p)
                } else {
                    c[my] = prev;
                    None
                }
            });
            if r.is_none() {
                caps[my] = saved;
            }
            r
        }
        Node::Concat(items) => match_seq(items, text, pos, caps, steps, fold, k),
        Node::Alt(branches) => {
            for b in branches {
                if let Some(e) = match_node(b, text, pos, caps, steps, fold, k) {
                    return Some(e);
                }
            }
            None
        }
        Node::Repeat { node, min, max } => {
            match_repeat(node, *min, *max, text, pos, 0, caps, steps, fold, k)
        }
    }
}

// CEP:WHAT: Matches a node sequence left to right via continuation chaining.
// CEP:WHY: Concatenation is the composition operator of the engine.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None when any element fails.
// CEP:ASSUMES: none
// CEP:COST: cold; bounded by STEP_BUDGET.
// CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
fn match_seq(
    items: &[Node],
    text: &[char],
    pos: usize,
    caps: &mut Vec<Option<(usize, usize)>>,
    steps: &std::cell::Cell<u32>,
    fold: bool,
    k: Cont<'_>,
) -> Option<usize> {
    match items.split_first() {
        None => k(pos, caps),
        Some((head, rest)) => match_node(head, text, pos, caps, steps, fold, &|p, c| {
            match_seq(rest, text, p, c, steps, fold, k)
        }),
    }
}

// CEP:WHAT: Matches a quantified node with greedy backtracking between min and max repetitions.
// CEP:WHY: Quantifiers are the core repetition mechanism.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None when no repetition count satisfies the continuation.
// CEP:ASSUMES: count never exceeds the remaining text length.
// CEP:COST: cold; bounded by STEP_BUDGET.
// CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
#[allow(clippy::too_many_arguments)]
fn match_repeat(
    node: &Node,
    min: u32,
    max: Option<u32>,
    text: &[char],
    pos: usize,
    count: u32,
    caps: &mut Vec<Option<(usize, usize)>>,
    steps: &std::cell::Cell<u32>,
    fold: bool,
    k: Cont<'_>,
) -> Option<usize> {
    let can_more = max.map(|m| count < m).unwrap_or(true);
    if can_more {
        let more = match_node(node, text, pos, caps, steps, fold, &|p, c| {
            if p == pos {
                // Zero-width progress guard: repeating without consuming input loops forever.
                return None;
            }
            match_repeat(node, min, max, text, p, count + 1, c, steps, fold, k)
        });
        if more.is_some() {
            return more;
        }
    }
    if count >= min {
        k(pos, caps)
    } else {
        None
    }
}

// CEP:WHAT: Tests one char against class items.
// CEP:WHY: Shared by classes and shorthand escapes.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(items).
// CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
fn class_matches(items: &[ClassItem], c: char, fold: bool) -> bool {
    let probe = if fold { c.to_ascii_lowercase() } else { c };
    items.iter().any(|it| match it {
        ClassItem::Ch(x) => {
            let x = if fold { x.to_ascii_lowercase() } else { *x };
            probe == x
        }
        ClassItem::Range(a, b) => {
            let (a, b) = if fold {
                (a.to_ascii_lowercase(), b.to_ascii_lowercase())
            } else {
                (*a, *b)
            };
            probe >= a && probe <= b
        }
        ClassItem::Digit => c.is_ascii_digit(),
        ClassItem::NonDigit => !c.is_ascii_digit(),
        ClassItem::Word => is_word(c),
        ClassItem::NonWord => !is_word(c),
        ClassItem::Space => c.is_whitespace(),
        ClassItem::NonSpace => !c.is_whitespace(),
    })
}

// CEP:WHAT: Word-char predicate for the word-boundary anchor.
// CEP:WHY: Word boundary semantics follow the standard definition.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold; O(1).
// CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

impl Parser {
    // CEP:WHAT: Parses an alternation of concat sequences separated by '|'.
    // CEP:WHY: Alternation is the top-level grammar production.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns Err on syntax errors with position.
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(pattern size).
    // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
    fn alt(&mut self) -> Result<Node, String> {
        let mut branches = vec![self.concat()?];
        while self.peek() == Some('|') {
            self.pos += 1;
            branches.push(self.concat()?);
        }
        if branches.len() == 1 {
            Ok(branches.pop().expect("len checked above"))
        } else {
            Ok(Node::Alt(branches))
        }
    }

    // CEP:WHAT: Parses a concatenation of quantified atoms.
    // CEP:WHY: Sequence is the second grammar production.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns Err on syntax errors.
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(pattern size).
    // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
    fn concat(&mut self) -> Result<Node, String> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            let atom = self.atom()?;
            let atom = self.quantify(atom)?;
            items.push(atom);
        }
        Ok(Node::Concat(items))
    }

    // CEP:ID: CS-r29
    // CEP:WHAT: Wraps an atom in a repeat construct if a quantifier follows.
    // CEP:WHY: Quantifiers bind to the immediately preceding atom.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns Err on malformed braces.
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(1).
    // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
    fn quantify(&mut self, node: Node) -> Result<Node, String> {
        match self.peek() {
            Some('*') => {
                self.pos += 1;
                Ok(Node::Repeat {
                    node: Box::new(node),
                    min: 0,
                    max: None,
                })
            }
            Some('+') => {
                self.pos += 1;
                Ok(Node::Repeat {
                    node: Box::new(node),
                    min: 1,
                    max: None,
                })
            }
            Some('?') => {
                self.pos += 1;
                Ok(Node::Repeat {
                    node: Box::new(node),
                    min: 0,
                    max: Some(1),
                })
            }
            Some('{') => {
                let save = self.pos;
                self.pos += 1;
                let min = self.number()?;
                let max = match self.peek() {
                    Some('}') => {
                        self.pos += 1;
                        Some(min)
                    }
                    Some(',') => {
                        self.pos += 1;
                        if self.peek() == Some('}') {
                            self.pos += 1;
                            None
                        } else {
                            let n = self.number()?;
                            if self.peek() != Some('}') {
                                return Err("expected '}' in quantifier".into());
                            }
                            self.pos += 1;
                            Some(n)
                        }
                    }
                    _ => {
                        self.pos = save;
                        return Ok(node);
                    }
                };
                // CEP:WHAT: Rejects inverted quantifier bounds at compile time.
                // CEP:WHY: a{3,2} can never match; compiling it would silently produce a dead pattern instead of surfacing the configuration error.
                // CEP:STATUS: complete
                // CEP:FAILURE: Returns an error when min exceeds max.
                // CEP:ASSUMES: none
                // CEP:COST: O(1).
                // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
                if let Some(m) = max {
                    if min > m {
                        return Err("quantifier min exceeds max".to_string());
                    }
                }
                Ok(Node::Repeat {
                    node: Box::new(node),
                    min,
                    max,
                })
            }
            _ => Ok(node),
        }
    }

    // CEP:WHAT: Parses a decimal number inside a quantifier.
    // CEP:WHY: Bounded repetition bounds need decimal parsing.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns Err when no digits follow.
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(digits).
    // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
    fn number(&mut self) -> Result<u32, String> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        s.parse().map_err(|_| "bad quantifier number".to_string())
    }

    // CEP:ID: CS-p79
    // CEP:WHAT: Parses one atom: escape, class, group, anchor, or literal char.
    // CEP:WHY: Atoms are the leaves of the pattern AST.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns Err on unknown escapes or unterminated groups and classes.
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(size of atom).
    // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
    fn atom(&mut self) -> Result<Node, String> {
        let c = self.peek().ok_or("unexpected end of pattern")?;
        match c {
            '(' => {
                self.pos += 1;
                let capturing = if self.peek() == Some('?') {
                    self.pos += 1;
                    if self.peek() != Some(':') {
                        return Err("only non-capturing (?: is supported".into());
                    }
                    self.pos += 1;
                    false
                } else {
                    true
                };
                let idx = if capturing {
                    self.ngroups += 1;
                    Some(self.ngroups)
                } else {
                    None
                };
                let inner = self.alt()?;
                if self.peek() != Some(')') {
                    return Err("unterminated group".into());
                }
                self.pos += 1;
                match idx {
                    Some(i) => Ok(Node::Group(i, Box::new(inner))),
                    None => Ok(inner),
                }
            }
            '[' => self.class(),
            '.' => {
                self.pos += 1;
                Ok(Node::Any)
            }
            '^' => {
                self.pos += 1;
                Ok(Node::Start)
            }
            '$' => {
                self.pos += 1;
                Ok(Node::End)
            }
            '\\' => {
                self.pos += 1;
                let e = self.peek().ok_or("dangling escape")?;
                self.pos += 1;
                Ok(match e {
                    'd' => Node::Class {
                        neg: false,
                        items: vec![ClassItem::Digit],
                    },
                    'D' => Node::Class {
                        neg: true,
                        items: vec![ClassItem::Digit],
                    },
                    'w' => Node::Class {
                        neg: false,
                        items: vec![ClassItem::Word],
                    },
                    'W' => Node::Class {
                        neg: true,
                        items: vec![ClassItem::Word],
                    },
                    's' => Node::Class {
                        neg: false,
                        items: vec![ClassItem::Space],
                    },
                    'S' => Node::Class {
                        neg: true,
                        items: vec![ClassItem::Space],
                    },
                    'b' => Node::WordB,
                    'n' => Node::Char('\n'),
                    't' => Node::Char('\t'),
                    'r' => Node::Char('\r'),
                    other => Node::Char(other),
                })
            }
            ')' => Err("unmatched ')'".into()),
            other => {
                self.pos += 1;
                Ok(Node::Char(other))
            }
        }
    }

    // CEP:ID: CS-q51
    // CEP:WHAT: Parses a character class with ranges, negation, and shorthand escapes.
    // CEP:WHY: Classes cover sets like [A-Z0-9-] used by rule patterns.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns Err on unterminated classes.
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(size of class).
    // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
    fn class(&mut self) -> Result<Node, String> {
        self.pos += 1; // consume '['
        let neg = if self.peek() == Some('^') {
            self.pos += 1;
            true
        } else {
            false
        };
        let mut items = Vec::new();
        let mut first = true;
        loop {
            let c = self.peek().ok_or("unterminated class")?;
            if c == ']' && !first {
                self.pos += 1;
                break;
            }
            first = false;
            let lo = if c == '\\' {
                self.pos += 1;
                let e = self.peek().ok_or("dangling escape in class")?;
                self.pos += 1;
                match e {
                    'd' => {
                        items.push(ClassItem::Digit);
                        continue;
                    }
                    'D' => {
                        items.push(ClassItem::NonDigit);
                        continue;
                    }
                    'w' => {
                        items.push(ClassItem::Word);
                        continue;
                    }
                    'W' => {
                        items.push(ClassItem::NonWord);
                        continue;
                    }
                    's' => {
                        items.push(ClassItem::Space);
                        continue;
                    }
                    'S' => {
                        items.push(ClassItem::NonSpace);
                        continue;
                    }
                    'n' => '\n',
                    't' => '\t',
                    other => other,
                }
            } else {
                self.pos += 1;
                c
            };
            if self.peek() == Some('-')
                && self.chars.get(self.pos + 1).copied() != Some(']')
                && self.pos + 1 < self.chars.len()
            {
                self.pos += 1;
                let hi_c = self.peek().ok_or("unterminated range")?;
                self.pos += 1;
                // CEP:WHAT: Decodes escapes in the range high endpoint like the low one.
                // CEP:WHY: Using the raw escape letter made [\t-\r] a range to 'r' and matched unrelated characters; both endpoints must share one decoder, and shorthand escapes have no place inside a range.
                // CEP:STATUS: complete
                // CEP:FAILURE: Returns an error for shorthand escapes as range endpoints.
                // CEP:ASSUMES: none
                // CEP:COST: O(1).
                // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
                let hi = if hi_c == '\\' {
                    let e = self.peek().ok_or("dangling escape in range")?;
                    self.pos += 1;
                    match e {
                        'd' | 'D' | 'w' | 'W' | 's' | 'S' => {
                            return Err("shorthand escape cannot bound a class range".to_string())
                        }
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        other => other,
                    }
                } else {
                    hi_c
                };
                if hi < lo {
                    return Err("class range endpoints are reversed".to_string());
                }
                items.push(ClassItem::Range(lo, hi));
            } else {
                items.push(ClassItem::Ch(lo));
            }
        }
        Ok(Node::Class { neg, items })
    }

    // CEP:WHAT: Peeks the next pattern char.
    // CEP:WHY: Every production needs one-char lookahead.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; O(1).
    // CEP:EVIDENCE: tools/cep_lint/tests/re_test.rs
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
}

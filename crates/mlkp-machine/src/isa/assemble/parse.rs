// CEP:FILE: crates/mlkp-machine/src/isa/assemble/parse.rs
// CEP:WHAT: The AT&T text parser: integer literals, comments, mnemonic and width-suffix resolution including the move grid, and full operand forms.
// CEP:WHY: The AT&T assembler closes the tier pair; the parser, the canonical form selection, and the fixpoint driver split per chapter 62.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns IsaError::Parse for malformed text and shapes the tier does not encode; nothing truncates silently.
// CEP:ASSUMES: one instruction per line; the canonical forms mirror GNU as byte for byte (gas differential).
// CEP:COST: cold; O(text) per parse.
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs.

use super::tables::{self, Alu, Width};
use super::{IsaError, IsaMn, IsaOp};

/// Parses an integer literal: decimal, negative decimal, or 0x hexadecimal.
// CEP:WHAT: Parses an integer literal.
// CEP:WHY: Immediates, displacements, and absolute branch targets.
// CEP:STATUS: complete
// CEP:FAILURE: None for non-numbers.
// CEP:ASSUMES: none
// CEP:COST: O(digits).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn parse_int(s: &str) -> Option<i64> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix("0x") {
        return i64::from_str_radix(hex, 16).ok();
    }
    if let Some(hex) = t.strip_prefix("-0x") {
        return i64::from_str_radix(hex, 16).ok().map(|v| -v);
    }
    t.parse::<i64>().ok()
}

/// Strips a comment from one source line.
// CEP:WHAT: Strips comments.
// CEP:WHY: # and // both introduce comments in AT&T text.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(line).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn strip_comment(line: &str) -> &str {
    let hash = line.find('#').unwrap_or(line.len());
    let slash = line.find("//").unwrap_or(line.len());
    &line[..hash.min(slash)]
}

/// Resolves a mnemonic token to (mnemonic, width).
// CEP:ID: JIT-q36
// CEP:WHAT: Resolves a mnemonic.
// CEP:WHY: AT&T suffixes and the two-suffix move grid encode the widths.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::Parse for mnemonics outside the tier.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn parse_mnemonic(tok: &str, line: usize) -> Result<(IsaMn, Width), IsaError> {
    let perr = |r: String| IsaError::Parse(line, r);
    if let Some(cond) = tok.strip_prefix('j') {
        if let Some(num) = tables::cond_number(cond) {
            return Ok((IsaMn::Jcc(num), Width::Q));
        }
    }
    if let Some(rest) = tok.strip_prefix("movz") {
        if rest.len() == 2 {
            let src = src_width(rest.as_bytes()[0], line)?;
            let dst = dst_width(rest.as_bytes()[1], line)?;
            return Ok((IsaMn::MovZx(src), dst));
        }
    }
    if let Some(rest) = tok.strip_prefix("movsl") {
        if rest == "q" {
            return Ok((IsaMn::MovSxd, Width::Q));
        }
    }
    if let Some(rest) = tok.strip_prefix("movs") {
        if rest.len() == 2 {
            let src = src_width(rest.as_bytes()[0], line)?;
            let dst = dst_width(rest.as_bytes()[1], line)?;
            return Ok((IsaMn::MovSx(src), dst));
        }
    }
    if tok == "movabsq" {
        return Ok((IsaMn::Mov, Width::Q));
    }
    if tok == "cqto" {
        return Ok((IsaMn::Cqo, Width::Q));
    }
    if tok == "cdq" {
        return Ok((IsaMn::Cqo, Width::L));
    }
    // Names that never take a width suffix.
    match tok {
        "call" => return Ok((IsaMn::Call, Width::Q)),
        "jmp" => return Ok((IsaMn::Jmp, Width::Q)),
        "ret" => return Ok((IsaMn::Ret, Width::Q)),
        "nop" => return Ok((IsaMn::Nop, Width::Q)),
        "movsd" => return Ok((IsaMn::MovSd, Width::Q)),
        "movapd" => return Ok((IsaMn::MovApd, Width::Q)),
        "addsd" => return Ok((IsaMn::AddSd, Width::Q)),
        "subsd" => return Ok((IsaMn::SubSd, Width::Q)),
        "mulsd" => return Ok((IsaMn::MulSd, Width::Q)),
        "divsd" => return Ok((IsaMn::DivSd, Width::Q)),
        "sqrtsd" => return Ok((IsaMn::SqrtSd, Width::Q)),
        "ucomisd" => return Ok((IsaMn::Ucomisd, Width::Q)),
        "comisd" => return Ok((IsaMn::Comisd, Width::Q)),
        _ => {}
    }
    // Strip one width suffix from the remaining names.
    let bytes = tok.as_bytes();
    let (base, width) = if bytes.len() >= 2 {
        match bytes[bytes.len() - 1] {
            b'b' => (&tok[..tok.len() - 1], Width::B),
            b'w' => (&tok[..tok.len() - 1], Width::W),
            b'l' => (&tok[..tok.len() - 1], Width::L),
            b'q' => (&tok[..tok.len() - 1], Width::Q),
            _ => (tok, Width::Q),
        }
    } else {
        (tok, Width::Q)
    };
    let mn = match base {
        "mov" => IsaMn::Mov,
        "lea" => IsaMn::Lea,
        "add" => IsaMn::Alu(Alu::Add),
        "or" => IsaMn::Alu(Alu::Or),
        "and" => IsaMn::Alu(Alu::And),
        "sub" => IsaMn::Alu(Alu::Sub),
        "xor" => IsaMn::Alu(Alu::Xor),
        "cmp" => IsaMn::Alu(Alu::Cmp),
        "test" => IsaMn::Test,
        "not" => IsaMn::Not,
        "neg" => IsaMn::Neg,
        "mul" => IsaMn::Mul,
        "imul" => IsaMn::Imul,
        "div" => IsaMn::Div,
        "idiv" => IsaMn::Idiv,
        "shl" | "sal" => IsaMn::Shl,
        "shr" => IsaMn::Shr,
        "sar" => IsaMn::Sar,
        "push" => IsaMn::Push,
        "pop" => IsaMn::Pop,
        "inc" => IsaMn::Inc,
        "dec" => IsaMn::Dec,
        "cvtsi2sd" => IsaMn::CvtSi2Sd,
        "cvttsd2si" => IsaMn::Cvttsd2Si,
        _ => return Err(perr(format!("unknown mnemonic '{tok}'"))),
    };
    Ok((mn, width))
}

/// Parses a move-grid source width character.
// CEP:WHAT: Parses a grid source width.
// CEP:WHY: movzbq and friends name both widths.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::Parse outside b and w.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn src_width(c: u8, line: usize) -> Result<Width, IsaError> {
    match c {
        b'b' => Ok(Width::B),
        b'w' => Ok(Width::W),
        _ => Err(IsaError::Parse(
            line,
            format!("bad move-grid source width {}", c as char),
        )),
    }
}

/// Parses a move-grid destination width character.
// CEP:WHAT: Parses a grid destination width.
// CEP:WHY: movzbq and friends name both widths.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::Parse outside w, l, and q.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn dst_width(c: u8, line: usize) -> Result<Width, IsaError> {
    match c {
        b'w' => Ok(Width::W),
        b'l' => Ok(Width::L),
        b'q' => Ok(Width::Q),
        _ => Err(IsaError::Parse(
            line,
            format!("bad move-grid destination width {}", c as char),
        )),
    }
}

/// Parses one operand token into an IsaOp and an optional branch symbol.
// CEP:ID: JIT-x66
// CEP:WHAT: Parses one operand.
// CEP:WHY: Immediates, registers, full memory forms, and bare symbols or absolute offsets.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::Parse with the line number.
// CEP:ASSUMES: the token is trimmed and split on top-level commas.
// CEP:COST: O(token).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn parse_operand(tok: &str, line: usize) -> Result<(IsaOp, Option<String>), IsaError> {
    let t = tok.trim();
    if t.starts_with('*') {
        return Err(IsaError::Parse(
            line,
            "indirect call and jump are outside the tier".into(),
        ));
    }
    if let Some(imm) = t.strip_prefix('$') {
        let v = parse_int(imm)
            .ok_or_else(|| IsaError::Parse(line, format!("bad immediate '{imm}'")))?;
        return Ok((IsaOp::Imm(v), None));
    }
    if let Some(name) = t.strip_prefix('%') {
        if let Some(idx) = name.strip_prefix("xmm") {
            let n: u32 = idx
                .parse()
                .map_err(|_| IsaError::Parse(line, format!("bad xmm register '%{name}'")))?;
            if n > 15 {
                return Err(IsaError::Parse(line, format!("xmm{n} outside 0..=15")));
            }
            return Ok((IsaOp::Xmm(n), None));
        }
        let (hw, _w) = tables::reg_hw(t)
            .ok_or_else(|| IsaError::Parse(line, format!("unknown register '{t}'")))?;
        return Ok((IsaOp::Gpr(hw), None));
    }
    if let Some(open) = t.find('(') {
        if !t.ends_with(')') {
            return Err(IsaError::Parse(
                line,
                format!("unbalanced memory operand '{t}'"),
            ));
        }
        let head = t[..open].trim();
        let inner = &t[open + 1..t.len() - 1];
        if inner.trim() == "%rip" {
            let disp = if head.is_empty() {
                0
            } else {
                parse_int(head).ok_or_else(|| {
                    IsaError::Parse(line, format!("bad rip displacement '{head}'"))
                })?
            };
            let sym = Some(head.to_string()).filter(|h| parse_int(h).is_none());
            return Ok((
                IsaOp::Mem {
                    base: None,
                    index: None,
                    scale: 1,
                    disp,
                    rip: true,
                },
                sym,
            ));
        }
        let mut parts = inner.split(',');
        let base_t = parts.next().unwrap_or("").trim();
        let idx_t = parts.next().map(str::trim).unwrap_or("");
        let scale_t = parts.next().map(str::trim).unwrap_or("");
        if parts.next().is_some() {
            return Err(IsaError::Parse(
                line,
                format!("too many memory fields in '{t}'"),
            ));
        }
        let base = if base_t.is_empty() {
            None
        } else {
            Some(
                tables::reg_hw(base_t)
                    .map(|(hw, _)| hw)
                    .ok_or_else(|| IsaError::Parse(line, format!("unknown base '{base_t}'")))?,
            )
        };
        let index = if idx_t.is_empty() {
            None
        } else {
            Some(
                tables::reg_hw(idx_t)
                    .map(|(hw, _)| hw)
                    .ok_or_else(|| IsaError::Parse(line, format!("unknown index '{idx_t}'")))?,
            )
        };
        let scale = if scale_t.is_empty() {
            1u8
        } else {
            let v: u8 = scale_t
                .parse()
                .map_err(|_| IsaError::Parse(line, format!("bad scale '{scale_t}'")))?;
            if v != 1 && v != 2 && v != 4 && v != 8 {
                return Err(IsaError::Parse(line, format!("scale {v} outside 1/2/4/8")));
            }
            v
        };
        let disp = if head.is_empty() {
            0
        } else {
            parse_int(head).ok_or_else(|| {
                IsaError::Parse(
                    line,
                    format!("symbolic absolute addressing '{head}' is outside the tier; use {head}(%rip)"),
                )
            })?
        };
        return Ok((
            IsaOp::Mem {
                base,
                index,
                scale,
                disp,
                rip: false,
            },
            None,
        ));
    }
    if let Some(v) = parse_int(t) {
        return Ok((IsaOp::Sym(v.to_string()), Some(v.to_string())));
    }
    Ok((IsaOp::Sym(t.to_string()), Some(t.to_string())))
}

/// Splits an operand list on top-level commas.
// CEP:ID: JIT-170
// CEP:WHAT: Splits the operand list.
// CEP:WHY: Memory operands contain commas of their own.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(text).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn split_operands(rest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut cur = String::new();
    for c in rest.chars() {
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                cur.push(c);
            }
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur = String::new();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

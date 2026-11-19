// CEP:FILE: crates/mlkp-machine/src/decode/reader.rs
// CEP:WHAT: Byte-level decoding primitives: the byte reader, prefix parsing, ModRM decoding, hardware-number mapping, and memory-tail reading.
// CEP:WHY: The decoder is the mechanical bytes-versus-text equivalence check of ticket CEP-322; one shared structural grammar keeps decoding identical across the CodeObject path and the general path.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module DecodeError variants; encodings outside the ISA tier are rejected loudly, never guessed at.
// CEP:ASSUMES: the bytes came from encode::compile for the CodeObject path; prefixes arrive in canonical order.
// CEP:COST: cold; O(insns).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs.

use super::tables::d;
use super::DecodeError;
use crate::target::preg;

/// A byte reader over the text section.
pub(crate) struct Reader<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) pos: usize,
}

impl<'a> Reader<'a> {
    /// Peeks one byte without consuming.
    // CEP:WHAT: Peeks one byte.
    // CEP:WHY: Prefix detection needs lookahead.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None at end of input.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
    pub(crate) fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    /// Consumes one byte.
    // CEP:WHAT: Consumes one byte.
    // CEP:WHY: Sequential decode.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics only when the caller skipped the bounds peek, which decode_one never does.
    // CEP:ASSUMES: a peek succeeded.
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
    pub(crate) fn take(&mut self) -> u8 {
        let b = self.bytes[self.pos];
        self.pos += 1;
        b
    }

    /// Consumes n bytes as a sign-extended little-endian integer.
    // CEP:WHAT: Reads a signed immediate or displacement.
    // CEP:WHY: imm8, imm32, imm64, disp8, disp32 tails.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics only when decode_insn skipped its bounds check.
    // CEP:ASSUMES: n is 1, 4, or 8 and the bytes exist.
    // CEP:COST: O(n).
    // CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
    pub(crate) fn take_int(&mut self, n: usize) -> i64 {
        let mut v: i64 = 0;
        for k in 0..n {
            v |= i64::from(self.take()) << (d::BYTE_BITS * k as i64);
        }
        let shift = match n {
            d::IMM8 => d::BYTE_SIGN_SHIFT,
            d::IMM32 => d::DWORD_SIGN_SHIFT,
            _ => 0, // imm64 needs no sign extension.
        };
        v << shift >> shift
    }
}

/// Decoded prefix state.
#[derive(Default)]
pub(crate) struct Prefixes {
    pub(crate) rex_w: bool,
    pub(crate) rex_r: bool,
    pub(crate) rex_b: bool,
    pub(crate) f2: bool,
    pub(crate) p66: bool,
}

/// Parses prefix bytes up to the primary opcode.
// CEP:ID: JIT-699
// CEP:WHAT: Parses prefixes.
// CEP:WHY: REX, F2, and 66 modify the following opcode.
// CEP:STATUS: complete
// CEP:FAILURE: Returns UnknownEncoding for a prefix the subset never emits.
// CEP:ASSUMES: the encoder emits prefixes in canonical order (segment-style first, REX last).
// CEP:COST: O(prefixes).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn parse_prefixes(r: &mut Reader, start: u32) -> Result<Prefixes, DecodeError> {
    let mut p = Prefixes::default();
    let mut rex_seen = false;
    loop {
        match r.peek() {
            Some(d::PRE_F2) if !rex_seen && !p.p66 && !p.f2 => {
                p.f2 = true;
                r.take();
            }
            Some(d::PRE_66) if !rex_seen && !p.f2 && !p.p66 => {
                p.p66 = true;
                r.take();
            }
            Some(b) if (d::REX_FIRST..=d::REX_LAST).contains(&b) && !rex_seen => {
                // Canonical order: F2/66 first, REX last; the subset's primary
                // opcodes never start in the REX range, so this is unambiguous.
                p.rex_w = b & d::REX_W != 0;
                p.rex_r = b & d::REX_R != 0;
                p.rex_b = b & d::REX_B != 0;
                rex_seen = true;
                r.take();
            }
            Some(_) => break,
            None => return Err(DecodeError::Truncated(start)),
        }
    }
    Ok(p)
}

/// Decodes one ModRM byte into (mod, reg, rm) as raw hardware numbers with REX bits applied.
///
/// Callers on integer paths map the numbers through `from_hw`; SSE callers use
/// them directly as xmm indexes because xmm encodings have no SP/BP swap.
// CEP:WHAT: Decodes ModRM.
// CEP:WHY: Every register or memory operand is described by it.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn decode_modrm(byte: u8, p: &Prefixes) -> (u8, u32, u32) {
    let b = u32::from(byte);
    let m = (b >> d::MOD_SHIFT) as u8;
    let reg = ((b >> d::REG_SHIFT) & d::REG_MASK) + u32::from(p.rex_r) * d::GPR_EXT;
    let rm = (b & d::REG_MASK) + u32::from(p.rex_b) * d::GPR_EXT;
    (m, reg, rm)
}

/// Maps a hardware register number back to its target-table id (the inverse of encode::hw).
// CEP:WHAT: Maps hardware numbers to table ids.
// CEP:WHY: Decoded registers must name the same table entries the encoder consumed.
// CEP:STATUS: complete
// CEP:FAILURE: Panics for numbers above 15; the subset never encodes more.
// CEP:ASSUMES: hardware numbers 0..=15.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn from_hw(h: u32) -> u32 {
    const LOW_GPR_MAX: u32 = 3;
    const HIGH_GPR_MIN: u32 = 8;
    const HIGH_GPR_MAX: u32 = 15;
    match h {
        0..=LOW_GPR_MAX => h,
        d::HW_SIB => preg::RSP,
        d::HW_RIP => preg::RBP,
        d::HW_RSI => preg::RSI,
        d::HW_RDI => preg::RDI,
        HIGH_GPR_MIN..=HIGH_GPR_MAX => h,
        _ => panic!("from_hw(): hardware number {h} outside the GPR range"),
    }
}

/// Reads the memory operand tail (optional SIB plus displacement) after a ModRM with mod != 3.
// CEP:WHAT: Reads a memory operand.
// CEP:WHY: Load and Store addressing.
// CEP:STATUS: complete
// CEP:FAILURE: Returns UnknownEncoding for SIB shapes the subset never emits (index registers, RIP-relative).
// CEP:ASSUMES: the encoder emits base-plus-displacement addressing only.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs
pub(crate) fn read_mem(
    r: &mut Reader,
    m: u8,
    rm: u32,
    p: &Prefixes,
) -> Result<(u32, i64), DecodeError> {
    let mut base = rm;
    if base & d::REG_MASK == d::HW_SIB {
        let sib = r.take();
        let sb = u32::from(sib);
        let sib_base = (sb & d::REG_MASK) + u32::from(p.rex_b) * d::GPR_EXT;
        let sib_index = (sb >> d::REG_SHIFT) & d::REG_MASK;
        if sib_index != d::SIB_NO_INDEX {
            return Err(DecodeError::UnknownEncoding(r.pos as u32));
        }
        base = sib_base;
    }
    if base & d::REG_MASK == d::HW_RIP && m == 0 {
        // mod 00 rm 101 means RIP-relative; the subset never emits it.
        return Err(DecodeError::UnknownEncoding(r.pos as u32));
    }
    let disp = match m {
        0 => 0,
        1 => r.take_int(d::IMM8),
        d::MOD_DISP32_B => r.take_int(d::IMM32),
        _ => return Err(DecodeError::UnknownEncoding(r.pos as u32)),
    };
    Ok((from_hw(base), disp))
}

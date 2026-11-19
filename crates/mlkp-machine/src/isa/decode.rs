// CEP:FILE: crates/mlkp-machine/src/isa/decode.rs
// CEP:WHAT: The general byte-level decoder for the scalar ISA tier: prefix parsing (66, F2, F3, REX with all bits), full ModRM and SIB decoding including RIP-relative and no-base forms, and the complete opcode dispatch for the tier's families in every width.
// CEP:WHY: Ticket CEP-325 completes the machine tier: arbitrary machine code of the tier decodes exactly once per instruction and encodings outside the tier are rejected loudly rather than guessed at.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns IsaError::Truncated for a stream that ends mid-instruction and IsaError::OutsideTier for every encoding the tier does not define.
// CEP:ASSUMES: legacy prefixes may repeat in any order; REX appears at most once, after the legacy prefixes; the tier's primary opcodes never fall in the REX range.
// CEP:COST: cold; O(bytes).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs.

#![deny(missing_docs)]

use super::escape::decode_escape;
use super::tables::{Alu, Grp3, Width};
use super::{IsaError, IsaInsn, IsaMn, IsaOp};

/// Legacy 66 prefix.
pub(crate) const PRE_66: u8 = 0x66;
/// Legacy F2 prefix.
pub(crate) const PRE_F2: u8 = 0xF2;
/// Legacy F3 prefix.
pub(crate) const PRE_F3: u8 = 0xF3;
/// Two-byte escape.
pub(crate) const ESC: u8 = 0x0F;
/// REX prefix base.
pub(crate) const REX_BASE: u8 = 0x40;
/// REX.W bit.
pub(crate) const REX_W: u8 = 0x08;
/// REX.R bit.
pub(crate) const REX_R: u8 = 0x04;
/// REX.X bit.
pub(crate) const REX_X: u8 = 0x02;
/// REX.B bit.
pub(crate) const REX_B: u8 = 0x01;
/// ModRM.mod shift.
pub(crate) const MOD_SHIFT: u32 = 6;
/// ModRM.reg shift.
pub(crate) const REG_SHIFT: u32 = 3;
/// ModRM low-bit mask.
pub(crate) const LOW3: u32 = 7;
/// Hardware number where R8 begins.
pub(crate) const GPR_EXT: u32 = 8;
/// rm 101 in mod 0: RIP-relative.
pub(crate) const RM_RIP: u32 = 5;
/// rm 100: a SIB byte follows.
pub(crate) const RM_SIB: u32 = 4;
/// SIB base 101 in mod 0: no base register, disp32 absolute.
pub(crate) const SIB_NO_BASE: u32 = 5;
/// SIB index 100: no index register.
pub(crate) const SIB_NO_INDEX: u32 = 4;
/// Mod 3: register-direct.
pub(crate) const MOD_REG: u8 = 3;
/// Highest hardware GPR number.
pub(crate) const GPR_MAX: u32 = 15;

/// Prefix state decoded from the legacy bytes.
#[derive(Clone, Copy, Default)]
pub(crate) struct Prefixes {
    /// 66 operand-size override.
    pub(crate) p66: bool,
    /// F2 repnz prefix.
    pub(crate) f2: bool,
    /// F3 rep prefix.
    pub(crate) f3: bool,
    /// REX.W.
    pub(crate) rex_w: bool,
    /// REX.R.
    pub(crate) rex_r: bool,
    /// REX.X.
    pub(crate) rex_x: bool,
    /// REX.B.
    pub(crate) rex_b: bool,
    /// Whether any REX byte is present.
    pub(crate) rex_present: bool,
    /// The raw REX byte.
    pub(crate) rex_raw: u8,
}

/// Reads one byte, failing past the end.
// CEP:WHAT: Reads one byte.
// CEP:WHY: Sequential decode with truncation detection.
// CEP:STATUS: complete
// CEP:FAILURE: Truncated at end of input.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn take(b: &[u8], pos: &mut usize, start: u32) -> Result<u8, IsaError> {
    let v = *b.get(*pos).ok_or(IsaError::Truncated(start))?;
    *pos += 1;
    Ok(v)
}

/// Reads a signed little-endian integer of n bytes.
// CEP:WHAT: Reads a signed immediate.
// CEP:WHY: imm8, disp8, imm16, disp32, imm32, and imm64 tails.
// CEP:STATUS: complete
// CEP:FAILURE: Truncated when fewer than n bytes remain.
// CEP:ASSUMES: n in 1..=8.
// CEP:COST: O(n).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn take_int(b: &[u8], pos: &mut usize, n: usize, start: u32) -> Result<i64, IsaError> {
    let mut v: i64 = 0;
    for k in 0..n {
        v |= i64::from(take(b, pos, start)?) << (8 * k);
    }
    let shift = match n {
        1 => 56,
        2 => 48,
        4 => 32,
        _ => 0,
    };
    Ok(v << shift >> shift)
}

/// Parses legacy prefixes and one REX byte.
// CEP:ID: JIT-s47
// CEP:WHAT: Parses prefixes.
// CEP:WHY: 66, F2, F3, and REX select widths and register extensions.
// CEP:STATUS: complete
// CEP:FAILURE: OutsideTier for address-size, segment, or lock prefixes and for a second REX; Truncated at end of input.
// CEP:ASSUMES: none
// CEP:COST: O(prefixes).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn parse_prefixes(b: &[u8], pos: &mut usize, start: u32) -> Result<Prefixes, IsaError> {
    let mut p = Prefixes::default();
    loop {
        let byte = take(b, pos, start)?;
        match byte {
            PRE_66 if !p.rex_present => p.p66 = true,
            PRE_F2 if !p.rex_present => p.f2 = true,
            PRE_F3 if !p.rex_present => p.f3 = true,
            byte @ REX_BASE..=0x4F if !p.rex_present => {
                p.rex_present = true;
                p.rex_raw = byte;
                p.rex_w = byte & REX_W != 0;
                p.rex_r = byte & REX_R != 0;
                p.rex_x = byte & REX_X != 0;
                p.rex_b = byte & REX_B != 0;
            }
            0x67 | 0xF0 | 0x2E | 0x3E | 0x26 | 0x36 | 0x64 | 0x65 => {
                return Err(IsaError::OutsideTier(start));
            }
            _ => {
                *pos -= 1;
                return Ok(p);
            }
        }
    }
}

/// Decodes one ModRM byte into (mod, reg, rm) with REX.R and REX.B applied.
// CEP:WHAT: Decodes ModRM.
// CEP:WHY: Every register or memory operand is described by it.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn modrm(byte: u8, p: &Prefixes) -> (u8, u32, u32) {
    let v = u32::from(byte);
    let m = (v >> MOD_SHIFT) as u8;
    let reg = ((v >> REG_SHIFT) & LOW3) + u32::from(p.rex_r) * GPR_EXT;
    let rm = (v & LOW3) + u32::from(p.rex_b) * GPR_EXT;
    (m, reg, rm)
}

/// Decodes the memory operand tail (SIB plus displacement) in hardware numbering.
// CEP:ID: JIT-223
// CEP:WHAT: Reads a memory operand.
// CEP:WHY: Every addressing form of the tier: base+disp, base+index+scale, RIP-relative, and absolute disp32.
// CEP:STATUS: complete
// CEP:FAILURE: Truncated on a missing SIB or displacement; OutsideTier for mod 3, which never reaches this path.
// CEP:ASSUMES: mod 3 operands are handled by the caller as registers.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn read_mem(
    b: &[u8],
    pos: &mut usize,
    m: u8,
    rm: u32,
    p: &Prefixes,
    start: u32,
) -> Result<IsaOp, IsaError> {
    let mut base = Some(rm);
    let mut index = None;
    let mut scale = 1u8;
    let mut rip = false;
    if rm & LOW3 == RM_SIB {
        let sib = take(b, pos, start)?;
        let sv = u32::from(sib);
        let sib_base = (sv & LOW3) + u32::from(p.rex_b) * GPR_EXT;
        let sib_index = ((sv >> REG_SHIFT) & LOW3) + u32::from(p.rex_x) * GPR_EXT;
        if sib_index & LOW3 != SIB_NO_INDEX {
            index = Some(sib_index);
            scale = 1u8 << (sv >> 6);
        }
        if m == 0 && sib_base & LOW3 == SIB_NO_BASE {
            base = None;
        } else {
            base = Some(sib_base);
        }
    } else if m == 0 && rm & LOW3 == RM_RIP {
        base = None;
        rip = true;
    }
    let disp = match m {
        0 if base.is_none() => take_int(b, pos, 4, start)?,
        0 => 0,
        1 => take_int(b, pos, 1, start)?,
        2 => take_int(b, pos, 4, start)?,
        _ => return Err(IsaError::OutsideTier(start)),
    };
    let base = base.filter(|_| !rip);
    Ok(IsaOp::Mem {
        base,
        index,
        scale,
        disp,
        rip,
    })
}

/// The register operand for a hardware number.
// CEP:WHAT: Builds a register operand.
// CEP:WHY: The general decoder speaks hardware numbers directly.
// CEP:STATUS: complete
// CEP:FAILURE: OutsideTier above R15.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn gpr(hw: u32) -> Result<IsaOp, IsaError> {
    if hw > GPR_MAX {
        return Err(IsaError::OutsideTier(u32::MAX));
    }
    Ok(IsaOp::Gpr(hw))
}

/// Decodes one instruction from the byte slice at pos, advancing pos.
// CEP:ID: JIT-h57
// CEP:WHAT: Decodes one instruction.
// CEP:WHY: The general decode walk over arbitrary tier machine code.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module errors; every unhandled opcode is OutsideTier.
// CEP:ASSUMES: none
// CEP:COST: O(1) per instruction.
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn decode_one(b: &[u8], pos: &mut usize, start: u32) -> Result<IsaInsn, IsaError> {
    let p = parse_prefixes(b, pos, start)?;
    let opc = take(b, pos, start)?;
    let w: Width = if p.rex_w {
        Width::Q
    } else if p.p66 {
        Width::W
    } else {
        Width::L
    };
    match opc {
        0x90 if !p.f2 && !p.f3 && !p.p66 && (!p.rex_present || p.rex_raw == REX_BASE) => {
            Ok(IsaInsn {
                mn: IsaMn::Nop,
                width: Width::Q,
                ops: vec![],
                target: None,
            })
        }
        0x99 if !p.f2 && !p.f3 && !p.p66 && p.rex_w => Ok(IsaInsn {
            mn: IsaMn::Cqo,
            width: Width::Q,
            ops: vec![],
            target: None,
        }),
        0x99 if !p.f2 && !p.f3 && !p.p66 && !p.rex_present => Ok(IsaInsn {
            mn: IsaMn::Cqo,
            width: Width::L,
            ops: vec![],
            target: None,
        }),
        0x50..=0x57 if !p.f2 && !p.f3 && !p.p66 && !p.rex_w => {
            let hw = u32::from(opc - 0x50) + u32::from(p.rex_b) * GPR_EXT;
            Ok(IsaInsn {
                mn: IsaMn::Push,
                width: Width::Q,
                ops: vec![gpr(hw)?],
                target: None,
            })
        }
        0x58..=0x5F if !p.f2 && !p.f3 && !p.p66 && !p.rex_w => {
            let hw = u32::from(opc - 0x58) + u32::from(p.rex_b) * GPR_EXT;
            Ok(IsaInsn {
                mn: IsaMn::Pop,
                width: Width::Q,
                ops: vec![gpr(hw)?],
                target: None,
            })
        }
        0x68 if !p.f2 && !p.f3 && !p.p66 && !p.rex_w => {
            let imm = take_int(b, pos, 4, start)?;
            Ok(IsaInsn {
                mn: IsaMn::Push,
                width: Width::Q,
                ops: vec![IsaOp::Imm(imm)],
                target: None,
            })
        }
        0x6A if !p.f2 && !p.f3 && !p.p66 && !p.rex_w => {
            let imm = take_int(b, pos, 1, start)?;
            Ok(IsaInsn {
                mn: IsaMn::Push,
                width: Width::Q,
                ops: vec![IsaOp::Imm(imm)],
                target: None,
            })
        }
        0x70..=0x7F if !p.f2 && !p.f3 && !p.p66 && !p.rex_present => {
            let rel = take_int(b, pos, 1, start)?;
            let end = i64::from(*pos as u32);
            Ok(IsaInsn {
                mn: IsaMn::Jcc(opc - 0x70),
                width: Width::Q,
                ops: vec![],
                target: Some(end + rel),
            })
        }
        0x88..=0x8B if !p.f2 && !p.f3 => {
            let width = if opc == 0x88 || opc == 0x8A {
                Width::B
            } else {
                w
            };
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, &p);
            let reg_op = gpr(reg)?;
            if m == MOD_REG {
                let rm_op = gpr(rm)?;
                // 89 moves rm <- reg; 8A/8B move reg <- rm.
                let ops = if opc == 0x88 || opc == 0x89 {
                    vec![reg_op, rm_op]
                } else {
                    vec![rm_op, reg_op]
                };
                Ok(IsaInsn {
                    mn: IsaMn::Mov,
                    width,
                    ops,
                    target: None,
                })
            } else {
                let mem = read_mem(b, pos, m, rm, &p, start)?;
                let ops = if opc == 0x88 || opc == 0x89 {
                    vec![reg_op, mem]
                } else {
                    vec![mem, reg_op]
                };
                Ok(IsaInsn {
                    mn: IsaMn::Mov,
                    width,
                    ops,
                    target: None,
                })
            }
        }
        0x8D if !p.f2 && !p.f3 => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, &p);
            if m == MOD_REG {
                return Err(IsaError::OutsideTier(start));
            }
            let mem = read_mem(b, pos, m, rm, &p, start)?;
            Ok(IsaInsn {
                mn: IsaMn::Lea,
                width: w,
                ops: vec![mem, gpr(reg)?],
                target: None,
            })
        }
        0x8F if !p.f2 && !p.f3 && !p.rex_w => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, &p);
            if reg != 0 {
                return Err(IsaError::OutsideTier(start));
            }
            let op = if m == MOD_REG {
                gpr(rm)?
            } else {
                read_mem(b, pos, m, rm, &p, start)?
            };
            Ok(IsaInsn {
                mn: IsaMn::Pop,
                width: w,
                ops: vec![op],
                target: None,
            })
        }
        0xB0..=0xB7 if !p.f2 && !p.f3 => {
            let imm = take_int(b, pos, 1, start)?;
            let hw = u32::from(opc - 0xB0) + u32::from(p.rex_b) * GPR_EXT;
            Ok(IsaInsn {
                mn: IsaMn::Mov,
                width: Width::B,
                ops: vec![IsaOp::Imm(imm), gpr(hw)?],
                target: None,
            })
        }
        0xB8..=0xBF if !p.f2 && !p.f3 => {
            let hw = u32::from(opc - 0xB8) + u32::from(p.rex_b) * GPR_EXT;
            let (width, n) = if p.rex_w {
                (Width::Q, 8usize)
            } else if p.p66 {
                (Width::W, 2)
            } else {
                (Width::L, 4)
            };
            let imm = take_int(b, pos, n, start)?;
            Ok(IsaInsn {
                mn: IsaMn::Mov,
                width,
                ops: vec![IsaOp::Imm(imm), gpr(hw)?],
                target: None,
            })
        }
        0xA8 if !p.f2 && !p.f3 && !p.p66 && !p.rex_present => {
            let imm = take_int(b, pos, 1, start)?;
            Ok(IsaInsn {
                mn: IsaMn::Test,
                width: Width::B,
                ops: vec![IsaOp::Imm(imm), IsaOp::Gpr(0)],
                target: None,
            })
        }
        0xA9 if !p.f2 && !p.f3 => {
            let imm = take_int(b, pos, 4, start)?;
            Ok(IsaInsn {
                mn: IsaMn::Test,
                width: w,
                ops: vec![IsaOp::Imm(imm), IsaOp::Gpr(0)],
                target: None,
            })
        }
        _ => decode_integer(b, pos, opc, &p, w, start),
    }
}

/// Decodes the integer ALU, group, shift, multiply, branch, and SSE families.
// CEP:ID: JIT-388
// CEP:WHAT: Decodes the integer and SSE families.
// CEP:WHY: The main dispatch body; each family validates its own shape rules.
// CEP:STATUS: complete
// CEP:FAILURE: Returns OutsideTier for every opcode and shape outside the tier.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
fn decode_integer(
    b: &[u8],
    pos: &mut usize,
    opc: u8,
    p: &Prefixes,
    w: Width,
    start: u32,
) -> Result<IsaInsn, IsaError> {
    // The 00-3F ALU map: an 8-byte stride per operation; forms at +0..+5.
    // +0 byte MR, +1 wide MR, +2 byte RM, +3 wide RM, +4 byte AL-imm8, +5 wide AL-imm32.
    if opc < 0x40 && opc & 0x07 <= 5 && !p.f2 && !p.f3 && !p.f2 && !p.f3 {
        let base = opc & 0xF8;
        let form = opc & 0x07;
        let alu = Alu::from_base(base).ok_or(IsaError::OutsideTier(start))?;
        let width = match form {
            0 | 2 | 4 => Width::B,
            _ => w,
        };
        if form == 4 {
            let imm = take_int(b, pos, 1, start)?;
            return Ok(IsaInsn {
                mn: IsaMn::Alu(alu),
                width,
                ops: vec![IsaOp::Imm(imm), IsaOp::Gpr(0)],
                target: None,
            });
        }
        if form == 5 {
            let imm = take_int(b, pos, 4, start)?;
            return Ok(IsaInsn {
                mn: IsaMn::Alu(alu),
                width,
                ops: vec![IsaOp::Imm(imm), IsaOp::Gpr(0)],
                target: None,
            });
        }
        let modrm_b = take(b, pos, start)?;
        let (m, reg, rm) = modrm(modrm_b, p);
        let reg_op = gpr(reg)?;
        if m == MOD_REG {
            let rm_op = gpr(rm)?;
            // MR: op r/m, reg -> AT&T (reg, r/m); RM: op reg, r/m -> (r/m, reg).
            let ops = if form <= 1 {
                vec![reg_op, rm_op]
            } else {
                vec![rm_op, reg_op]
            };
            return Ok(IsaInsn {
                mn: IsaMn::Alu(alu),
                width,
                ops,
                target: None,
            });
        }
        let mem = read_mem(b, pos, m, rm, p, start)?;
        let ops = if form <= 1 {
            vec![reg_op, mem]
        } else {
            vec![mem, reg_op]
        };
        return Ok(IsaInsn {
            mn: IsaMn::Alu(alu),
            width,
            ops,
            target: None,
        });
    }
    match opc {
        0x80 | 0x81 | 0x83 if !p.f2 && !p.f3 => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let alu = Alu::from_ext_low(reg).ok_or(IsaError::OutsideTier(start))?;
            let width = if opc == 0x80 { Width::B } else { w };
            let imm = if opc == 0x81 {
                take_int(b, pos, 4, start)?
            } else {
                take_int(b, pos, 1, start)?
            };
            let dst = if m == MOD_REG {
                gpr(rm)?
            } else {
                read_mem(b, pos, m, rm, p, start)?
            };
            Ok(IsaInsn {
                mn: IsaMn::Alu(alu),
                width,
                ops: vec![IsaOp::Imm(imm), dst],
                target: None,
            })
        }
        0x84 | 0x85 if !p.f2 && !p.f3 => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let width = if opc == 0x84 { Width::B } else { w };
            let reg_op = gpr(reg)?;
            let other = if m == MOD_REG {
                gpr(rm)?
            } else {
                read_mem(b, pos, m, rm, p, start)?
            };
            Ok(IsaInsn {
                mn: IsaMn::Test,
                width,
                ops: vec![reg_op, other],
                target: None,
            })
        }
        0xC0 | 0xC1 | 0xD0 | 0xD1 | 0xD2 | 0xD3 if !p.f2 && !p.f3 => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let width = match opc {
                0xC0 | 0xD0 | 0xD2 => Width::B,
                _ => w,
            };
            let mn = match reg {
                4 => IsaMn::Shl,
                5 => IsaMn::Shr,
                7 => IsaMn::Sar,
                _ => return Err(IsaError::OutsideTier(start)),
            };
            let dst = if m == MOD_REG {
                gpr(rm)?
            } else {
                read_mem(b, pos, m, rm, p, start)?
            };
            let src = match opc {
                0xC0 | 0xC1 => IsaOp::Imm(take_int(b, pos, 1, start)?),
                0xD0 | 0xD1 => IsaOp::Imm(1),
                _ => IsaOp::Gpr(1),
            };
            Ok(IsaInsn {
                mn,
                width,
                ops: vec![src, dst],
                target: None,
            })
        }
        0xF6 | 0xF7 if !p.f2 && !p.f3 => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let width = if opc == 0xF6 { Width::B } else { w };
            let dst = if m == MOD_REG {
                gpr(rm)?
            } else {
                read_mem(b, pos, m, rm, p, start)?
            };
            match Grp3::from_ext(reg as u8).ok_or(IsaError::OutsideTier(start))? {
                Grp3::Test => {
                    let imm = if opc == 0xF6 {
                        take_int(b, pos, 1, start)?
                    } else {
                        take_int(b, pos, 4, start)?
                    };
                    Ok(IsaInsn {
                        mn: IsaMn::Test,
                        width,
                        ops: vec![IsaOp::Imm(imm), dst],
                        target: None,
                    })
                }
                Grp3::Not => Ok(IsaInsn {
                    mn: IsaMn::Not,
                    width,
                    ops: vec![dst],
                    target: None,
                }),
                Grp3::Neg => Ok(IsaInsn {
                    mn: IsaMn::Neg,
                    width,
                    ops: vec![dst],
                    target: None,
                }),
                Grp3::Mul => Ok(IsaInsn {
                    mn: IsaMn::Mul,
                    width,
                    ops: vec![dst],
                    target: None,
                }),
                Grp3::Imul => Ok(IsaInsn {
                    mn: IsaMn::Imul,
                    width,
                    ops: vec![dst],
                    target: None,
                }),
                Grp3::Div => Ok(IsaInsn {
                    mn: IsaMn::Div,
                    width,
                    ops: vec![dst],
                    target: None,
                }),
                Grp3::Idiv => Ok(IsaInsn {
                    mn: IsaMn::Idiv,
                    width,
                    ops: vec![dst],
                    target: None,
                }),
            }
        }
        0xC6 | 0xC7 if !p.f2 && !p.f3 => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            if reg != 0 {
                return Err(IsaError::OutsideTier(start));
            }
            let width = if opc == 0xC6 { Width::B } else { w };
            let dst = if m == MOD_REG {
                gpr(rm)?
            } else {
                read_mem(b, pos, m, rm, p, start)?
            };
            let imm = if opc == 0xC6 {
                take_int(b, pos, 1, start)?
            } else {
                take_int(b, pos, 4, start)?
            };
            Ok(IsaInsn {
                mn: IsaMn::Mov,
                width,
                ops: vec![IsaOp::Imm(imm), dst],
                target: None,
            })
        }
        0xFE | 0xFF if !p.f2 && !p.f3 => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let width = if opc == 0xFE { Width::B } else { w };
            let dst = if m == MOD_REG {
                gpr(rm)?
            } else {
                read_mem(b, pos, m, rm, p, start)?
            };
            match reg {
                0 => Ok(IsaInsn {
                    mn: IsaMn::Inc,
                    width,
                    ops: vec![dst],
                    target: None,
                }),
                1 => Ok(IsaInsn {
                    mn: IsaMn::Dec,
                    width,
                    ops: vec![dst],
                    target: None,
                }),
                6 if opc == 0xFF && !p.rex_w && !p.p66 => Ok(IsaInsn {
                    mn: IsaMn::Push,
                    width: Width::Q,
                    ops: vec![dst],
                    target: None,
                }),
                _ => Err(IsaError::OutsideTier(start)),
            }
        }
        0x69 | 0x6B if !p.f2 && !p.f3 && !p.p66 => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let width = if p.rex_w { Width::Q } else { w };
            let other = if m == MOD_REG {
                gpr(rm)?
            } else {
                read_mem(b, pos, m, rm, p, start)?
            };
            let imm = if opc == 0x69 {
                take_int(b, pos, 4, start)?
            } else {
                take_int(b, pos, 1, start)?
            };
            Ok(IsaInsn {
                mn: IsaMn::Imul3,
                width,
                ops: vec![IsaOp::Imm(imm), other, gpr(reg)?],
                target: None,
            })
        }
        0x63 if !p.f2 && !p.f3 && !p.p66 && p.rex_w => {
            let modrm_b = take(b, pos, start)?;
            let (m, reg, rm) = modrm(modrm_b, p);
            let src = if m == MOD_REG {
                gpr(rm)?
            } else {
                read_mem(b, pos, m, rm, p, start)?
            };
            Ok(IsaInsn {
                mn: IsaMn::MovSxd,
                width: Width::Q,
                ops: vec![src, gpr(reg)?],
                target: None,
            })
        }
        0xE8 if !p.f2 && !p.f3 && !p.p66 && !p.rex_present => {
            let rel = take_int(b, pos, 4, start)?;
            let end = i64::from(*pos as u32);
            Ok(IsaInsn {
                mn: IsaMn::Call,
                width: Width::Q,
                ops: vec![],
                target: Some(end + rel),
            })
        }
        0xE9 if !p.f2 && !p.f3 && !p.p66 && !p.rex_present => {
            let rel = take_int(b, pos, 4, start)?;
            let end = i64::from(*pos as u32);
            Ok(IsaInsn {
                mn: IsaMn::Jmp,
                width: Width::Q,
                ops: vec![],
                target: Some(end + rel),
            })
        }
        0xEB if !p.f2 && !p.f3 && !p.p66 && !p.rex_present => {
            let rel = take_int(b, pos, 1, start)?;
            let end = i64::from(*pos as u32);
            Ok(IsaInsn {
                mn: IsaMn::Jmp,
                width: Width::Q,
                ops: vec![],
                target: Some(end + rel),
            })
        }
        0xC3 if !p.f2 && !p.f3 && !p.p66 && !p.rex_present => Ok(IsaInsn {
            mn: IsaMn::Ret,
            width: Width::Q,
            ops: vec![],
            target: None,
        }),
        0x0F => decode_escape(b, pos, p, w, start),
        _ => Err(IsaError::OutsideTier(start)),
    }
}

/// Decodes a byte slice into tier instructions.
// CEP:WHAT: Decodes a byte program.
// CEP:WHY: The general disassembler's entry point.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the first decode error.
// CEP:ASSUMES: none
// CEP:COST: O(bytes).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub fn decode_bytes(b: &[u8]) -> Result<Vec<IsaInsn>, IsaError> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < b.len() {
        let start = pos as u32;
        out.push(decode_one(b, &mut pos, start)?);
    }
    Ok(out)
}

/// Silence the escape constant check; ESC names the two-byte map for readers.
// CEP:WHAT: Names the escape byte.
// CEP:WHY: Documentation value; the dispatch matches 0x0F literally.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
const _: () = assert!(ESC == 0x0F);

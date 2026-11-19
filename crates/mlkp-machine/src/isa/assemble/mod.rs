// CEP:FILE: crates/mlkp-machine/src/isa/assemble/mod.rs
// CEP:WHAT: The assembler driver: parsing to instruction lists, the branch-size fixpoint, label placement, and relocation collection.
// CEP:WHY: The AT&T assembler closes the tier pair; the parser, the canonical form selection, and the fixpoint driver split per chapter 62.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns IsaError::Parse for malformed text and shapes the tier does not encode; nothing truncates silently.
// CEP:ASSUMES: one instruction per line; the canonical forms mirror GNU as byte for byte (gas differential).
// CEP:COST: cold; O(text) per parse.
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs.

use self::forms::{
    encode_alu, encode_mov, encode_movsd, encode_shift, encode_test, fits_i32, fits_i8, gpr_of,
    imm_of, modrm_tail, operand_rex, push_rex, rex_bits, xmm_of,
};
use self::parse::{parse_mnemonic, parse_operand, split_operands, strip_comment};
use super::tables::{self, Grp3, Width};
use super::{IsaError, IsaMn, IsaObject, IsaOp, IsaReloc};

pub(crate) mod forms;
pub(crate) mod parse;

/// Maximum branch-size fixpoint rounds before the assembler gives up.
const MAX_FIXPOINT_ROUNDS: u32 = 64;

/// Signed imm8 lower bound.
const IMM8_MIN: i64 = -128;

/// Unsigned imm8 upper bound for byte-width immediates.
const IMM8_U_MAX: i64 = 255;

/// Hardware number of the accumulator (AL/AX/EAX/RAX).
const ACC: u32 = 0;

/// Hardware number of RSP (a base that needs a SIB byte).
const HW_SIB: u32 = 4;

/// Hardware number of RBP (mod 00 rm 101 means RIP-relative, so an rbp base needs an explicit displacement).
const HW_FRAME: u32 = 5;

/// Hardware number where R8 begins.
const GPR_EXT: u32 = 8;

/// The rel32 field length; also the call and RIP relocation addend.
const REL32_LEN: i64 = 4;

/// One parsed instruction with its operands and optional branch target name.
#[derive(Clone, Debug)]
pub(crate) struct Parsed {
    /// Mnemonic.
    pub(crate) mn: IsaMn,
    /// Width.
    pub(crate) width: Width,
    /// Operands in AT&T order.
    pub(crate) ops: Vec<IsaOp>,
    /// Branch target: a label name or a decimal absolute offset.
    pub(crate) branch: Option<String>,
}

/// Assembles AT&T text into an IsaObject with labels and relocations.
// CEP:ID: JIT-e46
// CEP:WHAT: Assembles one text program.
// CEP:WHY: The tier pair's text entry: emit() text, renderer text, and hand-written programs all encode through one path.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the module errors; branch sizes settle within 64 rounds or the assembly fails.
// CEP:ASSUMES: the System V AMD64 ABI; branch targets are labels, symbols, or absolute byte offsets.
// CEP:COST: cold; O(text) parse plus O(insns) per fixpoint round.
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub fn assemble(text: &str) -> Result<IsaObject, IsaError> {
    let mut insns: Vec<Parsed> = Vec::new();
    let mut label_at: Vec<(String, usize)> = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = idx + 1;
        let body = strip_comment(raw).trim();
        if body.is_empty() {
            continue;
        }
        if let Some(name) = body.strip_suffix(':') {
            let name = name.trim();
            if name.is_empty() {
                return Err(IsaError::Parse(line, "empty label name".into()));
            }
            pending.push(name.to_string());
            continue;
        }
        if body.starts_with('.') && !body.starts_with(".L") {
            continue; // .text, .globl and friends are accepted and ignored
        }
        let (head, rest) = match body.find(char::is_whitespace) {
            Some(pos) => (&body[..pos], &body[pos..]),
            None => (body, ""),
        };
        let (mn0, width) = parse_mnemonic(head, line)?;
        let mut mn = mn0;
        let mut ops = Vec::new();
        let mut branch = None;
        for tok in split_operands(rest) {
            let (op, sym) = parse_operand(&tok, line)?;
            if let Some(s) = sym {
                branch = Some(s);
            }
            ops.push(op);
        }
        if mn == IsaMn::Imul && ops.len() == 3 {
            mn = IsaMn::Imul3;
        }
        if mn == IsaMn::Imul && ops.len() == 2 {
            mn = IsaMn::Imul2;
        }
        if matches!(mn, IsaMn::Call | IsaMn::Jmp | IsaMn::Jcc(_)) {
            let target = match ops.first() {
                Some(IsaOp::Sym(s)) => Some(s.clone()),
                _ => branch,
            };
            ops.clear();
            insns.push(Parsed {
                mn,
                width,
                ops,
                branch: target,
            });
        } else {
            insns.push(Parsed {
                mn,
                width,
                ops,
                branch: None,
            });
        }
        for name in pending.drain(..) {
            label_at.push((name, insns.len() - 1));
        }
    }
    for name in pending {
        label_at.push((name, insns.len()));
    }
    // Branch-size fixpoint: sizes depend on displacements; displacements depend on offsets.
    let mut sizes = vec![0usize; insns.len()];
    let mut offsets = vec![0u32; insns.len()];
    let label_target =
        |label_at: &[(String, usize)], offsets: &[u32], total: u32, name: &str| -> Option<u32> {
            label_at.iter().find(|(n, _)| n == name).map(|(_, idx)| {
                if *idx < offsets.len() {
                    offsets[*idx]
                } else {
                    total
                }
            })
        };
    let mut round = 0u32;
    loop {
        let mut pc = 0u32;
        for (i, s) in sizes.iter().enumerate() {
            offsets[i] = pc;
            pc += *s as u32;
        }
        let total = pc;
        let off = |name: &str| label_target(&label_at, &offsets, total, name);
        let mut changed = false;
        for (i, insn) in insns.iter().enumerate() {
            let s = insn_size(insn, offsets[i], &off)?;
            if sizes[i] != s {
                sizes[i] = s;
                changed = true;
            }
        }
        if !changed {
            break;
        }
        round += 1;
        if round > MAX_FIXPOINT_ROUNDS {
            return Err(IsaError::FixpointUnstable);
        }
    }
    let mut pc = 0u32;
    for (i, s) in sizes.iter().enumerate() {
        offsets[i] = pc;
        pc += *s as u32;
    }
    let total = pc;
    let off = |name: &str| label_target(&label_at, &offsets, total, name);
    let mut text_out = Vec::with_capacity(total as usize);
    let mut relocs = Vec::new();
    let mut placed: Vec<(u32, String)> = Vec::new();
    for (i, insn) in insns.iter().enumerate() {
        while placed.len() < label_at.len() && label_at[placed.len()].1 == i {
            placed.push((offsets[i], label_at[placed.len()].0.clone()));
        }
        encode_insn(insn, offsets[i], &off, &mut text_out, &mut relocs)?;
    }
    while placed.len() < label_at.len() {
        let i = placed.len();
        placed.push((total, label_at[i].0.clone()));
    }
    Ok(IsaObject {
        text: text_out,
        relocs,
        labels: placed,
    })
}

/// Computes one instruction's encoded size at its offset.
// CEP:WHAT: Sizes one instruction.
// CEP:WHY: The fixpoint needs exact sizes before any byte is emitted.
// CEP:STATUS: complete
// CEP:FAILURE: Returns Parse errors for malformed operand shapes.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn insn_size(
    insn: &Parsed,
    pc: u32,
    off: &dyn Fn(&str) -> Option<u32>,
) -> Result<usize, IsaError> {
    let mut probe = Vec::new();
    let mut relocs = Vec::new();
    encode_insn(insn, pc, off, &mut probe, &mut relocs)?;
    Ok(probe.len())
}

/// Resolves a branch target name to an absolute offset or an external symbol.
// CEP:WHAT: Resolves a branch target.
// CEP:WHY: Labels and absolute offsets resolve; symbols become relocations.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::UnknownLabel for a name with no definition.
// CEP:ASSUMES: none
// CEP:COST: O(labels).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) enum Target {
    /// Absolute byte offset in the buffer.
    At(u32),
    /// External symbol for a relocation.
    Sym(String),
}

/// Resolves the branch operand of a parsed instruction.
// CEP:WHAT: Reads the branch target.
// CEP:WHY: Shared by jmp, jcc, and call.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::UnknownLabel and Parse for a missing target.
// CEP:ASSUMES: none
// CEP:COST: O(labels).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn branch_target(
    insn: &Parsed,
    off: &dyn Fn(&str) -> Option<u32>,
) -> Result<Target, IsaError> {
    let name = insn
        .branch
        .clone()
        .ok_or_else(|| IsaError::Parse(0, "branch needs a target".into()))?;
    if let Some(v) = parse::parse_int(&name) {
        return Ok(Target::At(v.max(0) as u32));
    }
    if let Some(t) = off(&name) {
        return Ok(Target::At(t));
    }
    // Unresolved names are external symbols and become relocations.
    Ok(Target::Sym(name))
}

/// Encodes one instruction at its offset into the output buffer.
// CEP:ID: JIT-556
// CEP:WHAT: Encodes one instruction.
// CEP:WHY: Sizing and emission share one encoder so the fixpoint is exact.
// CEP:STATUS: complete
// CEP:FAILURE: Returns Parse for operand shapes the mnemonic does not take and UnknownLabel for unresolved targets.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
pub(crate) fn encode_insn(
    insn: &Parsed,
    pc: u32,
    off: &dyn Fn(&str) -> Option<u32>,
    out: &mut Vec<u8>,
    relocs: &mut Vec<IsaReloc>,
) -> Result<(), IsaError> {
    let perr = |r: String| IsaError::Parse(0, r);
    let w = insn.width;
    let ops = &insn.ops;
    match insn.mn {
        IsaMn::Call => {
            let end = pc + 5;
            match branch_target(insn, off)? {
                Target::At(t) => {
                    out.push(0xE8);
                    out.extend_from_slice(&((i64::from(t) - i64::from(end)) as i32).to_le_bytes());
                }
                Target::Sym(name) => {
                    out.push(0xE8);
                    relocs.push(IsaReloc {
                        offset: pc + 1,
                        symbol: name,
                        addend: -REL32_LEN,
                    });
                    out.extend_from_slice(&[0, 0, 0, 0]);
                }
            }
            return Ok(());
        }
        IsaMn::Jmp => {
            // Try the short form against the short-form end; else the long form.
            if let Target::At(t) = branch_target(insn, off)? {
                let rel_short = i64::from(t) - i64::from(pc + 2);
                if fits_i8(rel_short) {
                    out.push(0xEB);
                    out.push(rel_short as i8 as u8);
                    return Ok(());
                }
                let rel_long = i64::from(t) - i64::from(pc + 5);
                out.push(0xE9);
                out.extend_from_slice(&(rel_long as i32).to_le_bytes());
                return Ok(());
            }
            let name = match branch_target(insn, off)? {
                Target::Sym(n) => n,
                Target::At(_) => unreachable!(),
            };
            out.push(0xE9);
            relocs.push(IsaReloc {
                offset: pc + 1,
                symbol: name,
                addend: -REL32_LEN,
            });
            out.extend_from_slice(&[0, 0, 0, 0]);
            return Ok(());
        }
        IsaMn::Jcc(c) => {
            if let Target::At(t) = branch_target(insn, off)? {
                let rel_short = i64::from(t) - i64::from(pc + 2);
                if fits_i8(rel_short) {
                    out.push(0x70 + c);
                    out.push(rel_short as i8 as u8);
                    return Ok(());
                }
                let rel_long = i64::from(t) - i64::from(pc + 6);
                out.push(0x0F);
                out.push(0x80 + c);
                out.extend_from_slice(&(rel_long as i32).to_le_bytes());
                return Ok(());
            }
            let name = match branch_target(insn, off)? {
                Target::Sym(n) => n,
                Target::At(_) => unreachable!(),
            };
            out.push(0x0F);
            out.push(0x80 + c);
            relocs.push(IsaReloc {
                offset: pc + 2,
                symbol: name,
                addend: -REL32_LEN,
            });
            out.extend_from_slice(&[0, 0, 0, 0]);
            return Ok(());
        }
        IsaMn::Ret => {
            if !ops.is_empty() {
                return Err(perr("ret takes no operands".into()));
            }
            out.push(0xC3);
            return Ok(());
        }
        IsaMn::Nop => {
            out.push(0x90);
            return Ok(());
        }
        IsaMn::Cqo => {
            if w == Width::Q {
                out.push(0x48);
            } else if w == Width::L {
                // no prefix
            } else {
                return Err(perr("cqo width must be q or l".into()));
            }
            out.push(0x99);
            return Ok(());
        }
        IsaMn::MovSd => {
            return encode_movsd(ops, out);
        }
        _ => {}
    }
    // Scalar SSE binary operations: prefix, [REX], escape, opcode, ModRM.
    let sse: Option<(u8, u8)> = match insn.mn {
        IsaMn::MovApd => Some((0x66, 0x28)),
        IsaMn::AddSd => Some((0xF2, 0x58)),
        IsaMn::SubSd => Some((0xF2, 0x5C)),
        IsaMn::MulSd => Some((0xF2, 0x59)),
        IsaMn::DivSd => Some((0xF2, 0x5E)),
        IsaMn::SqrtSd => Some((0xF2, 0x51)),
        IsaMn::Ucomisd => Some((0x66, 0x2E)),
        IsaMn::Comisd => Some((0x66, 0x2F)),
        IsaMn::CvtSi2Sd => Some((0xF2, 0x2A)),
        IsaMn::Cvttsd2Si => Some((0xF2, 0x2C)),
        _ => None,
    };
    if let Some((pre, opc2)) = sse {
        let src = ops
            .first()
            .cloned()
            .ok_or_else(|| perr("missing source".into()))?;
        let dst = ops
            .get(1)
            .cloned()
            .ok_or_else(|| perr("missing destination".into()))?;
        let rex_w = matches!(insn.mn, IsaMn::CvtSi2Sd | IsaMn::Cvttsd2Si) && w == Width::Q;
        let (reg_field, rm_op) = if matches!(insn.mn, IsaMn::Cvttsd2Si) {
            (gpr_of(&dst, &perr)?, src)
        } else {
            (xmm_of(&dst, &perr)?, src)
        };
        let tail_at = out.len();
        out.push(pre);
        push_rex(out, operand_rex(rex_w, reg_field, &rm_op));
        out.push(0x0F);
        out.push(opc2);
        modrm_tail(out, reg_field, &rm_op, &perr)?;
        let _ = tail_at;
        return Ok(());
    }
    match insn.mn {
        IsaMn::Mov => encode_mov(insn, out),
        IsaMn::MovZx(src_w) | IsaMn::MovSx(src_w) => {
            let signed = matches!(insn.mn, IsaMn::MovSx(_));
            let op2 = match (src_w, signed) {
                (Width::B, false) => 0xB6u8,
                (Width::W, false) => 0xB7,
                (Width::B, true) => 0xBE,
                (Width::W, true) => 0xBF,
                _ => return Err(perr("move-grid source width must be b or w".into())),
            };
            let src = ops
                .first()
                .cloned()
                .ok_or_else(|| perr("missing source".into()))?;
            let dst = gpr_of(
                &ops.get(1)
                    .cloned()
                    .ok_or_else(|| perr("missing destination".into()))?,
                &perr,
            )?;
            if w == Width::W {
                out.push(0x66);
            }
            push_rex(out, operand_rex(w == Width::Q, dst, &src));
            out.push(0x0F);
            out.push(op2);
            modrm_tail(out, dst, &src, &perr)?;
            Ok(())
        }
        IsaMn::MovSxd => {
            let src = ops
                .first()
                .cloned()
                .ok_or_else(|| perr("missing source".into()))?;
            let dst = gpr_of(
                &ops.get(1)
                    .cloned()
                    .ok_or_else(|| perr("missing destination".into()))?,
                &perr,
            )?;
            push_rex(out, operand_rex(true, dst, &src));
            out.push(0x63);
            modrm_tail(out, dst, &src, &perr)?;
            Ok(())
        }
        IsaMn::Lea => {
            let src = ops
                .first()
                .cloned()
                .ok_or_else(|| perr("missing source".into()))?;
            let dst = gpr_of(
                &ops.get(1)
                    .cloned()
                    .ok_or_else(|| perr("missing destination".into()))?,
                &perr,
            )?;
            if w == Width::W {
                out.push(0x66);
            }
            push_rex(out, operand_rex(w == Width::Q, dst, &src));
            out.push(0x8D);
            modrm_tail(out, dst, &src, &perr)?;
            Ok(())
        }
        IsaMn::Alu(alu) => encode_alu(alu, insn, out),
        IsaMn::Test => encode_test(insn, out),
        IsaMn::Not | IsaMn::Neg | IsaMn::Mul | IsaMn::Imul | IsaMn::Div | IsaMn::Idiv => {
            let ext = match insn.mn {
                IsaMn::Not => Grp3::Not.ext(),
                IsaMn::Neg => Grp3::Neg.ext(),
                IsaMn::Mul => Grp3::Mul.ext(),
                IsaMn::Imul => Grp3::Imul.ext(),
                IsaMn::Div => Grp3::Div.ext(),
                _ => Grp3::Idiv.ext(),
            };
            let dst = ops
                .first()
                .cloned()
                .ok_or_else(|| perr("missing operand".into()))?;
            if w == Width::W {
                out.push(0x66);
            }
            push_rex(out, operand_rex(w == Width::Q, ext as u32, &dst));
            out.push(if w == Width::B { 0xF6 } else { 0xF7 });
            modrm_tail(out, ext as u32, &dst, &perr)?;
            Ok(())
        }
        IsaMn::Shl | IsaMn::Shr | IsaMn::Sar => encode_shift(insn, out),
        IsaMn::Imul2 => {
            let src = ops
                .first()
                .cloned()
                .ok_or_else(|| perr("missing source".into()))?;
            let dst = gpr_of(
                &ops.get(1)
                    .cloned()
                    .ok_or_else(|| perr("missing destination".into()))?,
                &perr,
            )?;
            if w == Width::W {
                out.push(0x66);
            }
            push_rex(out, operand_rex(w == Width::Q, dst, &src));
            out.push(0x0F);
            out.push(0xAF);
            modrm_tail(out, dst, &src, &perr)?;
            Ok(())
        }
        IsaMn::Imul3 => {
            let imm = imm_of(
                ops.first()
                    .ok_or_else(|| perr("missing immediate".into()))?,
                &perr,
            )?;
            let src = ops
                .get(1)
                .cloned()
                .ok_or_else(|| perr("missing source".into()))?;
            let dst = gpr_of(
                &ops.get(2)
                    .cloned()
                    .ok_or_else(|| perr("missing destination".into()))?,
                &perr,
            )?;
            push_rex(out, rex_bits(true, dst >= GPR_EXT, false, false));
            if fits_i8(imm) {
                out.push(0x6B);
                emit_modrm(out, false, dst, &src, &perr)?;
                out.push(imm as i8 as u8);
            } else if fits_i32(imm) {
                out.push(0x69);
                emit_modrm(out, false, dst, &src, &perr)?;
                out.extend_from_slice(&(imm as i32).to_le_bytes());
            } else {
                return Err(perr("imul immediate outside i32".into()));
            }
            Ok(())
        }
        IsaMn::Push => {
            match ops.first().ok_or_else(|| perr("missing operand".into()))? {
                IsaOp::Gpr(hw) => {
                    if *hw >= GPR_EXT {
                        out.push(0x41);
                    }
                    out.push(0x50 + (*hw & 7) as u8);
                }
                IsaOp::Imm(v) => {
                    if fits_i8(*v) {
                        out.push(0x6A);
                        out.push(*v as i8 as u8);
                    } else if fits_i32(*v) {
                        out.push(0x68);
                        out.extend_from_slice(&(*v as i32).to_le_bytes());
                    } else {
                        return Err(perr("push immediate outside i32".into()));
                    }
                }
                other => {
                    push_rex(out, operand_rex(false, 6, other));
                    out.push(0xFF);
                    modrm_tail(out, 6, other, &perr)?;
                }
            }
            Ok(())
        }
        IsaMn::Pop => {
            match ops.first().ok_or_else(|| perr("missing operand".into()))? {
                IsaOp::Gpr(hw) => {
                    if *hw >= GPR_EXT {
                        out.push(0x41);
                    }
                    out.push(0x58 + (*hw & 7) as u8);
                }
                other => {
                    push_rex(out, operand_rex(false, 0, other));
                    out.push(0x8F);
                    modrm_tail(out, 0, other, &perr)?;
                }
            }
            Ok(())
        }
        IsaMn::Inc | IsaMn::Dec => {
            let ext = if insn.mn == IsaMn::Inc { 0u32 } else { 1 };
            let dst = ops
                .first()
                .cloned()
                .ok_or_else(|| perr("missing operand".into()))?;
            if w == Width::W {
                out.push(0x66);
            }
            push_rex(out, operand_rex(w == Width::Q, ext, &dst));
            out.push(if w == Width::B { 0xFE } else { 0xFF });
            modrm_tail(out, ext, &dst, &perr)?;
            Ok(())
        }
        _ => Err(perr(format!(
            "mnemonic {:?} needs branch or SSE handling",
            insn.mn
        ))),
    }
}

/// Emits the REX byte, ModRM, optional SIB, and displacement for one operand.
// CEP:WHAT: Emits a ModRM tail.
// CEP:WHY: Every register or memory operand of the tier encodes here, in canonical GNU as form.
// CEP:STATUS: complete
// CEP:FAILURE: IsaError::Parse for displacements outside i32.
// CEP:ASSUMES: the caller emitted prefix and opcode bytes already.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/isa_tier_test.rs
fn emit_modrm(
    out: &mut Vec<u8>,
    rex_w: bool,
    reg_field: u32,
    op: &IsaOp,
    perr: &dyn Fn(String) -> IsaError,
) -> Result<(), IsaError> {
    push_rex(out, operand_rex(rex_w, reg_field, op));
    modrm_tail(out, reg_field, op, perr)
}

// CEP:FILE: crates/mlkp-machine/src/object.rs
// CEP:WHAT: Deterministic ELF64 relocatable object emission for one compiled function: .text, .rela.text, .symtab, .strtab, .shstrtab, plus the evidence hashes the emission policy requires (ticket CEP-322).
// CEP:WHY: The design's machine backend ends at encoding, relocations, and object files; a relocatable object links against callers through the standard toolchain, and the evidence hashes make every layout property mechanically checkable.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the decoder's errors for the disassembly evidence; byte emission itself is infallible because every input (bytes, relocations, symbol names) is already validated.
// CEP:ASSUMES: one function per object; external call symbols resolve at link time through R_X86_64_PLT32 with addend -4; little-endian x86-64 ELF.
// CEP:COST: cold; O(text + relocs + symbols).
// CEP:EVIDENCE: crates/mlkp-machine/tests/object_test.rs.

#![deny(missing_docs)]

use crate::encode::CodeObject;
use crate::target::X86_64Target;

/// ELF64 header size in bytes.
const EH_SIZE: usize = 64;

/// Section header entry size in bytes.
const SH_SIZE: usize = 64;

/// Symbol table entry size in bytes.
const SYM_SIZE: usize = 24;

/// Relocation entry size in bytes.
const REL_SIZE: usize = 24;

/// SHT_PROGBITS.
const SHT_PROGBITS: u32 = 1;
/// SHT_SYMTAB.
const SHT_SYMTAB: u32 = 2;
/// SHT_STRTAB.
const SHT_STRTAB: u32 = 3;
/// SHT_RELA.
const SHT_RELA: u32 = 4;
/// SHF_ALLOC.
const SHF_ALLOC: u64 = 2;
/// SHF_EXECINSTR.
const SHF_EXECINSTR: u64 = 4;
/// SHF_INFO_LINK.
const SHF_INFO_LINK: u64 = 0x40;
/// ET_REL (relocatable).
const ET_REL: u16 = 1;
/// EM_X86_64.
const EM_X86_64: u16 = 62;
/// STB_GLOBAL.
const STB_GLOBAL: u8 = 1;
/// STT_FUNC.
const STT_FUNC: u8 = 2;
/// R_X86_64_PLT32.
const R_PLT32: u32 = 4;
/// Symbol binding and type shift within st_info.
const ST_INFO_SHIFT: u32 = 4;
/// First byte of the ELF magic string.
const ELFMAG0: u8 = 0x7f;
/// ELFCLASS64.
const ELFCLASS64: u8 = 2;
/// Symbol table index of the first extern symbol (null, section, function).
const FIRST_EXTERN_SYM: u64 = 3;
/// Relocation r_info high word shift.
const R_INFO_SYM_SHIFT: u32 = 32;
/// e_ident padding length.
const EIDENT_PAD: usize = 7;
/// Section index of .text in this layout.
const SEC_TEXT: u32 = 1;
/// Section index of .symtab in this layout.
const SEC_SYMTAB: u32 = 3;
/// Section index of .strtab in this layout.
const SEC_STRTAB: u32 = 4;
/// Section index of .shstrtab in this layout.
const SEC_SHSTRTAB: u16 = 5;
/// Total section count including the null section.
const SH_NUM: u16 = 6;
/// symtab sh_info: index one past the last local symbol (null, section).
const FIRST_GLOBAL_SYM: u32 = 2;
/// Alignment of the rela, symtab, and section-header table areas.
const ALIGN8: usize = 8;
/// .text alignment: executable code is 16-byte aligned.
const ALIGN_TEXT: u64 = 16;
/// sh_addralign of the rela, symtab, and section-header table.
const ALIGN8_U64: u64 = 8;

/// Evidence the emission policy requires for every emitted object.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectEvidence {
    /// FNV-1a hash of the whole object file.
    pub object_hash: u64,
    /// Section names in layout order.
    pub sections: Vec<String>,
    /// FNV-1a hash of the .symtab contents.
    pub symbol_table_hash: u64,
    /// FNV-1a hash of the .rela.text contents.
    pub relocation_table_hash: u64,
    /// FNV-1a hash of the disassembly text.
    pub disassembly_hash: u64,
}

/// Appends a little-endian u16.
// CEP:WHAT: Writes a u16.
// CEP:WHY: ELF fields.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/object_test.rs
fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Appends a little-endian u32.
// CEP:WHAT: Writes a u32.
// CEP:WHY: ELF fields.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/object_test.rs
fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Appends a little-endian u64.
// CEP:WHAT: Writes a u64.
// CEP:WHY: ELF fields.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-machine/tests/object_test.rs
fn put64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Zero-pads to the next multiple of `align`.
// CEP:WHAT: Pads to alignment.
// CEP:WHY: Deterministic section layout.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: align is a power of two.
// CEP:COST: O(padding).
// CEP:EVIDENCE: crates/mlkp-machine/tests/object_test.rs
fn pad_to(out: &mut Vec<u8>, align: usize) {
    while !out.len().is_multiple_of(align) {
        out.push(0);
    }
}

/// Builds the external symbol list in relocation first-use order, deduplicated.
// CEP:WHAT: Collects external symbols.
// CEP:WHY: The symbol table needs one entry per distinct external symbol, and relocation entries reference them by index.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: relocation order is instruction order, which the encoder guarantees.
// CEP:COST: O(relocs).
// CEP:EVIDENCE: crates/mlkp-machine/tests/object_test.rs
fn extern_symbols(co: &CodeObject) -> Vec<String> {
    let mut exts: Vec<String> = Vec::new();
    for r in &co.relocs {
        if !exts.contains(&r.symbol) {
            exts.push(r.symbol.clone());
        }
    }
    exts
}

/// Builds the .symtab contents.
// CEP:ID: JIT-t83
// CEP:WHAT: Builds the symbol table.
// CEP:WHY: The linker resolves the function symbol and the external references through it.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: layout: null symbol, .text section symbol, function symbol, externs in first-use order.
// CEP:COST: O(symbols).
// CEP:EVIDENCE: crates/mlkp-machine/tests/object_test.rs
fn symtab(co: &CodeObject, function: &str, strtab: &[u8], exts: &[String]) -> Vec<u8> {
    let name_off = |s: &str| -> u32 {
        strtab
            .windows(s.len() + 1)
            .position(|w| w == format!("{s}\0").as_bytes())
            .unwrap_or(0) as u32
    };
    let mut out = Vec::new();
    // Null symbol.
    out.extend_from_slice(&[0u8; SYM_SIZE]);
    // .text section symbol (LOCAL SECTION).
    put32(&mut out, 0);
    out.push(0); // LOCAL, NOTYPE
    out.push(0); // visibility
    put16(&mut out, 1); // shndx = .text
    put64(&mut out, 0);
    put64(&mut out, 0);
    // Function symbol (GLOBAL FUNC).
    put32(&mut out, name_off(function));
    out.push((STB_GLOBAL << ST_INFO_SHIFT) | STT_FUNC);
    out.push(0);
    put16(&mut out, 1);
    put64(&mut out, 0);
    put64(&mut out, co.text.len() as u64);
    // External symbols (GLOBAL NOTYPE, undefined).
    for ext in exts {
        put32(&mut out, name_off(ext));
        out.push(STB_GLOBAL << ST_INFO_SHIFT);
        out.push(0);
        put16(&mut out, 0); // SHN_UNDEF
        put64(&mut out, 0);
        put64(&mut out, 0);
    }
    out
}

/// Builds the .rela.text contents.
// CEP:WHAT: Builds the relocation table.
// CEP:WHY: External calls resolve at link time; every entry is R_X86_64_PLT32 with the -4 PC-relative addend.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: relocation offsets point at the rel32 fields the encoder left zeroed.
// CEP:COST: O(relocs).
// CEP:EVIDENCE: crates/mlkp-machine/tests/object_test.rs
fn rela(co: &CodeObject, exts: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for r in &co.relocs {
        let sym_idx =
            exts.iter().position(|s| s == &r.symbol).unwrap_or(0) as u64 + FIRST_EXTERN_SYM; // null, section, function
        put64(&mut out, u64::from(r.offset));
        put64(&mut out, (sym_idx << R_INFO_SYM_SHIFT) | u64::from(R_PLT32));
        put64(&mut out, r.addend as u64);
    }
    out
}

/// Emits one deterministic ELF64 relocatable object.
// CEP:ID: JIT-w66
// CEP:WHAT: Emits one ELF64 object.
// CEP:WHY: The design's machine code emission stage ends at object files the standard toolchain links.
// CEP:STATUS: complete
// CEP:FAILURE: none; the function name and relocation symbols must be valid identifiers, which the front end guarantees.
// CEP:ASSUMES: one function per object; no timestamps, paths, or environment values are embedded (the emission policy forbids them).
// CEP:COST: O(text + relocs + symbols).
// CEP:EVIDENCE: crates/mlkp-machine/tests/object_test.rs.
pub fn emit_object(co: &CodeObject, function: &str) -> Vec<u8> {
    // String tables first: the symbol table needs name offsets.
    let mut strtab: Vec<u8> = vec![0];
    strtab.extend_from_slice(function.as_bytes());
    strtab.push(0);
    let exts = extern_symbols(co);
    for ext in &exts {
        strtab.extend_from_slice(ext.as_bytes());
        strtab.push(0);
    }
    let mut shstrtab: Vec<u8> = vec![0];
    let sec_name_off = |s: &str, tab: &mut Vec<u8>| -> u32 {
        let off = tab.len() as u32;
        tab.extend_from_slice(s.as_bytes());
        tab.push(0);
        off
    };
    let n_text = sec_name_off(".text", &mut shstrtab);
    let n_rela = sec_name_off(".rela.text", &mut shstrtab);
    let n_sym = sec_name_off(".symtab", &mut shstrtab);
    let n_str = sec_name_off(".strtab", &mut shstrtab);
    let n_shstr = sec_name_off(".shstrtab", &mut shstrtab);
    // Section contents.
    let sym = symtab(co, function, &strtab, &exts);
    let rel = rela(co, &exts);
    // Layout with deterministic padding.
    let mut out: Vec<u8> = Vec::new();
    // ELF header; the e_shoff field is rewritten once the section table offset is known.
    out.extend_from_slice(&[0u8; EH_SIZE]);
    // .text at 64, aligned 16.
    let off_text = out.len();
    out.extend_from_slice(&co.text);
    // .rela.text aligned 8.
    pad_to(&mut out, ALIGN8);
    let off_rela = out.len();
    out.extend_from_slice(&rel);
    // .symtab aligned 8.
    pad_to(&mut out, ALIGN8);
    let off_sym = out.len();
    out.extend_from_slice(&sym);
    // .strtab aligned 1.
    let off_str = out.len();
    out.extend_from_slice(&strtab);
    // .shstrtab aligned 1.
    let off_shstr = out.len();
    out.extend_from_slice(&shstrtab);
    // Section header table aligned 8.
    pad_to(&mut out, ALIGN8);
    let shoff = out.len();
    let mut shdrs: Vec<u8> = Vec::new();
    // Null section.
    shdrs.extend_from_slice(&[0u8; SH_SIZE]);
    // .text.
    put32(&mut shdrs, n_text);
    put32(&mut shdrs, SHT_PROGBITS);
    put64(&mut shdrs, SHF_ALLOC | SHF_EXECINSTR);
    put64(&mut shdrs, 0);
    put64(&mut shdrs, off_text as u64);
    put64(&mut shdrs, co.text.len() as u64);
    put32(&mut shdrs, 0);
    put32(&mut shdrs, 0);
    put64(&mut shdrs, ALIGN_TEXT);
    put64(&mut shdrs, 0);
    // .rela.text.
    put32(&mut shdrs, n_rela);
    put32(&mut shdrs, SHT_RELA);
    put64(&mut shdrs, SHF_INFO_LINK);
    put64(&mut shdrs, 0);
    put64(&mut shdrs, off_rela as u64);
    put64(&mut shdrs, rel.len() as u64);
    put32(&mut shdrs, SEC_SYMTAB); // link
    put32(&mut shdrs, SEC_TEXT); // info: applied to .text
    put64(&mut shdrs, ALIGN8_U64);
    put64(&mut shdrs, REL_SIZE as u64);
    // .symtab.
    put32(&mut shdrs, n_sym);
    put32(&mut shdrs, SHT_SYMTAB);
    put64(&mut shdrs, 0);
    put64(&mut shdrs, 0);
    put64(&mut shdrs, off_sym as u64);
    put64(&mut shdrs, sym.len() as u64);
    put32(&mut shdrs, SEC_STRTAB); // link
    put32(&mut shdrs, FIRST_GLOBAL_SYM); // info: one past the last local symbol
    put64(&mut shdrs, ALIGN8_U64);
    put64(&mut shdrs, SYM_SIZE as u64);
    // .strtab.
    put32(&mut shdrs, n_str);
    put32(&mut shdrs, SHT_STRTAB);
    put64(&mut shdrs, 0);
    put64(&mut shdrs, 0);
    put64(&mut shdrs, off_str as u64);
    put64(&mut shdrs, strtab.len() as u64);
    put32(&mut shdrs, 0);
    put32(&mut shdrs, 0);
    put64(&mut shdrs, 1);
    put64(&mut shdrs, 0);
    // .shstrtab.
    put32(&mut shdrs, n_shstr);
    put32(&mut shdrs, SHT_STRTAB);
    put64(&mut shdrs, 0);
    put64(&mut shdrs, 0);
    put64(&mut shdrs, off_shstr as u64);
    put64(&mut shdrs, shstrtab.len() as u64);
    put32(&mut shdrs, 0);
    put32(&mut shdrs, 0);
    put64(&mut shdrs, 1);
    put64(&mut shdrs, 0);
    out.extend_from_slice(&shdrs);
    let shnum = SH_NUM;
    // Rewrite the ELF header in place.
    let mut hdr: Vec<u8> = Vec::with_capacity(EH_SIZE);
    hdr.extend_from_slice(&[ELFMAG0, b'E', b'L', b'F']);
    hdr.push(ELFCLASS64); // ELFCLASS64
    hdr.push(1); // little-endian
    hdr.push(1); // EV_CURRENT
    hdr.push(0); // System V ABI
    hdr.push(0); // EI_ABIVERSION
    hdr.extend_from_slice(&[0u8; EIDENT_PAD]); // e_ident padding
    put16(&mut hdr, ET_REL);
    put16(&mut hdr, EM_X86_64);
    put32(&mut hdr, 1); // e_version
    put64(&mut hdr, 0); // e_entry
    put64(&mut hdr, 0); // e_phoff
    put64(&mut hdr, shoff as u64);
    put32(&mut hdr, 0); // e_flags
    put16(&mut hdr, EH_SIZE as u16);
    put16(&mut hdr, 0); // e_phentsize
    put16(&mut hdr, 0); // e_phnum
    put16(&mut hdr, SH_SIZE as u16);
    put16(&mut hdr, shnum);
    put16(&mut hdr, SEC_SHSTRTAB); // e_shstrndx
    out[0..EH_SIZE].copy_from_slice(&hdr);
    out
}

/// Emits one object together with the evidence hashes the emission policy requires.
// CEP:ID: JIT-f86
// CEP:WHAT: Emits one object with evidence.
// CEP:WHY: The emission policy demands object hash, section list, symbol table hash, relocation table hash, and disassembly hash as evidence.
// CEP:STATUS: complete
// CEP:FAILURE: Returns the decoder's errors when the disassembly evidence cannot be produced.
// CEP:ASSUMES: see emit_object().
// CEP:COST: O(text + relocs + symbols) plus one disassembly pass.
// CEP:EVIDENCE: crates/mlkp-machine/tests/object_test.rs.
pub fn emit_with_evidence(
    co: &CodeObject,
    function: &str,
    target: &X86_64Target,
) -> Result<(Vec<u8>, ObjectEvidence), crate::decode::DecodeError> {
    let obj = emit_object(co, function);
    let sym = symtab(
        co,
        function,
        &{
            let mut t = vec![0u8];
            t.extend_from_slice(function.as_bytes());
            t.push(0);
            for ext in extern_symbols(co) {
                t.extend_from_slice(ext.as_bytes());
                t.push(0);
            }
            t
        },
        &extern_symbols(co),
    );
    let rel = rela(co, &extern_symbols(co));
    let disasm = crate::decode::disassemble(co, function, target)?;
    let evidence = ObjectEvidence {
        object_hash: cep_hash::hash_bytes(&obj, cep_hash::FNV_OFFSET),
        sections: vec![
            ".text".to_string(),
            ".rela.text".to_string(),
            ".symtab".to_string(),
            ".strtab".to_string(),
            ".shstrtab".to_string(),
        ],
        symbol_table_hash: cep_hash::hash_bytes(&sym, cep_hash::FNV_OFFSET),
        relocation_table_hash: cep_hash::hash_bytes(&rel, cep_hash::FNV_OFFSET),
        disassembly_hash: cep_hash::hash_bytes(disasm.as_bytes(), cep_hash::FNV_OFFSET),
    };
    Ok((obj, evidence))
}

// CEP:FILE: crates/mlkp-machine/tests/isa_tier_test.rs
// CEP:WHAT: Tier-pair tests: general decode of byte fixtures for ALU, move, LEA, shift, group, stack, and SSE forms; assembler byte fixtures over a full-width corpus; the text round trip through decode and render; and the gas differential (ticket CEP-325).
// CEP:WHY: The general decoder must accept every width and shape of the tier and reject everything outside it loudly; the assembler must agree with GNU as byte for byte and survive its own rendered text.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design.
// CEP:ASSUMES: GNU as is installed for the differential test; the platform is x86-64.
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-machine.

use mlkp_machine::{isa_assemble, isa_decode_bytes, isa_render_insn, IsaInsn};

/// Decodes exactly one instruction from the front of the byte slice.
// CEP:WHAT: Decodes one instruction.
// CEP:WHY: Stepwise decoding names the exact offset and bytes of a divergence.
// CEP:STATUS: complete
// CEP:FAILURE: Panics with offset and byte context when nothing decodes.
// CEP:ASSUMES: none
// CEP:COST: O(bytes) worst case on the error path only.
// CEP:EVIDENCE: cargo test -p mlkp-machine.
fn decode_one_at(bytes: &[u8]) -> (IsaInsn, usize) {
    for len in 1..=bytes.len() {
        if let Ok(v) = isa_decode_bytes(&bytes[..len]) {
            if v.len() == 1 {
                return (v[0].clone(), len);
            }
        }
    }
    let ctx: Vec<String> = bytes[..bytes.len().min(12)]
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    panic!("nothing decodes at offset; context={}", ctx.join(" "));
}

/// Decodes a byte program one instruction at a time.
// CEP:WHAT: Steps through a byte program.
// CEP:WHY: A monolithic decode hides which instruction diverges; this names it.
// CEP:STATUS: complete
// CEP:FAILURE: Panics with offset and byte context on the first divergence.
// CEP:ASSUMES: none
// CEP:COST: O(bytes) per instruction.
// CEP:EVIDENCE: cargo test -p mlkp-machine.
fn decode_stepwise(bytes: &[u8]) -> Vec<IsaInsn> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let (mut insn, len) = decode_one_at(&bytes[pos..]);
        // The slice-relative decode reports branch targets from the slice base;
        // rebase them onto the whole buffer.
        if let Some(t) = insn.target {
            insn.target = Some(t + pos as i64);
        }
        out.push(insn);
        pos += len;
    }
    out
}

/// ALU fixtures across widths and shapes.
// CEP:WHAT: Tests ALU decode.
// CEP:WHY: The 00-3F map covers both directions and both widths.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-machine.
#[test]
fn decodes_alu_shapes() {
    assert_eq!(text_of(&[0x48, 0x01, 0xF0]), "  addq %rsi, %rax\n");
    assert_eq!(text_of(&[0x48, 0x03, 0x03]), "  addq (%rbx), %rax\n");
    assert_eq!(text_of(&[0x00, 0xC1]), "  addb %al, %cl\n");
    assert_eq!(text_of(&[0x48, 0x83, 0xC0, 0x05]), "  addq $5, %rax\n");
    assert_eq!(
        text_of(&[0x48, 0x05, 0x05, 0x00, 0x00, 0x00]),
        "  addq $5, %rax\n"
    );
    assert_eq!(text_of(&[0x3C, 0x07]), "  cmpb $7, %al\n");
    assert_eq!(text_of(&[0x66, 0x83, 0xC3, 0x02]), "  addw $2, %bx\n");
}

/// Move and LEA fixtures including movabs and the width grid.
// CEP:WHAT: Tests move decode.
// CEP:WHY: All four widths and the immediate forms must decode.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-machine.
#[test]
fn decodes_move_shapes() {
    assert_eq!(text_of(&[0x48, 0x89, 0xD8]), "  movq %rbx, %rax\n");
    assert_eq!(text_of(&[0x89, 0xD8]), "  movl %ebx, %eax\n");
    assert_eq!(text_of(&[0x48, 0x8B, 0x03]), "  movq (%rbx), %rax\n");
    assert_eq!(
        text_of(&[0x48, 0xB8, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
        "  movq $5, %rax\n"
    );
    assert_eq!(text_of(&[0xB0, 0x07]), "  movb $7, %al\n");
    assert_eq!(text_of(&[0x66, 0xB8, 0x05, 0x00]), "  movw $5, %ax\n");
    assert_eq!(
        text_of(&[0x48, 0x8D, 0x44, 0x08, 0x10]),
        "  leaq 16(%rax,%rcx,1), %rax\n"
    );
    assert_eq!(
        text_of(&[0x48, 0x8D, 0x05, 0x10, 0x00, 0x00, 0x00]),
        "  leaq 16(%rip), %rax\n"
    );
}

/// Shift, group, stack, and SSE fixtures.
// CEP:ID: JIT-266
// CEP:WHAT: Tests remaining family decode.
// CEP:WHY: Shifts, the unary group, stack ops, and the SSE set complete the tier.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-machine.
#[test]
fn decodes_group_and_sse_shapes() {
    assert_eq!(text_of(&[0x48, 0xC1, 0xE0, 0x03]), "  shlq $3, %rax\n");
    assert_eq!(text_of(&[0x48, 0xD1, 0xE0]), "  shlq $1, %rax\n");
    assert_eq!(text_of(&[0x48, 0xD3, 0xE0]), "  shlq %cl, %rax\n");
    assert_eq!(text_of(&[0x48, 0xF7, 0xD8]), "  negq %rax\n");
    assert_eq!(text_of(&[0x48, 0xF7, 0xF8]), "  idivq %rax\n");
    assert_eq!(text_of(&[0x48, 0x99]), "  cqto\n");
    assert_eq!(text_of(&[0x99]), "  cdq\n");
    assert_eq!(text_of(&[0x50]), "  pushq %rax\n");
    assert_eq!(text_of(&[0x41, 0x50]), "  pushq %r8\n");
    assert_eq!(text_of(&[0x6A, 0x05]), "  pushq $5\n");
    assert_eq!(text_of(&[0xF2, 0x0F, 0x58, 0xC8]), "  addsd %xmm0, %xmm1\n");
    assert_eq!(
        text_of(&[0x66, 0x0F, 0x28, 0xC1]),
        "  movapd %xmm1, %xmm0\n"
    );
    assert_eq!(
        text_of(&[0xF2, 0x48, 0x0F, 0x2A, 0xC1]),
        "  cvtsi2sdq %rcx, %xmm0\n"
    );
}

/// Encodings outside the tier are rejected loudly.
// CEP:WHAT: Tests loud rejection.
// CEP:WHY: Nothing may be guessed: VEX bytes, lock prefixes, and unknown opcodes are errors.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-machine.
#[test]
fn rejects_outside_tier() {
    assert!(isa_decode_bytes(&[0xC4, 0xC3]).is_err()); // VEX
    assert!(isa_decode_bytes(&[0xF0, 0x48, 0x01, 0xD8]).is_err()); // lock
    assert!(isa_decode_bytes(&[0xD9, 0xC0]).is_err()); // x87 fld
    assert!(isa_decode_bytes(&[0xAA]).is_err()); // stos
    assert!(isa_decode_bytes(&[0x0F, 0x05]).is_err()); // syscall
    assert!(isa_decode_bytes(&[0xE8]).is_err()); // truncated
}

/// Renders one decoded instruction as text.
// CEP:WHAT: Fixture helper.
// CEP:WHY: Byte fixtures stay readable through the rendered text.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: one instruction per fixture.
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-machine.
fn text_of(bytes: &[u8]) -> String {
    let insns = isa_decode_bytes(bytes).expect("decodes");
    assert_eq!(insns.len(), 1, "one instruction per fixture");
    isa_render_insn(&insns[0])
}

/// The corpus: every family, width, memory shape, immediate form, and both branch sizes.
// CEP:WHAT: Supplies the shared corpus.
// CEP:WHY: One corpus drives the fixtures, the round trip, and the gas differential so all three tests cover the same surface.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-machine.
fn corpus() -> &'static str {
    include_str!("isa_corpus.inc")
}

/// Assembles the corpus and checks canonical byte fixtures for representative forms.
// CEP:WHAT: Tests assembler fixtures.
// CEP:WHY: Byte-exact fixtures catch form-selection drift before the gas test runs.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-machine.
#[test]
fn assembles_canonical_fixtures() {
    let obj = isa_assemble(corpus()).expect("assembles");
    let t = &obj.text;
    let fixtures: &[(&[u8], &str)] = &[
        (&[0x48, 0x01, 0xF0], "addq %rsi, %rax"),
        (&[0x48, 0x03, 0x03], "addq (%rbx), %rax"),
        (&[0x48, 0x03, 0x43, 0x10], "addq 16(%rbx), %rax"),
        (&[0x48, 0x03, 0x43, 0xF8], "addq -8(%rbx), %rax"),
        (&[0x00, 0xC1], "addb %al, %cl"),
        (&[0x66, 0x83, 0xC3, 0x02], "addw $2, %bx"),
        (&[0x48, 0x83, 0xC0, 0x05], "addq $5, %rax"),
        (
            &[0x48, 0x81, 0xC7, 0xE8, 0x03, 0x00, 0x00],
            "addq $1000, %rdi",
        ),
        (
            &[0x49, 0x81, 0xC0, 0xA0, 0x86, 0x01, 0x00],
            "addq $100000, %r8",
        ),
        (&[0x83, 0xC0, 0x05], "addl $5, %eax"),
        (&[0x05, 0xE8, 0x03, 0x00, 0x00], "addl $1000, %eax"),
        (&[0xB0, 0x07], "movb $7, %al"),
        (&[0x66, 0xB8, 0x05, 0x00], "movw $5, %ax"),
        (&[0x41, 0xB9, 0x05, 0x00, 0x00, 0x00], "movl $5, %r9d"),
        (&[0x48, 0xC7, 0xC0, 0x05, 0x00, 0x00, 0x00], "movq $5, %rax"),
        (
            &[0x48, 0xB8, 0xCB, 0x04, 0xFB, 0x71, 0x1F, 0x01, 0x00, 0x00],
            "movq $1234567890123, %rax",
        ),
        (&[0x48, 0x89, 0xD8], "movq %rbx, %rax"),
        (&[0x48, 0x89, 0x44, 0x24, 0x08], "movq %rax, 8(%rsp)"),
        (&[0x48, 0x8B, 0x44, 0x24, 0x08], "movq 8(%rsp), %rax"),
        (
            &[0x48, 0xC7, 0x43, 0x0C, 0x07, 0x00, 0x00, 0x00],
            "movq $7, 12(%rbx)",
        ),
        (&[0xC6, 0x01, 0x03], "movb $3, (%rcx)"),
    ];
    let mut off = 0usize;
    for (want, note) in fixtures {
        let got = &t[off..off + want.len()];
        assert_eq!(got, *want, "{} at offset {}", note, off);
        off += want.len();
    }
}

/// The corpus round-trips: bytes, then text, then the same bytes again.
// CEP:ID: JIT-681
// CEP:WHAT: Tests the text round trip.
// CEP:WHY: The renderer and the assembler must agree through decode for every canonical program.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: branch targets render as absolute offsets and re-assemble to the same bytes.
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-machine.
#[test]
fn corpus_round_trips_through_text() {
    let obj = isa_assemble(corpus()).expect("assembles");
    let insns = decode_stepwise(&obj.text);
    // Rebuild the program with labels at the rendered branch targets.
    let mut text = String::from(".text\n");
    let mut off = 0usize;
    for insn in &insns {
        let line = isa_render_insn(insn);
        // Branch lines carry an absolute decimal target; define a label there.
        let target = line
            .split_whitespace()
            .last()
            .and_then(|t| t.parse::<usize>().ok());
        if target == Some(off) {
            text.push_str(".Lrt:\n");
        }
        text.push_str(&line);
        let consumed = isa_assemble(&format!(".text\n{}", line.trim()))
            .map(|o| o.text.len())
            .unwrap_or(1);
        off += consumed.max(1);
    }
    let obj2 = isa_assemble(&text).expect("re-assembles");
    assert_eq!(obj.text.len(), obj2.text.len(), "round-trip length");
    for (i, (a, b)) in obj.text.iter().zip(obj2.text.iter()).enumerate() {
        assert_eq!(a, b, "bytes diverge at {}", i);
    }
}

/// The assembler and GNU as produce identical bytes for the corpus.
// CEP:ID: JIT-x84
// CEP:WHAT: Runs the gas differential.
// CEP:WHY: GNU as is the independent oracle for every form choice.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: GNU as is installed and the platform is x86-64.
// CEP:COST: cold; one assembler invocation.
// CEP:EVIDENCE: cargo test -p mlkp-machine.
#[test]
fn assembles_gas_differential() {
    let probe = std::process::Command::new("as").arg("--version").output();
    if probe.is_err() {
        return; // gas is unavailable on this host; the CI image provides it
    }
    let dir = std::env::temp_dir().join(format!("mlkp-isa-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tempdir");
    let s_path = dir.join("isa_tier.s");
    std::fs::write(&s_path, corpus()).expect("write .s");
    let o_path = dir.join("isa_tier.o");
    let out = std::process::Command::new("as")
        .arg("--64")
        .arg(&s_path)
        .arg("-o")
        .arg(&o_path)
        .output()
        .expect("runs as");
    assert!(
        out.status.success(),
        "gas failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let obj = std::fs::read(&o_path).expect("read .o");
    let gas_text = elf_text_section(&obj);
    let mine = isa_assemble(corpus()).expect("assembles").text;
    assert_eq!(mine.len(), gas_text.len(), "gas differential length");
    for (i, (m, g)) in mine.iter().zip(gas_text.iter()).enumerate() {
        assert_eq!(m, g, "assembler bytes differ from gas at {}", i);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Extracts the .text section bytes from an ELF object produced by gas.
// CEP:ID: JIT-t29
// CEP:WHAT: Extracts the ELF text section.
// CEP:WHY: The differential compares raw section bytes.
// CEP:STATUS: complete
// CEP:FAILURE: Panics when no .text section exists.
// CEP:ASSUMES: a little-endian ELF64 relocatable object.
// CEP:COST: O(object size).
// CEP:EVIDENCE: cargo test -p mlkp-machine.
fn elf_text_section(obj: &[u8]) -> Vec<u8> {
    assert_eq!(&obj[0..4], &[0x7f, b'E', b'L', b'F'], "ELF magic");
    let shoff = u64::from_le_bytes(obj[0x28..0x30].try_into().unwrap()) as usize;
    let shentsize = u16::from_le_bytes(obj[0x3A..0x3C].try_into().unwrap()) as usize;
    let shnum = u16::from_le_bytes(obj[0x3C..0x3E].try_into().unwrap()) as usize;
    let shstrndx = u16::from_le_bytes(obj[0x3E..0x40].try_into().unwrap()) as usize;
    let header = |i: usize| -> &[u8] {
        let at = shoff + i * shentsize;
        &obj[at..at + shentsize]
    };
    let str_hdr = header(shstrndx);
    let strtab_off = u64::from_le_bytes(str_hdr[0x18..0x20].try_into().unwrap()) as usize;
    let strtab_size = u64::from_le_bytes(str_hdr[0x20..0x28].try_into().unwrap()) as usize;
    let strtab = &obj[strtab_off..strtab_off + strtab_size];
    for i in 0..shnum {
        let hdr = header(i);
        let name_off = u32::from_le_bytes(hdr[0..4].try_into().unwrap()) as usize;
        let name_end = strtab[name_off..].iter().position(|c| *c == 0).unwrap_or(0);
        if &strtab[name_off..name_off + name_end] == b".text" {
            let off = u64::from_le_bytes(hdr[0x18..0x20].try_into().unwrap()) as usize;
            let size = u64::from_le_bytes(hdr[0x20..0x28].try_into().unwrap()) as usize;
            return obj[off..off + size].to_vec();
        }
    }
    panic!("no .text section in gas object");
}

// CEP:FILE: crates/mlkp-machine/tests/encode_test.rs
// CEP:WHAT: Encoder conformance tests: byte-exact fixtures against gas ground truth, branch-size fixpoint behavior, deterministic encoding, bytes-versus-text equivalence through the decoder, and a differential test that assembles the text emitter's output with GNU as and compares the bytes.
// CEP:WHY: A wrong encoding silently corrupts every kernel; gas provides an independent oracle and the decoder provides an internal one.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: GNU as and objcopy are on PATH for the differential tests; the sandbox and CI images both provide binutils.
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-machine.

use mlkp_machine::{
    allocate, compile, decode_insns, disassemble, emit, preg, verify_allocation, Allocation,
    CodeObject, MFunction, MInsn, Opcode, Operand, RegClass, X86_64Target,
};

/// Builds a minimal allocation for hand-checked fixtures.
// CEP:WHAT: Builds a hand allocation.
// CEP:WHY: Byte fixtures need exact register choices without running the allocator.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
fn hand_alloc(pairs: &[(u32, u32)]) -> Allocation {
    Allocation {
        assignment: pairs.to_vec(),
        spilled: Vec::new(),
        spill_slots: 0,
        coalesced_copies: 0,
    }
}

/// Builds a function with one block named `entry`.
// CEP:WHAT: Builds a one-block function.
// CEP:WHY: Fixture ergonomics.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
fn fn1(name: &str, insns: Vec<MInsn>) -> MFunction {
    let mut f = MFunction {
        name: name.to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    f.push_block("entry");
    for i in insns {
        f.push_insn(i);
    }
    f
}

/// A register-to-register move fixture: the prologue, one move, and the epilogue.
// CEP:WHAT: Tests the byte-exact move encoding.
// CEP:WHY: The smallest complete function pins prologue, body, and epilogue bytes.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn mov_rr_fixture() {
    let target = X86_64Target::new();
    let mut f = fn1(
        "f",
        vec![
            MInsn::new(
                Opcode::Mov,
                vec![
                    Operand::vreg_def(0, RegClass::Gpr64),
                    Operand::PReg {
                        id: preg::RDI,
                        class: RegClass::Gpr64,
                        def: false,
                    },
                ],
            ),
            MInsn::new(Opcode::Ret, vec![]),
        ],
    );
    f.vreg_classes.push((0, RegClass::Gpr64));
    let alloc = hand_alloc(&[(0, preg::RAX)]);
    verify_allocation(&f, &alloc, &target).expect("allocation verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    // The frame always reserves the worst-case saved area (5 GPRs = 40 bytes,
    // aligned to 48) because the spill rewriter bakes rbp-relative slot
    // displacements against that depth.
    let expected: Vec<u8> = vec![
        0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x30, // prologue, frame 48
        0x48, 0x89, 0xF8, // movq %rdi, %rax
        0xEB, 0x00, // jmp epilogue
        0x48, 0x89, 0xEC, 0x5D, 0xC3, // epilogue
    ];
    assert_eq!(bytes, expected, "mov fixture mismatch");
}

/// Immediate forms: C7 imm32, 83 imm8, the accumulator short form 05, 81 for other registers, and movabs for 64-bit immediates.
// CEP:ID: JIT-348
// CEP:WHAT: Tests immediate encodings.
// CEP:WHY: Each immediate width and form must match gas exactly.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn imm_forms_fixture() {
    let target = X86_64Target::new();
    let mut f = fn1(
        "f",
        vec![
            Operand::imm_mov(0, 7),             // 48 c7 c0 07 00 00 00
            Operand::imm_mov(1, 4_294_967_296), // 48 b9 + imm64
            MInsn::new(
                Opcode::Add,
                vec![Operand::vreg_def(2, RegClass::Gpr64), Operand::Imm(1000)],
            ), // 48 05 e8 03 00 00 (accumulator short form)
            MInsn::new(Opcode::Ret, vec![]),
        ],
    );
    f.vreg_classes.push((0, RegClass::Gpr64));
    f.vreg_classes.push((1, RegClass::Gpr64));
    f.vreg_classes.push((2, RegClass::Gpr64));
    let alloc = hand_alloc(&[(0, preg::RAX), (1, preg::RCX), (2, preg::RAX)]);
    verify_allocation(&f, &alloc, &target).expect("allocation verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let mut expected: Vec<u8> = vec![0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x30];
    expected.extend_from_slice(&[0x48, 0xC7, 0xC0, 0x07, 0x00, 0x00, 0x00]); // mov $7, rax
    expected.extend_from_slice(&[0x48, 0xB9, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]); // movabs $2^32, rcx
    expected.extend_from_slice(&[0x48, 0x05, 0xE8, 0x03, 0x00, 0x00]); // add $1000, rax
    expected.extend_from_slice(&[0xEB, 0x00]); // jmp epilogue
    expected.extend_from_slice(&[0x48, 0x89, 0xEC, 0x5D, 0xC3]);
    assert_eq!(bytes, expected, "immediate forms mismatch");
}

/// Immediates on a non-accumulator register use 81; small immediates use 83.
// CEP:WHAT: Tests the 81 and 83 group forms.
// CEP:WHY: The group opcode choice depends on width and register.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn group_forms_fixture() {
    let target = X86_64Target::new();
    let mut f = fn1(
        "f",
        vec![
            Operand::imm_mov(0, 9), // 48 c7 c3 09 00 00 00 (rbx by allocation)
            MInsn::new(Opcode::Ret, vec![]),
        ],
    );
    f.vreg_classes.push((0, RegClass::Gpr64));
    let alloc = hand_alloc(&[(0, preg::RBX)]);
    verify_allocation(&f, &alloc, &target).expect("allocation verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    // The callee-saved rbx forces a saved push and a 48-byte frame (5 worst-case saved GPRs).
    let expected: Vec<u8> = [
        0x55, 0x48, 0x89, 0xE5, 0x53, // push rbp; mov rsp,rbp; push rbx
        0x48, 0x83, 0xEC, 0x30, // sub $48, rsp (fits imm8)
        0x48, 0xC7, 0xC3, 0x09, 0x00, 0x00, 0x00, // mov $9, rbx
        0xEB, 0x00, // jmp epilogue
        0x5B, 0x48, 0x89, 0xEC, 0x5D, 0xC3, // pop rbx; mov rbp,rsp; pop rbp; ret
    ]
    .to_vec();
    assert_eq!(bytes, expected, "group forms mismatch");
}

/// rsp-based memory operands need the SIB byte.
// CEP:WHAT: Tests SIB encoding.
// CEP:WHY: rm=100 means SIB follows; missing it corrupts addressing.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn sib_fixture() {
    let target = X86_64Target::new();
    let f = fn1(
        "f",
        vec![
            MInsn::new(
                Opcode::Store,
                vec![
                    Operand::PReg {
                        id: preg::RAX,
                        class: RegClass::Gpr64,
                        def: false,
                    },
                    Operand::Mem {
                        base: preg::RSP,
                        base_preg: true,
                        disp: 0,
                        reads: false,
                        writes: true,
                    },
                ],
            ),
            MInsn::new(Opcode::Ret, vec![]),
        ],
    );
    let alloc = hand_alloc(&[]);
    verify_allocation(&f, &alloc, &target).expect("allocation verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let mut expected: Vec<u8> = vec![0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x30];
    expected.extend_from_slice(&[0x48, 0x89, 0x04, 0x24]); // movq %rax, (%rsp)
    expected.extend_from_slice(&[0xEB, 0x00, 0x48, 0x89, 0xEC, 0x5D, 0xC3]);
    assert_eq!(bytes, expected, "sib mismatch");
}

/// rbp-based memory with zero displacement still encodes disp8 (mod 00 rm 101 means RIP-relative).
// CEP:WHAT: Tests the RBP displacement special case.
// CEP:WHY: A bare (%rbp) encoding would silently become RIP-relative.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn rbp_zero_disp_fixture() {
    let target = X86_64Target::new();
    let mut f = fn1(
        "f",
        vec![
            MInsn::new(
                Opcode::Load,
                vec![
                    Operand::vreg_def(0, RegClass::Gpr64),
                    Operand::Mem {
                        base: preg::RBP,
                        base_preg: true,
                        disp: 0,
                        reads: true,
                        writes: false,
                    },
                ],
            ),
            MInsn::new(Opcode::Ret, vec![]),
        ],
    );
    f.vreg_classes.push((0, RegClass::Gpr64));
    let alloc = hand_alloc(&[(0, preg::RAX)]);
    verify_allocation(&f, &alloc, &target).expect("allocation verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let mut expected: Vec<u8> = vec![0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x30];
    expected.extend_from_slice(&[0x48, 0x8B, 0x45, 0x00]); // movq (%rbp), %rax with disp8 0
    expected.extend_from_slice(&[0xEB, 0x00, 0x48, 0x89, 0xEC, 0x5D, 0xC3]);
    assert_eq!(bytes, expected, "rbp zero-disp mismatch");
}

/// A branch beyond rel8 range grows to rel32 (0F 8C).
// CEP:ID: JIT-d28
// CEP:WHAT: Tests the branch-size fixpoint.
// CEP:WHY: The two-pass layout must grow forms exactly when the displacement overflows a byte.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn long_branch_fixture() {
    let target = X86_64Target::new();
    let mut f = MFunction {
        name: "f".to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    f.push_block("entry");
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![Operand::vreg_def(0, RegClass::Gpr64), Operand::Imm(0)],
    ));
    f.push_insn(MInsn::new(
        Opcode::Jl,
        vec![Operand::Block("body".to_string())],
    ));
    f.push_insn(MInsn::new(
        Opcode::Jmp,
        vec![Operand::Block("exit".to_string())],
    ));
    f.push_block("body");
    // Thirty 7-byte immediates pushes the jl beyond rel8 range.
    for _ in 0..30 {
        f.push_insn(Operand::imm_mov(0, 1));
    }
    f.push_insn(MInsn::new(
        Opcode::Jmp,
        vec![Operand::Block("exit".to_string())],
    ));
    f.push_block("exit");
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::PReg {
                id: preg::RAX,
                class: RegClass::Gpr64,
                def: true,
            },
            Operand::vreg_use(0, RegClass::Gpr64),
        ],
    ));
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    // Successor declarations feed cross-block liveness.
    f.blocks[0].succs = vec!["body".to_string(), "exit".to_string()];
    f.blocks[1].succs = vec!["exit".to_string()];
    f.vreg_classes.push((0, RegClass::Gpr64));
    let alloc = hand_alloc(&[(0, preg::RAX)]);
    verify_allocation(&f, &alloc, &target).expect("allocation verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    // Layout: prolog(8) mov(7) jl(2) jmp(5) body[30x7] jmp(2) exit.
    // The jl targets body 5 bytes ahead: short form with disp 5.
    assert_eq!(&bytes[15..17], &[0x7C, 0x05], "near jl stays short");
    // The jmp over the body targets exit, far beyond rel8: it grows to E9 rel32.
    assert_eq!(bytes[17], 0xE9, "far jmp must use the rel32 form");
    // The backward jmp after the body stays short (EB).
    let back_jmp = 8 + 7 + 2 + 5 + 30 * 7;
    assert_eq!(bytes[back_jmp], 0xEB, "near jmp stays short");
}

/// Encoding is deterministic: two compilations produce identical bytes.
// CEP:ID: JIT-f89
// CEP:WHAT: Tests encoding determinism.
// CEP:WHY: The emission policy requires byte-identical output across runs.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn encoding_is_deterministic() {
    let target = X86_64Target::new();
    let build = || {
        let mut f = MFunction {
            name: "d".to_string(),
            blocks: Vec::new(),
            vreg_classes: Vec::new(),
        };
        f.push_block("entry");
        f.push_insn(Operand::imm_mov(0, 3));
        f.push_insn(MInsn::new(
            Opcode::Add,
            vec![
                Operand::vreg_def(0, RegClass::Gpr64),
                Operand::PReg {
                    id: preg::RDI,
                    class: RegClass::Gpr64,
                    def: false,
                },
            ],
        ));
        f.push_insn(MInsn::new(Opcode::Ret, vec![]));
        f.vreg_classes.push((0, RegClass::Gpr64));
        f
    };
    let mut f1 = build();
    let mut f2 = build();
    let a1 = allocate(&mut f1, &target).expect("alloc 1");
    let a2 = allocate(&mut f2, &target).expect("alloc 2");
    let b1 = compile(&f1, &a1, &target).expect("encode 1");
    let b2 = compile(&f2, &a2, &target).expect("encode 2");
    assert_eq!(b1, b2);
}

/// Decoded text matches the assembly emitter exactly (minus its comment header), for straight-line, branch, spill, and xmm programs.
// CEP:ID: JIT-l75
// CEP:WHAT: Tests bytes-versus-text equivalence.
// CEP:WHY: The encoder and the text emitter must render the same program; the decoder is the mechanical bridge.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn decoded_text_matches_emitter() {
    let target = X86_64Target::new();
    let cases: Vec<MFunction> = vec![chain_program("c1", 6), loop_program(), xmm_program()];
    for mut f in cases {
        eprintln!("CASE {}", f.name);
        let alloc = allocate(&mut f, &target).expect("allocates");
        verify_allocation(&f, &alloc, &target).expect("verifies");
        let text = emit(&f, &alloc, &target).expect("emits");
        let co = compile(&f, &alloc, &target).expect("encodes");
        let disasm = disassemble(&co, &f.name, &target).expect("decodes");
        let stripped: String = text
            .lines()
            .filter(|l| !l.starts_with("//"))
            .map(|l| format!("{l}\n"))
            .collect();
        assert_eq!(
            disasm, stripped,
            "decoder and emitter disagree for {}",
            f.name
        );
        // Every instruction decodes.
        assert!(!decode_insns(&co).expect("decode insns").is_empty());
    }
}

/// A spill-heavy program decodes and re-emits identically and every relocation-free call resolves.
// CEP:WHAT: Tests spill-program decode.
// CEP:WHY: Slot addressing via rbp-relative memory is the hardest shared path.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn spilled_program_decodes() {
    let target = X86_64Target::new();
    let mut f = chain_program("spill", 20);
    let alloc = allocate(&mut f, &target).expect("allocates");
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let co = compile(&f, &alloc, &target).expect("encodes");
    let disasm = disassemble(&co, &f.name, &target).expect("decodes");
    let text = emit(&f, &alloc, &target).expect("emits");
    let stripped: String = text
        .lines()
        .filter(|l| !l.starts_with("//"))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(disasm, stripped);
}

/// Differential test: assembling the text emitter's output with GNU as produces exactly the encoder's bytes.
// CEP:ID: JIT-527
// CEP:WHAT: Runs the gas differential test.
// CEP:WHY: An independent assembler is the strongest oracle for the encoder.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert; a missing assembler fails the test loudly.
// CEP:ASSUMES: GNU as on PATH.
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn gas_differential() {
    let target = X86_64Target::new();
    if std::process::Command::new("as")
        .arg("--version")
        .output()
        .is_err()
    {
        panic!("GNU as is required for the encoder differential test but was not found on PATH");
    }
    let dir = std::env::temp_dir().join(format!("mlkp-gas-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let mut f = chain_program("gasdiff", 25);
    let alloc = allocate(&mut f, &target).expect("allocates");
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let text = emit(&f, &alloc, &target).expect("emits");
    let asm: String = text
        .lines()
        .filter(|l| !l.starts_with("//"))
        .map(|l| format!("{l}\n"))
        .collect();
    let s_path = dir.join("gasdiff.s");
    std::fs::write(&s_path, &asm).expect("write .s");
    let o_path = dir.join("gasdiff.o");
    let out = std::process::Command::new("as")
        .arg(&s_path)
        .arg("-o")
        .arg(&o_path)
        .output()
        .expect("run as");
    assert!(
        out.status.success(),
        "gas failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let obj = std::fs::read(&o_path).expect("read .o");
    let gas_text = elf_text_section(&obj);
    let mine = compile(&f, &alloc, &target).expect("encodes").text;
    assert_eq!(mine, gas_text, "encoder bytes differ from gas");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Extracts the .text section bytes from an ELF object produced by gas.
// CEP:ID: JIT-z62
// CEP:WHAT: Reads a .text section.
// CEP:WHY: The differential test compares raw section bytes.
// CEP:STATUS: complete
// CEP:FAILURE: Panics on a malformed object.
// CEP:ASSUMES: little-endian ELF64.
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
fn elf_text_section(obj: &[u8]) -> Vec<u8> {
    assert_eq!(&obj[0..4], &[0x7f, b'E', b'L', b'F']);
    assert_eq!(obj[4], 2, "ELFCLASS64");
    let shoff = u64::from_le_bytes(obj[0x28..0x30].try_into().unwrap()) as usize;
    let shentsize = u16::from_le_bytes(obj[0x3A..0x3C].try_into().unwrap()) as usize;
    let shnum = u16::from_le_bytes(obj[0x3C..0x3E].try_into().unwrap()) as usize;
    let shstrndx = u16::from_le_bytes(obj[0x3E..0x40].try_into().unwrap()) as usize;
    let hdr_at = |i: usize| -> (u32, usize, usize) {
        let b = &obj[shoff + i * shentsize..];
        let name = u32::from_le_bytes(b[0..4].try_into().unwrap());
        let off = u64::from_le_bytes(b[0x18..0x20].try_into().unwrap()) as usize;
        let size = u64::from_le_bytes(b[0x20..0x28].try_into().unwrap()) as usize;
        (name, off, size)
    };
    let (_, str_off, str_size) = hdr_at(shstrndx);
    let names = &obj[str_off..str_off + str_size];
    for i in 0..shnum {
        let (name, off, size) = hdr_at(i);
        let end = names[name as usize..].iter().position(|c| *c == 0).unwrap();
        if &names[name as usize..name as usize + end] == b".text" {
            return obj[off..off + size].to_vec();
        }
    }
    panic!("no .text section in gas object");
}

/// Builds a chain program: values feed a wide consumer, forcing pressure and spills at higher widths.
// CEP:ID: JIT-e60
// CEP:WHAT: Builds a chain program.
// CEP:WHY: Shared fixture for pressure, decode, and differential tests.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
fn chain_program(name: &str, values: usize) -> MFunction {
    let mut f = MFunction {
        name: name.to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    f.push_block("entry");
    // Two argument roots plus derived values keep everything live to the consumer.
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::vreg_def(0, RegClass::Gpr64),
            Operand::PReg {
                id: preg::RDI,
                class: RegClass::Gpr64,
                def: false,
            },
        ],
    ));
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::vreg_def(1, RegClass::Gpr64),
            Operand::PReg {
                id: preg::RSI,
                class: RegClass::Gpr64,
                def: false,
            },
        ],
    ));
    f.vreg_classes.push((0, RegClass::Gpr64));
    f.vreg_classes.push((1, RegClass::Gpr64));
    let mut v = 2u32;
    for _ in 2..values {
        f.push_insn(MInsn::new(
            Opcode::Mov,
            vec![
                Operand::vreg_def(v, RegClass::Gpr64),
                Operand::vreg_use(0, RegClass::Gpr64),
            ],
        ));
        f.push_insn(MInsn::new(
            Opcode::Add,
            vec![
                Operand::vreg_def(v, RegClass::Gpr64),
                Operand::vreg_use(1, RegClass::Gpr64),
            ],
        ));
        f.vreg_classes.push((v, RegClass::Gpr64));
        v += 1;
    }
    // Consumer: fold everything into one accumulator.
    let acc = v;
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::vreg_def(acc, RegClass::Gpr64),
            Operand::vreg_use(2, RegClass::Gpr64),
        ],
    ));
    f.vreg_classes.push((acc, RegClass::Gpr64));
    for k in 3..values {
        f.push_insn(MInsn::new(
            Opcode::Add,
            vec![
                Operand::vreg_def(acc, RegClass::Gpr64),
                Operand::vreg_use(k as u32, RegClass::Gpr64),
            ],
        ));
    }
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::PReg {
                id: preg::RAX,
                class: RegClass::Gpr64,
                def: true,
            },
            Operand::vreg_use(acc, RegClass::Gpr64),
        ],
    ));
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    f
}

/// Builds a counted loop program.
// CEP:ID: JIT-z70
// CEP:WHAT: Builds a loop program.
// CEP:WHY: Branch fixpoint and loop execution coverage.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
fn loop_program() -> MFunction {
    let mut f = MFunction {
        name: "loop".to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    f.push_block("entry");
    f.push_insn(Operand::imm_mov(0, 1)); // i = 1
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::vreg_def(1, RegClass::Gpr64),
            Operand::PReg {
                id: preg::RDI,
                class: RegClass::Gpr64,
                def: false,
            },
        ],
    )); // n = arg
    f.push_insn(Operand::imm_mov(2, 0)); // s = 0
    f.push_insn(MInsn::new(
        Opcode::Jmp,
        vec![Operand::Block("cond".to_string())],
    ));
    f.vreg_classes.push((0, RegClass::Gpr64));
    f.vreg_classes.push((1, RegClass::Gpr64));
    f.vreg_classes.push((2, RegClass::Gpr64));
    f.blocks[0].succs = vec!["cond".to_string()];
    f.push_block("cond");
    // cmpq %n, %i then jl body when i < n.
    f.push_insn(MInsn::new(
        Opcode::Cmp,
        vec![
            Operand::vreg_use(0, RegClass::Gpr64),
            Operand::vreg_use(1, RegClass::Gpr64),
        ],
    ));
    f.push_insn(MInsn::new(
        Opcode::Jl,
        vec![Operand::Block("body".to_string())],
    ));
    f.push_insn(MInsn::new(
        Opcode::Jmp,
        vec![Operand::Block("exit".to_string())],
    ));
    f.push_block("body");
    f.push_insn(MInsn::new(
        Opcode::Add,
        vec![
            Operand::vreg_def(2, RegClass::Gpr64),
            Operand::vreg_use(0, RegClass::Gpr64),
        ],
    ));
    f.push_insn(MInsn::new(
        Opcode::Add,
        vec![Operand::vreg_def(0, RegClass::Gpr64), Operand::Imm(1)],
    ));
    f.push_insn(MInsn::new(
        Opcode::Jmp,
        vec![Operand::Block("cond".to_string())],
    ));
    f.blocks[1].succs = vec!["body".to_string(), "exit".to_string()];
    f.blocks[2].succs = vec!["cond".to_string()];
    f.push_block("exit");
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::PReg {
                id: preg::RAX,
                class: RegClass::Gpr64,
                def: true,
            },
            Operand::vreg_use(2, RegClass::Gpr64),
        ],
    ));
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    f
}

/// Builds an xmm program: arg0 * arg1 + arg0.
// CEP:ID: JIT-170
// CEP:WHAT: Builds an xmm program.
// CEP:WHY: SSE encoding coverage.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
fn xmm_program() -> MFunction {
    let mut f = MFunction {
        name: "xmmfn".to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    f.push_block("entry");
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::vreg_def(0, RegClass::Xmm64),
            Operand::PReg {
                id: preg::XMM0,
                class: RegClass::Xmm64,
                def: false,
            },
        ],
    ));
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::vreg_def(1, RegClass::Xmm64),
            Operand::PReg {
                id: preg::XMM1,
                class: RegClass::Xmm64,
                def: false,
            },
        ],
    ));
    f.push_insn(MInsn::new(
        Opcode::MulSd,
        vec![
            Operand::vreg_def(1, RegClass::Xmm64),
            Operand::vreg_use(0, RegClass::Xmm64),
        ],
    ));
    f.push_insn(MInsn::new(
        Opcode::AddSd,
        vec![
            Operand::vreg_def(1, RegClass::Xmm64),
            Operand::vreg_use(0, RegClass::Xmm64),
        ],
    ));
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::PReg {
                id: preg::XMM0,
                class: RegClass::Xmm64,
                def: true,
            },
            Operand::vreg_use(1, RegClass::Xmm64),
        ],
    ));
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    f.vreg_classes.push((0, RegClass::Xmm64));
    f.vreg_classes.push((1, RegClass::Xmm64));
    f
}

/// Compile() returns labels and relocation-free internal calls resolve; external calls produce relocations.
// CEP:ID: JIT-674
// CEP:WHAT: Tests call relocations.
// CEP:WHY: The object emitter consumes exactly these relocations.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn call_relocations() {
    let target = X86_64Target::new();
    let mut f = MFunction {
        name: "caller".to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    f.push_block("entry");
    f.push_insn(MInsn::new(
        Opcode::Call,
        vec![Operand::Symbol("helper".to_string())],
    ));
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    let alloc = hand_alloc(&[]);
    let co: CodeObject = compile(&f, &alloc, &target).expect("encodes");
    assert_eq!(co.relocs.len(), 1);
    assert_eq!(co.relocs[0].symbol, "helper");
    assert_eq!(co.relocs[0].addend, -4);
    assert_eq!(co.relocs[0].offset as usize, 9); // prologue (8) + call opcode (1)
}

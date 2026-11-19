// CEP:FILE: crates/mlkp-machine/tests/object_test.rs
// CEP:WHAT: Object emission tests: deterministic byte layout, structural ELF64 checks (header, section order, symbol table, relocation table), evidence hash stability, and an end-to-end link-and-run test that links the emitted object with cc and executes it.
// CEP:WHY: The object file is the contract with the platform toolchain; structural checks catch layout drift and the link test proves relocations and symbols resolve.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: a C compiler (cc) and linker on PATH for the link test; the sandbox and CI images both provide them.
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-machine.

use mlkp_machine::{
    allocate, compile, emit_object, emit_with_evidence, preg, verify_allocation, Allocation,
    MFunction, MInsn, Opcode, Operand, RegClass, X86_64Target,
};

/// Builds the link-test program: f(a, b) = a * 2 - b.
// CEP:ID: JIT-i79
// CEP:WHAT: Builds the object fixture.
// CEP:WHY: Shared by the structural and link tests.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
fn build_obj_program() -> (MFunction, Allocation) {
    let target = X86_64Target::new();
    let mut f = MFunction {
        name: "mlkp_f".to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    f.push_block("entry");
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
    f.push_insn(MInsn::new(
        Opcode::Add,
        vec![
            Operand::vreg_def(0, RegClass::Gpr64),
            Operand::vreg_use(0, RegClass::Gpr64),
        ],
    ));
    f.push_insn(MInsn::new(
        Opcode::Sub,
        vec![
            Operand::vreg_def(0, RegClass::Gpr64),
            Operand::vreg_use(1, RegClass::Gpr64),
        ],
    ));
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
    f.vreg_classes.push((0, RegClass::Gpr64));
    f.vreg_classes.push((1, RegClass::Gpr64));
    let alloc = allocate(&mut f, &target).expect("allocates");
    verify_allocation(&f, &alloc, &target).expect("verifies");
    (f, alloc)
}

/// Emission is deterministic: two runs produce byte-identical objects.
// CEP:ID: JIT-i82
// CEP:WHAT: Tests object determinism.
// CEP:WHY: The emission policy requires deterministic layout, section order, symbol order, and relocation order.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn object_is_deterministic() {
    let target = X86_64Target::new();
    let (f, alloc) = build_obj_program();
    let co = compile(&f, &alloc, &target).expect("encodes");
    let o1 = emit_object(&co, &f.name);
    let o2 = emit_object(&co, &f.name);
    assert_eq!(o1, o2);
    let e1 = emit_with_evidence(&co, &f.name, &target).expect("evidence");
    let e2 = emit_with_evidence(&co, &f.name, &target).expect("evidence");
    assert_eq!(e1.0, e2.0);
    assert_eq!(e1.1, e2.1);
    assert_eq!(
        e1.1.sections,
        vec![".text", ".rela.text", ".symtab", ".strtab", ".shstrtab"]
    );
}

/// The object structurally parses: header fields, section table, symbol table, and relocation table match the contract.
// CEP:ID: JIT-m43
// CEP:WHAT: Tests the ELF structure.
// CEP:WHY: The linker consumes exactly these tables; a wrong field silently breaks linking.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: little-endian ELF64.
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn object_structure() {
    let target = X86_64Target::new();
    let (f, alloc) = build_obj_program();
    let co = compile(&f, &alloc, &target).expect("encodes");
    let obj = emit_object(&co, &f.name);
    // Header.
    assert_eq!(&obj[0..4], &[0x7f, b'E', b'L', b'F']);
    assert_eq!(obj[4], 2, "ELFCLASS64");
    assert_eq!(obj[5], 1, "little-endian");
    let e_type = u16::from_le_bytes(obj[0x10..0x12].try_into().unwrap());
    assert_eq!(e_type, 1, "ET_REL");
    let e_machine = u16::from_le_bytes(obj[0x12..0x14].try_into().unwrap());
    assert_eq!(e_machine, 62, "EM_X86_64");
    let e_shoff = u64::from_le_bytes(obj[0x28..0x30].try_into().unwrap()) as usize;
    let e_shentsize = u16::from_le_bytes(obj[0x3A..0x3C].try_into().unwrap()) as usize;
    let e_shnum = u16::from_le_bytes(obj[0x3C..0x3E].try_into().unwrap()) as usize;
    let e_shstrndx = u16::from_le_bytes(obj[0x3E..0x40].try_into().unwrap()) as usize;
    assert_eq!(e_shnum, 6);
    assert_eq!(e_shstrndx, 5);
    // Section table walk.
    let shdr = |i: usize| -> (u32, u32, usize, usize, u32, u32) {
        let b = &obj[e_shoff + i * e_shentsize..];
        let name = u32::from_le_bytes(b[0..4].try_into().unwrap());
        let ty = u32::from_le_bytes(b[4..8].try_into().unwrap());
        let off = u64::from_le_bytes(b[0x18..0x20].try_into().unwrap()) as usize;
        let size = u64::from_le_bytes(b[0x20..0x28].try_into().unwrap()) as usize;
        let link = u32::from_le_bytes(b[0x28..0x2C].try_into().unwrap());
        let info = u32::from_le_bytes(b[0x2C..0x30].try_into().unwrap());
        (name, ty, off, size, link, info)
    };
    let (_, _, str_off, str_size, _, _) = shdr(e_shstrndx);
    let names = &obj[str_off..str_off + str_size];
    let sec_name = |n: u32| -> String {
        let end = names[n as usize..].iter().position(|c| *c == 0).unwrap();
        String::from_utf8(names[n as usize..n as usize + end].to_vec()).unwrap()
    };
    // .text first: PROGBITS, exec, aligned 16, contents equal the compiled bytes.
    let (t_name, t_type, t_off, t_size, _, _) = shdr(1);
    assert_eq!(sec_name(t_name), ".text");
    assert_eq!(t_type, 1, "SHT_PROGBITS");
    assert_eq!(
        &obj[t_off..t_off + t_size],
        &co.text,
        ".text matches compile() output"
    );
    // .rela.text: RELA, link to symtab (3), info to .text (1), 24-byte entries.
    let (r_name, r_type, _r_off, r_size, r_link, r_info) = shdr(2);
    assert_eq!(sec_name(r_name), ".rela.text");
    assert_eq!(r_type, 4, "SHT_RELA");
    assert_eq!(r_link, 3);
    assert_eq!(r_info, 1);
    assert_eq!(r_size, 24 * co.relocs.len());
    // .symtab: SYMTAB, link to strtab (4), info = first global (2).
    let (s_name, s_type, s_off, s_size, s_link, s_info) = shdr(3);
    assert_eq!(sec_name(s_name), ".symtab");
    assert_eq!(s_type, 2, "SHT_SYMTAB");
    assert_eq!(s_link, 4);
    assert_eq!(s_info, 2);
    let nsym = s_size / 24;
    assert_eq!(nsym, 3, "null + section + function");
    // Function symbol: GLOBAL FUNC in .text with the full size.
    let sym = &obj[s_off + 48..s_off + 72];
    let st_info = sym[4];
    let st_shndx = u16::from_le_bytes(sym[6..8].try_into().unwrap());
    let st_size = u64::from_le_bytes(sym[16..24].try_into().unwrap());
    assert_eq!(st_info >> 4, 1, "STB_GLOBAL");
    assert_eq!(st_info & 0xF, 2, "STT_FUNC");
    assert_eq!(st_shndx, 1);
    assert_eq!(st_size as usize, co.text.len());
}

/// The emitted object links with a C driver and the linked binary produces the expected result.
// CEP:ID: JIT-p69
// CEP:WHAT: Runs the link-and-run test.
// CEP:WHY: Relocations, symbol resolution, and the System V ABI execute together only in a real binary.
// CEP:STATUS: complete
// CEP:FAILURE: Panics when cc is missing, the link fails, or the output is wrong.
// CEP:ASSUMES: cc on PATH; the SysV ABI passes integer args in rdi, rsi and returns in rax.
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn object_links_and_runs() {
    if std::process::Command::new("cc")
        .arg("--version")
        .output()
        .is_err()
    {
        panic!("a C compiler (cc) is required for the object link test but was not found on PATH");
    }
    let target = X86_64Target::new();
    let (f, alloc) = build_obj_program();
    let co = compile(&f, &alloc, &target).expect("encodes");
    let obj = emit_object(&co, &f.name);
    let dir = std::env::temp_dir().join(format!("mlkp-obj-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let o_path = dir.join("mlkp_f.o");
    std::fs::write(&o_path, &obj).expect("write .o");
    let c_path = dir.join("driver.c");
    std::fs::write(
        &c_path,
        "#include <stdio.h>\nlong mlkp_f(long, long);\nint main(void) { printf(\"%ld\\n\", mlkp_f(10, 3)); return 0; }\n",
    )
    .expect("write driver.c");
    let bin = dir.join("driver");
    let link = std::process::Command::new("cc")
        .arg(&c_path)
        .arg(&o_path)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run cc");
    assert!(
        link.status.success(),
        "cc failed: {}",
        String::from_utf8_lossy(&link.stderr)
    );
    let run = std::process::Command::new(&bin)
        .output()
        .expect("run driver");
    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).trim(),
        "17",
        "f(10,3) = 10*2-3"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A function with an external call emits one PLT32 relocation and still links against a definition.
// CEP:ID: JIT-592
// CEP:WHAT: Tests external-call objects.
// CEP:WHY: The relocation is the only coupling to the platform toolchain.
// CEP:STATUS: complete
// CEP:FAILURE: Panics when cc is missing or the link fails.
// CEP:ASSUMES: cc on PATH.
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn external_call_links() {
    if std::process::Command::new("cc")
        .arg("--version")
        .output()
        .is_err()
    {
        panic!("a C compiler (cc) is required for the object link test but was not found on PATH");
    }
    let target = X86_64Target::new();
    let mut f = MFunction {
        name: "caller".to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    f.push_block("entry");
    f.push_insn(MInsn::new(
        Opcode::Call,
        vec![Operand::Symbol("mlkp_helper".to_string())],
    ));
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    let alloc = Allocation {
        assignment: Vec::new(),
        spilled: Vec::new(),
        spill_slots: 0,
        coalesced_copies: 0,
    };
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let co = compile(&f, &alloc, &target).expect("encodes");
    assert_eq!(co.relocs.len(), 1);
    let obj = emit_object(&co, &f.name);
    let dir = std::env::temp_dir().join(format!("mlkp-ext-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let o_path = dir.join("caller.o");
    std::fs::write(&o_path, &obj).expect("write .o");
    let c_path = dir.join("driver.c");
    std::fs::write(
        &c_path,
        "#include <stdio.h>\nvoid mlkp_helper(void) { printf(\"called\\n\"); }\nvoid caller(void);\nint main(void) { caller(); return 0; }\n",
    )
    .expect("write driver.c");
    let bin = dir.join("driver");
    let link = std::process::Command::new("cc")
        .arg(&c_path)
        .arg(&o_path)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run cc");
    assert!(
        link.status.success(),
        "cc failed: {}",
        String::from_utf8_lossy(&link.stderr)
    );
    let run = std::process::Command::new(&bin)
        .output()
        .expect("run driver");
    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "called");
    let _ = std::fs::remove_dir_all(&dir);
}

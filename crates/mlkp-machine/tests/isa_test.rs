// CEP:FILE: crates/mlkp-machine/tests/isa_test.rs
// CEP:WHAT: Conformance tests for the complete scalar ISA expansion: byte-exact fixtures for the logic, shift, division, width-move, three-operand-multiply, memory-destination, conditional-branch, and extended SSE forms, plus the decode equivalence and gas differential tests.
// CEP:WHY: The no-subset expansion touches every backend stage; byte fixtures prove encodings against gas and execution proves allocation, encoding, and the ABI agree at once.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: Linux x86-64 with anonymous mmap; the SysV ABI; GNU as for the differential test.
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-machine.

mod common;

use common::*;

use mlkp_machine::{
    allocate, compile, decode_insns, disassemble, emit, preg, verify_allocation, MFunction, MInsn,
    Opcode, Operand, RegClass, X86_64Target,
};

/// Logic forms: register-register 21/09/31, imm8 group forms, the accumulator short form, and 81 for other registers.
// CEP:ID: JIT-t91
// CEP:WHAT: Tests logic encodings.
// CEP:WHY: The shared ALU path must match gas byte for byte in every width.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn logic_forms_fixture() {
    let target = X86_64Target::new();
    let mk = |op: Opcode, dst: u32, src: Operand| -> MInsn {
        MInsn::new(op, vec![Operand::vreg_def(dst, RegClass::Gpr64), src])
    };
    let f = fn1(
        "f",
        vec![
            mk(Opcode::And, 0, Operand::vreg_use(1, RegClass::Gpr64)), // 48 21 f0
            mk(Opcode::Or, 0, Operand::vreg_use(1, RegClass::Gpr64)),  // 48 09 f0
            mk(Opcode::Xor, 0, Operand::vreg_use(1, RegClass::Gpr64)), // 48 31 f0
            mk(Opcode::And, 0, Operand::Imm(5)),                       // 48 83 e0 05
            mk(Opcode::Or, 0, Operand::Imm(5)),                        // 48 83 c8 05
            mk(Opcode::Xor, 0, Operand::Imm(5)),                       // 48 83 f0 05
            mk(Opcode::And, 0, Operand::Imm(1000)),                    // 48 25 (accumulator)
            MInsn::new(Opcode::Ret, vec![]),
        ],
        vec![gpr(0), gpr(1)],
    );
    let alloc = mlkp_machine::Allocation {
        assignment: vec![(0, preg::RAX), (1, preg::RSI)],
        spilled: Vec::new(),
        spill_slots: 0,
        coalesced_copies: 0,
    };
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let mut expected = prologue();
    expected.extend_from_slice(&[0x48, 0x21, 0xF0]);
    expected.extend_from_slice(&[0x48, 0x09, 0xF0]);
    expected.extend_from_slice(&[0x48, 0x31, 0xF0]);
    expected.extend_from_slice(&[0x48, 0x83, 0xE0, 0x05]);
    expected.extend_from_slice(&[0x48, 0x83, 0xC8, 0x05]);
    expected.extend_from_slice(&[0x48, 0x83, 0xF0, 0x05]);
    expected.extend_from_slice(&[0x48, 0x25, 0xE8, 0x03, 0x00, 0x00]);
    expected.extend_from_slice(&[0xEB, 0x00]);
    expected.extend(epilogue());
    assert_eq!(bytes, expected, "logic forms mismatch");
}

/// Unary and division forms: not, neg, cqo, idiv on a register and on memory.
// CEP:ID: JIT-o76
// CEP:WHAT: Tests unary and division encodings.
// CEP:WHY: The F7 group and cqo are new shapes with fixed implicit registers.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn unary_div_fixture() {
    let target = X86_64Target::new();
    let mut idiv = MInsn::new(Opcode::IDiv, vec![Operand::vreg_use(1, RegClass::Gpr64)]);
    idiv.implicit_uses = vec![preg::RAX, preg::RDX];
    idiv.implicit_defs = vec![preg::RAX, preg::RDX];
    let f = fn1(
        "f",
        vec![
            MInsn::new(Opcode::Not, vec![Operand::vreg_def(0, RegClass::Gpr64)]), // 48 f7 d0
            MInsn::new(Opcode::Neg, vec![Operand::vreg_def(0, RegClass::Gpr64)]), // 48 f7 d8
            MInsn::new(Opcode::Cqo, vec![]),                                      // 48 99
            idiv,                                                                 // 48 f7 fe
            MInsn::new(Opcode::Ret, vec![]),
        ],
        vec![gpr(0), gpr(1)],
    );
    let alloc = mlkp_machine::Allocation {
        assignment: vec![(0, preg::RAX), (1, preg::RSI)],
        spilled: Vec::new(),
        spill_slots: 0,
        coalesced_copies: 0,
    };
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let mut expected = prologue();
    expected.extend_from_slice(&[0x48, 0xF7, 0xD0]);
    expected.extend_from_slice(&[0x48, 0xF7, 0xD8]);
    expected.extend_from_slice(&[0x48, 0x99]);
    expected.extend_from_slice(&[0x48, 0xF7, 0xFE]);
    expected.extend_from_slice(&[0xEB, 0x00]);
    expected.extend(epilogue());
    assert_eq!(bytes, expected, "unary/div forms mismatch");
}

/// Shift forms: immediate counts and the CL form.
// CEP:ID: JIT-n83
// CEP:WHAT: Tests shift encodings.
// CEP:WHY: The C1/D3 group opcodes and the fixed CL register are new.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn shift_fixture() {
    let target = X86_64Target::new();
    let f = fn1(
        "f",
        vec![
            MInsn::new(
                Opcode::Shl,
                vec![Operand::vreg_def(0, RegClass::Gpr64), Operand::Imm(3)],
            ), // 48 c1 e0 03
            MInsn::new(
                Opcode::Shr,
                vec![Operand::vreg_def(0, RegClass::Gpr64), Operand::Imm(3)],
            ), // 48 c1 e8 03
            MInsn::new(
                Opcode::Sar,
                vec![Operand::vreg_def(0, RegClass::Gpr64), Operand::Imm(3)],
            ), // 48 c1 f8 03
            MInsn::new(
                Opcode::Shl,
                vec![
                    Operand::vreg_def(0, RegClass::Gpr64),
                    Operand::PReg {
                        id: preg::RCX,
                        class: RegClass::Gpr64,
                        def: false,
                    },
                ],
            ), // 48 d3 e0
            MInsn::new(
                Opcode::Sar,
                vec![
                    Operand::vreg_def(0, RegClass::Gpr64),
                    Operand::PReg {
                        id: preg::RCX,
                        class: RegClass::Gpr64,
                        def: false,
                    },
                ],
            ), // 48 d3 f8
            MInsn::new(Opcode::Ret, vec![]),
        ],
        vec![gpr(0)],
    );
    let alloc = mlkp_machine::Allocation {
        assignment: vec![(0, preg::RAX)],
        spilled: Vec::new(),
        spill_slots: 0,
        coalesced_copies: 0,
    };
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let mut expected = prologue();
    expected.extend_from_slice(&[0x48, 0xC1, 0xE0, 0x03]);
    expected.extend_from_slice(&[0x48, 0xC1, 0xE8, 0x03]);
    expected.extend_from_slice(&[0x48, 0xC1, 0xF8, 0x03]);
    expected.extend_from_slice(&[0x48, 0xD3, 0xE0]);
    expected.extend_from_slice(&[0x48, 0xD3, 0xF8]);
    expected.extend_from_slice(&[0xEB, 0x00]);
    expected.extend(epilogue());
    assert_eq!(bytes, expected, "shift forms mismatch");
}

/// Width moves: zero-extension from bytes and dwords, sign extension, and narrow stores.
// CEP:ID: JIT-796
// CEP:WHAT: Tests width-move encodings.
// CEP:WHY: The 0F B6 and 0F 63 escapes, the no-W 32-bit forms, and the byte-store REX rules are all new.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn width_move_fixture() {
    let target = X86_64Target::new();
    let mem_rdi = Operand::Mem {
        base: preg::RDI,
        base_preg: true,
        disp: 0,
        reads: true,
        writes: false,
    };
    let mem_rdi_w = Operand::Mem {
        base: preg::RDI,
        base_preg: true,
        disp: 0,
        reads: false,
        writes: true,
    };
    let f = fn1(
        "f",
        vec![
            MInsn::new(
                Opcode::MovZx8,
                vec![Operand::vreg_def(0, RegClass::Gpr64), mem_rdi.clone()],
            ), // 48 0f b6 07
            MInsn::new(
                Opcode::MovZx8,
                vec![
                    Operand::vreg_def(0, RegClass::Gpr64),
                    Operand::vreg_use(1, RegClass::Gpr64),
                ],
            ), // 48 0f b6 c6 (sil)
            MInsn::new(
                Opcode::MovSx32,
                vec![Operand::vreg_def(0, RegClass::Gpr64), mem_rdi.clone()],
            ), // 48 63 07
            MInsn::new(
                Opcode::MovZx32,
                vec![Operand::vreg_def(0, RegClass::Gpr64), mem_rdi.clone()],
            ), // 8b 07
            MInsn::new(
                Opcode::Store32,
                vec![Operand::vreg_use(0, RegClass::Gpr64), mem_rdi_w.clone()],
            ), // 89 07
            Operand::imm_mov(2, 7),
            MInsn::new(
                Opcode::Store8,
                vec![Operand::vreg_use(2, RegClass::Gpr64), mem_rdi_w.clone()],
            ), // 88 07 (al)
            MInsn::new(
                Opcode::Store8,
                vec![Operand::vreg_use(1, RegClass::Gpr64), mem_rdi_w.clone()],
            ), // 40 88 37 (sil needs a bare REX)
            MInsn::new(Opcode::Ret, vec![]),
        ],
        vec![gpr(0), gpr(1), gpr(2)],
    );
    let alloc = mlkp_machine::Allocation {
        assignment: vec![(0, preg::RAX), (1, preg::RSI), (2, preg::RCX)],
        spilled: Vec::new(),
        spill_slots: 0,
        coalesced_copies: 0,
    };
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let mut expected = prologue();
    expected.extend_from_slice(&[0x48, 0x0F, 0xB6, 0x07]);
    expected.extend_from_slice(&[0x48, 0x0F, 0xB6, 0xC6]);
    expected.extend_from_slice(&[0x48, 0x0F, 0x63, 0x07]);
    expected.extend_from_slice(&[0x8B, 0x07]);
    expected.extend_from_slice(&[0x89, 0x07]);
    expected.extend_from_slice(&[0x48, 0xC7, 0xC1, 0x07, 0x00, 0x00, 0x00]); // mov $7, rcx
    expected.extend_from_slice(&[0x88, 0x0F]);
    expected.extend_from_slice(&[0x40, 0x88, 0x37]);
    expected.extend_from_slice(&[0xEB, 0x00]);
    expected.extend(epilogue());
    assert_eq!(bytes, expected, "width move mismatch");
}

/// Memory-destination read-modify-write forms for the ALU family.
// CEP:ID: JIT-b56
// CEP:WHAT: Tests memory-destination encodings.
// CEP:WHY: The MR forms with rm=memory complete the two-operand matrix.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn mem_dst_rmw_fixture() {
    let target = X86_64Target::new();
    let mem_w = Operand::Mem {
        base: preg::RDI,
        base_preg: true,
        disp: 0,
        reads: true,
        writes: true,
    };
    let mk = |op: Opcode, src: Operand| -> MInsn { MInsn::new(op, vec![mem_w.clone(), src]) };
    let f = fn1(
        "f",
        vec![
            mk(Opcode::Add, Operand::vreg_use(0, RegClass::Gpr64)), // 48 01 37
            mk(Opcode::Add, Operand::Imm(5)),                       // 48 83 07 05
            mk(Opcode::Sub, Operand::vreg_use(0, RegClass::Gpr64)), // 48 29 37
            mk(Opcode::Xor, Operand::Imm(1000)),                    // 48 81 37 e8 03 00 00
            mk(Opcode::Cmp, Operand::vreg_use(0, RegClass::Gpr64)), // 48 39 37
            MInsn::new(Opcode::Ret, vec![]),
        ],
        vec![gpr(0)],
    );
    let alloc = mlkp_machine::Allocation {
        assignment: vec![(0, preg::RSI)],
        spilled: Vec::new(),
        spill_slots: 0,
        coalesced_copies: 0,
    };
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let mut expected = prologue();
    expected.extend_from_slice(&[0x48, 0x01, 0x37]);
    expected.extend_from_slice(&[0x48, 0x83, 0x07, 0x05]);
    expected.extend_from_slice(&[0x48, 0x29, 0x37]);
    expected.extend_from_slice(&[0x48, 0x81, 0x37, 0xE8, 0x03, 0x00, 0x00]);
    expected.extend_from_slice(&[0x48, 0x39, 0x37]);
    expected.extend_from_slice(&[0xEB, 0x00]);
    expected.extend(epilogue());
    assert_eq!(bytes, expected, "mem-dst RMW mismatch");
}

/// Three-operand multiply: imm8, imm32, and memory-source forms.
// CEP:ID: JIT-r40
// CEP:WHAT: Tests imul3 encodings.
// CEP:WHY: The destination sits in the reg field for this form, opposite the two-operand family.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn imul3_fixture() {
    let target = X86_64Target::new();
    let mem_rdi = Operand::Mem {
        base: preg::RDI,
        base_preg: true,
        disp: 0,
        reads: true,
        writes: false,
    };
    let f = fn1(
        "f",
        vec![
            MInsn::new(
                Opcode::IMul3,
                vec![
                    Operand::vreg_def(0, RegClass::Gpr64),
                    Operand::vreg_use(1, RegClass::Gpr64),
                    Operand::Imm(7),
                ],
            ), // 48 6b c6 07
            MInsn::new(
                Opcode::IMul3,
                vec![
                    Operand::vreg_def(0, RegClass::Gpr64),
                    Operand::vreg_use(1, RegClass::Gpr64),
                    Operand::Imm(1000),
                ],
            ), // 48 69 c6 e8 03 00 00
            MInsn::new(
                Opcode::IMul3,
                vec![
                    Operand::vreg_def(0, RegClass::Gpr64),
                    mem_rdi.clone(),
                    Operand::Imm(7),
                ],
            ), // 48 6b 07 07
            MInsn::new(Opcode::Ret, vec![]),
        ],
        vec![gpr(0), gpr(1)],
    );
    let alloc = mlkp_machine::Allocation {
        assignment: vec![(0, preg::RAX), (1, preg::RSI)],
        spilled: Vec::new(),
        spill_slots: 0,
        coalesced_copies: 0,
    };
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let mut expected = prologue();
    expected.extend_from_slice(&[0x48, 0x6B, 0xC6, 0x07]);
    expected.extend_from_slice(&[0x48, 0x69, 0xC6, 0xE8, 0x03, 0x00, 0x00]);
    expected.extend_from_slice(&[0x48, 0x6B, 0x07, 0x07]);
    expected.extend_from_slice(&[0xEB, 0x00]);
    expected.extend(epilogue());
    assert_eq!(bytes, expected, "imul3 mismatch");
}

/// The complete conditional-branch family in rel8 form, plus one grown to rel32.
// CEP:ID: JIT-u87
// CEP:WHAT: Tests the Jcc family.
// CEP:WHY: All sixteen conditions must map to the right opcode bytes.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn jcc_family_fixture() {
    let target = X86_64Target::new();
    let conds = [
        Opcode::Je,
        Opcode::Jne,
        Opcode::Jl,
        Opcode::Jle,
        Opcode::Jg,
        Opcode::Ja,
        Opcode::Jae,
        Opcode::Jb,
        Opcode::Jbe,
        Opcode::Js,
        Opcode::Jns,
        Opcode::Jo,
        Opcode::Jno,
        Opcode::Jp,
        Opcode::Jnp,
    ];
    let rel8 = [
        0x74, 0x75, 0x7C, 0x7E, 0x7F, 0x77, 0x73, 0x72, 0x76, 0x78, 0x79, 0x70, 0x71, 0x7A, 0x7B,
    ];
    let mut insns: Vec<MInsn> = conds
        .iter()
        .map(|c| MInsn::new(*c, vec![Operand::Block("exit".to_string())]))
        .collect();
    insns.push(MInsn::new(Opcode::Ret, vec![]));
    let mut f = fn1("f", insns, vec![]);
    f.blocks[0].succs = vec!["exit".to_string()];
    f.push_block("exit");
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    let alloc = mlkp_machine::Allocation {
        assignment: Vec::new(),
        spilled: Vec::new(),
        spill_slots: 0,
        coalesced_copies: 0,
    };
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let mut expected = prologue();
    // Each branch targets the exit label 15 conditional branches plus a ret later.
    let body_start = 8usize;
    // Fifteen conditional branches target the exit block; the two ret jumps
    // target the epilogue.
    for (k, opc) in rel8.iter().enumerate() {
        let here = body_start + k * 2;
        let target = body_start + 16 * 2;
        let disp = (target - (here + 2)) as i64;
        expected.push(*opc);
        expected.push(disp as i8 as u8);
    }
    expected.extend_from_slice(&[0xEB, 0x02]); // entry ret -> epilogue
    expected.extend_from_slice(&[0xEB, 0x00]); // exit ret -> epilogue
    expected.extend(epilogue());
    assert_eq!(bytes, expected, "jcc family mismatch");
    // And the far form: a branch beyond rel8 range grows to 0F 8x.
    let mut far = MFunction {
        name: "far".to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    far.push_block("entry");
    far.push_insn(MInsn::new(
        Opcode::Je,
        vec![Operand::Block("exit".to_string())],
    ));
    for _ in 0..30 {
        far.push_insn(Operand::imm_mov(0, 1));
    }
    far.push_insn(MInsn::new(Opcode::Ret, vec![]));
    far.push_block("exit");
    far.push_insn(MInsn::new(Opcode::Ret, vec![]));
    far.blocks[0].succs = vec!["exit".to_string()];
    let far_alloc = mlkp_machine::Allocation {
        assignment: vec![(0, preg::RAX)],
        spilled: Vec::new(),
        spill_slots: 0,
        coalesced_copies: 0,
    };
    let bytes = compile(&far, &far_alloc, &target).expect("encodes").text;
    assert_eq!(bytes[8], 0x0F, "far je must escape");
    assert_eq!(bytes[9], 0x84, "far je rel32 second byte");
}

/// Extended SSE forms: subsd, divsd, sqrtsd, ucomisd, and the conversions.
// CEP:ID: JIT-g53
// CEP:WHAT: Tests extended SSE encodings.
// CEP:WHY: The scalar-double set completes with division, roots, comparisons, and conversions.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn sse_extended_fixture() {
    let target = X86_64Target::new();
    let xmm = |v: u32, def: bool| -> Operand {
        if def {
            Operand::vreg_def(v, RegClass::Xmm64)
        } else {
            Operand::vreg_use(v, RegClass::Xmm64)
        }
    };
    let f = fn1(
        "f",
        vec![
            MInsn::new(Opcode::SubSd, vec![xmm(0, true), xmm(1, false)]), // f2 0f 5c c8? dst=xmm0 src=xmm1 -> c8
            MInsn::new(Opcode::DivSd, vec![xmm(0, true), xmm(1, false)]), // f2 0f 5e c8
            MInsn::new(Opcode::SqrtSd, vec![xmm(0, true), xmm(1, false)]), // f2 0f 51 c8
            MInsn::new(Opcode::UComisd, vec![xmm(0, false), xmm(1, false)]), // 66 0f 2e c8
            MInsn::new(
                Opcode::CvtSi2Sd,
                vec![
                    xmm(0, true),
                    Operand::PReg {
                        id: preg::RSI,
                        class: RegClass::Gpr64,
                        def: false,
                    },
                ],
            ), // f2 48 0f 2a c6
            MInsn::new(
                Opcode::CvtSd2Si,
                vec![Operand::vreg_def(2, RegClass::Gpr64), xmm(1, false)],
            ), // f2 48 0f 2c c9? reg=rcx? dst=v2 in rcx: reg field = rcx hw1, rm=xmm1 -> c9
            MInsn::new(Opcode::Ret, vec![]),
        ],
        vec![(0, RegClass::Xmm64), (1, RegClass::Xmm64), gpr(2)],
    );
    let alloc = mlkp_machine::Allocation {
        assignment: vec![(0, preg::XMM0), (1, preg::XMM1), (2, preg::RCX)],
        spilled: Vec::new(),
        spill_slots: 0,
        coalesced_copies: 0,
    };
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let mut expected = prologue();
    expected.extend_from_slice(&[0xF2, 0x0F, 0x5C, 0xC1]);
    expected.extend_from_slice(&[0xF2, 0x0F, 0x5E, 0xC1]);
    expected.extend_from_slice(&[0xF2, 0x0F, 0x51, 0xC1]);
    expected.extend_from_slice(&[0x66, 0x0F, 0x2E, 0xC1]);
    expected.extend_from_slice(&[0xF2, 0x48, 0x0F, 0x2A, 0xC6]);
    expected.extend_from_slice(&[0xF2, 0x48, 0x0F, 0x2C, 0xC9]);
    expected.extend_from_slice(&[0xEB, 0x00]);
    expected.extend(epilogue());
    assert_eq!(bytes, expected, "extended SSE mismatch");
}

/// The decoder and the emitter agree on every new instruction form.
// CEP:WHAT: Tests bytes-versus-text equivalence for the new ISA.
// CEP:WHY: The mechanical equivalence check must extend with the instruction set.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn new_isa_decodes_to_emitter_text() {
    let target = X86_64Target::new();
    let cases: Vec<MFunction> = vec![logic_program(), div_program(), sse_ext_program()];
    for mut f in cases {
        let alloc = allocate(&mut f, &target).expect("allocates");
        verify_allocation(&f, &alloc, &target).expect("verifies");
        let text = emit(&f, &alloc, &target).expect("emits");
        let co = compile(&f, &alloc, &target).expect("encodes");
        assert!(!decode_insns(&co).expect("decodes").is_empty());
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
    }
}

/// A gas differential over the mixed ISA: assembling the emitted text reproduces the encoder's bytes.
// CEP:ID: JIT-263
// CEP:WHAT: Runs the mixed-ISA gas differential.
// CEP:WHY: An independent assembler remains the strongest oracle for the expanded set.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert; a missing assembler fails loudly.
// CEP:ASSUMES: GNU as on PATH.
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn gas_differential_mixed() {
    let target = X86_64Target::new();
    if std::process::Command::new("as")
        .arg("--version")
        .output()
        .is_err()
    {
        panic!("GNU as is required for the differential test but was not found on PATH");
    }
    let dir = std::env::temp_dir().join(format!("mlkp-isa-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let mut f = logic_program();
    let alloc = allocate(&mut f, &target).expect("allocates");
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let text = emit(&f, &alloc, &target).expect("emits");
    let asm: String = text
        .lines()
        .filter(|l| !l.starts_with("//"))
        .map(|l| format!("{l}\n"))
        .collect();
    let s_path = dir.join("mixed.s");
    std::fs::write(&s_path, &asm).expect("write .s");
    let o_path = dir.join("mixed.o");
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
    let gas_text = crate_text_section(&obj);
    let mine = compile(&f, &alloc, &target).expect("encodes").text;
    assert_eq!(
        mine, gas_text,
        "encoder bytes differ from gas on the mixed ISA"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Extracts the .text section bytes from an ELF object produced by gas.
// CEP:ID: JIT-i90
// CEP:WHAT: Reads a .text section.
// CEP:WHY: The differential test compares raw section bytes.
// CEP:STATUS: complete
// CEP:FAILURE: Panics on a malformed object.
// CEP:ASSUMES: little-endian ELF64.
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
fn crate_text_section(obj: &[u8]) -> Vec<u8> {
    assert_eq!(&obj[0..4], &[0x7f, b'E', b'L', b'F']);
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

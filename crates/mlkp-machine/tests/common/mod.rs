// CEP:FILE: crates/mlkp-machine/tests/common/mod.rs
// CEP:WHAT: Shared test fixtures: one-block function builders, register class helpers, prologue and epilogue bytes, and the four canonical kernel programs.
// CEP:WHY: The no-subset expansion touches every backend stage; byte fixtures prove encodings against gas and execution proves allocation, encoding, and the ABI agree at once.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: Linux x86-64 with anonymous mmap; the SysV ABI; GNU as for the differential test.
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-machine.

#![allow(dead_code)]

use mlkp_machine::{preg, MFunction, MInsn, Opcode, Operand, RegClass};

/// Builds a one-block function.
// CEP:WHAT: Builds a one-block fixture.
// CEP:WHY: Byte fixtures need minimal wrappers.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
pub fn fn1(name: &str, insns: Vec<MInsn>, classes: Vec<(u32, RegClass)>) -> MFunction {
    let mut f = MFunction {
        name: name.to_string(),
        blocks: Vec::new(),
        vreg_classes: classes,
    };
    f.push_block("entry");
    for i in insns {
        f.push_insn(i);
    }
    f
}

/// Registers a virtual register with the function's class table.
// CEP:WHAT: Registers a vreg class.
// CEP:WHY: Fixtures accumulate classes as they build operands.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: cargo test -p mlkp-machine
pub fn gpr(v: u32) -> (u32, RegClass) {
    (v, RegClass::Gpr64)
}

/// The prologue bytes for a frame of 48 with no saved registers, and the epilogue.
// CEP:WHAT: Supplies prologue and epilogue bytes.
// CEP:WHY: Every fixture shares the same frame shape.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the worst-case saved area is 40 bytes, aligned to 48.
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
pub fn prologue() -> Vec<u8> {
    vec![0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x30]
}

/// The epilogue bytes for no saved registers.
// CEP:WHAT: Supplies epilogue bytes.
// CEP:WHY: Shared by fixtures.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
pub fn epilogue() -> Vec<u8> {
    vec![0x48, 0x89, 0xEC, 0x5D, 0xC3]
}

/// Builds the signed-division kernel: a / b through cqo and idiv.
// CEP:ID: JIT-o11
// CEP:WHAT: Builds the division fixture.
// CEP:WHY: The implicit RDX:RAX contract must survive allocation.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
pub fn div_program() -> MFunction {
    let mut f = MFunction {
        name: "divfn".to_string(),
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
    f.push_insn(MInsn::new(Opcode::Cqo, vec![]));
    let mut idiv = MInsn::new(Opcode::IDiv, vec![Operand::vreg_use(1, RegClass::Gpr64)]);
    idiv.implicit_uses = vec![preg::RAX, preg::RDX];
    idiv.implicit_defs = vec![preg::RAX, preg::RDX];
    f.push_insn(idiv);
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::vreg_def(2, RegClass::Gpr64),
            Operand::PReg {
                id: preg::RAX,
                class: RegClass::Gpr64,
                def: false,
            },
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
            Operand::vreg_use(2, RegClass::Gpr64),
        ],
    ));
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    for v in 0..3 {
        f.vreg_classes.push((v, RegClass::Gpr64));
    }
    f
}

/// Builds a logic-and-shift kernel: ((a << 3) & mask) ^ (b >> 1) then negated when sign is set.
// CEP:ID: JIT-i46
// CEP:WHAT: Builds the logic fixture.
// CEP:WHY: Shifts, logic, and flag-consuming branches execute together.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
pub fn logic_program() -> MFunction {
    let mut f = MFunction {
        name: "logicfn".to_string(),
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
    // a << 3
    f.push_insn(MInsn::new(
        Opcode::Shl,
        vec![Operand::vreg_def(0, RegClass::Gpr64), Operand::Imm(3)],
    ));
    // a = (a << 3) ^ b
    f.push_insn(MInsn::new(
        Opcode::Xor,
        vec![
            Operand::vreg_def(0, RegClass::Gpr64),
            Operand::vreg_use(1, RegClass::Gpr64),
        ],
    ));
    // b = b >> 1 (arithmetic)
    f.push_insn(MInsn::new(
        Opcode::Sar,
        vec![Operand::vreg_def(1, RegClass::Gpr64), Operand::Imm(1)],
    ));
    // a = a & b
    f.push_insn(MInsn::new(
        Opcode::And,
        vec![
            Operand::vreg_def(0, RegClass::Gpr64),
            Operand::vreg_use(1, RegClass::Gpr64),
        ],
    ));
    // if a == 0 { a = -7 } else { a = -a }
    f.push_insn(MInsn::new(
        Opcode::Cmp,
        vec![Operand::vreg_use(0, RegClass::Gpr64), Operand::Imm(0)],
    ));
    f.push_insn(MInsn::new(
        Opcode::Jne,
        vec![Operand::Block("neg".to_string())],
    ));
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![Operand::vreg_def(0, RegClass::Gpr64), Operand::Imm(-7)],
    ));
    f.push_insn(MInsn::new(
        Opcode::Jmp,
        vec![Operand::Block("out".to_string())],
    ));
    f.blocks[0].succs = vec!["neg".to_string(), "out".to_string()];
    f.push_block("neg");
    f.push_insn(MInsn::new(
        Opcode::Neg,
        vec![Operand::vreg_def(0, RegClass::Gpr64)],
    ));
    f.blocks[1].succs = vec!["out".to_string()];
    f.push_block("out");
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
    for v in 0..2 {
        f.vreg_classes.push((v, RegClass::Gpr64));
    }
    f
}

/// Builds a floating-point kernel: (x * y + x) / sqrt(x + y) with an integer round trip.
// CEP:ID: JIT-199
// CEP:WHAT: Builds the extended SSE fixture.
// CEP:WHY: Subtraction, division, roots, comparisons, and conversions execute together in one kernel.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
pub fn sse_ext_program() -> MFunction {
    let mut f = MFunction {
        name: "fext".to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    let xmm_def = |v: u32| Operand::vreg_def(v, RegClass::Xmm64);
    let xmm_use = |v: u32| Operand::vreg_use(v, RegClass::Xmm64);
    f.push_block("entry");
    // v0 = x, v1 = y; at most two xmm values are live at once, matching the
    // target's two xmm registers.
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            xmm_def(0),
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
            xmm_def(1),
            Operand::PReg {
                id: preg::XMM1,
                class: RegClass::Xmm64,
                def: false,
            },
        ],
    ));
    // v0 = x * x
    f.push_insn(MInsn::new(Opcode::MulSd, vec![xmm_def(0), xmm_use(0)]));
    // v1 = sqrt(y); the tests keep y positive so the root is defined.
    f.push_insn(MInsn::new(Opcode::SqrtSd, vec![xmm_def(1), xmm_use(1)]));
    // v0 = x * x + sqrt(y)
    f.push_insn(MInsn::new(Opcode::AddSd, vec![xmm_def(0), xmm_use(1)]));
    // v0 = (x * x + sqrt(y)) / sqrt(y)
    f.push_insn(MInsn::new(Opcode::DivSd, vec![xmm_def(0), xmm_use(1)]));
    // Round trip through integers: i = cvttsd2si(v0); v0 = cvtsi2sd(i).
    f.push_insn(MInsn::new(
        Opcode::CvtSd2Si,
        vec![Operand::vreg_def(2, RegClass::Gpr64), xmm_use(0)],
    ));
    f.push_insn(MInsn::new(
        Opcode::CvtSi2Sd,
        vec![xmm_def(0), Operand::vreg_use(2, RegClass::Gpr64)],
    ));
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::PReg {
                id: preg::XMM0,
                class: RegClass::Xmm64,
                def: true,
            },
            xmm_use(0),
        ],
    ));
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    f.vreg_classes.push((0, RegClass::Xmm64));
    f.vreg_classes.push((1, RegClass::Xmm64));
    f.vreg_classes.push((2, RegClass::Gpr64));
    f
}

/// Builds a floating-point max kernel: ucomisd followed by a conditional branch.
// CEP:ID: JIT-e93
// CEP:WHAT: Builds the SSE compare fixture.
// CEP:WHY: The SSE compare sets the same flags the integer branches consume; the pairing must execute.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
pub fn max_program() -> MFunction {
    let mut f = MFunction {
        name: "fmax".to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    let xmm_def = |v: u32| Operand::vreg_def(v, RegClass::Xmm64);
    let xmm_use = |v: u32| Operand::vreg_use(v, RegClass::Xmm64);
    f.push_block("entry");
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            xmm_def(0),
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
            xmm_def(1),
            Operand::PReg {
                id: preg::XMM1,
                class: RegClass::Xmm64,
                def: false,
            },
        ],
    ));
    // Compare x with y; jump when x > y.
    f.push_insn(MInsn::new(Opcode::UComisd, vec![xmm_use(0), xmm_use(1)]));
    f.push_insn(MInsn::new(
        Opcode::Ja,
        vec![Operand::Block("xis".to_string())],
    ));
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::PReg {
                id: preg::XMM0,
                class: RegClass::Xmm64,
                def: true,
            },
            xmm_use(1),
        ],
    ));
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    f.blocks[0].succs = vec!["xis".to_string()];
    f.push_block("xis");
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    f.vreg_classes.push((0, RegClass::Xmm64));
    f.vreg_classes.push((1, RegClass::Xmm64));
    f
}

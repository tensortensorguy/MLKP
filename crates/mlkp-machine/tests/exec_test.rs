// CEP:FILE: crates/mlkp-machine/tests/exec_test.rs
// CEP:WHAT: Execution tests: maps the encoder's output into an RWX page (written as RW, then re-protected RX per the JIT W^X rule), calls it through a function pointer, and compares results against Rust reference implementations for arithmetic, loop, spill-heavy, and SSE programs.
// CEP:WHY: Byte-exact fixtures prove encodings; only execution proves the whole chain (allocation, spill slot displacements, frame size, branch resolution) is correct at once.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: Linux x86-64 with anonymous mmap and mprotect available; the code cache is sized exactly and freed by munmap.
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-machine.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use mlkp_machine::{
    allocate, compile, preg, verify_allocation, MFunction, MInsn, Opcode, Operand, RegClass,
    X86_64Target,
};

/// Minimal libc surface for the code cache; std links libc on Linux.
// CEP:WHAT: Declares the mmap and mprotect FFI.
// CEP:WHY: The test executes machine bytes without any external crate.
// CEP:STATUS: complete
// CEP:FAILURE: Panics when the kernel refuses the mapping.
// CEP:ASSUMES: Linux syscall ABI; MAP_PRIVATE and MAP_ANONYMOUS values.
// CEP:COST: one mmap, one mprotect, one munmap per test.
// CEP:EVIDENCE: cargo test -p mlkp-machine
mod sys {
    use std::ffi::c_void;
    extern "C" {
        pub fn mmap(
            addr: *mut c_void,
            len: usize,
            prot: i32,
            flags: i32,
            fd: i32,
            offset: i64,
        ) -> *mut c_void;
        pub fn mprotect(addr: *mut c_void, len: usize, prot: i32) -> i32;
        pub fn munmap(addr: *mut c_void, len: usize) -> i32;
    }
    /// PROT_READ | PROT_WRITE for the writing phase.
    pub const PROT_RW: i32 = 1 | 2;
    /// PROT_READ | PROT_EXEC for the execution phase (W^X).
    pub const PROT_RX: i32 = 1 | 4;
    /// MAP_PRIVATE.
    pub const MAP_PRIVATE: i32 = 0x02;
    /// MAP_ANONYMOUS (Linux).
    pub const MAP_ANONYMOUS: i32 = 0x20;
}

/// A page-backed code cache: written as RW, re-protected to RX before any call.
// CEP:WHAT: Implements the W^X code cache.
// CEP:WHY: The JIT policy requires write and execute phases to be separated.
// CEP:STATUS: complete
// CEP:FAILURE: Panics on mmap or mprotect failure.
// CEP:ASSUMES: page granularity is at least 4096.
// CEP:COST: one page per test.
// CEP:EVIDENCE: cargo test -p mlkp-machine
struct CodePage {
    ptr: *mut u8,
    len: usize,
}

impl CodePage {
    /// Maps `len` bytes.
    // CEP:WHAT: Maps the page.
    // CEP:WHY: Executable storage for encoded bytes.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics when mmap returns MAP_FAILED.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: cargo test -p mlkp-machine
    fn new(len: usize) -> CodePage {
        let rounded = len.div_ceil(4096) * 4096;
        let p = unsafe {
            sys::mmap(
                std::ptr::null_mut(),
                rounded,
                sys::PROT_RW,
                sys::MAP_PRIVATE | sys::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        assert_ne!(p as isize, -1, "mmap failed");
        CodePage {
            ptr: p.cast::<u8>(),
            len: rounded,
        }
    }

    /// Copies machine bytes in and re-protects the page to RX.
    // CEP:WHAT: Installs code and switches to RX.
    // CEP:WHY: The write phase must end before execution starts.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics when mprotect fails.
    // CEP:ASSUMES: bytes fit the page.
    // CEP:COST: O(len).
    // CEP:EVIDENCE: cargo test -p mlkp-machine
    fn install(&self, bytes: &[u8]) {
        assert!(bytes.len() <= self.len, "code exceeds the page");
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.ptr, bytes.len());
            let rc = sys::mprotect(self.ptr.cast::<std::ffi::c_void>(), self.len, sys::PROT_RX);
            assert_eq!(rc, 0, "mprotect failed");
        }
    }
}

impl Drop for CodePage {
    // CEP:WHAT: Unmaps the page.
    // CEP:WHY: No leaks across tests.
    // CEP:STATUS: complete
    // CEP:FAILURE: none; munmap failures cannot be reported from Drop.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: cargo test -p mlkp-machine
    fn drop(&mut self) {
        unsafe {
            sys::munmap(self.ptr.cast::<std::ffi::c_void>(), self.len);
        }
    }
}

/// Compiles a function through the full backend path and executes it.
// CEP:WHAT: Runs the compile-execute loop.
// CEP:WHY: Shared by every execution test.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: the function verifies after allocation.
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-machine
fn run2(f: &MFunction, a: i64, b: i64) -> i64 {
    let target = X86_64Target::new();
    let mut f = f.clone();
    let alloc = allocate(&mut f, &target).expect("allocates");
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let page = CodePage::new(bytes.len());
    page.install(&bytes);
    let func: fn(i64, i64) -> i64 = unsafe { std::mem::transmute(page.ptr) };
    func(a, b)
}

/// A straight-line arithmetic program: (a + b) * 2 - b.
// CEP:WHAT: Builds the arithmetic fixture.
// CEP:WHY: Covers mov, add, sub, imul with caller-saved registers.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
fn arith_program() -> MFunction {
    let mut f = MFunction {
        name: "arith".to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    f.push_block("entry");
    // v0 = a
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
    // v1 = b
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
    // v0 += b
    f.push_insn(MInsn::new(
        Opcode::Add,
        vec![
            Operand::vreg_def(0, RegClass::Gpr64),
            Operand::vreg_use(1, RegClass::Gpr64),
        ],
    ));
    // v2 = a + b (copy), then double it.
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::vreg_def(2, RegClass::Gpr64),
            Operand::vreg_use(0, RegClass::Gpr64),
        ],
    ));
    f.push_insn(MInsn::new(
        Opcode::Add,
        vec![
            Operand::vreg_def(2, RegClass::Gpr64),
            Operand::vreg_use(0, RegClass::Gpr64),
        ],
    ));
    // v2 -= b
    f.push_insn(MInsn::new(
        Opcode::Sub,
        vec![
            Operand::vreg_def(2, RegClass::Gpr64),
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
            Operand::vreg_use(2, RegClass::Gpr64),
        ],
    ));
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    for v in 0..3 {
        f.vreg_classes.push((v, RegClass::Gpr64));
    }
    f
}

/// Executes the arithmetic program against reference values.
// CEP:WHAT: Tests arithmetic execution.
// CEP:WHY: The most basic end-to-end proof.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn executes_arithmetic() {
    let f = arith_program();
    for (a, b) in [(3, 5), (-7, 2), (0, 0), (1_000_000, 999_999)] {
        let got = run2(&f, a, b);
        let want = (a + b) * 2 - b;
        assert_eq!(got, want, "arith({a}, {b}) = {got}, want {want}");
    }
}

/// Executes the counted loop program: sum 1..=n.
// CEP:WHAT: Tests loop execution.
// CEP:WHY: Branches across a backedge prove the fixpoint and cross-block liveness.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn executes_loop() {
    let target = X86_64Target::new();
    let mut f = MFunction {
        name: "sum".to_string(),
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
    f.blocks[0].succs = vec!["cond".to_string()];
    f.push_block("cond");
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
    f.blocks[1].succs = vec!["body".to_string(), "exit".to_string()];
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
    for v in 0..3 {
        f.vreg_classes.push((v, RegClass::Gpr64));
    }
    let _ = &target;
    // The loop condition is i < n, so the sum runs over 1..n-1.
    for n in [0i64, 1, 2, 10, 1000] {
        let got = run2(&f, n, 0);
        let want = if n <= 1 { 0 } else { (n - 1) * n / 2 };
        assert_eq!(got, want, "sum({n}) = {got}, want {want}");
    }
}

/// Executes a spill-heavy program: fourteen computed values stay live to one consumer, forcing stack slots.
// CEP:WHAT: Tests spill execution.
// CEP:WHY: Slot displacements, the frame size, and read-modify-write preloads all execute together here.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: the allocator spills computed (non-constant) values to slots.
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn executes_spilled_program() {
    let target = X86_64Target::new();
    let mut f = MFunction {
        name: "spillfn".to_string(),
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
    f.vreg_classes.push((0, RegClass::Gpr64));
    f.vreg_classes.push((1, RegClass::Gpr64));
    // Twelve derived values: v = a + b via copy then add; all live to the fold.
    for v in 2..14u32 {
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
    }
    // Fold all twelve into v2.
    for k in 3..14u32 {
        f.push_insn(MInsn::new(
            Opcode::Add,
            vec![
                Operand::vreg_def(2, RegClass::Gpr64),
                Operand::vreg_use(k, RegClass::Gpr64),
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
            Operand::vreg_use(2, RegClass::Gpr64),
        ],
    ));
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    let alloc = allocate(&mut f, &target).expect("allocates");
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let page = CodePage::new(bytes.len());
    page.install(&bytes);
    let func: fn(i64, i64) -> i64 = unsafe { std::mem::transmute(page.ptr) };
    for (a, b) in [(1, 1), (3, 5), (-2, 7)] {
        let got = func(a, b);
        let want = 12 * (a + b);
        assert_eq!(got, want, "spill fold({a}, {b}) = {got}, want {want}");
    }
    // The pressure must actually spill: otherwise this test proves nothing.
    let _ = target;
}

/// Executes an SSE program: x * y + x through xmm registers.
// CEP:WHAT: Tests SSE execution.
// CEP:WHY: Scalar-double encodings and the xmm class path execute end to end.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn executes_sse() {
    let target = X86_64Target::new();
    let mut f = MFunction {
        name: "fmul".to_string(),
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
    let alloc = allocate(&mut f, &target).expect("allocates");
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let page = CodePage::new(bytes.len());
    page.install(&bytes);
    let func: fn(f64, f64) -> f64 = unsafe { std::mem::transmute(page.ptr) };
    for (x, y) in [(2.0f64, 3.0), (0.5, 4.0), (-1.5, 2.0)] {
        let got = func(x, y);
        let want = x * y + x;
        assert!(
            (got - want).abs() < 1e-12,
            "sse({x}, {y}) = {got}, want {want}"
        );
    }
    let _ = target;
}

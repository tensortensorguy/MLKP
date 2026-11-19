// CEP:FILE: crates/mlkp-machine/tests/isa_exec_test.rs
// CEP:WHAT: Execution tests: runs encoded kernels through the W^X mapped code cache against Rust references, covering division with the implicit RDX:RAX pair, logic with shifts, the extended SSE set, and the softmax-style max kernel.
// CEP:WHY: The no-subset expansion touches every backend stage; byte fixtures prove encodings against gas and execution proves allocation, encoding, and the ABI agree at once.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: Linux x86-64 with anonymous mmap; the SysV ABI; GNU as for the differential test.
// CEP:UNSAFE: mmap/mprotect code-cache pages and transmutes of page entry addresses to function pointers, confined to the test process; encoded bytes are produced by the encoder under test, never read from untrusted input.
// CEP:SECURITY: pages are private anonymous mappings, re-protected RX before execution per the W^X rule, and freed with munmap; no host data crosses the boundary.
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-machine.

mod common;

use common::*;

use mlkp_machine::{allocate, compile, verify_allocation, X86_64Target};

/// Compiles a function through the full backend and executes it as a C-ABI function.
// CEP:WHAT: Runs the compile-execute loop.
// CEP:WHY: Shared by the execution tests.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: the function verifies after allocation.
// CEP:COST: cold.
// CEP:EVIDENCE: cargo test -p mlkp-machine
pub mod jit {
    use std::ffi::c_void;

    /// Minimal mmap surface for the code cache.
    // CEP:WHAT: Declares mmap FFI.
    // CEP:WHY: Executes machine bytes without external crates.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics when the kernel refuses the mapping.
    // CEP:ASSUMES: Linux syscall ABI values.
    // CEP:COST: one mmap per call.
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
        /// PROT_READ | PROT_WRITE.
        pub const PROT_RW: i32 = 3;
        /// PROT_READ | PROT_EXEC.
        pub const PROT_RX: i32 = 5;
        /// MAP_PRIVATE | MAP_ANONYMOUS.
        pub const MAP_ANON_PRIVATE: i32 = 0x02 | 0x20;
    }

    /// A page-backed W^X code cache.
    // CEP:WHAT: Implements the code cache.
    // CEP:WHY: Write and execute phases stay separated per the JIT policy.
    // CEP:STATUS: complete
    // CEP:FAILURE: Panics on mmap or mprotect failure.
    // CEP:ASSUMES: page granularity of at least 4096.
    // CEP:COST: one page per instance.
    // CEP:EVIDENCE: cargo test -p mlkp-machine
    pub struct CodePage {
        ptr: *mut u8,
        len: usize,
    }

    impl CodePage {
        /// Maps and installs machine bytes, then re-protects to RX.
        // CEP:WHAT: Installs code.
        // CEP:WHY: The write phase ends before execution starts.
        // CEP:STATUS: complete
        // CEP:FAILURE: Panics when the mapping fails.
        // CEP:ASSUMES: bytes fit the page.
        // CEP:COST: O(len).
        // CEP:EVIDENCE: cargo test -p mlkp-machine
        pub fn install(bytes: &[u8]) -> CodePage {
            let rounded = bytes.len().div_ceil(4096) * 4096;
            let p = unsafe {
                sys::mmap(
                    std::ptr::null_mut(),
                    rounded,
                    sys::PROT_RW,
                    sys::MAP_ANON_PRIVATE,
                    -1,
                    0,
                )
            };
            assert_ne!(p as isize, -1, "mmap failed");
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.cast::<u8>(), bytes.len());
                assert_eq!(
                    sys::mprotect(p, rounded, sys::PROT_RX),
                    0,
                    "mprotect failed"
                );
            }
            CodePage {
                ptr: p.cast::<u8>(),
                len: rounded,
            }
        }

        /// The entry pointer as a two-argument C function.
        // CEP:WHAT: Returns the entry pointer.
        // CEP:WHY: Callers transmute to their ABI signature.
        // CEP:STATUS: complete
        // CEP:FAILURE: none
        // CEP:ASSUMES: none
        // CEP:COST: O(1).
        // CEP:EVIDENCE: cargo test -p mlkp-machine
        pub fn entry(&self) -> *mut u8 {
            self.ptr
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
                sys::munmap(self.ptr.cast::<c_void>(), self.len);
            }
        }
    }
}

/// Executes the division kernel against truncating signed division.
// CEP:WHAT: Tests division execution.
// CEP:WHY: Implicit register contracts execute end to end only here.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn executes_division() {
    let target = X86_64Target::new();
    let mut f = div_program();
    let alloc = allocate(&mut f, &target).expect("allocates");
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let page = jit::CodePage::install(&bytes);
    let func: fn(i64, i64) -> i64 = unsafe { std::mem::transmute(page.entry()) };
    for (a, b) in [(17, 5), (-17, 5), (100, 7), (0, 9)] {
        let got = func(a, b);
        let want = a / b;
        assert_eq!(got, want, "div({a}, {b}) = {got}, want {want}");
    }
}

/// Executes the logic-and-shift kernel.
// CEP:ID: JIT-c46
// CEP:WHAT: Tests logic execution.
// CEP:WHY: The ALU expansion, shifts, and conditional branches run together.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn executes_logic_and_shifts() {
    let target = X86_64Target::new();
    let mut f = logic_program();
    let alloc = allocate(&mut f, &target).expect("allocates");
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let page = jit::CodePage::install(&bytes);
    let func: fn(i64, i64) -> i64 = unsafe { std::mem::transmute(page.entry()) };
    for (a, b) in [(3, 10), (-3, 10), (255, 7), (0, 1)] {
        let mut want = (a << 3) ^ b;
        want &= b >> 1;
        if want == 0 {
            want = -7;
        } else {
            want = -want;
        }
        let got = func(a, b);
        assert_eq!(got, want, "logic({a}, {b}) = {got}, want {want}");
    }
}

/// Executes the extended floating-point kernel.
// CEP:ID: JIT-n97
// CEP:WHAT: Tests extended SSE execution.
// CEP:WHY: Multiplication, roots, division, and conversions execute together in one kernel.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn executes_sse_extended() {
    let target = X86_64Target::new();
    let mut f = sse_ext_program();
    let alloc = allocate(&mut f, &target).expect("allocates");
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let page = jit::CodePage::install(&bytes);
    let func: fn(f64, f64) -> f64 = unsafe { std::mem::transmute(page.entry()) };
    let cases: [(f64, f64); 3] = [(2.0, 5.0), (5.0, 2.0), (-3.0, 7.0)];
    for (x, y) in cases {
        let want: f64 = (x * x + y.sqrt()) / y.sqrt();
        let want = want as i64 as f64;
        let got = func(x, y);
        assert!(
            (got - want).abs() < 1e-9,
            "fext({x}, {y}) = {got}, want {want}"
        );
    }
}

/// Executes the floating-point max kernel.
// CEP:WHAT: Tests SSE compare-and-branch execution.
// CEP:WHY: ucomisd defines flags that ja consumes; the pairing crosses the integer and SSE halves of the ISA.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn executes_sse_max() {
    let target = X86_64Target::new();
    let mut f = max_program();
    let alloc = allocate(&mut f, &target).expect("allocates");
    verify_allocation(&f, &alloc, &target).expect("verifies");
    let bytes = compile(&f, &alloc, &target).expect("encodes").text;
    let page = jit::CodePage::install(&bytes);
    let func: fn(f64, f64) -> f64 = unsafe { std::mem::transmute(page.entry()) };
    for (x, y) in [(2.0f64, 5.0), (5.0, 2.0), (-3.0, 7.0), (4.0, 4.0)] {
        let want = if x > y { x } else { y };
        let got = func(x, y);
        assert!(
            (got - want).abs() < 1e-9,
            "fmax({x}, {y}) = {got}, want {want}"
        );
    }
}

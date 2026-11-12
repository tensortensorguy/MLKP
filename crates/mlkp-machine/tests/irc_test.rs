// CEP:FILE: crates/mlkp-machine/tests/irc_test.rs
// CEP:WHAT: Machine backend conformance tests: interference construction, IRC allocation under pressure, copy coalescing, fixed constraints, allocation verification, and deterministic emission.
// CEP:WHY: The register allocator is the flagship component of the machine backend; a wrong allocation silently corrupts every emitted program.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: none
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-machine.

use mlkp_machine::{
    allocate, block_live_out, build_interference, emit, preg, verify_allocation, MFunction, MInsn,
    Opcode, Operand, RegClass, X86_64Target,
};

// CEP:WHAT: Builds a straight-line function where k-1 values are live simultaneously.
// CEP:WHY: Shared fixture for pressure tests; all defined values are consumed by one wide instruction so they interfere pairwise.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
fn straight_line(live_values: usize) -> MFunction {
    let mut f = MFunction {
        name: "f".to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    f.push_block("entry");
    for v in 0..live_values {
        f.vreg_classes.push((v as u32, RegClass::Gpr64));
        f.push_insn(Operand::imm_mov(v as u32, v as i64));
    }
    // One wide consumer keeps every value live at once: def = live_values, uses = all.
    let mut ops = vec![Operand::vreg_def(live_values as u32, RegClass::Gpr64)];
    for v in 0..live_values {
        ops.push(Operand::vreg_use(v as u32, RegClass::Gpr64));
    }
    f.vreg_classes.push((live_values as u32, RegClass::Gpr64));
    f.push_insn(MInsn::new(Opcode::Add, ops));
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    f
}

// CEP:WHAT: Verifies interference construction finds the expected edges.
// CEP:WHY: Two values live simultaneously must interfere.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn interference_edges_follow_liveness() {
    let target = X86_64Target::new();
    let f = straight_line(8);
    let ig = build_interference(&f, &target);
    // Values 0..8 are all live simultaneously at the wide consumer: they pairwise interfere.
    let deg = ig.degree(3);
    assert!(
        deg >= 7,
        "value 3 must interfere with all other live values, got {deg}"
    );
    assert!(ig.neighbors(3).contains(&1));
    assert!(ig.neighbors(3).contains(&7));
}

// CEP:WHAT: Verifies block liveness reaches a fixed point.
// CEP:WHY: Backward dataflow must converge before interference is built.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn block_liveness_converges() {
    let f = straight_line(4);
    let out = block_live_out(&f);
    assert_eq!(out.len(), 1);
}

// CEP:WHAT: Verifies IRC allocates under pressure within available colors.
// CEP:WHY: The allocator must color the graph or spill; it must never overlap live values.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn irc_allocates_under_pressure() {
    let target = X86_64Target::new();
    let mut f = straight_line(12);
    let alloc = allocate(&mut f, &target).expect("allocate");
    assert!(verify_allocation(&f, &alloc, &target).is_ok());
    // 14 allocatable GPRs cover 12 simultaneously live values without spilling.
    assert!(alloc.spilled.is_empty(), "no spill expected for 12 values");
}

// CEP:WHAT: Verifies spilling occurs when pressure exceeds colors and verification still passes.
// CEP:WHY: Over-pressured functions must spill rather than produce overlapping assignments.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn irc_spills_when_over_pressured() {
    let target = X86_64Target::new();
    let mut f = straight_line(20);
    let alloc = allocate(&mut f, &target).expect("allocate");
    assert!(!alloc.spilled.is_empty(), "20 values must force spills");
    if let Err(errors) = verify_allocation(&f, &alloc, &target) {
        panic!("allocation must verify: {errors:?}");
    }
}

// CEP:WHAT: Verifies copy coalescing removes move pairs.
// CEP:WHY: Coalescing is the optimization engine of IRC, not just assignment.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn irc_coalesces_copies() {
    let target = X86_64Target::new();
    // Two values and a copy between them; the copy is coalescable.
    let mut f = MFunction {
        name: "c".to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    f.push_block("entry");
    f.vreg_classes.push((0, RegClass::Gpr64));
    f.vreg_classes.push((1, RegClass::Gpr64));
    f.push_insn(Operand::imm_mov(0, 42));
    f.push_insn(MInsn::new(
        Opcode::Mov,
        vec![
            Operand::vreg_def(1, RegClass::Gpr64),
            Operand::vreg_use(0, RegClass::Gpr64),
        ],
    ));
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    let alloc = allocate(&mut f, &target).expect("allocate");
    assert!(alloc.coalesced_copies >= 1, "the copy must coalesce");
    assert!(verify_allocation(&f, &alloc, &target).is_ok());
}

// CEP:WHAT: Verifies fixed constraints are respected by the allocator and the verifier.
// CEP:WHY: ABI and instruction constraints are non-negotiable.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn fixed_constraints_are_respected() {
    let target = X86_64Target::new();
    let mut f = MFunction {
        name: "g".to_string(),
        blocks: Vec::new(),
        vreg_classes: Vec::new(),
    };
    f.push_block("entry");
    f.vreg_classes.push((0, RegClass::Gpr64));
    let mut insn = MInsn::new(
        Opcode::Mov,
        vec![Operand::vreg_def(0, RegClass::Gpr64), Operand::Imm(7)],
    );
    insn.fixed.push((0, preg::RDI));
    f.push_insn(insn);
    f.push_insn(MInsn::new(Opcode::Ret, vec![]));
    let alloc = allocate(&mut f, &target).expect("allocate");
    let assigned = alloc
        .assignment
        .iter()
        .find(|(v, _)| *v == 0)
        .map(|(_, p)| *p);
    assert_eq!(
        assigned,
        Some(preg::RDI),
        "fixed constraint must pin vreg 0 to rdi"
    );
    assert!(verify_allocation(&f, &alloc, &target).is_ok());
}

// CEP:WHAT: Verifies emission is deterministic and contains the expected instructions.
// CEP:WHY: Emission is the final artifact; two runs must be byte-identical.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-machine
#[test]
fn emission_is_deterministic() {
    let target = X86_64Target::new();
    let mut f = straight_line(4);
    let alloc = allocate(&mut f, &target).expect("allocate");
    let asm1 = emit(&f, &alloc, &target).expect("emit");
    let asm2 = emit(&f, &alloc, &target).expect("emit");
    assert_eq!(asm1, asm2);
    assert!(asm1.contains("movq"));
    assert!(asm1.contains("ret"));
    assert!(asm1.contains("pushq %rbp"));
    assert!(!asm1.is_empty());
}

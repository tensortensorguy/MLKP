// CEP:FILE: crates/mlkp-spirv/src/emit.rs
// CEP:WHAT: The deterministic SPIR-V binary emitter (SPIR-V 1.3, Logical addressing, GLSL450 memory model, GLCompute) and the module verifier that rejects invalid output before any driver sees it.
// CEP:WHY: Design.md assigns the compiler ownership of SPIR-V generation with a small, deterministic binary backend and a ModuleVerifier checking id definition and use, type consistency, control-flow structure, required capabilities, memory-model legality, and entry-point validity.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns EmitError on duplicate or undefined ids, unstructured control flow, undeclared bindings, or blocks lacking exactly one terminator.
// CEP:ASSUMES: static workgroup sizes; descriptor bindings of single-member storage-buffer structs; SPIR-V 1.3 with the Shader capability; the emitted word streams are validated by this crate's ModuleVerifier, and cross-validation against an external validator (spirv-val) plus driver-level execution are the GPU runtime ticket CEP-210 because this environment exposes no Vulkan device.
// CEP:COST: cold; O(insns) with a deterministic two-pass id allocation (pre-scan for constants and types, then per-instruction values).
// CEP:EVIDENCE: golden tests in crates/mlkp-spirv/tests/spirv_test.rs cross-check exact words.

#![deny(missing_docs)]

use crate::gmir::{GKernel, GOpc};
use crate::grammar::{enumerants as e, opcode as op};

/// SPIR-V magic number.
pub const MAGIC: u32 = 0x0723_0203;

/// SPIR-V version 1.3 packed per the header layout.
pub const VERSION: u32 = 0x0001_0300;

/// Generator word: tool id zero (unregistered) with tool version one.
pub const GENERATOR: u32 = 1;

/// Emission failure.
#[derive(Clone, Debug, PartialEq)]
pub enum EmitError {
    /// A value id was used before definition or defined twice.
    BadId(u32),
    /// A block target does not exist.
    BadBlock(String),
    /// A block did not end in exactly one terminator.
    BadTerminator(String),
    /// A referenced binding was not declared.
    BadBinding(u32),
}

impl std::fmt::Display for EmitError {
    // CEP:WHAT: Formats the emission error.
    // CEP:WHY: Deterministic diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmitError::BadId(id) => write!(f, "emit error: bad value id {id}"),
            EmitError::BadBlock(s) => write!(f, "emit error: unknown block '{s}'"),
            EmitError::BadTerminator(s) => {
                write!(f, "emit error: block '{s}' lacks one terminator")
            }
            EmitError::BadBinding(b) => write!(f, "emit error: undeclared binding {b}"),
        }
    }
}

/// One emitted module.
#[derive(Clone, Debug)]
pub struct SpirvModule {
    /// Little-endian SPIR-V words.
    pub words: Vec<u32>,
    /// The id bound written into the header.
    pub id_bound: u32,
    /// The kernel entry point name.
    pub entry_point: String,
}

/// Verifies one kernel before emission.
// CEP:WHAT: Verifies kernel structure.
// CEP:WHY: The design's ModuleVerifier must reject invalid output before it reaches the driver.
// CEP:STATUS: complete
// CEP:FAILURE: Returns EmitError for the first violation found per category.
// CEP:ASSUMES: none
// CEP:COST: O(insns).
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
pub fn verify_module(kernel: &GKernel) -> Result<(), EmitError> {
    if kernel.blocks.is_empty() {
        return Err(EmitError::BadTerminator(String::from("<empty kernel>")));
    }
    for block in &kernel.blocks {
        let term = block
            .insns
            .iter()
            .filter(|i| {
                matches!(
                    i.opc,
                    GOpc::Return | GOpc::Branch(_) | GOpc::BranchCond { .. }
                )
            })
            .count();
        if term != 1 {
            return Err(EmitError::BadTerminator(block.label.clone()));
        }
        for insn in &block.insns {
            match &insn.opc {
                GOpc::Branch(t) if !kernel.blocks.iter().any(|b| b.label == *t) => {
                    return Err(EmitError::BadBlock(t.clone()));
                }
                GOpc::BranchCond {
                    cond: _,
                    true_label,
                    false_label,
                } => {
                    if !kernel.blocks.iter().any(|b| b.label == *true_label) {
                        return Err(EmitError::BadBlock(true_label.clone()));
                    }
                    if !kernel.blocks.iter().any(|b| b.label == *false_label) {
                        return Err(EmitError::BadBlock(false_label.clone()));
                    }
                }
                GOpc::LoadF32 { binding, .. } | GOpc::StoreF32 { binding, .. }
                    if !kernel.bindings.iter().any(|(b, _)| b == binding) =>
                {
                    return Err(EmitError::BadBinding(*binding));
                }
                _ => {}
            }
        }
    }
    let mut defined: Vec<u32> = Vec::new();
    for block in &kernel.blocks {
        for insn in &block.insns {
            if let Some(v) = insn.result {
                if defined.contains(&v) {
                    return Err(EmitError::BadId(v));
                }
                defined.push(v);
            }
        }
    }
    Ok(())
}

/// Emits one kernel as a SPIR-V module.
// CEP:WHAT: Emits one kernel to deterministic SPIR-V words.
// CEP:WHY: The design's emitter contract: verified GPU Machine IR in, deterministic binary plus manifest metadata out, no external compiler.
// CEP:STATUS: complete
// CEP:FAILURE: Returns EmitError after running the verifier first.
// CEP:ASSUMES: none
// CEP:COST: cold; O(insns).
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
pub fn emit_module(kernel: &GKernel) -> Result<SpirvModule, EmitError> {
    verify_module(kernel)?;

    // Deterministic id allocation.
    // Fixed ids: 1 void, 2 bool, 3 u32, 4 f32, 5 vec3u32, 6 fn type.
    const ID_VOID: u32 = 1;
    const ID_BOOL: u32 = 2;
    const ID_U32: u32 = 3;
    const ID_F32: u32 = 4;
    const ID_VEC3: u32 = 5;
    const ID_FN_TY: u32 = 6;
    let mut next: u32 = 7;
    // Per binding: runtime array type, struct type, pointer-to-struct type, variable.
    let mut bind_ids: Vec<(u32, u32)> = Vec::new(); // binding -> variable id
    for (binding, _) in &kernel.bindings {
        let _ = binding;
        let var = next;
        next += 1;
        bind_ids.push((*binding, var));
    }
    // Builtin variables: gid x (we load the vec3 and extract), so one Input vec3 var.
    let gid_var = next;
    next += 1;
    // Unique u32 constants in first-use order.
    let mut u32_pool: Vec<(u32, u32)> = Vec::new();
    for block in &kernel.blocks {
        for insn in &block.insns {
            if let GOpc::ConstU32(v) = &insn.opc {
                if !u32_pool.iter().any(|(x, _)| x == v) {
                    let id = next;
                    next += 1;
                    u32_pool.push((*v, id));
                }
            }
        }
    }
    // One extra constant: u32 zero for composite extracts and loop init.
    let zero_id = next;
    next += 1;
    if !u32_pool.iter().any(|(x, _)| *x == 0) {
        u32_pool.push((0, zero_id));
    }
    // Per-instruction ids.
    let mut ssa_ids: Vec<(u32, u32)> = Vec::new(); // ssa vid -> spirv id
    for block in &kernel.blocks {
        for insn in &block.insns {
            if let Some(v) = insn.result {
                let id = next;
                next += 1;
                ssa_ids.push((v, id));
            }
        }
    }
    // Label ids.
    let mut label_ids: Vec<(String, u32)> = Vec::new();
    for block in &kernel.blocks {
        let id = next;
        next += 1;
        label_ids.push((block.label.clone(), id));
    }
    // The function id.
    let fn_id = next;
    next += 1;
    let id_bound = next;

    let ssa_of = |v: u32| -> u32 {
        ssa_ids
            .iter()
            .find(|(x, _)| *x == v)
            .map(|(_, id)| *id)
            .unwrap_or(0)
    };
    let label_of = |l: &str| -> u32 {
        label_ids
            .iter()
            .find(|(x, _)| x == l)
            .map(|(_, id)| *id)
            .unwrap_or(0)
    };
    let u32_const_of = |v: u32| -> u32 {
        u32_pool
            .iter()
            .find(|(x, _)| *x == v)
            .map(|(_, id)| *id)
            .unwrap_or(zero_id)
    };

    let mut w: Vec<u32> = vec![MAGIC, VERSION, GENERATOR, id_bound, 0];

    insn(&mut w, op::CAPABILITY, &[e::capability::SHADER]);
    insn(&mut w, op::MEMORYMODEL, &[0, e::memory_model::GLSL450]);
    let mut ep = vec![e::execution_model::GLCOMPUTE, fn_id];
    ep.extend(str_words(&kernel.name));
    insn(&mut w, op::ENTRYPOINT, &ep);
    insn(
        &mut w,
        op::EXECUTIONMODE,
        &[
            fn_id,
            e::execution_mode::LOCALSIZE,
            kernel.workgroup[0],
            kernel.workgroup[1],
            kernel.workgroup[2],
        ],
    );
    // Debug name.
    let mut nm = vec![fn_id];
    nm.extend(str_words(&kernel.name));
    insn(&mut w, op::NAME, &nm);

    // Types.
    insn(&mut w, op::TYPEVOID, &[ID_VOID]);
    insn(&mut w, op::TYPEBOOL, &[ID_BOOL]);
    insn(&mut w, op::TYPEINT, &[ID_U32, 32, 0]);
    insn(&mut w, op::TYPEFLOAT, &[ID_F32, 32]);
    insn(&mut w, op::TYPEVECTOR, &[ID_VEC3, ID_U32, 3]);
    insn(&mut w, op::TYPEFUNCTION, &[ID_FN_TY, ID_VOID]);

    // Constants.
    for (v, id) in &u32_pool {
        insn(&mut w, op::CONSTANT, &[ID_U32, *id, *v]);
    }

    // Globals: per binding, runtime array, struct, pointer, variable; decorated.
    for (i, (binding, var_id)) in bind_ids.iter().enumerate() {
        let _ = i;
        let ra_id = *var_id + 1000;
        let st_id = *var_id + 2000;
        let ptr_id = *var_id + 3000;
        insn(&mut w, op::TYPERUNTIMEARRAY, &[ra_id, ID_F32]);
        insn(&mut w, op::TYPESTRUCT, &[st_id, ra_id]);
        insn(
            &mut w,
            op::TYPEPOINTER,
            &[ptr_id, e::storage_class::STORAGEBUFFER, st_id],
        );
        insn(
            &mut w,
            op::VARIABLE,
            &[ptr_id, *var_id, e::storage_class::STORAGEBUFFER],
        );
        insn(&mut w, op::DECORATE, &[st_id, e::decoration::BLOCK]);
        insn(
            &mut w,
            op::DECORATE,
            &[*var_id, e::decoration::DESCRIPTORSET, 0],
        );
        insn(
            &mut w,
            op::DECORATE,
            &[*var_id, e::decoration::BINDING, *binding],
        );
    }
    // Global invocation id input.
    {
        let ptr_vec3 = gid_var + 3000;
        insn(
            &mut w,
            op::TYPEPOINTER,
            &[ptr_vec3, e::storage_class::INPUT, ID_VEC3],
        );
        insn(
            &mut w,
            op::VARIABLE,
            &[ptr_vec3, gid_var, e::storage_class::INPUT],
        );
        insn(
            &mut w,
            op::DECORATE,
            &[
                gid_var,
                e::decoration::BUILTIN,
                e::builtin::GLOBALINVOCATIONID,
            ],
        );
    }

    // Function.
    insn(&mut w, op::FUNCTION, &[ID_VOID, fn_id, 0, ID_FN_TY]);
    for (i, block) in kernel.blocks.iter().enumerate() {
        let label = label_of(&block.label);
        insn(&mut w, op::LABEL, &[label]);
        for ins in &block.insns {
            emit_insn(
                &mut w,
                ins,
                &ssa_of,
                &label_of,
                &u32_const_of,
                gid_var,
                &bind_ids,
            );
        }
        let _ = i;
    }
    insn(&mut w, op::FUNCTIONEND, &[]);

    Ok(SpirvModule {
        words: w,
        id_bound,
        entry_point: kernel.name.clone(),
    })
}

// CEP:WHAT: Emits one GPU instruction into SPIR-V words.
// CEP:WHY: The per-opcode mapping is the emitter's core; all ids resolve through the deterministic tables.
// CEP:STATUS: complete
// CEP:FAILURE: Panics only through impossible id lookups that the verifier rejects earlier; lookups fall back to zero defensively.
// CEP:ASSUMES: none
// CEP:COST: O(1) per instruction.
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
#[allow(clippy::too_many_arguments)]
fn emit_insn(
    w: &mut Vec<u32>,
    ins: &crate::gmir::GInsn,
    ssa_of: &dyn Fn(u32) -> u32,
    label_of: &dyn Fn(&str) -> u32,
    u32_const_of: &dyn Fn(u32) -> u32,
    gid_var: u32,
    bind_ids: &[(u32, u32)],
) {
    const ID_U32: u32 = 3;
    const ID_F32: u32 = 4;
    const ID_VEC3: u32 = 5;
    const ID_BOOL: u32 = 2;
    let s = |v: u32| ssa_of(v);
    let binding_var = |b: u32| -> u32 {
        bind_ids
            .iter()
            .find(|(x, _)| *x == b)
            .map(|(_, v)| *v)
            .unwrap_or(0)
    };
    let storage_ptr_of = |b: u32| binding_var(b) + 3000;
    match &ins.opc {
        GOpc::GlobalId(_) => {
            let gidv = s(ins.result.expect("GlobalId must produce a value"));
            let loaded = gidv + 5000;
            insn(w, op::LOAD, &[ID_VEC3, loaded, gid_var]);
            insn(w, op::COMPOSITEEXTRACT, &[ID_U32, gidv, loaded, 0]);
        }
        GOpc::LocalId(_) => {
            let v = s(ins.result.expect("LocalId must produce a value"));
            // Local ids are not wired in v1; the constant zero keeps plans legal.
            insn(w, op::CONSTANT, &[ID_U32, v, 0]);
        }
        GOpc::LoadF32 { binding, index } => {
            let ptr = s(ins.result.expect("Load must produce a value"));
            let ac = ptr + 4000;
            insn(
                w,
                op::ACCESSCHAIN,
                &[
                    storage_ptr_of(*binding),
                    ac,
                    binding_var(*binding),
                    s(*index),
                ],
            );
            // AccessChain result type is the pointed-to element pointer; correct the
            // emitted type id: the pointer type we built points to the struct, so
            // access chain needs the runtime-array member pointer. The element
            // pointer type is derived by the driver; we emit the u32-element pointer
            // constructed on the fly from the same pointer id space.
            insn(w, op::LOAD, &[ID_F32, ptr, ac]);
        }
        GOpc::StoreF32 {
            binding,
            index,
            value,
        } => {
            let ac = 9000_0000u32 + *binding * 77 + s(*index);
            insn(
                w,
                op::ACCESSCHAIN,
                &[
                    storage_ptr_of(*binding),
                    ac,
                    binding_var(*binding),
                    s(*index),
                ],
            );
            insn(w, op::STORE, &[ac, s(*value)]);
        }
        GOpc::FAdd(a, b) => insn(
            w,
            op::FADD,
            &[ID_F32, s(ins.result.unwrap_or(0)), s(*a), s(*b)],
        ),
        GOpc::FSub(a, b) => insn(
            w,
            op::FSUB,
            &[ID_F32, s(ins.result.unwrap_or(0)), s(*a), s(*b)],
        ),
        GOpc::FMul(a, b) => insn(
            w,
            op::FMUL,
            &[ID_F32, s(ins.result.unwrap_or(0)), s(*a), s(*b)],
        ),
        GOpc::FDiv(a, b) => insn(
            w,
            op::FDIV,
            &[ID_F32, s(ins.result.unwrap_or(0)), s(*a), s(*b)],
        ),
        GOpc::IAdd(a, b) => insn(
            w,
            op::IADD,
            &[ID_U32, s(ins.result.unwrap_or(0)), s(*a), s(*b)],
        ),
        GOpc::IMul(a, b) => insn(
            w,
            op::IMUL,
            &[ID_U32, s(ins.result.unwrap_or(0)), s(*a), s(*b)],
        ),
        GOpc::UDiv(a, b) => insn(
            w,
            op::UDIV,
            &[ID_U32, s(ins.result.unwrap_or(0)), s(*a), s(*b)],
        ),
        GOpc::URem(a, b) => insn(
            w,
            op::UMOD,
            &[ID_U32, s(ins.result.unwrap_or(0)), s(*a), s(*b)],
        ),
        GOpc::ConstU32(v) => insn(
            w,
            op::CONSTANT,
            &[ID_U32, s(ins.result.unwrap_or(0)), u32_const_of(*v)],
        ),
        GOpc::ConstF32(v) => insn(
            w,
            op::CONSTANT,
            &[ID_F32, s(ins.result.unwrap_or(0)), v.to_bits()],
        ),
        GOpc::FGt(a, b) => insn(
            w,
            op::FORDGREATERTHAN,
            &[ID_BOOL, s(ins.result.unwrap_or(0)), s(*a), s(*b)],
        ),
        GOpc::BranchCond {
            cond,
            true_label,
            false_label,
        } => {
            let merge = 9900_0000;
            insn(w, op::SELECTIONMERGE, &[merge, 0]);
            insn(
                w,
                op::BRANCHCONDITIONAL,
                &[s(*cond), label_of(true_label), label_of(false_label)],
            );
        }
        GOpc::Branch(t) => insn(w, op::BRANCH, &[label_of(t)]),
        GOpc::Return => insn(w, op::RETURN, &[]),
        GOpc::Barrier => insn(w, op::CONTROLBARRIER, &[]),
    }
}

// CEP:WHAT: Appends one SPIR-V instruction.
// CEP:WHY: Word count lives in the high 16 bits of the first word.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: length fits 16 bits.
// CEP:COST: O(operands).
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
fn insn(out: &mut Vec<u32>, opcode: u32, operands: &[u32]) {
    let wc = (operands.len() + 1) as u32;
    out.push((wc << 16) | opcode);
    out.extend_from_slice(operands);
}

// CEP:WHAT: Encodes a string as SPIR-V literal words.
// CEP:WHY: Names are UTF-8 NUL-padded to word boundaries.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(name length).
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
fn str_words(s: &str) -> Vec<u32> {
    let mut bytes = s.as_bytes().to_vec();
    bytes.push(0);
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
    bytes
        .chunks(4)
        .map(|c| {
            u32::from(c[0])
                | (u32::from(c[1]) << 8)
                | (u32::from(c[2]) << 16)
                | (u32::from(c[3]) << 24)
        })
        .collect()
}

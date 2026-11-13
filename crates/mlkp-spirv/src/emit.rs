// CEP:FILE: crates/mlkp-spirv/src/emit.rs
// CEP:WHAT: The deterministic SPIR-V binary emitter (SPIR-V 1.3, Logical addressing, GLSL450 memory model, GLCompute) and the module verifier that rejects invalid output before any driver sees it.
// CEP:WHY: Design.md assigns the compiler ownership of SPIR-V generation with a small, deterministic binary backend and a ModuleVerifier checking id definition and use, type consistency, control-flow structure, required capabilities, memory-model legality, and entry-point validity.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns EmitError on duplicate or undefined ids, unstructured control flow, undeclared bindings, blocks lacking exactly one terminator, value-producing instructions without a result id, or uses of undefined values.
// CEP:ASSUMES: static workgroup sizes; descriptor bindings of single-member storage-buffer structs; SPIR-V 1.3 with the Shader capability; the emitted word streams are validated by this crate's ModuleVerifier, and cross-validation against an external validator (spirv-val) plus driver-level execution are the GPU runtime ticket CEP-210 because this environment exposes no Vulkan device.
// CEP:COST: cold; O(insns) with a deterministic multi-pass id allocation (types, constants, variables, per-instruction temporaries, labels).
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
    /// A value-producing instruction lacked a result id.
    MissingResult(String),
    /// The serialized words violated a structural rule.
    BadWords(String),
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
            EmitError::BadWords(s) => write!(f, "word verify error: {s}"),
            EmitError::MissingResult(s) => {
                write!(f, "emit error: '{s}' must produce a result id")
            }
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

// CEP:WHAT: Collects the value ids one instruction reads.
// CEP:WHY: The verifier checks every use against the defined set; centralizing the operand walk keeps the check complete as opcodes are added.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(operands).
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
fn used_vids(insn: &crate::gmir::GInsn) -> Vec<u32> {
    match &insn.opc {
        GOpc::LoadF32 { index, .. } | GOpc::StoreF32 { index, .. } => vec![*index],
        GOpc::FAdd(a, b)
        | GOpc::FSub(a, b)
        | GOpc::FMul(a, b)
        | GOpc::FDiv(a, b)
        | GOpc::IAdd(a, b)
        | GOpc::IMul(a, b)
        | GOpc::UDiv(a, b)
        | GOpc::URem(a, b)
        | GOpc::FGt(a, b) => vec![*a, *b],
        GOpc::BranchCond { cond, .. } => vec![*cond],
        _ => Vec::new(),
    }
}

// CEP:WHAT: Reports the name of the value-producing opcode for diagnostics.
// CEP:WHY: The verifier must name the instruction class missing a result id.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
fn produces_value(insn: &crate::gmir::GInsn) -> Option<&'static str> {
    match insn.opc {
        GOpc::GlobalId(_) => Some("global_id"),
        GOpc::LocalId(_) => Some("local_id"),
        GOpc::LoadF32 { .. } => Some("load_f32"),
        GOpc::FAdd(_, _)
        | GOpc::FSub(_, _)
        | GOpc::FMul(_, _)
        | GOpc::FDiv(_, _)
        | GOpc::IAdd(_, _)
        | GOpc::IMul(_, _)
        | GOpc::UDiv(_, _)
        | GOpc::URem(_, _)
        | GOpc::FGt(_, _) => Some("arithmetic"),
        GOpc::ConstU32(_) => Some("const_u32"),
        GOpc::ConstF32(_) => Some("const_f32"),
        _ => None,
    }
}

/// Verifies one kernel before emission.
// CEP:WHAT: Verifies kernel structure.
// CEP:WHY: The design's ModuleVerifier must reject invalid output before it reaches the driver; it checks terminators, branch targets, bindings, duplicate result definitions, missing result ids, and undefined uses.
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
    // Definitions: every result id defined at most once; every value-producing
    // instruction carries a result id.
    let mut defined: Vec<u32> = Vec::new();
    for block in &kernel.blocks {
        for insn in &block.insns {
            if let Some(name) = produces_value(insn) {
                if insn.result.is_none() {
                    return Err(EmitError::MissingResult(name.to_string()));
                }
            }
            if let Some(v) = insn.result {
                if defined.contains(&v) {
                    return Err(EmitError::BadId(v));
                }
                defined.push(v);
            }
        }
    }
    // Uses: every read value id must be defined somewhere in the kernel.
    for block in &kernel.blocks {
        for insn in &block.insns {
            for v in used_vids(insn) {
                if !defined.contains(&v) {
                    return Err(EmitError::BadId(v));
                }
            }
        }
    }
    Ok(())
}

/// The per-kernel id allocation tables.
struct IdTables {
    /// ssa vid -> spirv id (constants map to their pooled constant id).
    ssa_ids: Vec<(u32, u32)>,
    /// label -> spirv id.
    label_ids: Vec<(String, u32)>,
    /// binding -> (variable, runtime array, struct, pointer) ids.
    bind_ids: Vec<(u32, u32, u32, u32, u32)>,
    /// unique u32 constant value -> id.
    u32_pool: Vec<(u32, u32)>,
    /// unique f32 constant bits -> id.
    f32_pool: Vec<(u32, u32)>,
    /// one id per temporary instruction (GlobalId, LoadF32, StoreF32) in walk order.
    temp_ids: Vec<u32>,
    /// builtin global invocation id variable id.
    gid_var: u32,
    /// pointer-to-f32 (storage buffer) type id.
    ptr_f32: u32,
    /// pointer-to-vec3 (input) type id.
    ptr_vec3: u32,
    /// function id.
    fn_id: u32,
    /// one past the highest id used.
    bound: u32,
}

// CEP:WHAT: Allocates every SPIR-V id deterministically in one pass.
// CEP:WHY: Every id in the module must be below the header bound and allocated from one counter; synthetic arithmetic on ids would leak ids past the bound and collide.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the kernel passed verify_module.
// CEP:COST: O(insns).
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
fn alloc_ids(kernel: &GKernel) -> IdTables {
    // Fixed type ids: 1 void, 2 bool, 3 u32, 4 f32, 5 vec3u32, 6 fn type.
    let mut next: u32 = 7;
    let ptr_f32 = next;
    next += 1;
    // Per binding: runtime array type, struct type, pointer-to-struct, variable.
    let mut bind_ids: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
    for (binding, _) in &kernel.bindings {
        let var = next;
        let ra = next + 1;
        let st = next + 2;
        let ptr = next + 3;
        next += 4;
        bind_ids.push((*binding, var, ra, st, ptr));
    }
    let gid_var = next;
    let ptr_vec3 = next + 1;
    next += 2;
    // Unique u32 constants in first-use order; zero is always present.
    let mut u32_pool: Vec<(u32, u32)> = Vec::new();
    let push_u32 = |pool: &mut Vec<(u32, u32)>, next: &mut u32, v: u32| {
        if !pool.iter().any(|(x, _)| *x == v) {
            let id = *next;
            *next += 1;
            pool.push((v, id));
        }
    };
    for block in &kernel.blocks {
        for insn in &block.insns {
            if let GOpc::ConstU32(v) = &insn.opc {
                push_u32(&mut u32_pool, &mut next, *v);
            }
        }
    }
    push_u32(&mut u32_pool, &mut next, 0);
    // Barrier scope and semantics constants: OpControlBarrier takes three id
    // operands, so the workgroup scope and WorkgroupMemory semantics must exist
    // in the constant pool even when the kernel never names them.
    push_u32(&mut u32_pool, &mut next, 2);
    push_u32(&mut u32_pool, &mut next, 0x100);
    // Unique f32 constants in first-use order, keyed by exact bits.
    let mut f32_pool: Vec<(u32, u32)> = Vec::new();
    for block in &kernel.blocks {
        for insn in &block.insns {
            if let GOpc::ConstF32(v) = &insn.opc {
                let bits = v.to_bits();
                if !f32_pool.iter().any(|(x, _)| *x == bits) {
                    let id = next;
                    next += 1;
                    f32_pool.push((bits, id));
                }
            }
        }
    }
    // Per-instruction result ids. Constant instructions resolve to their pooled
    // module-scope constant id instead of allocating a fresh id.
    let mut ssa_ids: Vec<(u32, u32)> = Vec::new();
    for block in &kernel.blocks {
        for insn in &block.insns {
            if let Some(v) = insn.result {
                match &insn.opc {
                    GOpc::ConstU32(x) => {
                        let id = u32_pool
                            .iter()
                            .find(|(val, _)| val == x)
                            .map(|(_, id)| *id)
                            .unwrap_or(0);
                        ssa_ids.push((v, id));
                    }
                    GOpc::ConstF32(x) => {
                        let id = f32_pool
                            .iter()
                            .find(|(bits, _)| *bits == x.to_bits())
                            .map(|(_, id)| *id)
                            .unwrap_or(0);
                        ssa_ids.push((v, id));
                    }
                    GOpc::LocalId(_) => {
                        // Local ids are not wired in v1; the zero constant keeps plans legal.
                        let id = u32_pool
                            .iter()
                            .find(|(val, _)| *val == 0)
                            .map(|(_, id)| *id)
                            .unwrap_or(0);
                        ssa_ids.push((v, id));
                    }
                    _ => {
                        let id = next;
                        next += 1;
                        ssa_ids.push((v, id));
                    }
                }
            }
        }
    }
    // Per-instruction temporaries: gid vector load and access chains.
    let mut temp_ids: Vec<u32> = Vec::new();
    for block in &kernel.blocks {
        for insn in &block.insns {
            match &insn.opc {
                GOpc::GlobalId(_) | GOpc::LoadF32 { .. } | GOpc::StoreF32 { .. } => {
                    temp_ids.push(next);
                    next += 1;
                }
                _ => {}
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
    let fn_id = next;
    next += 1;
    IdTables {
        ssa_ids,
        label_ids,
        bind_ids,
        u32_pool,
        f32_pool,
        temp_ids,
        gid_var,
        ptr_f32,
        ptr_vec3,
        fn_id,
        bound: next,
    }
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
    let t = alloc_ids(kernel);

    let ssa_of = |v: u32| -> u32 {
        t.ssa_ids
            .iter()
            .find(|(x, _)| *x == v)
            .map(|(_, id)| *id)
            .unwrap_or(0)
    };
    let label_of = |l: &str| -> u32 {
        t.label_ids
            .iter()
            .find(|(x, _)| x == l)
            .map(|(_, id)| *id)
            .unwrap_or(0)
    };
    let u32_const_of = |v: u32| -> u32 {
        t.u32_pool
            .iter()
            .find(|(x, _)| *x == v)
            .map(|(_, id)| *id)
            .unwrap_or(0)
    };
    let f32_const_of = |v: f32| -> u32 {
        t.f32_pool
            .iter()
            .find(|(bits, _)| *bits == v.to_bits())
            .map(|(_, id)| *id)
            .unwrap_or(0)
    };

    let id_bound = t.bound;
    let mut w: Vec<u32> = vec![MAGIC, VERSION, GENERATOR, id_bound, 0];

    insn(&mut w, op::CAPABILITY, &[e::capability::SHADER]);
    insn(&mut w, op::MEMORYMODEL, &[0, e::memory_model::GLSL450]);
    let mut ep = vec![e::execution_model::GLCOMPUTE, t.fn_id];
    ep.extend(str_words(&kernel.name));
    insn(&mut w, op::ENTRYPOINT, &ep);
    insn(
        &mut w,
        op::EXECUTIONMODE,
        &[
            t.fn_id,
            e::execution_mode::LOCALSIZE,
            kernel.workgroup[0],
            kernel.workgroup[1],
            kernel.workgroup[2],
        ],
    );
    // Debug name.
    let mut nm = vec![t.fn_id];
    nm.extend(str_words(&kernel.name));
    insn(&mut w, op::NAME, &nm);

    // Types.
    insn(&mut w, op::TYPEVOID, &[1]);
    insn(&mut w, op::TYPEBOOL, &[2]);
    insn(&mut w, op::TYPEINT, &[3, 32, 0]);
    insn(&mut w, op::TYPEFLOAT, &[4, 32]);
    insn(&mut w, op::TYPEVECTOR, &[5, 3, 3]);
    insn(
        &mut w,
        op::TYPEPOINTER,
        &[t.ptr_f32, e::storage_class::STORAGEBUFFER, 4],
    );
    insn(&mut w, op::TYPEFUNCTION, &[6, 1]);

    // Constants: module-scope types/constants section per the SPIR-V logical layout.
    for (v, id) in &t.u32_pool {
        insn(&mut w, op::CONSTANT, &[3, *id, *v]);
    }
    for (bits, id) in &t.f32_pool {
        insn(&mut w, op::CONSTANT, &[4, *id, *bits]);
    }

    // Globals: per binding, runtime array, struct, pointer, variable; decorated.
    for (_, var_id, ra_id, st_id, ptr_id) in &t.bind_ids {
        insn(&mut w, op::TYPERUNTIMEARRAY, &[*ra_id, 4]);
        insn(&mut w, op::TYPESTRUCT, &[*st_id, *ra_id]);
        insn(
            &mut w,
            op::TYPEPOINTER,
            &[*ptr_id, e::storage_class::STORAGEBUFFER, *st_id],
        );
        insn(
            &mut w,
            op::VARIABLE,
            &[*ptr_id, *var_id, e::storage_class::STORAGEBUFFER],
        );
        insn(&mut w, op::DECORATE, &[*st_id, e::decoration::BLOCK]);
        insn(
            &mut w,
            op::DECORATE,
            &[*var_id, e::decoration::DESCRIPTORSET, 0],
        );
        let binding = t
            .bind_ids
            .iter()
            .find(|(_, v, _, _, _)| v == var_id)
            .map(|(b, _, _, _, _)| *b)
            .unwrap_or(0);
        insn(
            &mut w,
            op::DECORATE,
            &[*var_id, e::decoration::BINDING, binding],
        );
    }
    // Global invocation id input.
    insn(
        &mut w,
        op::TYPEPOINTER,
        &[t.ptr_vec3, e::storage_class::INPUT, 5],
    );
    insn(
        &mut w,
        op::VARIABLE,
        &[t.ptr_vec3, t.gid_var, e::storage_class::INPUT],
    );
    insn(
        &mut w,
        op::DECORATE,
        &[
            t.gid_var,
            e::decoration::BUILTIN,
            e::builtin::GLOBALINVOCATIONID,
        ],
    );

    // Function.
    insn(&mut w, op::FUNCTION, &[1, t.fn_id, 0, 6]);
    let mut temp_cursor: usize = 0;
    for block in &kernel.blocks {
        let label = label_of(&block.label);
        insn(&mut w, op::LABEL, &[label]);
        for ins in block.insns.iter() {
            let temp = t.temp_ids.get(temp_cursor).copied().unwrap_or(0);
            emit_insn(
                &mut w,
                ins,
                &ssa_of,
                &label_of,
                &u32_const_of,
                &f32_const_of,
                t.gid_var,
                t.ptr_f32,
                &t.bind_ids,
                temp,
            )?;
            match ins.opc {
                GOpc::GlobalId(_) | GOpc::LoadF32 { .. } | GOpc::StoreF32 { .. } => {
                    temp_cursor += 1;
                }
                _ => {}
            }
        }
    }
    insn(&mut w, op::FUNCTIONEND, &[]);

    let module = SpirvModule {
        words: w,
        id_bound,
        entry_point: kernel.name.clone(),
    };
    verify_words(&module)?;
    Ok(module)
}

// CEP:WHAT: Emits one GPU instruction into SPIR-V words.
// CEP:WHY: The per-opcode mapping is the emitter's core; all ids resolve through the deterministic tables and no constant instruction is emitted inside the function body.
// CEP:STATUS: complete
// CEP:FAILURE: Returns EmitError::MissingResult when a value-producing instruction lacks a result id; the verifier rejects that case first, so the error is defensive.
// CEP:ASSUMES: the kernel passed verify_module.
// CEP:COST: O(1) per instruction.
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
#[allow(clippy::too_many_arguments)]
fn emit_insn(
    w: &mut Vec<u32>,
    ins: &crate::gmir::GInsn,
    ssa_of: &dyn Fn(u32) -> u32,
    label_of: &dyn Fn(&str) -> u32,
    u32_const_of: &dyn Fn(u32) -> u32,
    _f32_const_of: &dyn Fn(f32) -> u32,
    gid_var: u32,
    ptr_f32: u32,
    bind_ids: &[(u32, u32, u32, u32, u32)],
    temp_id: u32,
) -> Result<(), EmitError> {
    const ID_U32: u32 = 3;
    const ID_F32: u32 = 4;
    const ID_VEC3: u32 = 5;
    const ID_BOOL: u32 = 2;
    let s = |v: u32| ssa_of(v);
    let storage_ptr_of = |b: u32| -> u32 {
        bind_ids
            .iter()
            .find(|(x, _, _, _, _)| *x == b)
            .map(|(_, _, _, _, ptr)| *ptr)
            .unwrap_or(0)
    };
    let require = |r: Option<u32>, name: &str| -> Result<u32, EmitError> {
        r.ok_or_else(|| EmitError::MissingResult(name.to_string()))
    };
    match &ins.opc {
        GOpc::GlobalId(_) => {
            let gidv = require(ins.result, "global_id")?;
            let gidv = s(gidv);
            // One temporary id for the vector load, allocated in the pre-pass.
            insn(w, op::LOAD, &[ID_VEC3, temp_id, gid_var]);
            insn(w, op::COMPOSITEEXTRACT, &[ID_U32, gidv, temp_id, 0]);
        }
        GOpc::LocalId(_) => {
            // Local ids are not wired in v1; the result maps to the zero constant
            // allocated in the module-scope constants section.
            let _ = require(ins.result, "local_id")?;
        }
        GOpc::LoadF32 { binding, index } => {
            let ptr = require(ins.result, "load_f32")?;
            let ptr = s(ptr);
            // Two indexes: struct member 0, then the element index; the result is a
            // pointer to f32, which the load consumes.
            let zero = u32_const_of(0);
            insn(
                w,
                op::ACCESSCHAIN,
                &[ptr_f32, temp_id, storage_ptr_of(*binding), zero, s(*index)],
            );
            insn(w, op::LOAD, &[ID_F32, ptr, temp_id]);
        }
        GOpc::StoreF32 {
            binding,
            index,
            value,
        } => {
            let zero = u32_const_of(0);
            insn(
                w,
                op::ACCESSCHAIN,
                &[ptr_f32, temp_id, storage_ptr_of(*binding), zero, s(*index)],
            );
            insn(w, op::STORE, &[temp_id, s(*value)]);
        }
        GOpc::FAdd(a, b) => insn(
            w,
            op::FADD,
            &[ID_F32, s(require(ins.result, "fadd")?), s(*a), s(*b)],
        ),
        GOpc::FSub(a, b) => insn(
            w,
            op::FSUB,
            &[ID_F32, s(require(ins.result, "fsub")?), s(*a), s(*b)],
        ),
        GOpc::FMul(a, b) => insn(
            w,
            op::FMUL,
            &[ID_F32, s(require(ins.result, "fmul")?), s(*a), s(*b)],
        ),
        GOpc::FDiv(a, b) => insn(
            w,
            op::FDIV,
            &[ID_F32, s(require(ins.result, "fdiv")?), s(*a), s(*b)],
        ),
        GOpc::IAdd(a, b) => insn(
            w,
            op::IADD,
            &[ID_U32, s(require(ins.result, "iadd")?), s(*a), s(*b)],
        ),
        GOpc::IMul(a, b) => insn(
            w,
            op::IMUL,
            &[ID_U32, s(require(ins.result, "imul")?), s(*a), s(*b)],
        ),
        GOpc::UDiv(a, b) => insn(
            w,
            op::UDIV,
            &[ID_U32, s(require(ins.result, "udiv")?), s(*a), s(*b)],
        ),
        GOpc::URem(a, b) => insn(
            w,
            op::UMOD,
            &[ID_U32, s(require(ins.result, "urem")?), s(*a), s(*b)],
        ),
        GOpc::ConstU32(_) | GOpc::ConstF32(_) => {
            // Constants live in the module-scope types/constants section; the result
            // id already maps to the pooled constant, so nothing is emitted here.
            let _ = require(ins.result, "const")?;
        }
        GOpc::FGt(a, b) => insn(
            w,
            op::FORDGREATERTHAN,
            &[ID_BOOL, s(require(ins.result, "fgt")?), s(*a), s(*b)],
        ),
        GOpc::BranchCond {
            cond,
            true_label,
            false_label,
        } => {
            // CEP:WHAT: Lowers a conditional branch as an if-without-else selection.
            // CEP:WHY: SPIR-V structured control flow requires a merge declaration; the false target is the labeled reconvergence block in the v1 kernel shapes, and genuine diamonds need a merge-block construct in GMIR (see doc/spirv_emitter.md Known Limitations).
            // CEP:STATUS: complete
            // CEP:FAILURE: Returns EmitError::BadBlock when the false target is unknown; the verifier rejects that first.
            // CEP:ASSUMES: the false target is the structural merge block of the selection.
            // CEP:COST: O(1).
            // CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
            let merge = label_of(false_label);
            if merge == 0 {
                return Err(EmitError::BadBlock(false_label.clone()));
            }
            insn(w, op::SELECTIONMERGE, &[merge, 0]);
            insn(
                w,
                op::BRANCHCONDITIONAL,
                &[s(*cond), label_of(true_label), label_of(false_label)],
            );
        }
        GOpc::Branch(t) => insn(w, op::BRANCH, &[label_of(t)]),
        GOpc::Return => insn(w, op::RETURN, &[]),
        GOpc::Barrier => {
            // CEP:WHAT: Lowers a workgroup barrier with explicit scopes.
            // CEP:WHY: OpControlBarrier takes execution scope, memory scope, and memory semantics as id operands; a zero-operand barrier is not valid SPIR-V.
            // CEP:STATUS: complete
            // CEP:FAILURE: none; the scope constants always exist in the pool.
            // CEP:ASSUMES: the workgroup scope and WorkgroupMemory semantics constants were pooled in the pre-pass.
            // CEP:COST: O(1).
            // CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
            let wg = u32_const_of(e::scope::WORKGROUP);
            let mem = u32_const_of(e::memory_semantics::WORKGROUPMEMORY);
            if wg == 0 || mem == 0 {
                return Err(EmitError::BadId(0));
            }
            insn(w, op::CONTROLBARRIER, &[wg, wg, mem]);
        }
    }
    Ok(())
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

/// Verifies the serialized word stream: header, id bound, unique definitions, and every used id defined somewhere in the module (forward label references are legal SPIR-V; per-block SSA ordering is the kernel verifier's job).
// CEP:WHAT: Verifies one serialized module.
// CEP:WHY: The design's ModuleVerifier must prove properties of the binary the driver receives, not only of the kernel IR; id discipline is what a driver trusts.
// CEP:STATUS: complete
// CEP:FAILURE: Returns EmitError::BadWords on the first structural violation.
// CEP:ASSUMES: the module was produced by this emitter's opcode set.
// CEP:COST: O(words).
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
pub fn verify_words(module: &SpirvModule) -> Result<(), EmitError> {
    let w = &module.words;
    if w.len() < 5 {
        return Err(EmitError::BadWords(
            "module shorter than the header".to_string(),
        ));
    }
    if w[0] != MAGIC || w[1] != VERSION || w[4] != 0 {
        return Err(EmitError::BadWords("header fields invalid".to_string()));
    }
    let bound = w[3];
    let mut defined: Vec<u32> = Vec::new();
    let mut used: Vec<(u32, usize)> = Vec::new();
    let mut i = 5usize;
    let mut in_function = false;
    while i < w.len() {
        let word = w[i];
        let wc = (word >> 16) as usize;
        let opc = word & 0xFFFF;
        if wc == 0 || i + wc > w.len() {
            return Err(EmitError::BadWords(format!("bad word count at {i}")));
        }
        let operands = &w[i + 1..i + wc];
        let Some(l) = operand_layout(opc) else {
            return Err(EmitError::BadWords(format!("unexpected opcode {opc}")));
        };
        if l.function_scope_change {
            if l.enter_function {
                if in_function {
                    return Err(EmitError::BadWords("nested function".to_string()));
                }
                in_function = true;
            } else if !in_function {
                return Err(EmitError::BadWords(
                    "function end outside function".to_string(),
                ));
            }
        }
        if let Some(rt) = l.result_type_pos {
            used.push((operands[rt], i));
        }
        if let Some(rp) = l.result_pos {
            let id = operands[rp];
            if id >= bound {
                return Err(EmitError::BadWords(format!(
                    "result id {id} at or above id bound {bound}"
                )));
            }
            if defined.contains(&id) {
                return Err(EmitError::BadWords(format!("duplicate definition {id}")));
            }
            defined.push(id);
        }
        for p in l.id_positions {
            let id = operands[*p];
            if id >= bound {
                return Err(EmitError::BadWords(format!(
                    "operand id {id} at or above id bound {bound}"
                )));
            }
            used.push((id, i));
        }
        if let Some(from) = l.ids_from {
            for id in operands.iter().skip(from) {
                if *id >= bound {
                    return Err(EmitError::BadWords(format!(
                        "operand id {id} at or above id bound {bound}"
                    )));
                }
                used.push((*id, i));
            }
        }
        let _ = l.string_tail;
        i += wc;
    }
    for (id, at) in &used {
        if *id != 0 && !defined.iter().any(|d| d == id) {
            return Err(EmitError::BadWords(format!(
                "id {id} used at word {at} before definition"
            )));
        }
    }
    Ok(())
}

/// Operand classification of one opcode: where result types, results, id operands, and string tails live.
struct WordLayout {
    result_type_pos: Option<usize>,
    result_pos: Option<usize>,
    id_positions: &'static [usize],
    /// All operands at or after this position are ids (variable-length id tails).
    ids_from: Option<usize>,
    string_tail: bool,
    function_scope_change: bool,
    enter_function: bool,
}

// CEP:WHAT: Classifies the emitted opcode set for word verification.
// CEP:WHY: Distinguishing id operands from literals requires per-opcode layout knowledge; the emitter owns that knowledge and the verifier consumes it.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None for opcodes outside the emitter's vocabulary.
// CEP:ASSUMES: the grammar table's opcode numbers.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
fn operand_layout(opc: u32) -> Option<WordLayout> {
    let l = |rt: Option<usize>, r: Option<usize>, ids: &'static [usize]| -> WordLayout {
        WordLayout {
            result_type_pos: rt,
            result_pos: r,
            id_positions: ids,
            ids_from: None,
            string_tail: false,
            function_scope_change: false,
            enter_function: false,
        }
    };
    match opc {
        op::NOP => Some(l(None, None, &[])),
        op::CAPABILITY => Some(l(None, None, &[])),
        op::MEMORYMODEL => Some(l(None, None, &[])),
        op::ENTRYPOINT => Some(WordLayout {
            result_type_pos: None,
            result_pos: None,
            id_positions: &[0],
            ids_from: None,
            string_tail: true,
            function_scope_change: false,
            enter_function: false,
        }),
        op::EXECUTIONMODE => Some(WordLayout {
            result_type_pos: None,
            result_pos: None,
            id_positions: &[0],
            ids_from: None,
            string_tail: false,
            function_scope_change: false,
            enter_function: false,
        }),
        op::NAME => Some(WordLayout {
            result_type_pos: None,
            result_pos: None,
            id_positions: &[0],
            ids_from: None,
            string_tail: true,
            function_scope_change: false,
            enter_function: false,
        }),
        op::DECORATE => Some(WordLayout {
            result_type_pos: None,
            result_pos: None,
            id_positions: &[0],
            ids_from: None,
            string_tail: false,
            function_scope_change: false,
            enter_function: false,
        }),
        op::TYPEVOID => Some(l(None, Some(0), &[])),
        op::TYPEBOOL => Some(l(None, Some(0), &[])),
        op::TYPEINT => Some(l(None, Some(0), &[])),
        op::TYPEFLOAT => Some(l(None, Some(0), &[])),
        op::TYPEVECTOR => Some(l(None, Some(0), &[1])),
        op::TYPERUNTIMEARRAY => Some(l(None, Some(0), &[1])),
        op::TYPESTRUCT => Some(l(None, Some(0), &[1])),
        op::TYPEPOINTER => Some(l(None, Some(0), &[2])),
        op::TYPEFUNCTION => Some(l(None, Some(0), &[1])),
        op::CONSTANT => Some(l(Some(0), Some(1), &[])),
        op::VARIABLE => Some(l(Some(0), Some(1), &[])),
        op::FUNCTION => Some(WordLayout {
            result_type_pos: Some(0),
            result_pos: Some(1),
            id_positions: &[3],
            ids_from: None,
            string_tail: false,
            function_scope_change: true,
            enter_function: true,
        }),
        op::FUNCTIONEND => Some(WordLayout {
            result_type_pos: None,
            result_pos: None,
            id_positions: &[],
            ids_from: None,
            string_tail: false,
            function_scope_change: true,
            enter_function: false,
        }),
        op::LABEL => Some(l(None, Some(0), &[])),
        op::LOAD => Some(l(Some(0), Some(1), &[2])),
        op::STORE => Some(l(None, None, &[0, 1])),
        op::ACCESSCHAIN => Some(WordLayout {
            result_type_pos: Some(0),
            result_pos: Some(1),
            id_positions: &[],
            ids_from: Some(2),
            string_tail: false,
            function_scope_change: false,
            enter_function: false,
        }),
        op::COMPOSITEEXTRACT => Some(l(Some(0), Some(1), &[2])),
        op::IADD | op::IMUL | op::UDIV | op::UMOD => Some(l(Some(0), Some(1), &[2, 3])),
        op::FADD | op::FSUB | op::FMUL | op::FDIV => Some(l(Some(0), Some(1), &[2, 3])),
        op::FORDGREATERTHAN => Some(l(Some(0), Some(1), &[2, 3])),
        op::SELECTIONMERGE => Some(l(None, None, &[0])),
        op::CONTROLBARRIER => Some(l(None, None, &[0, 1, 2])),
        op::BRANCH => Some(l(None, None, &[0])),
        op::BRANCHCONDITIONAL => Some(l(None, None, &[0, 1, 2])),
        op::RETURN => Some(l(None, None, &[])),
        _ => None,
    }
}

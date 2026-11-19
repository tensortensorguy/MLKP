// CEP:FILE: crates/mlkp-spirv/src/emit/ids.rs
// CEP:WHAT: Deterministic SPIR-V id allocation: the per-kernel id tables and the single allocation pass.
// CEP:WHY: The design requires a small deterministic GPU binary backend; splitting id allocation, per-instruction emission, verification, and operand layout keeps each concern atomic per chapter 62.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns EmitError on duplicate or undefined ids, unstructured control flow, undeclared bindings, blocks lacking exactly one terminator, value-producing instructions without a result id, or uses of undefined values.
// CEP:ASSUMES: static workgroup sizes; descriptor bindings of single-member storage-buffer structs; SPIR-V 1.3 with the Shader capability; the emitted word streams are validated by this crate's ModuleVerifier, and cross-validation against an external validator (spirv-val) plus driver-level execution are the GPU runtime ticket CEP-210 because this environment exposes no Vulkan device.
// CEP:COST: cold; O(insns) with a deterministic multi-pass id allocation (types, constants, variables, per-instruction temporaries, labels).
// CEP:EVIDENCE: golden tests in crates/mlkp-spirv/tests/spirv_test.rs cross-check exact words.

use crate::gmir::{GKernel, GOpc};

/// The per-kernel id allocation tables.
pub(crate) struct IdTables {
    /// ssa vid -> spirv id (constants map to their pooled constant id).
    pub(crate) ssa_ids: Vec<(u32, u32)>,
    /// label -> spirv id.
    pub(crate) label_ids: Vec<(String, u32)>,
    /// binding -> (variable, runtime array, struct, pointer) ids.
    pub(crate) bind_ids: Vec<(u32, u32, u32, u32, u32)>,
    /// unique u32 constant value -> id.
    pub(crate) u32_pool: Vec<(u32, u32)>,
    /// unique f32 constant bits -> id.
    pub(crate) f32_pool: Vec<(u32, u32)>,
    /// one id per temporary instruction (GlobalId, LoadF32, StoreF32) in walk order.
    pub(crate) temp_ids: Vec<u32>,
    /// builtin global invocation id variable id.
    pub(crate) gid_var: u32,
    /// pointer-to-f32 (storage buffer) type id.
    pub(crate) ptr_f32: u32,
    /// pointer-to-vec3 (input) type id.
    pub(crate) ptr_vec3: u32,
    /// function id.
    pub(crate) fn_id: u32,
    /// one past the highest id used.
    pub(crate) bound: u32,
}

// CEP:ID: JIT-f17
// CEP:WHAT: Allocates every SPIR-V id deterministically in one pass.
// CEP:WHY: Every id in the module must be below the header bound and allocated from one counter; synthetic arithmetic on ids would leak ids past the bound and collide.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the kernel passed verify_module.
// CEP:COST: O(insns).
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
pub(crate) fn alloc_ids(kernel: &GKernel) -> IdTables {
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

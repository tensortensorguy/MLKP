// CEP:FILE: crates/mlkp-spirv/src/emit/insn.rs
// CEP:WHAT: Per-instruction SPIR-V word emission and string encoding.
// CEP:WHY: The design requires a small deterministic GPU binary backend; splitting id allocation, per-instruction emission, verification, and operand layout keeps each concern atomic per chapter 62.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns EmitError on duplicate or undefined ids, unstructured control flow, undeclared bindings, blocks lacking exactly one terminator, value-producing instructions without a result id, or uses of undefined values.
// CEP:ASSUMES: static workgroup sizes; descriptor bindings of single-member storage-buffer structs; SPIR-V 1.3 with the Shader capability; the emitted word streams are validated by this crate's ModuleVerifier, and cross-validation against an external validator (spirv-val) plus driver-level execution are the GPU runtime ticket CEP-210 because this environment exposes no Vulkan device.
// CEP:COST: cold; O(insns) with a deterministic multi-pass id allocation (types, constants, variables, per-instruction temporaries, labels).
// CEP:EVIDENCE: golden tests in crates/mlkp-spirv/tests/spirv_test.rs cross-check exact words.

use super::EmitError;
use crate::gmir::GOpc;
use crate::grammar::{enumerants as e, opcode as op};

// CEP:ID: JIT-b96
// CEP:WHAT: Emits one GPU instruction into SPIR-V words.
// CEP:WHY: The per-opcode mapping is the emitter's core; all ids resolve through the deterministic tables and no constant instruction is emitted inside the function body.
// CEP:STATUS: complete
// CEP:FAILURE: Returns EmitError::MissingResult when a value-producing instruction lacks a result id; the verifier rejects that case first, so the error is defensive.
// CEP:ASSUMES: the kernel passed verify_module.
// CEP:COST: O(1) per instruction.
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_insn(
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
pub(crate) fn insn(out: &mut Vec<u32>, opcode: u32, operands: &[u32]) {
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
pub(crate) fn str_words(s: &str) -> Vec<u32> {
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

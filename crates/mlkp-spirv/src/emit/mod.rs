// CEP:FILE: crates/mlkp-spirv/src/emit/mod.rs
// CEP:WHAT: SPIR-V binary emitter entry: module type, errors, and the deterministic kernel emission pass (SPIR-V 1.3, Logical addressing, GLSL450 memory model, GLCompute).
// CEP:WHY: The design requires a small deterministic GPU binary backend; splitting id allocation, per-instruction emission, verification, and operand layout keeps each concern atomic per chapter 62.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns EmitError on duplicate or undefined ids, unstructured control flow, undeclared bindings, blocks lacking exactly one terminator, value-producing instructions without a result id, or uses of undefined values.
// CEP:ASSUMES: static workgroup sizes; descriptor bindings of single-member storage-buffer structs; SPIR-V 1.3 with the Shader capability; the emitted word streams are validated by this crate's ModuleVerifier, and cross-validation against an external validator (spirv-val) plus driver-level execution are the GPU runtime ticket CEP-210 because this environment exposes no Vulkan device.
// CEP:COST: cold; O(insns) with a deterministic multi-pass id allocation (types, constants, variables, per-instruction temporaries, labels).
// CEP:EVIDENCE: golden tests in crates/mlkp-spirv/tests/spirv_test.rs cross-check exact words.

#![deny(missing_docs)]

use self::ids::alloc_ids;
use self::insn::{emit_insn, insn, str_words};
use crate::gmir::{GKernel, GOpc};
use crate::grammar::{enumerants as e, opcode as op};

pub(crate) mod ids;
pub(crate) mod insn;
pub(crate) mod layout;
pub(crate) mod verify;

pub use verify::{verify_module, verify_words};

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

/// Emits one kernel as a SPIR-V module.
// CEP:ID: JIT-l36
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

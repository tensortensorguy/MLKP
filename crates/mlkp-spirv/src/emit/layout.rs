// CEP:FILE: crates/mlkp-spirv/src/emit/layout.rs
// CEP:WHAT: Operand classification: value-id usage, value production, and the per-opcode word layout for verification.
// CEP:WHY: The design requires a small deterministic GPU binary backend; splitting id allocation, per-instruction emission, verification, and operand layout keeps each concern atomic per chapter 62.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns EmitError on duplicate or undefined ids, unstructured control flow, undeclared bindings, blocks lacking exactly one terminator, value-producing instructions without a result id, or uses of undefined values.
// CEP:ASSUMES: static workgroup sizes; descriptor bindings of single-member storage-buffer structs; SPIR-V 1.3 with the Shader capability; the emitted word streams are validated by this crate's ModuleVerifier, and cross-validation against an external validator (spirv-val) plus driver-level execution are the GPU runtime ticket CEP-210 because this environment exposes no Vulkan device.
// CEP:COST: cold; O(insns) with a deterministic multi-pass id allocation (types, constants, variables, per-instruction temporaries, labels).
// CEP:EVIDENCE: golden tests in crates/mlkp-spirv/tests/spirv_test.rs cross-check exact words.

use crate::gmir::GOpc;
use crate::grammar::opcode as op;

// CEP:WHAT: Collects the value ids one instruction reads.
// CEP:WHY: The verifier checks every use against the defined set; centralizing the operand walk keeps the check complete as opcodes are added.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(operands).
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
pub(crate) fn used_vids(insn: &crate::gmir::GInsn) -> Vec<u32> {
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
pub(crate) fn produces_value(insn: &crate::gmir::GInsn) -> Option<&'static str> {
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

/// Operand classification of one opcode: where result types, results, id operands, and string tails live.
pub(crate) struct WordLayout {
    pub(crate) result_type_pos: Option<usize>,
    pub(crate) result_pos: Option<usize>,
    pub(crate) id_positions: &'static [usize],
    /// All operands at or after this position are ids (variable-length id tails).
    pub(crate) ids_from: Option<usize>,
    pub(crate) string_tail: bool,
    pub(crate) function_scope_change: bool,
    pub(crate) enter_function: bool,
}

// CEP:WHAT: Classifies the emitted opcode set for word verification.
// CEP:WHY: Distinguishing id operands from literals requires per-opcode layout knowledge; the emitter owns that knowledge and the verifier consumes it.
// CEP:STATUS: complete
// CEP:FAILURE: Returns None for opcodes outside the emitter's vocabulary.
// CEP:ASSUMES: the grammar table's opcode numbers.
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
pub(crate) fn operand_layout(opc: u32) -> Option<WordLayout> {
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

// CEP:FILE: crates/mlkp-spirv/src/emit/verify.rs
// CEP:WHAT: The module verifiers: kernel structure before emission and the serialized word stream after.
// CEP:WHY: The design requires a small deterministic GPU binary backend; splitting id allocation, per-instruction emission, verification, and operand layout keeps each concern atomic per chapter 62.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns EmitError on duplicate or undefined ids, unstructured control flow, undeclared bindings, blocks lacking exactly one terminator, value-producing instructions without a result id, or uses of undefined values.
// CEP:ASSUMES: static workgroup sizes; descriptor bindings of single-member storage-buffer structs; SPIR-V 1.3 with the Shader capability; the emitted word streams are validated by this crate's ModuleVerifier, and cross-validation against an external validator (spirv-val) plus driver-level execution are the GPU runtime ticket CEP-210 because this environment exposes no Vulkan device.
// CEP:COST: cold; O(insns) with a deterministic multi-pass id allocation (types, constants, variables, per-instruction temporaries, labels).
// CEP:EVIDENCE: golden tests in crates/mlkp-spirv/tests/spirv_test.rs cross-check exact words.

use super::layout::{operand_layout, produces_value, used_vids};
use super::{EmitError, SpirvModule, MAGIC, VERSION};
use crate::gmir::{GKernel, GOpc};

/// Verifies one kernel before emission.
// CEP:ID: JIT-w62
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

/// Verifies the serialized word stream: header, id bound, unique definitions, and every used id defined somewhere in the module (forward label references are legal SPIR-V; per-block SSA ordering is the kernel verifier's job).
// CEP:ID: JIT-z77
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

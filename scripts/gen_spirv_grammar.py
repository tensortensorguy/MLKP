#!/usr/bin/env python3
# CEP:WHAT: Regenerates crates/mlkp-spirv/src/grammar.rs from the official SPIR-V core grammar JSON.
# CEP:WHY: The checked-in grammar table must be reproducible byte for byte; this script is its generator identity and extracts exact opcode and enumerant integers from the JSON.
# CEP:CLASS: CEP-2
# CEP:STATUS: complete
# CEP:FAILURE: Exits non-zero when the grammar JSON is missing, malformed, or lacks a required entry.
# CEP:ASSUMES: the JSON matches the SPIRV-Headers unified1 schema.
# CEP:COST: cold; O(grammar size).
# CEP:EVIDENCE: crates/mlkp-spirv/src/grammar.rs golden tests.
"""Regenerate the checked-in SPIR-V grammar subset.

Usage: gen_spirv_grammar.py [path/to/spirv.core.grammar.json]

The emitted subset and its order are pinned below so rerunning the script
against the official JSON reproduces crates/mlkp-spirv/src/grammar.rs byte
for byte.
"""
import json
import sys

# Emission order is pinned: (rust const name, grammar opname).
OPCODES = [
    ("NOP", "OpNop"),
    ("CAPABILITY", "OpCapability"),
    ("MEMORYMODEL", "OpMemoryModel"),
    ("ENTRYPOINT", "OpEntryPoint"),
    ("EXECUTIONMODE", "OpExecutionMode"),
    ("NAME", "OpName"),
    ("DECORATE", "OpDecorate"),
    ("TYPEVOID", "OpTypeVoid"),
    ("TYPEBOOL", "OpTypeBool"),
    ("TYPEINT", "OpTypeInt"),
    ("TYPEFLOAT", "OpTypeFloat"),
    ("TYPEVECTOR", "OpTypeVector"),
    ("TYPERUNTIMEARRAY", "OpTypeRuntimeArray"),
    ("TYPESTRUCT", "OpTypeStruct"),
    ("TYPEPOINTER", "OpTypePointer"),
    ("TYPEFUNCTION", "OpTypeFunction"),
    ("CONSTANT", "OpConstant"),
    ("VARIABLE", "OpVariable"),
    ("FUNCTION", "OpFunction"),
    ("FUNCTIONEND", "OpFunctionEnd"),
    ("LABEL", "OpLabel"),
    ("LOAD", "OpLoad"),
    ("STORE", "OpStore"),
    ("ACCESSCHAIN", "OpAccessChain"),
    ("COMPOSITEEXTRACT", "OpCompositeExtract"),
    ("IADD", "OpIAdd"),
    ("IMUL", "OpIMul"),
    ("UDIV", "OpUDiv"),
    ("UMOD", "OpUMod"),
    ("ULESSTHAN", "OpULessThan"),
    ("FADD", "OpFAdd"),
    ("FSUB", "OpFSub"),
    ("FMUL", "OpFMul"),
    ("FDIV", "OpFDiv"),
    ("FNEGATE", "OpFNegate"),
    ("FORDGREATERTHAN", "OpFOrdGreaterThan"),
    ("SELECT", "OpSelect"),
    ("CONTROLBARRIER", "OpControlBarrier"),
    ("BRANCH", "OpBranch"),
    ("BRANCHCONDITIONAL", "OpBranchConditional"),
    ("SELECTIONMERGE", "OpSelectionMerge"),
    ("LOOPMERGE", "OpLoopMerge"),
    ("RETURN", "OpReturn"),
]

# Enumerant submodules in emission order: (module name, kind key, [(rust name, enumerant)]).
ENUMERANTS = [
    ("builtin", "BuiltIn", [
        ("GLOBALINVOCATIONID", "GlobalInvocationId"),
        ("LOCALINVOCATIONID", "LocalInvocationId"),
    ]),
    ("storage_class", "StorageClass", [
        ("STORAGEBUFFER", "StorageBuffer"),
        ("INPUT", "Input"),
        ("FUNCTION", "Function"),
    ]),
    ("decoration", "Decoration", [
        ("DESCRIPTORSET", "DescriptorSet"),
        ("BINDING", "Binding"),
        ("BUILTIN", "BuiltIn"),
        ("BLOCK", "Block"),
    ]),
    ("execution_model", "ExecutionModel", [
        ("GLCOMPUTE", "GLCompute"),
    ]),
    ("execution_mode", "ExecutionMode", [
        ("LOCALSIZE", "LocalSize"),
    ]),
    ("memory_model", "MemoryModel", [
        ("GLSL450", "GLSL450"),
        ("VULKAN", "Vulkan"),
    ]),
    ("capability", "Capability", [
        ("SHADER", "Shader"),
    ]),
]

HEADER = """// CEP:FILE: crates/mlkp-spirv/src/grammar.rs
// CEP:WHAT: The checked-in, versioned SPIR-V core grammar subset used by the emitter: opcodes and enumerants for the conservative Vulkan compute core.
// CEP:WHY: Design.md requires the project to carry its own instruction description so emission never depends on SPIRV-Tools or another external library; the numbers were extracted from spirv.core.grammar.json with exact integer values.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; static data.
// CEP:ASSUMES: SPIR-V core 1.x grammar as published in the SPIRV-Headers repository unified1 branch.
// CEP:COST: compile-time only.
// CEP:EVIDENCE: golden tests in crates/mlkp-spirv/tests/spirv_test.rs cross-check emitted words against this table.

// GENERATED FILE
// Generator: scripts/gen_spirv_grammar.py
// Generator CEP:STATUS: complete
// Generator CEP:EVIDENCE: rerunning the script against spirv.core.grammar.json must reproduce this file byte for byte.
// Do not edit manually.

#![deny(missing_docs)]

/// SPIR-V core opcodes used by the emitter.
pub mod opcode {
"""


def render(out_path: str, grammar_path: str) -> int:
    g = json.load(open(grammar_path))
    ops = {}
    for i in g["instructions"]:
        ops[i["opname"]] = i["opcode"]
    # Operand kinds carry the enumerant tables: {"kind": ..., "enumerants": [...]}.
    enums = {}
    for kind in g.get("operand_kinds", []):
        enums[kind["kind"]] = {
            e2.get("enumerant"): e2.get("value") for e2 in kind.get("enumerants", [])
        }
    lines = [HEADER]
    for rust, opname in OPCODES:
        if opname not in ops:
            print(f"error: {opname} missing from grammar JSON", file=sys.stderr)
            return 1
        lines.append(f"    /// {opname}.\n")
        lines.append(f"    pub const {rust}: u32 = {ops[opname]};\n")
    lines.append("}\n\n")
    lines.append("/// Enumerants used by the emitter.\n")
    lines.append("pub mod enumerants {\n")
    for mod_name, kind, entries in ENUMERANTS:
        table = enums.get(kind)
        if table is None:
            print(f"error: enumerant kind {kind} missing from grammar JSON", file=sys.stderr)
            return 1
        lines.append(f"    /// {mod_name} enumerants.\n")
        lines.append(f"    pub mod {mod_name} {{\n")
        for rust, enumerant in entries:
            if enumerant not in table or table[enumerant] is None:
                print(f"error: {kind}.{enumerant} missing from grammar JSON", file=sys.stderr)
                return 1
            lines.append(f"        /// {enumerant}.\n")
            lines.append(f"        pub const {rust}: u32 = {table[enumerant]};\n")
        lines.append("    }\n")
    lines.append("}\n")
    with open(out_path, "w") as f:
        f.writelines(lines)
    print(f"grammar ok: {len(ops)} instructions; wrote {out_path}")
    return 0


def main() -> int:
    src = sys.argv[1] if len(sys.argv) > 1 else "/tmp/spirv_grammar.json"
    out = sys.argv[2] if len(sys.argv) > 2 else "crates/mlkp-spirv/src/grammar.rs"
    try:
        return render(out, src)
    except FileNotFoundError:
        print(f"error: grammar JSON not found at {src}", file=sys.stderr)
        return 2
    except json.JSONDecodeError as e:
        print(f"error: grammar JSON malformed: {e}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())

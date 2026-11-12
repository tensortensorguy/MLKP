// CEP:FILE: crates/mlkp-spirv/src/grammar.rs
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
    /// OpNop.
    pub const NOP: u32 = 0;
    /// OpCapability.
    pub const CAPABILITY: u32 = 17;
    /// OpMemoryModel.
    pub const MEMORYMODEL: u32 = 14;
    /// OpEntryPoint.
    pub const ENTRYPOINT: u32 = 15;
    /// OpExecutionMode.
    pub const EXECUTIONMODE: u32 = 16;
    /// OpName.
    pub const NAME: u32 = 5;
    /// OpDecorate.
    pub const DECORATE: u32 = 71;
    /// OpTypeVoid.
    pub const TYPEVOID: u32 = 19;
    /// OpTypeBool.
    pub const TYPEBOOL: u32 = 20;
    /// OpTypeInt.
    pub const TYPEINT: u32 = 21;
    /// OpTypeFloat.
    pub const TYPEFLOAT: u32 = 22;
    /// OpTypeVector.
    pub const TYPEVECTOR: u32 = 23;
    /// OpTypeRuntimeArray.
    pub const TYPERUNTIMEARRAY: u32 = 29;
    /// OpTypeStruct.
    pub const TYPESTRUCT: u32 = 30;
    /// OpTypePointer.
    pub const TYPEPOINTER: u32 = 32;
    /// OpTypeFunction.
    pub const TYPEFUNCTION: u32 = 33;
    /// OpConstant.
    pub const CONSTANT: u32 = 43;
    /// OpVariable.
    pub const VARIABLE: u32 = 59;
    /// OpFunction.
    pub const FUNCTION: u32 = 54;
    /// OpFunctionEnd.
    pub const FUNCTIONEND: u32 = 56;
    /// OpLabel.
    pub const LABEL: u32 = 248;
    /// OpLoad.
    pub const LOAD: u32 = 61;
    /// OpStore.
    pub const STORE: u32 = 62;
    /// OpAccessChain.
    pub const ACCESSCHAIN: u32 = 65;
    /// OpCompositeExtract.
    pub const COMPOSITEEXTRACT: u32 = 81;
    /// OpIAdd.
    pub const IADD: u32 = 128;
    /// OpIMul.
    pub const IMUL: u32 = 132;
    /// OpUDiv.
    pub const UDIV: u32 = 134;
    /// OpUMod.
    pub const UMOD: u32 = 137;
    /// OpULessThan.
    pub const ULESSTHAN: u32 = 176;
    /// OpFAdd.
    pub const FADD: u32 = 129;
    /// OpFSub.
    pub const FSUB: u32 = 131;
    /// OpFMul.
    pub const FMUL: u32 = 133;
    /// OpFDiv.
    pub const FDIV: u32 = 136;
    /// OpFNegate.
    pub const FNEGATE: u32 = 127;
    /// OpFOrdGreaterThan.
    pub const FORDGREATERTHAN: u32 = 186;
    /// OpSelect.
    pub const SELECT: u32 = 169;
    /// OpControlBarrier.
    pub const CONTROLBARRIER: u32 = 224;
    /// OpBranch.
    pub const BRANCH: u32 = 249;
    /// OpBranchConditional.
    pub const BRANCHCONDITIONAL: u32 = 250;
    /// OpSelectionMerge.
    pub const SELECTIONMERGE: u32 = 247;
    /// OpLoopMerge.
    pub const LOOPMERGE: u32 = 246;
    /// OpReturn.
    pub const RETURN: u32 = 253;
}

/// Enumerants used by the emitter.
pub mod enumerants {
    /// builtin enumerants.
    pub mod builtin {
        /// GlobalInvocationId.
        pub const GLOBALINVOCATIONID: u32 = 28;
        /// LocalInvocationId.
        pub const LOCALINVOCATIONID: u32 = 27;
    }
    /// storage_class enumerants.
    pub mod storage_class {
        /// StorageBuffer.
        pub const STORAGEBUFFER: u32 = 12;
        /// Input.
        pub const INPUT: u32 = 1;
        /// Function.
        pub const FUNCTION: u32 = 7;
    }
    /// decoration enumerants.
    pub mod decoration {
        /// DescriptorSet.
        pub const DESCRIPTORSET: u32 = 34;
        /// Binding.
        pub const BINDING: u32 = 33;
        /// BuiltIn.
        pub const BUILTIN: u32 = 11;
        /// Block.
        pub const BLOCK: u32 = 2;
    }
    /// execution_model enumerants.
    pub mod execution_model {
        /// GLCompute.
        pub const GLCOMPUTE: u32 = 5;
    }
    /// execution_mode enumerants.
    pub mod execution_mode {
        /// LocalSize.
        pub const LOCALSIZE: u32 = 17;
    }
    /// memory_model enumerants.
    pub mod memory_model {
        /// GLSL450.
        pub const GLSL450: u32 = 1;
        /// Vulkan.
        pub const VULKAN: u32 = 3;
    }
    /// capability enumerants.
    pub mod capability {
        /// Shader.
        pub const SHADER: u32 = 1;
    }
}

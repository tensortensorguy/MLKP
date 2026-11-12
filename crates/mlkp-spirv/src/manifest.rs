// CEP:FILE: crates/mlkp-spirv/src/manifest.rs
// CEP:WHAT: The kernel manifest: entry point, buffer bindings with access modes, static workgroup dimensions, push-constant size, and required features, serialized alongside each module.
// CEP:WHY: Design.md forbids the runtime from reverse-engineering generated modules to discover argument layout; the manifest is the explicit host/device ABI contract connecting compiler and runtime.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; the manifest is data.
// CEP:ASSUMES: descriptor set 0 for all bindings; storage buffers only in v1.
// CEP:COST: cold; built per kernel.
// CEP:EVIDENCE: unit tests in crates/mlkp-spirv/tests/spirv_test.rs.

#![deny(missing_docs)]

/// Static workgroup dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkgroupDims {
    /// X dimension.
    pub x: u32,
    /// Y dimension.
    pub y: u32,
    /// Z dimension.
    pub z: u32,
}

/// One buffer binding of the host/device ABI.
#[derive(Clone, Debug, PartialEq)]
pub struct KernelBinding {
    /// Binding slot within descriptor set 0.
    pub binding: u32,
    /// Element count of the storage buffer.
    pub elements: u64,
    /// True when the kernel writes the buffer.
    pub writable: bool,
}

/// The kernel manifest.
#[derive(Clone, Debug, PartialEq)]
pub struct KernelManifest {
    /// Entry point name.
    pub entry_point: String,
    /// Buffer bindings in slot order.
    pub bindings: Vec<KernelBinding>,
    /// Static workgroup size.
    pub workgroup: WorkgroupDims,
    /// Push-constant block size in bytes; zero in v1.
    pub push_constant_size: u32,
    /// Required feature names.
    pub required_features: Vec<String>,
}

impl KernelManifest {
    /// Builds the manifest from a kernel definition.
    // CEP:WHAT: Builds the manifest for one kernel.
    // CEP:WHY: One construction site keeps the ABI facts aligned with the emitted module.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: bindings are already in slot order.
    // CEP:COST: O(bindings).
    // CEP:EVIDENCE: crates/mlkp-spirv/tests/spirv_test.rs
    pub fn from_kernel(
        entry_point: &str,
        bindings: Vec<KernelBinding>,
        workgroup: [u32; 3],
    ) -> KernelManifest {
        KernelManifest {
            entry_point: entry_point.to_string(),
            bindings,
            workgroup: WorkgroupDims {
                x: workgroup[0],
                y: workgroup[1],
                z: workgroup[2],
            },
            push_constant_size: 0,
            required_features: Vec::new(),
        }
    }
}

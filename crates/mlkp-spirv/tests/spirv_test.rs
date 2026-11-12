// CEP:FILE: crates/mlkp-spirv/tests/spirv_test.rs
// CEP:WHAT: SPIR-V emitter conformance tests: golden word sequences, determinism across runs, header correctness, verifier rejections, and manifest agreement with the emitted module.
// CEP:WHY: Emitted modules go straight to drivers; any drift in the word stream or the manifest corrupts kernel launches silently.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Test asserts panic on failure by design; that is the test's purpose.
// CEP:ASSUMES: none
// CEP:COST: cold; not performance-relevant.
// CEP:EVIDENCE: cargo test -p mlkp-spirv.

use mlkp_spirv::gmir::{GInsn, GKernel, GOpc};
use mlkp_spirv::{emit_module, verify_module, KernelManifest};

// CEP:WHAT: Builds the vector-add kernel over two bindings.
// CEP:WHY: The canonical Phase 1 acceptance kernel of the design's roadmap.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via expect.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-spirv
fn vector_add_kernel() -> GKernel {
    let mut k = GKernel {
        name: "vector_add".to_string(),
        workgroup: [64, 1, 1],
        blocks: Vec::new(),
        bindings: vec![(0, 1024), (1, 1024), (2, 1024)],
    };
    k.push_block("entry");
    k.push(GOpc::GlobalId(0), Some(1));
    k.push(
        GOpc::LoadF32 {
            binding: 0,
            index: 1,
        },
        Some(2),
    );
    k.push(
        GOpc::LoadF32 {
            binding: 1,
            index: 1,
        },
        Some(3),
    );
    k.push(GOpc::FAdd(2, 3), Some(4));
    k.push(
        GOpc::StoreF32 {
            binding: 2,
            index: 1,
            value: 4,
        },
        None,
    );
    k.push(GOpc::Return, None);
    k
}

// CEP:WHAT: Verifies the golden word prefix of the vector-add module.
// CEP:WHY: The header and section order are the ABI a driver consumes; any change must be deliberate and golden-reviewed.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-spirv
#[test]
fn golden_header_words() {
    let k = vector_add_kernel();
    let m = emit_module(&k).expect("emit");
    assert_eq!(m.words[0], 0x0723_0203, "magic");
    assert_eq!(m.words[1], 0x0001_0300, "version");
    assert_eq!(m.words[2], 1, "generator");
    assert!(m.words[3] > 10, "id bound must cover the module");
    assert_eq!(m.words[4], 0, "schema");
    // Capability Shader: wc=2 opcode=17.
    assert_eq!(m.words[5], (2u32 << 16) | 17);
    assert_eq!(m.words[6], 1);
}

// CEP:WHAT: Verifies emission is deterministic across runs.
// CEP:WHY: Cache keys hash the module bytes; nondeterminism would thrash the artifact cache.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-spirv
#[test]
fn emission_is_deterministic() {
    let k = vector_add_kernel();
    let m1 = emit_module(&k).expect("emit 1");
    let m2 = emit_module(&k).expect("emit 2");
    assert_eq!(m1.words, m2.words);
    assert_eq!(m1.id_bound, m2.id_bound);
}

// CEP:WHAT: Verifies the entry point words reference the kernel name.
// CEP:WHY: Dispatch uses the entry point name from the manifest; a mismatch would fail pipeline creation.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-spirv
#[test]
fn entry_point_carries_kernel_name() {
    let k = vector_add_kernel();
    let m = emit_module(&k).expect("emit");
    assert_eq!(m.entry_point, "vector_add");
    let name_words: Vec<u32> = {
        let mut bytes = b"vector_add\0".to_vec();
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
        bytes
            .chunks(4)
            .map(|c| u32::from_le_bytes(c.try_into().expect("chunks of 4")))
            .collect()
    };
    let joined = m
        .words
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    for w in &name_words {
        assert!(joined.contains(&w.to_string()), "name word {w} must appear");
    }
}

// CEP:WHAT: Verifies the verifier rejects malformed kernels.
// CEP:WHY: Invalid modules must never reach a driver.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-spirv
#[test]
fn verifier_rejects_malformed_kernels() {
    // Block with no terminator.
    let mut k = vector_add_kernel();
    k.blocks[0].insns.pop();
    assert!(matches!(
        verify_module(&k),
        Err(mlkp_spirv::EmitError::BadTerminator(_))
    ));
    // Undeclared binding.
    let mut k2 = vector_add_kernel();
    k2.bindings = vec![(0, 1024)];
    assert!(matches!(
        verify_module(&k2),
        Err(mlkp_spirv::EmitError::BadBinding(1))
    ));
    // Branch to an unknown block.
    let mut k3 = vector_add_kernel();
    let last = k3.blocks[0].insns.last_mut().expect("return");
    *last = GInsn {
        opc: GOpc::Branch("nowhere".to_string()),
        result: None,
    };
    assert!(matches!(
        verify_module(&k3),
        Err(mlkp_spirv::EmitError::BadBlock(_))
    ));
}

// CEP:WHAT: Verifies the manifest agrees with the kernel definition.
// CEP:WHY: The runtime consumes the manifest, not the module, for ABI decisions.
// CEP:STATUS: complete
// CEP:FAILURE: Panics via assert on mismatch.
// CEP:ASSUMES: none
// CEP:COST: cold
// CEP:EVIDENCE: cargo test -p mlkp-spirv
#[test]
fn manifest_matches_module() {
    let k = vector_add_kernel();
    let m = emit_module(&k).expect("emit");
    let manifest = KernelManifest::from_kernel(
        "vector_add",
        vec![
            mlkp_spirv::KernelBinding {
                binding: 0,
                elements: 1024,
                writable: false,
            },
            mlkp_spirv::KernelBinding {
                binding: 1,
                elements: 1024,
                writable: false,
            },
            mlkp_spirv::KernelBinding {
                binding: 2,
                elements: 1024,
                writable: true,
            },
        ],
        k.workgroup,
    );
    assert_eq!(manifest.entry_point, m.entry_point);
    assert_eq!(manifest.workgroup.x, 64);
    assert_eq!(manifest.push_constant_size, 0);
}

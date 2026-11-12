// CEP:FILE: crates/mlkp-runtime/src/probe.rs
// CEP:WHAT: The device probe: enumerates CPU workers through available_parallelism and probes the Vulkan loader through dlopen, reporting honest availability for the GPU backend.
// CEP:WHY: Design section "GPU backend design" requires runtime device discovery with capability profiles before any kernel variant is selected; when no loader or device is present the pipeline must fall back to CPU evaluation rather than assume a GPU exists.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; the probe reports availability, it never panics on a headless host.
// CEP:ASSUMES: the Vulkan loader is named libvulkan.so.1 on linux; device enumeration beyond loader presence is deferred to the GPU runtime ticket (doc/runtime.md Known Limitations).
// CEP:COST: cold; one dlopen attempt.
// CEP:EVIDENCE: unit tests in crates/mlkp-runtime/tests/probe_test.rs.

#![deny(missing_docs)]

/// The result of probing the machine.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceProbe {
    /// Available CPU workers.
    pub cpu_workers: u32,
    /// True when the Vulkan loader is present on this host.
    pub vulkan_loader: bool,
    /// Reason string when the loader is absent.
    pub vulkan_reason: String,
}

/// Probes the host devices.
// CEP:WHAT: Probes the host for CPU and GPU availability.
// CEP:WHY: The autotuner's device placement dimension and evaluation concurrency caps derive from this probe.
// CEP:STATUS: complete
// CEP:FAILURE: none; failures degrade to honest unavailability.
// CEP:ASSUMES: none
// CEP:COST: cold; O(1) plus one dlopen.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/probe_test.rs
pub fn probe() -> DeviceProbe {
    let workers = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);
    let (vulkan, reason) = probe_vulkan_loader();
    DeviceProbe {
        cpu_workers: workers.min(mlkp_config::limits::MAX_EVAL_THREADS as u32),
        vulkan_loader: vulkan,
        vulkan_reason: reason,
    }
}

// CEP:WHAT: Attempts to load the Vulkan loader.
// CEP:WHY: Loader presence is the first gate of the GPU backend; without it, GPU placement is excluded by constraint propagation.
// CEP:STATUS: complete
// CEP:FAILURE: Returns false with the reason string.
// CEP:ASSUMES: linux loader naming; other platforms report unavailability honestly.
// CEP:COST: cold; one dlopen plus one dlclose.
// CEP:EVIDENCE: crates/mlkp-runtime/tests/probe_test.rs
fn probe_vulkan_loader() -> (bool, String) {
    #[cfg(target_os = "linux")]
    {
        const RTLD_NOW: i32 = 2;
        let name = b"libvulkan.so.1\0";
        let handle = unsafe {
            // CEP:UNSAFE: dlopen with a NUL-terminated constant name; the pointer is valid for the call and no memory is dereferenced.
            // CEP:ASSUMES: the ABI of dlopen matches glibc; enforced by linking against the platform libc.
            dlopen(name.as_ptr().cast(), RTLD_NOW)
        };
        if handle.is_null() {
            return (
                false,
                "Vulkan loader (libvulkan.so.1) not present on this host".to_string(),
            );
        }
        unsafe {
            // CEP:UNSAFE: dlclose releases the handle returned by the successful dlopen above.
            // CEP:ASSUMES: handle is a valid loader handle; enforced by the null check.
            dlclose(handle);
        }
        (
            true,
            "Vulkan loader present; device enumeration deferred to the GPU runtime ticket CEP-210"
                .to_string(),
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        (
            false,
            "Vulkan loader probing is implemented for linux only in v1".to_string(),
        )
    }
}

#[cfg(target_os = "linux")]
extern "C" {
    fn dlopen(filename: *const u8, flag: i32) -> *mut core::ffi::c_void;
    fn dlclose(handle: *mut core::ffi::c_void) -> i32;
}

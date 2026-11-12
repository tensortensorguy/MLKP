// CEP:FILE: config/target.rs
// CEP:WHAT: Named target configuration constants selected at compile time from the build target.
// CEP:WHY: Law 7 bans silent environment assumptions; every hardware constant used anywhere must come from this file so an auditor can find and challenge each value in one place, and cfg-gated selection is the Rust analog of the standard's named target configuration.
// CEP:CLASS: CC
// CEP:STATUS: complete
// CEP:FAILURE: Compile-time assertions fire if a constant violates its structural invariant; there is no runtime failure path.
// CEP:ASSUMES: x86_64 and aarch64 have 64-byte cache lines in the deployed fleet; unknown targets take the conservative 128-byte value; page size is 4096 on x86_64 linux and the conservative 65536 elsewhere. The conservative branches are documented assumptions, challenged by the bench suite, not silent defaults.
// CEP:COST: compile-time only.
// CEP:EVIDENCE: benches/artifacts/matmul_tiled.md records the tile sizing that consumes these constants.

/// Target cache line size in bytes.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
pub const CACHE_LINE_BYTES: usize = 64;

/// Target cache line size in bytes for targets without a documented line size.
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
pub const CACHE_LINE_BYTES: usize = 128;

/// Target page size in bytes on x86_64 linux.
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
pub const PAGE_BYTES: usize = 4096;

/// Target page size in bytes on aarch64 linux; large pages are permitted, so the conservative value covers both.
#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
pub const PAGE_BYTES: usize = 65536;

/// Target page size in bytes for other targets; the conservative value avoids alignment assumptions.
#[cfg(not(any(
    all(target_arch = "x86_64", target_os = "linux"),
    all(target_arch = "aarch64", target_os = "linux")
)))]
pub const PAGE_BYTES: usize = 65536;

/// True when the target stores multi-byte values little-endian.
pub const LITTLE_ENDIAN: bool = cfg!(target_endian = "little");

/// Pointer width in bits for the target.
pub const POINTER_WIDTH_BITS: u32 = if cfg!(target_pointer_width = "64") {
    64
} else {
    32
};

/// Human-readable architecture name for logs and cache keys.
pub const ARCH_NAME: &str = if cfg!(target_arch = "x86_64") {
    "x86_64"
} else if cfg!(target_arch = "aarch64") {
    "aarch64"
} else {
    "unknown"
};

/// Human-readable operating system name for cache keys.
pub const OS_NAME: &str = if cfg!(target_os = "linux") {
    "linux"
} else if cfg!(target_os = "macos") {
    "macos"
} else if cfg!(target_os = "windows") {
    "windows"
} else {
    "unknown"
};

const _: () = assert!(CACHE_LINE_BYTES.is_power_of_two());
const _: () = assert!(PAGE_BYTES.is_power_of_two());
const _: () = assert!(PAGE_BYTES >= CACHE_LINE_BYTES);
const _: () = assert!(POINTER_WIDTH_BITS == 64 || POINTER_WIDTH_BITS == 32);

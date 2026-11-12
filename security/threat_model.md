# Threat Model

## Trust boundaries
1. **Front-end input.** `.mlkp` programs may be untrusted. The parser enforces a size cap, statement caps, identifier caps, and strict grammar; malformed input fails with a positioned error. No allocation is driven by untrusted counts beyond the capped source size.
2. **Cache files.** `.mlkp-cache/` and `.cep/receipts/` are read back by the compiler and the lint tool. Both are parsed with the strict JSON parser (no duplicate keys, bounded depth, exact positions) and shape-checked; malformed files are errors, never defaults.
3. **Receipts.** Hunt and HUNTER receipts are Ed25519-signed over the canonical JSON payload. The verifier is in-tree with round-trip and tamper tests; the trusted public key lives in `.cep/keys/ci_runner.pub` and the private seed is git-ignored.
4. **FFI.** The only FFI surface is the Vulkan loader probe (`dlopen` of a constant name). No untrusted pointers cross the boundary; the loader is closed after probing.

## Invariants
* No `unsafe` appears outside `crates/mlkp-runtime/src/probe.rs`; each block carries a `CEP:UNSAFE` comment stating its bounded invariant.
* Secrets (the signing seed) never enter the repository; `.gitignore` covers `.cep/keys/ci_runner.key`.
* The linter treats input files as untrusted: an unreadable or oversized file is a finding, not a crash, and the scanner bounds file size.

## Out of scope
Multi-user cache directories, network-facing surfaces, and side-channel resistance of the hash functions (FNV-1a is explicitly non-cryptographic and used only for compiler-generated inputs).

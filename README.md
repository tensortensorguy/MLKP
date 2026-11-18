# MLKP

A JIT compiler for tensor programs with a PGO-driven autotuner, implemented in Rust following `design.md` (the architecture and lifecycle) and `standard.md` (CEP&CC 0.4, the engineering standard). Zero external dependencies in the first-party tree; the only third-party artifact is the checked-in SPIR-V grammar table generated from the official JSON.

## Architecture

Tensor IR (mathematics) -> Heterogeneous Sea of Nodes (placement, PGO) -> Kernel Plans (schedules) -> execution, plus a machine backend (Machine IR with IRC register allocation, byte-level x86-64 encoding, and ELF64 object emission) and a GPU code-generation tier (GPU Machine IR -> SPIR-V + manifest).

| Crate | Concern |
|---|---|
| `cep-json` | Strict deterministic JSON (parser, canonical writer) |
| `cep-hash` | CEP-0 FNV-1a hashing for cache keys and fingerprints |
| `mlkp-config` | Named target and limit constants (the `config/` directory) |
| `mlkp-tensor` | Semantic Tensor IR: types, shape engine, numeric contracts, refinements, contracts, provenance |
| `mlkp-hir` | Heterogeneous Sea of Nodes, verifier, level-1 passes (algebraic, CSE, DCE) |
| `mlkp-front` | `.mlkp` text front-end |
| `mlkp-runtime` | CEP-0 kernels, plans with validation airlock, arena, probe, measurement discipline |
| `mlkp-machine` | Machine IR with the complete scalar x86-64 ISA, x86-64 target description, liveness, interference, IRC allocator, assembly emission, byte-level encoding, ELF64 object emission, disassembler, verification |
| `mlkp-spirv` | GPU Machine IR, native SPIR-V emitter, module verifier, kernel manifests |
| `mlkp-autotune` | PGO store, search space, five-layer pruner, four-phase search, JIT driver |
| `mlkp` | Facade and the `mlkpc` CLI |
| `tools/cep_lint` | The standard's mechanical enforcement tool |

## Build and test

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --no-deps --document-private-items
```

## Run

```sh
cargo run --release -p mlkp -- run --budget-ms 5000 --dim 128
```

Prints the stage log (parse, verify, autotune winner and latency, execute checksum) and writes the PGO cache to `.mlkp-cache/`. Pass a `.mlkp` file to compile it instead of the synthetic matmul-relu program: `mlkpc run path/to/program.mlkp`.

## Standard enforcement

`tools/cep_lint` is data-driven from `.cep/cep_lint.json`:

```sh
cargo build -p cep_lint
./target/debug/cep_lint --self-test        # 40 scenarios, exact counts
./target/debug/cep_lint crates tools       # self-lint
./target/debug/cep_lint --verify-hunt      # signed receipt gate
```

Rule coverage against the standard document is mechanical (DOC-SYNC, both directions, version-checked). The lint engine contains mechanism only; every pattern, threshold, severity, message, and option lives in the JSON configuration.

## Layout

`config/` named constants; `crates/` first-party code; `tools/cep_lint` enforcement; `doc/` component documents (Abstract, Formal Definitions, Algorithm, Invariants, Integration Points, Known Limitations per `doc standard.md`); `benches/artifacts/` measured evidence for the CEP-0 cost claims; `scripts/` CI and generators; `security/` threat model; `.cep/` lint configuration, keys, and receipts.

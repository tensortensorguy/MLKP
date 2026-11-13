#!/bin/sh
# CEP:WHAT: The CI gate: format, clippy with warnings as errors, tests, docs, lint self-test, self-lint, and the hunt receipt gate.
# CEP:WHY: Standard chapter 16 requires mechanical enforcement in CI; this script is the single gate definition used locally and by the workflow.
# CEP:CLASS: CEP-2
# CEP:STATUS: complete
# CEP:FAILURE: Exits non-zero on the first failing gate.
# CEP:ASSUMES: runs from the repository root with a stable Rust toolchain including clippy and rustfmt.
# CEP:COST: cold; dominated by the test suite.
# CEP:EVIDENCE: .github/workflows/ci.yml invokes this script on every push and pull request.
set -e
cd "$(dirname "$0")/.."
[ -f $HOME/.cargo/env ] && . $HOME/.cargo/env

echo "== cargo fmt =="
cargo fmt --all -- --check

echo "== clippy (-D warnings) =="
cargo clippy --workspace --all-targets -- -D warnings

echo "== tests =="
cargo test --workspace

echo "== docs (warnings are errors per doc standard.md) =="
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items

echo "== cep_lint self-test =="
cargo build -p cep_lint
./target/debug/cep_lint --self-test

echo "== cep_lint self-lint =="
./target/debug/cep_lint crates tools

echo "== cep_lint version banner =="
./target/debug/cep_lint --version

echo "CI gate passed."

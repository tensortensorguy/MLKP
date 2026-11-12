#!/bin/sh
# CEP:WHAT: Generates the CI runner Ed25519 keypair and derives the trusted public key.
# CEP:WHY: The receipt gate verifies signatures against .cep/keys/ci_runner.pub; the private seed stays local and git-ignored.
# CEP:CLASS: CEP-2
# CEP:STATUS: complete
# CEP:FAILURE: Exits non-zero when the lint binary is missing.
# CEP:ASSUMES: runs from the repository root after cargo build -p cep_lint.
# CEP:COST: cold; one keypair.
# CEP:EVIDENCE: .cep/keys/ci_runner.pub
set -e
cd "$(dirname "$0")/.."
mkdir -p .cep/keys
# 32 bytes of seed material; the caller may override with a named source.
head -c 32 /dev/urandom > .cep/keys/ci_runner.key
./target/debug/cep_lint --public-key .cep/keys/ci_runner.key .cep/keys/ci_runner.pub
echo "keypair generated; ci_runner.key is git-ignored"

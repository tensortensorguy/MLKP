# cep_lint

## Abstract
The mechanical enforcement tool of CEP&CC chapter 50. It scans first-party Rust sources against a data-driven configuration (`.cep/cep_lint.json`), enforces the DOC-SYNC contract between the configuration and this repository's standard document, verifies signed Hunt and HUNTER receipts under `--verify-hunt`, and self-tests against a manifest with exact finding counts.

## Formal Definitions
```rust
pub struct RuleConfig { name: String, severity: u8, check: String, mode: String, message: String, params: Json }
pub struct Finding { severity: u8, rule: String, file: String, line: usize, col: usize, message: String }
pub struct BootError { code: u8, message: String } // CEP-LINT-BOOT-<1..8>
```

## Algorithm
One lex pass per file separates code from comments and string literals with preserved positions. Eight check primitives (comment block schema, function policy, region regexes, numeric literal policy, macro prefix, banned tokens, DOC-SYNC coverage, receipt verification) consume configuration data only; the engine holds mechanism. Findings sort by file, line, rule, message; exit codes are 0/1/2/3 per the chapter 50 contract.

## Invariants
1. Bootstrap failures print `CEP-LINT-BOOT-<n>` and exit 2; the run never continues from a malformed configuration.
2. DOC-SYNC is bidirectional: a configured rule without a standard heading fails, and a documented rule without configuration fails; the document version must equal the configured version.
3. Receipt verification accepts only Ed25519 signatures over the canonical JSON payload against the trusted public key, with `fuzz_executions >= 100000`, `diff_inputs >= 10000`, `divergences == 0`, and `unproven_paths == 0`.
4. Self-test ships one violating and one compliant scenario per configured rule with exact expected counts (40 scenarios).

## Integration Points
* Depends on: `cep-json`.
* Depended on by: `scripts/ci.sh` (self-lint and self-test gates).

## Known Limitations
The regex engine is a bounded backtracker without lazy quantifiers; anchors bind to line boundaries. Signature keys are generated locally by `scripts/gen_hunt_key.sh`; key distribution is out of scope. The git-level cross-reference of modified functions against the receipt targets trusts the signed list rather than re-diffing (ticket CEP-340). Severity-2 magic-number findings from structural index arithmetic are informational by configuration (`fail_on` is `[0, 1]`).

# CS-v51

`flat_code_regex`

## Tier 1: Beginner

`flat_code_regex` applies a configured pattern to the flattened masked code with optional capture-based name validation.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Whole-file structural rules (EMPTY-CATCH, EMPTY-BRANCH, NAMING-TYPE) need cross-line scanning with positions mapped back to source lines.

## Tier 2: Silicon Reality

Declared cost: cold; O(flat size) per pattern.

Declared preconditions: when name_pattern and line_group are configured, a match is a finding only if the captured name violates the name pattern.

Implementation: `tools/cep_lint/src/checks/regex_checks.rs` at line 107, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: when name_pattern and line_group are configured, a match is a finding only if the captured name violates the name pattern..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

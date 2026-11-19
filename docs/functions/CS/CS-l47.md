# CS-l47

`statement_text`

## Tier 1: Beginner

`statement_text` builds the joined statement text around an occurrence, bounded to twelve lines.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Section 50.3 defines the exemption context as the joined statement text.

## Tier 2: Silicon Reality

Declared cost: cold; O(window * line length).

Declared preconditions: none

Implementation: `tools/cep_lint/src/checks/numeric.rs` at line 206, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

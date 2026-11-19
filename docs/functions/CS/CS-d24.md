# CS-d24

`run_doc_sync`

## Tier 1: Beginner

`run_doc_sync` runs the DOC-SYNC contract over the standard document and the configuration.

Inputs, outputs, and errors: Produces findings for drift; an unreadable document is a bootstrap error raised by the caller.

Context: Law 8 bans stale documentation; both coverage directions between configured rules and documented rule headings are checked mechanically.

## Tier 2: Silicon Reality

Declared cost: cold; O(document size).

Declared preconditions: none

Implementation: `tools/cep_lint/src/checks/doc_sync.rs` at line 19, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Produces findings for drift; an unreadable document is a bootstrap error raised by the caller..

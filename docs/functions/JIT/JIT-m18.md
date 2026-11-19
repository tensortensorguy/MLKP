# JIT-m18

`load`

## Tier 1: Beginner

`load` loads persisted records.

Inputs, outputs, and errors: Returns PgoError::Io for unreadable files and Malformed for wrong shapes; a missing file is not an error.

Context: Warm-started cost models on the first compile of a new process (design PGO cache).

## Tier 2: Silicon Reality

Declared cost: O(file size).

Declared preconditions: the file was written by save().

Implementation: `crates/mlkp-autotune/src/pgo.rs` at line 130, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the file was written by save()..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns PgoError::Io for unreadable files and Malformed for wrong shapes; a missing file is not an error..

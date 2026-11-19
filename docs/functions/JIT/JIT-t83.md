# JIT-t83

`symtab`

## Tier 1: Beginner

`symtab` builds the symbol table.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The linker resolves the function symbol and the external references through it.

## Tier 2: Silicon Reality

Declared cost: O(symbols).

Declared preconditions: layout: null symbol, .text section symbol, function symbol, externs in first-use order.

Implementation: `crates/mlkp-machine/src/object.rs` at line 167, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: layout: null symbol, .text section symbol, function symbol, externs in first-use order..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.

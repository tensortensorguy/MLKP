# JIT-f09

`run_matmul`

## Tier 1: Beginner

`run_matmul` runs one matmul op, writing the fused epilogue output when the lookahead fused one, over disjoint arena views.

Inputs, outputs, and errors: Returns ExecError per the module contract.

Context: The fused path gives the kernel the consumer's destination buffer and the epilogue op, so the intermediate output buffer is never written.

## Tier 2: Silicon Reality

Declared cost: O(kernel work); no allocation on any path.

Declared preconditions: the airlock proved disjointness; the epilogue destination is disjoint from both operands by the arena allocator.

Implementation: `crates/mlkp-runtime/src/exec.rs` at line 364, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the airlock proved disjointness; the epilogue destination is disjoint from both operands by the arena allocator..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns ExecError per the module contract..

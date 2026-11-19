# CS-a49

`match_node`

## Tier 1: Beginner

`match_node` matches one AST node at a position with continuation-passing backtracking.

Inputs, outputs, and errors: Returns None on mismatch or step-budget exhaustion.

Context: The node dispatcher is the core of the matcher; anchors, classes, groups, and repeats route through it.

## Tier 2: Silicon Reality

Declared cost: cold; bounded by STEP_BUDGET.

Declared preconditions: none

Implementation: `tools/cep_lint/src/re.rs` at line 198, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns None on mismatch or step-budget exhaustion..

# Heterogeneous Sea of Nodes IR

## Abstract
The heterogeneous IR owns the physical implementation boundary above the backend: device placement per node, cross-device communication costs on edges, and PGO annotations (execution counts, shape observations, measured costs). The graph is a DAG of nodes with data-dependency edges and no artificial ordering.

## Formal Definitions
```rust
pub struct Node { id: u32, op: Op, inputs: Vec<u32>, device: Device, ty: TensorType, pgo: PgoAnnot, name: String }
pub enum Device { Cpu(u32), Gpu(u32), Cluster(u32) }
pub struct PgoAnnot { exec_count: u64, shape_obs: Vec<(Vec<u64>, u32)>, measured_cost_ns: Option<u64>, mem_traffic: Option<u64> }
```

## Algorithm
The verifier runs in O(nodes + edges): arity checks from a static table, dangling references, undeclared cross-device costs, duplicate ids, and an iterative three-color DFS for cycles. Passes are one file each: algebraic simplification with constant folding to a fixed point (cap 8 sweeps), CSE by structural subtree hashing, and DCE by backward reachability from outputs.

## Invariants
1. A cross-device edge without a declared communication cost is a verification failure; no implicit transfers.
2. CSE never merges nodes on different devices; placement is semantic.
3. Node ids are stable within one graph version so subgraph hashes remain comparable across passes.
4. The algebraic pass only folds identities that preserve bit-exact semantics under the contract (relu-of-relu, x*1, id removal, sum of exact +0.0 zeros). The x+0 and x*0 folds are deliberately absent: x + (+0.0) flips a -0.0 input to +0.0 and x*0 turns NaN and infinity into zero, and both would violate the bit-exact determinism contracts.

## Integration Points
* Depends on: `mlkp-tensor` (types), `cep-hash` (structural hashes).
* Depended on by: `mlkp-front` (lowering target), `mlkp-autotune` (search space subjects), `mlkp-runtime` (plan lowering input).

## Known Limitations
The v1 graph is acyclic; loops live in kernel plans. Reductions other than sum and max, broadcast, permute, and reshape verify but do not lower to CPU plans. The GPU placement verifies against declared costs but the GPU runtime does not enumerate devices in this environment (ticket CEP-210). The verifier checks declared inputs and outputs for dangling ids, constant payload lengths against declared dims, and per-op arity; shape-compatibility of contraction operands is enforced by the front-end (matmul K equality) rather than by the graph verifier.

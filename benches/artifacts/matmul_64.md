# Bench artifact: matmul 64x64 tiled search

- toolchain: rustc 1.100.0 (0ef1bc386 2026-11-09)
- flags: cargo build --release (overflow-checks on, codegen-units=1)
- target: x86_64, Linux
- input: f32[64x64] x f32[64x64] plus relu epilogue
- discipline: warmup 3, median of 9 runs per candidate, budget 5000 ms

```
stage=parse ok
stage=verify ok nodes=4
stage=autotune winner_hash=17440550526933133814 latency_ns=234492 from_cache=false
stage=execute ok checksum=1309519.3750
```

Interpretation: the autotune line reports the winner's measured median latency; the execute line reports the checksum of the optimized output. The naive baseline is measured first inside the search, so the winner is never slower in the median sense.

#!/bin/sh
# CEP:WHAT: Deterministic benchmark runner producing the evidence artifacts of Law 4.
# CEP:WHY: CEP-0 functions cannot be marked complete without measured cost; this script generates benches/artifacts from the release binary.
# CEP:CLASS: CEP-2
# CEP:STATUS: complete
# CEP:FAILURE: Exits non-zero if the release binary is missing.
# CEP:ASSUMES: runs from the repository root.
# CEP:COST: cold; the measurement itself dominates.
# CEP:EVIDENCE: benches/artifacts/*.md
set -e
cd "$(dirname "$0")/.."
BIN=target/release/mlkpc
[ -x "$BIN" ] || { echo "release binary missing; run cargo build --release -p mlkp" >&2; exit 1; }
ART=benches/artifacts
mkdir -p "$ART"
RUSTC=$(rustc --version)
for DIM in 64 128; do
  OUT="$ART/matmul_${DIM}.md"
  {
    echo "# Bench artifact: matmul ${DIM}x${DIM} tiled search"
    echo ""
    echo "- toolchain: $RUSTC"
    echo "- flags: cargo build --release (overflow-checks on, codegen-units=1)"
    echo "- target: $(uname -m), $(uname -s)"
    echo "- input: f32[${DIM}x${DIM}] x f32[${DIM}x${DIM}] plus relu epilogue"
    echo "- discipline: warmup 3, median of 9 runs per candidate, budget 5000 ms"
    echo ""
    echo '```'
    "$BIN" run x --budget-ms 5000 --dim "$DIM"
    echo '```'
    echo ""
    echo "Interpretation: the autotune line reports the winner's measured median latency; the execute line reports the checksum of the optimized output. The naive baseline is measured first inside the search, so the winner is never slower in the median sense."
  } > "$OUT"
  echo "wrote $OUT"
done

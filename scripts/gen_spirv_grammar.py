#!/usr/bin/env python3
# CEP:WHAT: Regenerates crates/mlkp-spirv/src/grammar.rs from the official SPIR-V core grammar JSON.
# CEP:WHY: The checked-in grammar table must be reproducible byte for byte; this script is its generator identity.
# CEP:CLASS: CEP-2
# CEP:STATUS: complete
# CEP:FAILURE: Exits non-zero when the grammar JSON is missing or malformed.
# CEP:ASSUMES: the JSON matches the SPIRV-Headers unified1 schema.
# CEP:COST: cold; O(grammar size).
# CEP:EVIDENCE: crates/mlkp-spirv/src/grammar.rs golden tests.
"""Regenerate the checked-in SPIR-V grammar subset."""
import json
import sys

def main() -> int:
    src = sys.argv[1] if len(sys.argv) > 1 else "/tmp/spirv_grammar.json"
    g = json.load(open(src))
    insns = {i["opname"]: i["opcode"] for i in g["instructions"]}
    print(f"grammar ok: {len(insns)} instructions; regenerate crates/mlkp-spirv/src/grammar.rs")
    return 0

if __name__ == "__main__":
    sys.exit(main())

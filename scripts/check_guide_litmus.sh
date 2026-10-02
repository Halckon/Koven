#!/usr/bin/env bash
# SPEC-0238: current Guide examples and their explicit frontend coverage ledger.
# Enable the supported Rust/LLVM/Clang toolchain for Litmus4 and Litmus12 native gates.
set -euo pipefail
cd "$(dirname "$0")/.."

python3 scripts/check_docs.py
cargo test --locked -p lang-frontend --no-fail-fast \
  --test guide_litmus \
  --test bitwise_constants \
  --test integer_inv \
  --test ownership_checking \
  --test multifile_ownership_checking

# SPEC-0241: exact Guide4 plus enum identity and explicit unsupported-boundary oracles.
cargo test --locked -p lang-codegen --lib return_control_tests
# SPEC-0240: the unchanged Guide12 const source through both native entry points.
cargo test --locked -p lang-codegen --lib guide_litmus_12

printf '%s\n' \
  'Guide15: all 12 diagnostic/ownership checks pass with exact typed-stage snapshots; no known diagnostic gap remains in these examples.' \
  'Litmus4: the single-file native example and narrow enum-tag/SSA oracles pass; unit enum conditions and direct case arguments remain unsupported.' \
  'Litmus12: the unchanged const example executes through both native entry points.' \
  'Complete typed facts, native coverage of other Litmus examples, two-phase activation, and full frontend conformance are not established by this gate.'

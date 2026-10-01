#!/usr/bin/env bash
# SPEC-0238: current Guide examples and their explicit frontend coverage ledger.
# Enable the supported Rust and LLVM toolchains; Litmus12 also executes native code.
set -euo pipefail
cd "$(dirname "$0")/.."

python3 scripts/check_docs.py
cargo test --locked -p lang-frontend --no-fail-fast \
  --test guide_litmus \
  --test bitwise_constants \
  --test integer_inv \
  --test ownership_checking \
  --test multifile_ownership_checking

cargo test --locked -p lang-codegen --lib guide_litmus_12

printf '%s\n' \
  'Guide15: 11 diagnostic/ownership checks pass with exact typed-stage snapshots; 1 known diagnostic gap remains (return-when). Litmus12 executes through both native entry points.' \
  'Known-gap checks are not feature completion. Complete typed facts, native coverage of other Litmus examples, two-phase activation, and full frontend conformance are not established by this gate.'

#!/usr/bin/env bash
# SPEC-0238: current Guide examples and their explicit frontend coverage ledger.
# Enable the supported Rust/LLVM/Clang toolchain for the narrow Litmus4 native gate.
set -euo pipefail
cd "$(dirname "$0")/.."

python3 scripts/check_docs.py
cargo test --locked -p lang-frontend --no-fail-fast \
  --test guide_litmus \
  --test ownership_checking \
  --test multifile_ownership_checking

# SPEC-0241: exact Guide4 plus enum identity and explicit unsupported-boundary oracles.
cargo test --locked -p lang-codegen --lib return_control_tests

printf '%s\n' \
  'Guide15: 11 diagnostic/ownership checks pass with exact typed-stage snapshots; 1 known diagnostic gap remains (const bitwise).' \
  'Litmus4: the single-file native example and narrow enum-tag/SSA oracles pass; unit enum conditions and direct case arguments remain unsupported.' \
  'Known-gap checks are not feature completion. Complete typed facts, other SSA/native coverage, two-phase activation, and full frontend conformance are not established by this gate.'

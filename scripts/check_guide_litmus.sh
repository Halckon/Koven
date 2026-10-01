#!/usr/bin/env bash
# SPEC-0238: current Guide examples and their explicit frontend coverage ledger.
# No LLVM is needed for these suites; enable the repository's supported Rust first.
set -euo pipefail
cd "$(dirname "$0")/.."

python3 scripts/check_docs.py
cargo test --locked -p lang-frontend --no-fail-fast \
  --test guide_litmus \
  --test ownership_checking \
  --test multifile_ownership_checking

printf '%s\n' \
  'Guide15: 10 diagnostic/ownership checks pass with exact typed-stage snapshots; 2 known diagnostic gaps remain (return-when, const bitwise).' \
  'Known-gap checks are not feature completion. Complete typed facts, SSA/native, two-phase activation, and full frontend conformance are not established by this gate.'

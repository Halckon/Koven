#!/usr/bin/env bash
# Candidate-only validation. Keep all current Linux core gates; no skip options.
set -euo pipefail
cd "$(dirname "$0")/.."
bash /opt/koven-ci/verify_ci_toolchain_image.sh
mkdir -p "$RUNNER_TEMP/toolchain-evidence"
cp /opt/koven-ci/{installed-packages.txt,input-sha256.txt} "$RUNNER_TEMP/toolchain-evidence/"
# Fail early if the default container cannot launch the debugger. This does not
# replace the later Koven source/file/line/frame assertions or relax security.
debugger_probe="$RUNNER_TEMP/toolchain-evidence/debugger-probe"
printf 'int main(void) { return 0; }\n' > "$debugger_probe.c"
/usr/bin/cc -g "$debugger_probe.c" -o "$debugger_probe"
lldb --batch -o "target create '$debugger_probe'" -o 'breakpoint set --name main' \
  -o run -o 'frame info' 2>&1 | tee "$debugger_probe.log"
grep -Fq 'stop reason = breakpoint' "$debugger_probe.log"
# Separate Clippy and test targets just as the production jobs do.
export CARGO_TARGET_DIR="$RUNNER_TEMP/clippy-target"
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
# Tutorial and CLI gates intentionally resolve the repository target/debug/kovenc.
# A fresh checkout is required; no target cache is restored by this workflow.
test ! -e target
export CARGO_TARGET_DIR="$PWD/target"
export CARGO_PROFILE_DEV_DEBUG=1 CARGO_PROFILE_TEST_DEBUG=1
bash scripts/check_integration.sh
python3 scripts/check_word_frequency.py --evidence "$RUNNER_TEMP/koven-word-frequency"
python3 scripts/conformance_gate.py --baseline conformance/baseline-aef3f4f8.tsv \
  --evidence "$RUNNER_TEMP/koven-conformance"
python3 scripts/check_native_sanitizers.py --linux --artifacts "$RUNNER_TEMP/koven-sanitizers"
python3 scripts/check_generated_owners.py --artifacts "$RUNNER_TEMP/koven-generated-owners"

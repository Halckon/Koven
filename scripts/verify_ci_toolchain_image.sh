#!/usr/bin/env bash
# Offline readiness checks. No apt/rustup/network fallback is permitted here.
set -euo pipefail
source /etc/os-release
[[ "$ID" == ubuntu && "$VERSION_ID" == 26.04 && "$(uname -m)" == x86_64 ]]
[[ "$LLVM_SYS_211_PREFIX" == /usr/lib/llvm-21 ]]
[[ "$(rustc --version)" == 'rustc 1.96.0 (ac68faa20 2026-05-25)' ]]
[[ "$(cargo --version)" == 'cargo 1.96.0 (30a34c682 2026-05-25)' ]]
[[ "$(cargo clippy --version)" == 'clippy 0.1.96 (ac68faa20c 2026-05-25)' ]]
[[ "$(rustfmt --version)" == 'rustfmt 1.9.0-stable (ac68faa20c 2026-05-25)' ]]
[[ "$(llvm-config --version)" == 21.1.8 ]]
llvm-config --targets-built | grep -qw AArch64
llvm-config --targets-built | grep -qw X86
clang --version | grep -Fq 'clang version 21.1.8'
llvm-dwarfdump --version | grep -Fq 'LLVM version 21.1.8'
lldb --version | grep -Fq 'lldb version 21.1.8'
lldb --batch -o "script print('KOVEN_LLDB_SB_API_OK' if hasattr(lldb.SBFileSpec, 'fullpath') and hasattr(lldb.SBFrame, 'GetFunctionName') else 'KOVEN_LLDB_SB_API_MISSING')" | grep -Fx 'KOVEN_LLDB_SB_API_OK'
/usr/bin/cc --version
python3 --version
git --version
for sanitizer in asan lsan; do
  test -f "$(clang --print-resource-dir)/lib/linux/libclang_rt.$sanitizer-x86_64.a"
done
# This repeats installed identity checks, not archive-byte verification. The image
# build's existing installer verifies signed index SHA256 before installation.
for package in llvm-21-dev clang-21 libclang-cpp21 libclang-rt-21-dev \
               lldb-21 liblldb-21 python3-lldb-21; do
  [[ "$(dpkg-query -W -f='${Version} ${Architecture}' "$package:amd64")" == '1:21.1.8-6ubuntu1 amd64' ]]
done

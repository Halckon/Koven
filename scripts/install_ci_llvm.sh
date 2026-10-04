#!/usr/bin/env bash
# Official signed LLVM packages for GitHub's Ubuntu 24.04 x86_64 runner.
# Intentionally fail if this pinned build disappears; never drift to another LLVM.
set -euo pipefail
source /etc/os-release
[[ "$ID" == ubuntu && "$VERSION_ID" == 24.04 && "$(uname -m)" == x86_64 ]] || {
  echo 'This installer requires Ubuntu 24.04 x86_64.' >&2
  exit 1
}
readonly version='1:21.1.8~++20251221032922+2078da43e25a-1~exp1~20251221153059.70'
readonly fingerprint='6084F3CF814B57C1CF12EFD515CF4D18AF4F7421'
key="$(mktemp)"
trap 'rm -f "$key"' EXIT
curl --fail --silent --show-error --location --retry 3 \
  https://apt.llvm.org/llvm-snapshot.gpg.key -o "$key"
key_info="$(gpg --batch --show-keys --with-colons "$key")"
primary_count="$(printf '%s\n' "$key_info" | awk -F: '$1 == "pub" {count++} END {print count+0}')"
actual="$(printf '%s\n' "$key_info" | awk -F: '$1 == "fpr" {print $10; exit}')"
[[ "$primary_count" == 1 ]] || { echo 'Unexpected LLVM keyring contents' >&2; exit 1; }
[[ "$actual" == "$fingerprint" ]] || { echo 'LLVM signing key mismatch' >&2; exit 1; }
sudo install -m 0644 "$key" /usr/share/keyrings/koven-llvm.asc
printf '%s\n' 'deb [arch=amd64 signed-by=/usr/share/keyrings/koven-llvm.asc] https://apt.llvm.org/noble/ llvm-toolchain-noble-21 main' \
  | sudo tee /etc/apt/sources.list.d/koven-llvm.list >/dev/null
sudo apt-get update
sudo apt-get install --yes --no-install-recommends \
  "llvm-21-dev=$version" "clang-21=$version" "libclang-cpp21=$version" "libclang-rt-21-dev=$version" \
  build-essential
actual_version="$(/usr/lib/llvm-21/bin/llvm-config --version)"
[[ "$actual_version" == 21.1.8 ]] || {
  echo "Expected LLVM 21.1.8, found $actual_version" >&2
  exit 1
}
# Debian ships the legacy lib/linux layout. Clang 21 --print-runtime-dir can
# instead return a nonexistent per-target directory, before linker fallback.
resource_dir="$(/usr/lib/llvm-21/bin/clang --print-resource-dir)"
runtime_dir="$resource_dir/lib/linux"
for sanitizer in asan lsan; do
  archive="$runtime_dir/libclang_rt.$sanitizer-x86_64.a"
  [[ -f "$archive" ]] || { echo "Missing pinned LLVM sanitizer archive: $archive" >&2; exit 1; }
  echo "Verified sanitizer archive: $archive"
done

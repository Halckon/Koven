#!/usr/bin/env bash
# Official signed LLVM packages for GitHub's Ubuntu 24.04 x86_64 runner.
# Intentionally fail if this pinned build disappears; never drift to another LLVM.
set -euo pipefail
source /etc/os-release
# Core CI migrates independently; the Preview producer/consumer remain on Noble.
if [[ "$ID" == ubuntu && "$VERSION_ID" == 26.04 ]]; then
  exec bash "$(dirname -- "${BASH_SOURCE[0]}")/install_ci_llvm_ubuntu26.sh"
fi
[[ "$ID" == ubuntu && "$VERSION_ID" == 24.04 && "$(uname -m)" == x86_64 ]] || {
  echo 'This installer requires Ubuntu 24.04 x86_64.' >&2
  exit 1
}
readonly version='1:21.1.8~++20251221032922+2078da43e25a-1~exp1~20251221153059.70'
readonly fingerprint='6084F3CF814B57C1CF12EFD515CF4D18AF4F7421'
# Ubuntu noble-security/noble-updates, signed xz-utils binary package.
# https://archive.ubuntu.com/ubuntu/pool/main/x/xz-utils/liblzma5_5.6.1+really5.4.5-1ubuntu0.4_amd64.deb
readonly lzma_version='5.6.1+really5.4.5-1ubuntu0.4'
readonly lzma_deb_sha256='5bcf5da2339331e21d3f42a831e82c61c54c6b2f29804010e8d59abd0ae52167'
readonly lzma_sha256='26b37d25f7ca2a2d348de8ecabacda12e921cd933b21fa069a179760db77219d'
readonly lzma_library='/usr/lib/x86_64-linux-gnu/liblzma.so.5.4.5'
before_version="$(dpkg-query -W -f='${Version}' liblzma5:amd64)"
printf 'liblzma before: liblzma5:amd64 %s\n' "$before_version"
dpkg --compare-versions "$before_version" le "$lzma_version" || {
  echo "Refusing to downgrade liblzma5 from $before_version to $lzma_version" >&2
  exit 1
}
key="$(mktemp)"
trap 'rm -f "$key"' EXIT
# Bound each key download and its retry window; do not wait on a stalled connection.
curl --fail --silent --show-error --location --connect-timeout 15 --max-time 60 \
  --retry 2 --retry-max-time 180 \
  https://apt.llvm.org/llvm-snapshot.gpg.key -o "$key"
key_info="$(gpg --batch --show-keys --with-colons "$key")"
primary_count="$(printf '%s\n' "$key_info" | awk -F: '$1 == "pub" {count++} END {print count+0}')"
actual="$(printf '%s\n' "$key_info" | awk -F: '$1 == "fpr" {print $10; exit}')"
[[ "$primary_count" == 1 ]] || { echo 'Unexpected LLVM keyring contents' >&2; exit 1; }
[[ "$actual" == "$fingerprint" ]] || { echo 'LLVM signing key mismatch' >&2; exit 1; }
sudo install -m 0644 "$key" /usr/share/keyrings/koven-llvm.asc
printf '%s\n' 'deb [arch=amd64 signed-by=/usr/share/keyrings/koven-llvm.asc] https://apt.llvm.org/noble/ llvm-toolchain-noble-21 main' \
  | sudo tee /etc/apt/sources.list.d/koven-llvm.list >/dev/null
# Retry individual downloads only; update errors must not silently reuse stale indexes.
sudo apt-get -o Acquire::Retries=2 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 \
  -o APT::Update::Error-Mode=any update
apt-cache policy liblzma5:amd64
# Installed dpkg status alone is insufficient: require the indexed exact package.
indexed_sha256="$(apt-cache show "liblzma5:amd64=$lzma_version" | awk '$1 == "SHA256:" {print $2}' | sort -u)"
[[ "$indexed_sha256" == "$lzma_deb_sha256" ]] || {
  echo 'Pinned liblzma5 package is unavailable or its indexed SHA256 differs' >&2
  exit 1
}
sudo apt-get -o Acquire::Retries=2 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 \
  install --yes --no-install-recommends \
  "llvm-21-dev=$version" "clang-21=$version" "libclang-cpp21=$version" "libclang-rt-21-dev=$version" \
  "lldb-21=$version" "liblldb-21=$version" "python3-lldb-21=$version" \
  "liblzma5:amd64=$lzma_version" build-essential
installed_lzma="$(dpkg-query -W -f='${Version} ${Architecture}' liblzma5:amd64)"
[[ "$installed_lzma" == "$lzma_version amd64" ]] || {
  echo "Unexpected installed liblzma5 version/architecture: $installed_lzma" >&2
  exit 1
}
[[ "$(dpkg-query -S "$lzma_library")" == "liblzma5:amd64: $lzma_library" ]] || {
  echo 'Pinned liblzma library owner differs' >&2
  exit 1
}
resolved_lzma="$(readlink -e /usr/lib/x86_64-linux-gnu/liblzma.so.5)"
[[ "$resolved_lzma" == "$lzma_library" ]] || {
  echo "Unexpected liblzma library target: $resolved_lzma" >&2
  exit 1
}
library_sha256="$(sha256sum "$resolved_lzma")"
[[ "${library_sha256%% *}" == "$lzma_sha256" ]] || {
  echo 'Installed liblzma library SHA256 mismatch' >&2
  exit 1
}
printf 'liblzma after: '
dpkg-query -W -f='${binary:Package} ${Version} ${Architecture} ${source:Package} ${source:Version}\n' liblzma5:amd64
printf 'Verified liblzma SHA256: %s\n' "$library_sha256"
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

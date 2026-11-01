#!/usr/bin/env bash
# Core CI only: Ubuntu Resolute's signed LLVM 21.1.8 packages.
# Preview keeps its independently measured Noble installer and runtime closure.
set -euo pipefail
source /etc/os-release
[[ "$ID" == ubuntu && "$VERSION_ID" == 26.04 && "$(uname -m)" == x86_64 ]] || {
  echo 'This installer requires Ubuntu 26.04 x86_64.' >&2
  exit 1
}
readonly version='1:21.1.8-6ubuntu1'
# Official resolute/{main,universe}/binary-amd64 Packages, 2026-11-01.
# Keep exact package identities; do not accept a preinstalled dpkg status alone.
readonly pins='llvm-21-dev aa13e8f4ad91572eaf40e959232d32e07362856e14f22861236017c544ed11d4
clang-21 792701d9c82e5f237879cc0bda552d8a127ea24b0c7d520e815aff14bd314b03
libclang-cpp21 53687c45a0a4d9eac7dc1f70b2c098a9b4994f2c12d66518b26fc7f7a09bba49
libclang-rt-21-dev 1addf72593c6a53aa1ed9e373af88fb625610ffffa050d23fa871cea95f32f4a
lldb-21 5de8f49f7c51cb72b8737a951db824cd89f88e03295e91d88f067de2e5ed8b31
liblldb-21 1e52c21ba8098e9356239c80466df9c4e3a94db9765b8262c3fb159850a10bba
python3-lldb-21 c78939a1166e9e33e21c6ee2db28aa68f36d16f15bc6c4cfb41447fea78db833'
# Use the runner's Ubuntu archive keyring and configured signed repositories.
# Never add Noble/apt.llvm.org sources or allow unauthenticated installation.
apt_options=(-o Acquire::Retries=2 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30
  -o Acquire::AllowInsecureRepositories=false -o Acquire::AllowDowngradeToInsecureRepositories=false
  -o APT::Get::AllowUnauthenticated=false)
sudo apt-get "${apt_options[@]}" -o APT::Update::Error-Mode=any update
packages=()
while read -r package expected; do
  actual="$(apt-cache show "$package:amd64=$version" | awk '$1 == "SHA256:" {print $2}' | sort -u)"
  [[ "$actual" == "$expected" ]] || {
    echo "Pinned $package package unavailable or indexed SHA256 differs" >&2
    exit 1
  }
  installed="$(dpkg-query -W -f='${Version}' "$package:amd64" 2>/dev/null || true)"
  if [[ -n "$installed" ]] && dpkg --compare-versions "$installed" gt "$version"; then
    echo "Refusing to downgrade $package from $installed to $version" >&2
    exit 1
  fi
  packages+=("$package:amd64=$version")
done <<< "$pins"
# Reinstall the verified direct packages so image leftovers are not trusted as payloads.
sudo apt-get "${apt_options[@]}" install --yes --no-install-recommends --reinstall \
  "${packages[@]}" build-essential
while read -r package _; do
  [[ "$(dpkg-query -W -f='${Version} ${Architecture}' "$package:amd64")" == "$version amd64" ]] || {
    echo "Unexpected installed $package identity" >&2
    exit 1
  }
done <<< "$pins"
[[ "$(/usr/lib/llvm-21/bin/llvm-config --version)" == 21.1.8 ]]
resource_dir="$(/usr/lib/llvm-21/bin/clang --print-resource-dir)"
for sanitizer in asan lsan; do
  archive="$resource_dir/lib/linux/libclang_rt.$sanitizer-x86_64.a"
  [[ -f "$archive" ]] || { echo "Missing pinned LLVM sanitizer archive: $archive" >&2; exit 1; }
  echo "Verified sanitizer archive: $archive"
done
# Record dependencies, not a claim that the new runtime closure is Preview-compatible.
dpkg-query -W -f='${binary:Package} ${Version} ${Architecture}\n' \
  libc6:amd64 liblzma5:amd64 libxml2-16:amd64 zlib1g:amd64 libpython3.14:amd64

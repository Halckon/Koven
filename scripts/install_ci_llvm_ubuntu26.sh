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
# Official resolute/{main,universe}/binary-{amd64,amd64v3} Packages, 2026-11-01.
# Columns: package, amd64 SHA256, amd64v3 SHA256. Both signed archive variants
# report Architecture: amd64 and the same version, but have different payload bytes.
# Keep exact package identities; do not accept a preinstalled dpkg status alone.
readonly pins='llvm-21-dev aa13e8f4ad91572eaf40e959232d32e07362856e14f22861236017c544ed11d4 4b608885acf3d34e9e9aa90464ca5f3f3ede19a9e47511b7470d43a4312771e2
clang-21 792701d9c82e5f237879cc0bda552d8a127ea24b0c7d520e815aff14bd314b03 1862e22dd11644b0d77e54a7b1bf6e88f9ebc896178923c102804821ad78717d
libclang-cpp21 53687c45a0a4d9eac7dc1f70b2c098a9b4994f2c12d66518b26fc7f7a09bba49 926457cf375b502727996041c6574037de54600ae33eafe1386ee15bf51c8a95
libclang-rt-21-dev 1addf72593c6a53aa1ed9e373af88fb625610ffffa050d23fa871cea95f32f4a 9ef30bfd2ceaf365ec043f6516b5d886eedf3f222e6f2e1d80b7c6db3bb5ac53
lldb-21 5de8f49f7c51cb72b8737a951db824cd89f88e03295e91d88f067de2e5ed8b31 35db9d45bc87048753c7c19a05c8977e9e8663b4150aed80575786f6788548d9
liblldb-21 1e52c21ba8098e9356239c80466df9c4e3a94db9765b8262c3fb159850a10bba 69637e7c9e8a136b4e93915f653dedf1147ae10739b6c3b0b749acd40c795dd6
python3-lldb-21 c78939a1166e9e33e21c6ee2db28aa68f36d16f15bc6c4cfb41447fea78db833 3bb37d2e0c1e45f621d24ce4d13344d4893f5a59895a74e332db312ff49eefb1'
# Use the runner's Ubuntu archive keyring and configured signed repositories.
# Never add Noble/apt.llvm.org sources or allow unauthenticated installation.
apt_options=(-o Acquire::Retries=2 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30
  -o Acquire::AllowInsecureRepositories=false -o Acquire::AllowDowngradeToInsecureRepositories=false
  -o APT::Get::AllowUnauthenticated=false)
sudo apt-get "${apt_options[@]}" -o APT::Update::Error-Mode=any update
packages=()
while read -r package expected expected_v3; do
  records="$(apt-cache show "$package:amd64=$version")"
  printf '%s\n' "$records" | grep -E '^(Package|Version|Architecture|Architecture-Variant|Filename|SHA256):'
  actual="$(printf '%s\n' "$records" | awk '$1 == "SHA256:" {print $2}' | sort -u)"
  [[ "$actual" == "$expected" || "$actual" == "$expected_v3" ]] || {
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

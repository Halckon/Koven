#!/usr/bin/env bash
# Official Rust distribution, pinned against the 2026-05-28 release manifest.
set -euo pipefail
[[ "$(uname -m)" == x86_64 ]]
readonly release=1.96.0
readonly host=x86_64-unknown-linux-gnu
readonly origin=https://static.rust-lang.org/dist/2026-05-28
# channel-rust-1.96.0.toml SHA256:
# 9af50610e1d82699f78a40b985c9277ae1a2c5a0bec86ae430cfe58832038285
readonly pins='rustc 7d7fa1d0cfb0fab71a956bb78f41107202c17f30ab56c45288e869a37fd9633d
cargo dee75c3c8f9f600ad75bc0c93249e767d3047845a4dd668327ce43ab039ba266
rust-std c09c7c646248f14f473f5f7a029af15ee57c3a9f9bc93dfa72d9621938586b82
clippy 9b0b89c67d5ce8195e1c8733587a63e326353fdda65f62455a64fa03e25659f2
rustfmt 9754ab68f1b8871f8a714548f66d8d44e91f6d904505882945b27e6e3eeb08e2'
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
while read -r component expected; do
  name="$component-$release-$host"
  curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
    --connect-timeout 15 --max-time 300 --retry 2 --retry-max-time 900 \
    "$origin/$name.tar.xz" -o "$work/$name.tar.xz"
  printf '%s  %s\n' "$expected" "$work/$name.tar.xz" | sha256sum --check --strict
  tar -xJf "$work/$name.tar.xz" -C "$work"
  bash "$work/$name/install.sh" --prefix=/opt/rust --disable-ldconfig
  rm -rf "$work/$name" "$work/$name.tar.xz"
done <<< "$pins"
[[ "$(/opt/rust/bin/rustc --version)" == 'rustc 1.96.0 (ac68faa20 2026-05-25)' ]]
/opt/rust/bin/cargo --version
/opt/rust/bin/cargo-clippy --version
/opt/rust/bin/rustfmt --version

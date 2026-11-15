#!/usr/bin/env bash
# Build the default-feature codegen harness with Rust source line tables.
# Keep these settings shared by the receipt, sanitizer and generated-owner callers.
set -euo pipefail
cd "$(dirname "$0")/.."
exec cargo test --locked -p lang-codegen --lib \
  --config 'profile.test.package.lang-codegen.debug="line-tables-only"' \
  --config 'profile.test.package.lang-codegen.codegen-units=64' "$@"

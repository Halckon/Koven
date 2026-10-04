#!/usr/bin/env bash
# Core targets include the source assets owned by lang-std.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test --locked -p lang-frontend --lib
cargo test --locked -p lang-codegen
cargo test --locked -p lang-cli
cargo test --locked -p lang-lsp
cargo test --locked -p lang-std

#!/usr/bin/env bash
# Public non-cfg(test) library publication: CI-only competition, never local execution.
set -euo pipefail
mode="${1:-}"
[[ "$mode" == '' || "$mode" == --list-only ]] || { echo 'usage: check_d10_publication_ci.sh [--list-only]' >&2; exit 2; }
if [[ "$mode" != --list-only ]]; then
  [[ "${GITHUB_ACTIONS:-}" == true ]] || { echo 'D10 publication competition requires GitHub Actions' >&2; exit 2; }
fi
cd "$(dirname "$0")/.."
target=ci_only_concurrent_create_only_publishers_preserve_one_winner
listing=$(cargo test --locked -j1 --no-default-features -p lang-codegen --features d10-catalog-experiment --test d10_create_only_ci "$target" -- --exact --ignored --list)
[[ $(grep -Fxc "$target: test" <<< "$listing") == 1 ]]
if [[ "$mode" == --list-only ]]; then
  echo 'public D10 concurrent publication control compiled/listed; not executed'
else
  cargo test --locked -j1 --no-default-features -p lang-codegen --features d10-catalog-experiment --test d10_create_only_ci "$target" -- --exact --ignored
fi

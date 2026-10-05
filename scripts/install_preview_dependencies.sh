#!/usr/bin/env bash
# Prepare the documented external LLVM and system C driver, without a checkout.
set -euo pipefail
script_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
case "$(uname -s)/$(uname -m)" in
  Darwin/arm64)
    [[ -x /opt/homebrew/bin/brew ]] || {
      echo 'Install Homebrew at /opt/homebrew before preparing this arm64 candidate.' >&2
      exit 1
    }
    /opt/homebrew/bin/brew install llvm@21
    /usr/bin/xcrun --find clang >/dev/null
    /usr/bin/xcrun --show-sdk-path >/dev/null
    [[ "$(/opt/homebrew/opt/llvm@21/bin/llvm-config --version)" == 21.1.* ]] || {
      echo 'Expected external LLVM 21.1.x.' >&2
      exit 1
    }
    ;;
  Linux/x86_64)
    bash "$script_directory/install_ci_llvm.sh"
    sudo /sbin/ldconfig
    [[ -x /usr/bin/cc ]] || { echo 'Missing /usr/bin/cc.' >&2; exit 1; }
    ;;
  *)
    echo 'This candidate supports measured macOS arm64 and Ubuntu 24.04 x86_64 hosts only.' >&2
    exit 1
    ;;
esac

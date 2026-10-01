# Contributing to Koven

Thank you for your interest in contributing to Koven! Koven is a modern, GC-free systems programming language combining the readability of Kotlin with the performance, safety, and deterministic resource management of Rust.

Before contributing, please read our [Code of Conduct](CODE_OF_CONDUCT.md).

---

## 🛠️ Development Setup

### Prerequisites

1. **Rust Toolchain**:
   Koven requires Rust **1.96.0** (with `clippy` and `rustfmt`), pinned via `rust-toolchain.toml`:
   ```bash
   rustup show
   ```

2. **LLVM 21**:
   In accordance with [ADR-0007](docs/adr/accepted/0007-llvm-toolchain-and-first-target.md), native codegen currently targets `aarch64-apple-darwin` using Homebrew `llvm@21`:
   ```bash
   brew install llvm@21
   export LLVM_SYS_211_PREFIX="$(brew --prefix llvm@21)"
   ```

3. **Python 3**:
   Used for documentation and structural validation (`python3 scripts/check_docs.py`).

---

## 🌿 Spec-Driven Branch Workflow

Koven strictly follows a **branch-driven Spec development process**:

1. **Create a Dedicated Branch**:
   Always branch off the latest `main`:
   ```bash
   git checkout main
   git pull origin main
   git checkout -b feature/spec-<id>   # e.g., feature/spec-0211
   # or fix/spec-<id> for targeted bug fixes
   ```
2. **Develop in Isolation**:
   All implementation, test cases, and Spec/Architecture snapshot updates must stay self-contained within your feature branch.
3. **Verify Locally**:
   Run targeted tests per our [testing guide](docs/development/testing.md) before pushing:
   ```bash
   python3 scripts/check_docs.py
   cargo fmt --all -- --check
   cargo clippy -p <affected-crate> --all-targets -- -D warnings
   cargo test -p <affected-crate> --lib <filter>
   ```
   > **Note**: Avoid running whole workspace full suites (`cargo test --workspace --all-targets`) locally to avoid long-running frontend stress matrices.
4. **Submit a Pull Request (PR)**:
   Open a PR against `main`. Our GitHub Actions CI will run automated verification and status checks. Once CI passes and Spec acceptance is signed off, the PR can be merged.

---

## 📚 Essential Reading

- [AGENTS.md](AGENTS.md): Global invariants, boundaries, and agent rules.
- [Language Specification](docs/guide/README.md): The normative definition of Koven syntax and semantics (currently v0.38).
- [Workflow and Phases](docs/development/workflow-and-phases.md): Phase responsibilities and change lifecycle.
- [Testing & Layered Verification](docs/development/testing.md): Rules for minimal sufficient testing.
- [Architecture Snapshots](docs/architecture/README.md): Current facts of the compiler pipeline.

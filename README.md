# Koven

[![CI](https://github.com/Halckon/koven/actions/workflows/ci.yml/badge.svg)](https://github.com/Halckon/koven/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.96.0-orange.svg)](rust-toolchain.toml)

[English](README.md) | [简体中文](README_CN.md)

**Koven** is a systems language under development, with Kotlin-inspired syntax, explicit ownership, call-site loans, and LLVM native code generation.

The runtime uses no tracing garbage collector. Memory and resources follow checked ownership facts: pure-memory values use ASAP drop, while resource values use lexical cleanup. Implementation and native support are bounded; design goals are not claims that every feature or memory-safety property has been verified.

The current specification is [Guide v0.44](docs/guide/README.md); start with the [executable tutorial](docs/tutorials/README.md). The native subset includes sequential collections, Map lookup/update/removal and bounded borrowed iteration, ordinary borrow results, and borrowed List/View ranges with bounded local control flow. These are specific supported combinations, not a promise of arbitrary collection APIs or control flow. See the [current tour notes](docs/tutorials/koven-tour.md#collections-and-borrow-results), [Spec index](docs/specs/README.md), and [architecture](docs/architecture/README.md) for the boundaries. Performance and cost acceptance remains deferred.

---

## ⚡ Highlights

These highlights cover [Guide-defined language features](docs/guide/README.md) alongside implemented capabilities, not a complete native support checklist. Native support depends on the compilation entry, types, and control-flow combinations. See the [tutorial](docs/tutorials/README.md) and [architecture](docs/architecture/README.md) for the delivered subsets.

- **Kotlin-Inspired Syntax & Expressiveness**
  The Guide defines expression-oriented syntax, local type inference, first-class functions, lambdas, and `when` pattern matching. Native callable and matching support is limited to documented forms.
- **Deterministic Ownership Management (No Tracing GC)**
  Memory follows move semantics, single-owner tracking, and call-site loans. Zero overhead and performance equivalence are not accepted claims.
- **Fearless Borrowing Without Lifetime Annotations**
  `Borrow` and `Inout` use checked call-site loans. Supported direct named functions can return a borrow with one `from` source; callers can keep it with `borrow val` or pass it to a synchronous Borrow parameter within the implemented subset. This does not introduce storable reference types, user lifetime parameters, or general borrow transport across control flow. See the [tour notes](docs/tutorials/koven-tour.md#collections-and-borrow-results).
- **Dual-Track Deterministic Drop**
  Pure-memory MoveOnly values use ASAP drop at the earliest safe boundary. Types with deinit or recursively held resources use reverse lexical cleanup, without dropping early merely after their last use.
- **Structurally Derived Capabilities**
  The compiler automatically derives type capabilities like `Copyable` (bitwise or structural duplication) and `Transferable` (a structural ownership-transfer capability, not evidence that thread APIs are delivered).
- **First-Class Null Safety & Flow Typing**
  The Guide defines non-nullable defaults, explicit nullable types (`T?`), safe calls (`?.`), Elvis (`?:`), and control-flow-aware smart casts. Native support covers only bounded combinations.
- **Algebraic Data Types & Expressive Class Family**
  `value class` for flat value layouts; `class` for heap-allocated exclusive reference types; and `enum class` for typed algebraic data types (ADTs) with variant payloads.
- **Explicit Error Handling**
  No hidden exception unwinding or `try`/`catch` overhead. The Guide defines `Result<T, E>` and postfix `?` propagation for recoverable errors. The ordinary single-file and project CLI do not yet automatically load the prelude declarations for `Pair`/`Result`; the Guide’s propagation rules do not establish complete `Result` propagation support in those public entries. Unrecoverable invariant violations trigger an immediate `error(...)` abort.
- **Native LLVM Compilation**
  Emits verified LLVM IR and native executables for AArch64 macOS (Mach-O) and x86_64 Linux with glibc (ELF64).
- **First-Class Developer Tooling**
  Includes the `kovenc` CLI orchestrator, a standard Language Server Protocol (`lang-lsp`) implementation, a non-destructive code formatter, and editor support for Tree-sitter and TextMate.

---

## 🔍 Language Tour

Executable sources and complete CLI output contracts live in the [current Koven tour](docs/tutorials/koven-tour.md). The [static inventory](docs/tutorials/examples.json) contains 17 executable examples, four diagnostic examples, and one planned thread example; expanding argument cases yields 32 non-planned contracts (28 runtime cases and four diagnostic cases). Recorded execution evidence is linked from the [tutorial index](docs/tutorials/README.md).

| Topic | Canonical source examples |
|---|---|
| Hello, functions, branches, constants | [hello](docs/tutorials/koven-tour.md#hello), [function](docs/tutorials/koven-tour.md#function), [branch](docs/tutorials/koven-tour.md#branch), [constant](docs/tutorials/koven-tour.md#constant) |
| String.clone and automatic borrowing | [strings](docs/tutorials/koven-tour.md#strings), [borrowing](docs/tutorials/koven-tour.md#borrowing) |
| Collections, borrow results, and range boundaries | [current capability notes](docs/tutorials/koven-tour.md#collections-and-borrow-results) |
| Map-based argument word counts | [argv-word-frequency](docs/tutorials/koven-tour.md#argv-word-frequency) |
| Sequential iteration, root replace/swap, resource deinit | [iteration](docs/tutorials/koven-tour.md#iteration), [root-replace-swap](docs/tutorials/koven-tour.md#root-replace-swap), [deinit](docs/tutorials/koven-tour.md#deinit) |
| Program arguments and multi-file projects | [arguments](docs/tutorials/koven-tour.md#arguments), [cross-file](docs/tutorials/koven-tour.md#cross-file)  [parameter-report](docs/tutorials/koven-tour.md#parameter-report) |
| Numeric literals, cleanup, and iteration exits | [numbers-bitwise](docs/tutorials/koven-tour.md#numbers-bitwise), [scope-cleanup](docs/tutorials/koven-tour.md#scope-cleanup), [unit-loop-cleanup](docs/tutorials/koven-tour.md#unit-loop-cleanup) |
| Mutable place and iteration loan diagnostics | [reject-immutable-place](docs/tutorials/koven-tour.md#reject-immutable-place), [reject-iteration-move](docs/tutorials/koven-tour.md#reject-iteration-move) |

Declarations use `own param: T` for ownership transfer, default or `borrow param: T` for shared loans, and `inout param: T` for mutable loans. Mutable place arguments use `&place`; shared arguments have no call-site Borrow marker. Class families, nullable values, and Result are specified in the [Guide](docs/guide/README.md), while native representations and unsupported combinations are recorded in [Architecture](docs/architecture/README.md). The word-frequency example traverses the argument array twice and uses Map `get`/`put`/`remove`; it does not iterate a Map. List/View `take`/`drop`/`dropLast` use the project CLI with `koven.algorithms` imports; the single-file CLI does not load those standard-library sources. Map borrowed iteration has a bounded single/unit backend subset, including tested owned MutableMap parameters, transparent grouped sources, and existing Resource representations; see the [source and exit matrix](docs/architecture/map-borrowed-iteration.md). This does not add a public CLI tour contract or complete SPEC-0331. Collection `forEach`/`map`/`filter`, unlisted collection APIs, String tokenization, and general integer formatting must not be inferred from Kotlin.

---

## 🏗 Compiler Architecture

The Koven repository is structured as a Cargo workspace with strict unidirectional layering:

```text
.ko Source
   │
   ▼
[ crates/lang-frontend ]
   │  - Lexer & Parser / AST
   │  - Name Resolution & Scope Indexing
   │  - Type Checking & Generics
   │  - Deterministic Ownership & Borrow Checker
   │  - Formatter & Structured Diagnostics
   ▼
[ crates/lang-codegen ]
   │  - Owner-Aware Typed SSA
   │  - SSA Verifier & Frontend Lowering
   │  - LLVM IR Generation (Inkwell)
   │  - Object File Emission (.o)
   ▼
[ crates/lang-cli ] (kovenc)
   │  - Project Manifest & Source Set Discovery
   │  - Native Linker Integration
   │  - Process Execution & Terminal Diagnostics
   ▼
Native Executable
```

### Workspace Crates

| Crate | Path | Responsibility |
|---|---|---|
| `lang-frontend` | [`crates/lang-frontend/`](crates/lang-frontend/) | Source/Span, Lexer, AST, name/type analysis, ownership checking, diagnostics, code formatter. Free of LLVM dependencies. |
| `lang-codegen` | [`crates/lang-codegen/`](crates/lang-codegen/) | Typed SSA representation, verification, LLVM IR translation, object file emission. |
| `lang-cli` | [`crates/lang-cli/`](crates/lang-cli/) | The `kovenc` driver: build/run pipeline, project discovery, linking, and CLI/JSON diagnostic rendering. |
| `lang-lsp` | [`crates/lang-lsp/`](crates/lang-lsp/) | Language Server Protocol server (stdio, single-file & multi-file compilation-unit snapshots). |
| `lang-std` | [`crates/lang-std/`](crates/lang-std/) | Koven standard library sources (`koven/**/*.ko`) and bootstrap prelude. |

---

## 🛠 Tooling & CLI Usage

Koven provides the `kovenc` command-line tool for compiling, running, and formatting Koven code.

### 1. Compile a Single File

Compile a standalone `.ko` source file directly to a native executable:

```bash
kovenc build main.ko -o my_app
```

Optionally specify a custom entry function:

```bash
kovenc build main.ko --entry bootstrapHello -o my_app
```

### 2. Run Directly

Compile and execute in one step:

```bash
kovenc run main.ko
kovenc run main.ko --entry customEntry -- arg1 arg2
```

### 3. Multi-File Projects

Build or run structured multi-file projects using `project.toml`:

```bash
kovenc build --project project.toml --entry my_package.main -o my_app
kovenc run --project project.toml --entry my_package.main
```

### 4. Non-Destructive Code Formatter

Format source code to stdout or verify compliance:

```bash
kovenc format path/to/file.ko
kovenc format --check path/to/file.ko
```

### 5. IDE & Machine Diagnostics

Export structured, machine-readable compiler diagnostics as JSON Lines:

```bash
kovenc --message-format=json build main.ko -o my_app
```

---

## 🚀 Getting Started

### Prerequisites

- **Rust**: The repository pins Rust 1.96.0 in `rust-toolchain.toml`; the manifest MSRV is also 1.96.0 (Rust 2024 edition).
- **Supported hosts**: AArch64 macOS (`aarch64-apple-darwin`) or x86_64 Linux with glibc (`x86_64-unknown-linux-gnu`). Compilation targets the host; there is no `--target` or cross-compilation support. Linux musl, Linux AArch64, Intel macOS, and Windows are not supported.
- **LLVM**: LLVM 21.1.x with matching development headers/libraries, `llvm-config`, and the host backend. The workspace uses Inkwell 0.10.0 with AArch64 and X86 target features. Set `LLVM_SYS_211_PREFIX` to the LLVM installation prefix; its shared libraries must be discoverable at build time and runtime.
- **System C toolchain**: macOS requires Xcode Command Line Tools and `/usr/bin/clang`; Linux requires `/usr/bin/cc`, glibc development files, and a working system linker. LLVM emits the object directly, then this C driver links it.
- **Native test tools**: Linux LLVM IR instrumentation tests require matching Clang 21, preferably at `LLVM_SYS_211_PREFIX/bin/clang` (otherwise `clang` on PATH). Linux DWARF tests use matching `llvm-dwarfdump`, also prefix-first with a PATH fallback. LLVM IR instrumentation on both hosts needs matching Clang 21; macOS debugging tests use `/usr/bin/lldb`. These tools are separate from the production link driver.

Point to an existing LLVM installation before building. On AArch64 macOS with Homebrew `llvm@21`:

```bash
export LLVM_SYS_211_PREFIX="$(brew --prefix llvm@21)"
"$LLVM_SYS_211_PREFIX/bin/llvm-config" --version
```

On Linux, use the prefix containing your LLVM 21.1.x `bin/llvm-config`; adjust this example to its actual installation path:

```bash
export LLVM_SYS_211_PREFIX=/usr/lib/llvm-21
"$LLVM_SYS_211_PREFIX/bin/llvm-config" --version
```

See the [native target decision](docs/adr/accepted/0026-linux-x86-64-native-host.md) for the support boundary and [testing guide](docs/development/testing.md#本机目标与工具前提) for platform-specific checks.

### Building Koven from Source

Clone the repository and build the workspace:

```bash
git clone https://github.com/Halckon/koven.git
cd koven
cargo build --locked --release
```

The compiler binary will be generated at `target/release/kovenc`.
Add this build directory to the current shell PATH to use the `kovenc` commands above; this does not install system-wide:

```bash
export PATH="$(pwd)/target/release:$PATH"
```

### Running the Prelude Smoke Test

Verify your setup by running the standard library hello bootstrap:

```bash
cargo run --locked -p lang-cli -- run crates/lang-std/koven/prelude.ko --entry bootstrapHello
```

Output:
```text
Hello, World!
```

---

## 📚 Documentation & Specifications

The repository contains comprehensive documentation organized under [`docs/`](docs/):

- [**Current executable tutorial**](docs/tutorials/README.md): Markdown source authority and real CLI contracts.
- [**Specs and evolution status**](docs/specs/README.md): Bounded contracts, archive relationships, and capability gaps.
- [**Language Specification (v0.44)**](docs/guide/README.md): The normative source of truth for Koven syntax, semantics, type rules, and ownership mechanics.
- [**Compiler Architecture**](docs/architecture/README.md): Detailed snapshots of the compilation pipeline, typed SSA design, and codegen.
- [**Development & Testing Guide**](docs/development/README.md): Guidelines for testing, layered verification, and code invariants.
- [**Architecture Decision Records (ADRs)**](docs/adr/README.md): Records of long-term architectural designs and technical rationales.

---

## 📄 License

Koven is distributed under the terms of both the MIT license and the Apache License (Version 2.0).

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) for details.

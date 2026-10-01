# Koven

[![CI](https://github.com/Halckon/koven/actions/workflows/ci.yml/badge.svg)](https://github.com/Halckon/koven/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.96.0-orange.svg)](rust-toolchain.toml)

[English](README.md) | [简体中文](README_CN.md)

**Koven** is a modern systems programming language designed to blend the elegance, expressiveness, and readability of Kotlin with the bare-metal performance, deterministic resource management, and fearless safety of Rust.

Koven operates with **no garbage collector (GC-free)**, eliminates runtime pauses, and compiles directly to native machine code via **LLVM**. By adopting an innovative ownership model based on **call-site loans** and **explicit moves**, Koven achieves complete memory safety without requiring complex lifetime annotations (`'a`, `'b`).

---

## ⚡ Highlights

- **Kotlin-Inspired Syntax & Expressiveness**
  Clean, readable, expression-oriented syntax with powerful type inference, first-class functions, lambdas, and pattern matching.
- **Zero-Cost Deterministic Memory Safety (No GC)**
  No garbage collection pauses or background runtime overhead. Memory is managed deterministically through move semantics, strict single-owner tracking, and call-site borrowing.
- **Fearless Borrowing Without Lifetime Annotations**
  Call-site loans (`Borrow` and `Inout`) are strictly bounded to the synchronous call stack. Borrows cannot be returned or stored into heap structures, eliminating lifetime parameters entirely while preserving total safety.
- **ASAP (As-Soon-As-Possible) Drop**
  Non-copyable resources (`MoveOnly`) are automatically deallocated at the earliest proven inactive boundary, rather than waiting for enclosing block braces to close.
- **Structurally Derived Capabilities**
  The compiler automatically derives type capabilities like `Copyable` (bitwise or structural duplication) and `Transferable` (safe cross-thread transfer without data races).
- **First-Class Null Safety & Flow Typing**
  Non-nullable by default. Explicit nullable types (`T?`), safe navigation (`?.`), Elvis operator (`?:`), and control-flow-aware smart casts.
- **Algebraic Data Types & Expressive Class Family**
  Lightweight `value class` for flat, stack-allocated structs; `class` for heap-allocated exclusive reference types; and `enum class` for typed algebraic data types (ADTs) with variant payloads.
- **Explicit Error Handling**
  No hidden exception unwinding or `try`/`catch` overhead. Recoverable errors are represented as `Result<T, E>` with ergonomic `?` propagation, while unrecoverable invariant violations trigger an immediate `error(...)` abort.
- **Native LLVM Compilation**
  Emits verified LLVM IR and native executables for AArch64 macOS (Mach-O) and x86_64 Linux with glibc (ELF64).
- **First-Class Developer Tooling**
  Includes the `kovenc` CLI orchestrator, a standard Language Server Protocol (`lang-lsp`) implementation, a non-destructive code formatter, and editor support for Tree-sitter and TextMate.

---

## 🔍 Language Tour

### 1. Hello World & Functions

```kotlin
fun main(): Unit {
    println("Hello, Koven!")
}

fun add(a: Int, b: Int): Int = a + b
```

### 2. Classes, Value Classes & ADTs

Koven provides distinct constructs for different memory and structural requirements:

```kotlin
// Flat, stack-allocated value type (zero heap overhead, Copyable if fields are Copyable)
value class Point(val x: Int, val y: Int)

// Heap-allocated entity with exclusive ownership
class Buffer(val capacity: Int, var size: Int)

// Algebraic Data Type (enum with payloads)
enum class Shape {
    Circle(radius: Int),
    Rectangle(width: Int, height: Int),
    Point
}

// Exhaustive pattern matching with smart casts
fun area(shape: Shape): Int = when (shape) {
    is Shape.Circle -> 3 * shape.radius * shape.radius
    is Shape.Rectangle -> shape.width * shape.height
    is Shape.Point -> 0
}
```

### 3. Ownership & Call-Site Loans

In Koven, parameters explicitly declare ownership behavior:

- `own param: T`: Transfers ownership of `T`.
- `param: T` (or `borrow param: T`): Shared, read-only loan during the synchronous call.
- `inout param: T`: Exclusive mutable loan passed at the call-site using `&place`.

```kotlin
class Resource(val id: Int)

fun inspect(item: Resource): Unit {
    // Read-only access via shared call-site loan
    println("Inspecting resource")
}

fun consume(own item: Resource): Unit {
    // Takes ownership; resource will be dropped ASAP
}

fun update(inout target: Resource, own replacement: Resource): Unit {
    // Replaces target place exclusively; old target is dropped
    target = replacement
}

fun example(): Unit {
    val res = Resource(1)
    inspect(res)          // Shared loan (res remains valid)
    consume(res)          // Ownership moved (res is no longer available)
    // inspect(res)       // Compile error: use after move!
}
```

### 4. Null Safety & Result Error Handling

```kotlin
// Null safety with Elvis operator and smart cast
fun printLength(text: String?): Unit {
    val len = text?.length ?: 0
    println("Length is: " + len)
}

// Result propagation with postfix ?
enum class MathError { DivisionByZero }

fun divide(numerator: Int, denominator: Int): Result<Int, MathError> {
    if (denominator == 0) {
        return Result.Err(MathError.DivisionByZero)
    }
    return Result.Ok(numerator / denominator)
}

fun compute(a: Int, b: Int): Result<Int, MathError> {
    val quotient = divide(a, b)?   // Propagates Err early if failed
    return Result.Ok(quotient * 2)
}
```

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

- **Rust**: MSRV 1.96.0 or later (Rust 2024 edition).
- **Supported hosts**: AArch64 macOS (`aarch64-apple-darwin`) or x86_64 Linux with glibc (`x86_64-unknown-linux-gnu`). Compilation targets the host; there is no `--target` or cross-compilation support. Linux musl, Linux AArch64, Intel macOS, and Windows are not supported.
- **LLVM**: LLVM 21.1.x with matching development headers/libraries, `llvm-config`, and the host backend. The workspace uses Inkwell 0.10.0 with AArch64 and X86 target features. Set `LLVM_SYS_211_PREFIX` to the LLVM installation prefix; its shared libraries must be discoverable at build time and runtime.
- **System C toolchain**: macOS requires Xcode Command Line Tools and `/usr/bin/clang`; Linux requires `/usr/bin/cc`, glibc development files, and a working system linker. LLVM emits the object directly, then this C driver links it.
- **Native test tools**: Linux LLVM IR instrumentation tests require matching Clang 21, preferably at `LLVM_SYS_211_PREFIX/bin/clang` (otherwise `clang` on PATH). Linux DWARF tests use matching `llvm-dwarfdump`, also prefix-first with a PATH fallback. macOS tests keep `/usr/bin/clang` and `/usr/bin/lldb`. These test tools are separate from the production link driver.

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
cargo build --release
```

The compiler binary will be generated at `target/release/kovenc`.

### Running the Prelude Smoke Test

Verify your setup by running the standard library hello bootstrap:

```bash
cargo run -p lang-cli -- run crates/lang-std/koven/prelude.ko --entry bootstrapHello
```

Output:
```text
Hello, World!
```

---

## 📚 Documentation & Specifications

The repository contains comprehensive documentation organized under [`docs/`](docs/):

- [**Language Specification (v0.40)**](docs/guide/README.md): The normative source of truth for Koven syntax, semantics, type rules, and ownership mechanics.
- [**Compiler Architecture**](docs/architecture/README.md): Detailed snapshots of the compilation pipeline, typed SSA design, and codegen.
- [**Development & Testing Guide**](docs/development/README.md): Guidelines for testing, layered verification, and code invariants.
- [**Architecture Decision Records (ADRs)**](docs/adr/README.md): Records of long-term architectural designs and technical rationales.

---

## 📄 License

Koven is distributed under the terms of both the MIT license and the Apache License (Version 2.0).

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) for details.

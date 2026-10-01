# Koven

[![CI](https://github.com/Halckon/koven/actions/workflows/ci.yml/badge.svg)](https://github.com/Halckon/koven/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.96.0-orange.svg)](rust-toolchain.toml)

[English](README.md) | [简体中文](README_CN.md)

**Koven** 是一门现代系统编程语言，旨在融合 **Kotlin** 的优雅、高表现力与清晰语法，以及 **Rust** 的极致原生性能、确定性资源管理与无畏安全性。

Koven 运行于**无垃圾回收（GC-free）**的确定性运行时环境，消除一切垃圾回收暂停与后台开销，并通过 **LLVM** 后端直接编译生成原生机器码。借助基于**调用期借用（Call-site Loans）**与**显式移动（Move）**的创新所有权模型，Koven 在无需编写复杂生命周期参数（`'a`, `'b`）的前提下，达成了完备的静态内存安全。

---

## ⚡ 核心亮点

- **Kotlin 风格的优雅语法与表现力**
  清晰、现代、以表达式为导向的语法，具备强大的局部类型推断、一等函数、闭包与模式匹配能力。
- **零成本的确定性内存安全（无 GC）**
  没有垃圾回收停顿或隐藏的运行时负荷。内存完全通过单所有者跟踪、移动语义以及调用期借用进行确定性管理。
- **免生命周期标注的无畏借用**
  调用期借用（`Borrow` 与 `Inout`）严格受限于同步调用栈。借用绝不脱离栈帧返回，也无法存储进长生命周期的堆结构中，从而在彻底消除生命周期标注心智负担的同时保障绝对安全。
- **ASAP（尽快）确定性析构**
  非复制资源（`MoveOnly`）会在代码控制流中被证明不再活跃（inactive）的最早静态边界自动释放，无需苦等外层闭合花括号结束。
- **编译器结构化推导的类型能力**
  由编译器自动递归推导类型能力：`Copyable`（按值位拷贝或结构浅拷贝）与 `Transferable`（可安全跨线程转移所有权，杜绝跨线程数据竞争）。
- **一等公民的空安全与 Flow Typing**
  默认不可为空。通过 `T?` 明确表示可空，配合安全调用运算符（`?.`）、Elvis 运算符（`?:`）以及基于控制流分析的自动智能类型转换（Smart Casts）。
- **代数数据类型与完备的类型家族**
  提供用于无堆开销扁平值布局的 `value class`；用于独占引用堆分配对象的普通 `class`；以及带关联负载的代数数据类型（ADT）`enum class`。
- **显式无异常错误处理**
  没有隐式的异常栈展开与 `try`/`catch` 运行时包袱。可预期失败通过 `Result<T, E>` 搭配后缀 `?` 运算符优雅传播；不可恢复的不变量破坏直接触发 `error(...)` abort。
- **LLVM 原生编译**
  生成经过验证的 typed SSA 与 LLVM IR，支持 AArch64 macOS（Mach-O）和 x86_64 Linux + glibc（ELF64）本机可执行文件。
- **开箱即用的工程化工具链**
  包含编译器驱动 `kovenc` CLI、标准 Language Server Protocol (`lang-lsp`) 服务端、无破坏性代码格式化器，以及 Tree-sitter 与 TextMate 编辑器语法支持。

---

## 🔍 语法与特性巡礼

### 1. 基础函数与表达式

```kotlin
fun main(): Unit {
    println("Hello, Koven!")
}

// 表达式单行函数与类型推断
fun add(a: Int, b: Int): Int = a + b
```

### 2. 类家族、值对象与代数数据类型 (ADT)

Koven 根据内存布局和所有权需求提供了细分明确的结构：

```kotlin
// 扁平展开的栈分配值类型（无堆分配开销，字段全部 Copyable 时自动满足 Copyable）
value class Point(val x: Int, val y: Int)

// 堆上分配的实体对象，具备排他性所有权
class Buffer(val capacity: Int, var size: Int)

// 代数数据类型（带关联数据的 enum class）
enum class Shape {
    Circle(radius: Int),
    Rectangle(width: Int, height: Int),
    Point
}

// 穷尽性 when 模式匹配与智能转换（Smart Casts）
fun area(shape: Shape): Int = when (shape) {
    is Shape.Circle -> 3 * shape.radius * shape.radius
    is Shape.Rectangle -> shape.width * shape.height
    is Shape.Point -> 0
}
```

### 3. 所有权、移动与调用期借用

在 Koven 中，函数参数在声明端明确其所有权契约：

- `own param: T`：转移所有权（Move）。
- `param: T`（或显式 `borrow param: T`）：调用期只读共享借用（Shared Loan），调用方保持所有权。
- `inout param: T`：调用期独占可变借用（Exclusive Loan），在调用点通过 `&place` 显式传入。

```kotlin
class Resource(val id: Int)

fun inspect(item: Resource): Unit {
    // 通过调用期共享借用只读访问
    println("Inspecting resource")
}

fun consume(own item: Resource): Unit {
    // 接管所有权；离开活跃期后由 ASAP 析构自动释放
}

fun update(inout target: Resource, own replacement: Resource): Unit {
    // 独占替换目标位置，旧值被安全析构
    target = replacement
}

fun example(): Unit {
    val res = Resource(1)
    inspect(res)          // 共享借用（res 在调用后依然有效）
    consume(res)          // 所有权转移（res 在此后不可再用）
    // inspect(res)       // 编译报错：使用已被移动的值！
}
```

### 4. 空安全与基于 Result 的显式错误处理

```kotlin
// 空安全、安全调用与 Elvis 兜底
fun printLength(text: String?): Unit {
    val len = text?.length ?: 0
    println("Length is: " + len)
}

// Result 显式错误与后缀 ? 运算符传播
enum class MathError { DivisionByZero }

fun divide(numerator: Int, denominator: Int): Result<Int, MathError> {
    if (denominator == 0) {
        return Result.Err(MathError.DivisionByZero)
    }
    return Result.Ok(numerator / denominator)
}

fun compute(a: Int, b: Int): Result<Int, MathError> {
    val quotient = divide(a, b)?   // 若失败则立即向外传播 Err
    return Result.Ok(quotient * 2)
}
```

---

## 🏗 编译器架构与流水线

Koven 仓库采用单一代码库（Monorepo）与 Cargo workspace 组织，维持严格的单向无环分层依赖：

```text
.ko 源码
   │
   ▼
[ crates/lang-frontend ]
   │  - Lexer 词法分析与 Parser / AST 语法树构建
   │  - 名称解析与作用域索引 (Name Resolution)
   │  - 类型推断、泛型与一致性检查 (Type Checking)
   │  - 确定性所有权与借用检查器 (Ownership Checker)
   │  - 源码格式化器与结构化诊断 (Diagnostics)
   ▼
[ crates/lang-codegen ]
   │  - 感知所有权的 Typed SSA 中间表示
   │  - SSA Verifier 与前端 Lowering
   │  - LLVM IR 生成 (Inkwell 封装)
   │  - 机器目标文件输出 (.o)
   ▼
[ crates/lang-cli ] (kovenc)
   │  - 工程配置 (project.toml) 与 Source Set 发现
   │  - 系统原生链接器集成与桥接
   │  - 编译执行编排与终端高亮诊断渲染
   ▼
原生可执行文件 (Native Executable)
```

### Workspace 模块划分

| Crate 模块 | 目录路径 | 核心职责 |
|---|---|---|
| `lang-frontend` | [`crates/lang-frontend/`](crates/lang-frontend/) | Source/Span 追踪、Lexer、AST、名称/类型分析、所有权检查器、结构化诊断、格式化器。完全不依赖 LLVM。 |
| `lang-codegen` | [`crates/lang-codegen/`](crates/lang-codegen/) | Typed SSA 结构定义、校验器、LLVM IR 翻译、原生目标文件生成。 |
| `lang-cli` | [`crates/lang-cli/`](crates/lang-cli/) | `kovenc` 命令行入口：流水线编排、工程发现、系统链接器调用、终端/JSON 诊断输出。 |
| `lang-lsp` | [`crates/lang-lsp/`](crates/lang-lsp/) | 标准语言服务协议实现（stdio 通信，支持单文件与多文件 source-set 会话及定义跳转）。 |
| `lang-std` | [`crates/lang-std/`](crates/lang-std/) | Koven 标准库源码实现（`koven/**/*.ko`）与基础 prelude 入口。 |

---

## 🛠 工具链与 CLI 用法

Koven 提供了名为 `kovenc` 的命令行工具，用于代码编译、运行与格式化。

### 1. 单文件编译

将独立的 `.ko` 源文件编译为本地原生可执行文件：

```bash
kovenc build main.ko -o my_app
```

可指定自定义入口函数：

```bash
kovenc build main.ko --entry bootstrapHello -o my_app
```

### 2. 直接运行

一步完成编译与运行：

```bash
kovenc run main.ko
kovenc run main.ko --entry customEntry -- arg1 arg2
```

### 3. 多文件工程项目

通过 `project.toml` 构建或运行结构化项目：

```bash
kovenc build --project project.toml --entry my_package.main -o my_app
kovenc run --project project.toml --entry my_package.main
```

### 4. 非破坏性代码格式化器

将排版良好的代码输出至终端，或检查文件是否符合规范：

```bash
kovenc format path/to/file.ko
kovenc format --check path/to/file.ko
```

### 5. IDE 与机器诊断输出

输出机器可读的结构化 JSON Lines 诊断，方便编辑器集成：

```bash
kovenc --message-format=json build main.ko -o my_app
```

---

## 🚀 快速上手

### 环境准备

- **Rust 工具链**：MSRV 1.96.0 或更高版本（Rust 2024 edition）。
- **受支持宿主**：AArch64 macOS（`aarch64-apple-darwin`）或 x86_64 Linux + glibc（`x86_64-unknown-linux-gnu`）。编译目标始终是当前宿主，不提供 `--target` 或交叉编译；不支持 Linux musl、Linux AArch64、Intel macOS 或 Windows。
- **LLVM**：LLVM 21.1.x 及匹配的开发头文件、库、`llvm-config` 和宿主 backend。workspace 使用 Inkwell 0.10.0，启用 AArch64 与 X86 target feature。将 `LLVM_SYS_211_PREFIX` 设置为 LLVM 安装前缀，并确保构建及运行环境可加载其动态库。
- **系统 C 工具链**：macOS 需要 Xcode Command Line Tools 与 `/usr/bin/clang`；Linux 需要 `/usr/bin/cc`、glibc 开发文件和可用的系统 linker。LLVM 直接生成 object，再由该 C driver 链接。
- **Native 测试工具**：Linux LLVM IR 插桩测试需要匹配的 Clang 21，优先使用 `LLVM_SYS_211_PREFIX/bin/clang`，否则使用 PATH 中的 `clang`。Linux DWARF 测试使用匹配的 `llvm-dwarfdump`，同样优先 prefix、缺失时回退到 PATH。macOS 测试保留 `/usr/bin/clang` 与 `/usr/bin/lldb`。这些测试工具与生产链接 driver 分开。

构建前指向已有 LLVM 安装并核对版本。AArch64 macOS 已安装 Homebrew `llvm@21` 时：

```bash
export LLVM_SYS_211_PREFIX="$(brew --prefix llvm@21)"
"$LLVM_SYS_211_PREFIX/bin/llvm-config" --version
```

Linux 使用包含 LLVM 21.1.x `bin/llvm-config` 的安装前缀，请按实际安装路径修改示例：

```bash
export LLVM_SYS_211_PREFIX=/usr/lib/llvm-21
"$LLVM_SYS_211_PREFIX/bin/llvm-config" --version
```

支持边界见[本机目标决策](docs/adr/accepted/0026-linux-x86-64-native-host.md)，分平台检查见[测试指南](docs/development/testing.md#本机目标与工具前提)。

### 源码编译安装

克隆仓库并编译 workspace：

```bash
git clone https://github.com/Halckon/koven.git
cd koven
cargo build --release
```

编译生成的目标程序位于 `target/release/kovenc`。

### 运行标准库 Smoke 测试

通过标准库的 hello bootstrap 验证编译管线：

```bash
cargo run -p lang-cli -- run crates/lang-std/koven/prelude.ko --entry bootstrapHello
```

输出：
```text
Hello, World!
```

---

## 📚 文档与规范索引

仓库内包含组织在 [`docs/`](docs/) 下的完整文档体系：

- [**语言规范 (v0.40)**](docs/guide/README.md)：Koven 语法、语义、类型规则与所有权机制的权威真源。
- [**编译器架构快照**](docs/architecture/README.md)：编译流水线、Typed SSA 与代码生成的当前事实说明。
- [**开发与测试指南**](docs/development/README.md)：分层验证、测试门禁与代码不变式规范。
- [**架构决策记录 (ADR)**](docs/adr/README.md)：重大架构决策的历史背景、长期考量与技术设计记录。

---

## 📄 许可证

Koven 采用双重开源授权，您可以自由选择基于以下任一协议使用：

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) 或 http://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE-MIT) 或 http://opensource.org/licenses/MIT)

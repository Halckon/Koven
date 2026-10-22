# Koven

[![CI](https://github.com/Halckon/koven/actions/workflows/ci.yml/badge.svg)](https://github.com/Halckon/koven/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.96.0-orange.svg)](rust-toolchain.toml)

[English](README.md) | [简体中文](README_CN.md)

**Koven** 是一门开发中的系统编程语言，采用 Kotlin 风格语法、显式所有权与调用期借用，通过 LLVM 生成本机代码。

运行时不使用追踪式垃圾回收。内存与资源按已验证的所有权事实确定性清理：纯内存类型使用 ASAP，资源类型采用词法作用域清理。当前实现和原生支持是有界的，不将语言设计目标视为全部功能或内存安全已完成验证。

当前规范为 [Guide v0.46](docs/guide/README.md)，可执行入门示例见[当前教程](docs/tutorials/README.md)。当前原生子集包括顺序集合、Map 查询/更新/删除及有界借用遍历、普通借用返回，以及带有界局部控制流的 List/View 范围借用。这些是具体组合的支持，不代表任意集合 API 或控制流均已实现；使用方式和限制见[tour 当前能力说明](docs/tutorials/koven-tour.md#collections-and-borrow-results)、[Spec索引](docs/specs/README.md)与[架构快照](docs/architecture/README.md)。性能/成本验收仍延期，不作比较性能承诺。

---

## ⚡ 核心亮点

以下亮点同时介绍[Guide定义的语言特性](docs/guide/README.md)与已实现能力，不是完整原生支持清单。原生支持取决于编译入口、类型和控制流组合；已交付子集见[教程](docs/tutorials/README.md)与[架构快照](docs/architecture/README.md)。

- **Kotlin 风格的优雅语法与表现力**
  Guide 定义以表达式为导向的语法、局部类型推断、一等函数、闭包和 `when` 模式匹配；当前 callable 与匹配的原生支持限于已记录的形式。
- **确定性所有权管理（无追踪式 GC）**
  通过单所有者跟踪、移动与调用期借用管理内存；没有经过验收的零开销或性能等价承诺。
- **免生命周期标注的无畏借用**
  `Borrow` 与 `Inout` 使用受检查的调用期借用。已支持的直接具名函数可以从唯一 `from` 来源返回借用，调用方可在已实现范围内用 `borrow val` 保留结果，或直接传给同步 Borrow 参数；这不引入可存储引用类型、用户生命周期参数或任意控制流间的借用运输。见[tour 当前能力说明](docs/tutorials/koven-tour.md#collections-and-borrow-results)。
- **双轨确定性析构**
  纯内存MoveOnly值在最早安全边界ASAP释放；有deinit或递归持有资源的类型按词法作用域逆序清理，不能因最后使用提前析构。
- **编译器结构化推导的类型能力**
  由编译器自动递归推导类型能力：`Copyable`（按值位拷贝或结构浅拷贝）与 `Transferable`（所有权转移的结构能力；不代表线程API已交付）。
- **一等公民的空安全与 Flow Typing**
  Guide 定义默认非空、显式可空类型（`T?`）、安全调用（`?.`）、Elvis（`?:`）及基于控制流的智能类型转换；当前原生支持仅覆盖有界组合。
- **代数数据类型与完备的类型家族**
  提供用于扁平值布局的 `value class`；用于独占引用堆分配对象的普通 `class`；以及带关联负载的代数数据类型（ADT）`enum class`。
- **显式无异常错误处理**
  没有隐式的异常栈展开与 `try`/`catch` 运行时包袱。Guide 定义了可预期失败的 `Result<T, E>` 与后缀 `?` 传播规则；当前普通单文件和 project CLI 尚不自动加载 prelude 中的 `Pair`/`Result` 声明，传播规则不代表这些公开入口已经完整支持 `Result` 传播。不可恢复的不变量破坏直接触发 `error(...)` abort。
- **LLVM 原生编译**
  生成经过验证的 typed SSA 与 LLVM IR，支持 AArch64 macOS（Mach-O）和 x86_64 Linux + glibc（ELF64）本机可执行文件。
- **开箱即用的工程化工具链**
  包含编译器驱动 `kovenc` CLI、标准 Language Server Protocol (`lang-lsp`) 服务端、无破坏性代码格式化器，以及 Tree-sitter 与 TextMate 编辑器语法支持。

---

## 🔍 语法与特性巡礼

示例源码与完整 CLI 输出合同统一维护在[当前 Koven tour](docs/tutorials/koven-tour.md)。[静态库存](docs/tutorials/examples.json)包含 17 个执行正例、4 个诊断负例和 1 个 planned 线程例；展开实参用例后共有 32 组非 planned 合同（28 组正常运行用例和 4 组诊断用例）。已有执行证据见[教程索引](docs/tutorials/README.md)。

| 入门内容 | 单一源码示例 |
|---|---|
| Hello、函数、分支、常量 | [hello](docs/tutorials/koven-tour.md#hello)、[function](docs/tutorials/koven-tour.md#function)、[branch](docs/tutorials/koven-tour.md#branch)、[constant](docs/tutorials/koven-tour.md#constant) |
| String.clone与调用自动借用 | [strings](docs/tutorials/koven-tour.md#strings)、[borrowing](docs/tutorials/koven-tour.md#borrowing) |
| 集合、借用返回与范围边界 | [当前能力说明](docs/tutorials/koven-tour.md#collections-and-borrow-results) |
| 使用 Map 的参数词频 | [argv-word-frequency](docs/tutorials/koven-tour.md#argv-word-frequency) |
| 顺序迭代、root replace/swap、资源deinit | [iteration](docs/tutorials/koven-tour.md#iteration)、[root-replace-swap](docs/tutorials/koven-tour.md#root-replace-swap)、[deinit](docs/tutorials/koven-tour.md#deinit) |
| 参数与多文件工程 | [arguments](docs/tutorials/koven-tour.md#arguments)、[cross-file](docs/tutorials/koven-tour.md#cross-file)  [parameter-report](docs/tutorials/koven-tour.md#parameter-report) |
| 数字字面量、资源清理及迭代退出 | [numbers-bitwise](docs/tutorials/koven-tour.md#numbers-bitwise)、[scope-cleanup](docs/tutorials/koven-tour.md#scope-cleanup)、[unit-loop-cleanup](docs/tutorials/koven-tour.md#unit-loop-cleanup) |
| 可变place与迭代借用诊断 | [reject-immutable-place](docs/tutorials/koven-tour.md#reject-immutable-place)、[reject-iteration-move](docs/tutorials/koven-tour.md#reject-iteration-move) |

函数声明以`own param: T`表达所有权交付，默认或`borrow param: T`表达共享借用，`inout param: T`表达可变借用；调用点的可变place交付使用`&place`，Borrow不写调用marker。类家族、nullable与Result等语义见[Guide](docs/guide/README.md)，具体native表示与未支持组合以[Architecture](docs/architecture/README.md)为准。词频示例借用参数数组两遍，使用 Map 的 `get`/`put`/`remove`，并不遍历 Map。Guide v0.46 规定单文件与 project 使用相同的 `koven.algorithms` 标准源加载与显式 import 规则。管线迁移和实现缺口由 [SPEC-0341](docs/specs/active/0341-shared-lowering-core.md) 跟踪；规范启用不表示迁移已完成。Map 借用遍历已有有界 single/unit 后端子集，覆盖已验参数、temporary、local/alias 来源、真实实参/callable 出生、有限控制流与替换 epoch、嵌套 provider，以及循环后 owner 返回、Move 或更新；确切组合和独立调用/ABI 前置见[支持矩阵](docs/architecture/map-borrowed-iteration.md)。这些后端回归不新增公开 CLI tour 合同；Field/Inout 原生来源仍按 Guide 延后。集合 `forEach`/`map`/`filter`、未列出的集合 API、String 分词和通用整数格式化仍不能从 Kotlin 经验外推。

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

- **Rust 工具链**：仓库`rust-toolchain.toml`固定1.96.0，manifest的MSRV也为1.96.0（Rust 2024 edition）。
- **受支持宿主**：AArch64 macOS（`aarch64-apple-darwin`）或 x86_64 Linux + glibc（`x86_64-unknown-linux-gnu`）。编译目标始终是当前宿主，不提供 `--target` 或交叉编译；不支持 Linux musl、Linux AArch64、Intel macOS 或 Windows。
- **LLVM**：LLVM 21.1.x 及匹配的开发头文件、库、`llvm-config` 和宿主 backend。workspace 使用 Inkwell 0.10.0，启用 AArch64 与 X86 target feature。将 `LLVM_SYS_211_PREFIX` 设置为 LLVM 安装前缀，并确保构建及运行环境可加载其动态库。
- **系统 C 工具链**：macOS 需要 Xcode Command Line Tools 与 `/usr/bin/clang`；Linux 需要 `/usr/bin/cc`、glibc 开发文件和可用的系统 linker。LLVM 直接生成 object，再由该 C driver 链接。
- **Native 测试工具**：Linux LLVM IR 插桩测试需要匹配的 Clang 21，优先使用 `LLVM_SYS_211_PREFIX/bin/clang`，否则使用 PATH 中的 `clang`。Linux DWARF 测试使用匹配的 `llvm-dwarfdump`，同样优先 prefix、缺失时回退到 PATH。LLVM IR插桩在两个宿主均需要匹配Clang 21；macOS调试测试保留`/usr/bin/lldb`。这些测试工具与生产链接driver分开。

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
cargo build --locked --release
```

编译生成的目标程序位于 `target/release/kovenc`。
把本次构建目录加入当前shell的PATH后，可使用上面的`kovenc`命令（不会安装到系统目录）：

```bash
export PATH="$(pwd)/target/release:$PATH"
```

### 运行标准库 Smoke 测试

通过标准库的 hello bootstrap 验证编译管线：

```bash
cargo run --locked -p lang-cli -- run crates/lang-std/koven/prelude.ko --entry bootstrapHello
```

输出：
```text
Hello, World!
```

---

## 📚 文档与规范索引

仓库内包含组织在 [`docs/`](docs/) 下的完整文档体系：

- [**当前可执行教程**](docs/tutorials/README.md)：从Markdown单一源码运行真实CLI合同。
- [**Spec与演进账本**](docs/specs/README.md)：有界合同、归档关系与能力缺口。
- [**语言规范 (v0.46)**](docs/guide/README.md)：Koven 语法、语义、类型规则与所有权机制的权威真源。
- [**编译器架构快照**](docs/architecture/README.md)：编译流水线、Typed SSA 与代码生成的当前事实说明。
- [**开发与测试指南**](docs/development/README.md)：分层验证、测试门禁与代码不变式规范。
- [**架构决策记录 (ADR)**](docs/adr/README.md)：重大架构决策的历史背景、长期考量与技术设计记录。

---

## 📄 许可证

Koven 采用双重开源授权，您可以自由选择基于以下任一协议使用：

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) 或 http://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE-MIT) 或 http://opensource.org/licenses/MIT)

# ADR-0012: 标准库源码 bootstrap 边界

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：任务涉及本 ADR 决策边界时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

2026-08-25 依据当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权接受。
本决策只封闭现行 v0.28 留给 SPEC-0042 的仓库内 bootstrap 编排，不定义通用源码入口、包构建、
安装布局或新的语言语义。

## 背景

`lang-std` 已作为 Cargo workspace member 存在，公共实现必须以 `koven/**/*.ko` 为唯一真源；其
Rust target 目前只验证 `prelude.ko` 存在。另一方面，SPEC-0039 已能把显式 verified SSA
`FunctionId` 生成 object、链接并运行，但生产 crate 尚未暴露 frontend→SSA→object 的受控边界，
`kovenc` 也尚未编排完整源码流水线。

SPEC-0042 需要真实构建并运行一份标准库目标语言源码，同时不能提前把某个函数名宣布为 Koven
通用入口，也不能让 `lang-std` 通过 Rust `build.rs`、复制实现或新增 runtime crate 绕过目标语言
bootstrap。该选择会长期影响 `lang-frontend`、`lang-codegen`、`lang-cli` 与 `lang-std` 的依赖和
测试边界，因此需要先形成 ADR。

## 决策

### `lang-std` 保持纯源码包边界

- `crates/lang-std/koven/**/*.ko` 继续是标准库公共实现的唯一真源；Rust library 只承载 Cargo
  package 与源码包不变量，不加入标准库函数的 Rust 镜像、`build.rs` 或编译器 build-dependency。
- SPEC-0042 在 `prelude.ko` 中加入一个最小、无参数、返回 `Unit` 的 bootstrap smoke callable。
  它只证明真实目标语言编译链；prelude、`error()` 和其他公共 API 仍由 SPEC-0043 起逐项实施。
- bootstrap 产物位于一次运行拥有的临时目录，不提交 object/executable，也不把 Cargo target
  目录当作标准库安装布局。

### CLI 编排、frontend 与 codegen 保持单向职责

- `lang-cli` 的仓库内 bootstrap driver 显式持有 source path、entry selector、临时 object 与
  executable 路径，并依次调用 Lexer、Parser、名称解析、类型检查、所有权检查、codegen object
  emission、ADR-0010 linker driver 和进程运行。
- `lang-frontend` 提供唯一的编译器内建名称/类型环境构造入口，避免 CLI 与测试各自复制完整
  builtin 列表。环境仍是显式值，不使用可变全局状态。
- `lang-codegen` 暴露最小 workspace API：输入同一条已验证 frontend analysis chain、显式解析
  后的非泛型顶层 callable `SymbolId`、`SourceMap` 与输出路径，输出 object 或结构化 codegen
  错误。它不读取文件、不创建临时目录、不链接、不运行进程。

### Bootstrap entry 不是通用源码入口语义

- 仓库拥有的 bootstrap target 通过显式配置给出 source file 和 entry name；CLI 在该文件的
  `NameResolution` 中要求恰好一个对应的顶层 function symbol，再把解析后的 `SymbolId` 交给
  codegen。backend 仍只消费身份，不比较 `"main"`、`"error"` 或其他源码字符串。
- entry 必须是非泛型 `() -> Unit`，并且属于本次 analysis chain；缺失、重载、错误签名、foreign
  identity、任一 frontend diagnostic、ownership deferred 或不受当前 SSA lowering 支持的节点都
  在 object/link 前失败。
- 该显式仓库配置不建立用户可见 `kovenc` 参数、默认入口、package 规则或稳定构建清单。通用
  `.ko`→CLI 行为仍须由后续 guide/Spec 定义。

### 验收必须真实执行目标语言产物

- `lang-cli` 测试读取磁盘上的 `lang-std/koven/prelude.ko`，不得内嵌等价 Rust/Koven 字符串作为
  替代；完整流水线生成 Mach-O object，经既有 linker driver 链接并运行，进程必须以 0 退出。
- 测试同时锁定源码为空/损坏、entry 缺失或签名错误时不会误用其他函数，也不会把旧 object 或
  executable 当作成功。零个 `.ko` bootstrap source 必须失败。
- 测试只验证仓库 bootstrap contract，不把临时路径、object 字节或 linker stderr 固化为语言
  协议。

## 替代方案

### 在 `lang-std/build.rs` 中调用编译器

不采用。它会让 Cargo 的普通 check/build 隐式执行 LLVM、linker 和目标程序，并迫使
`lang-std` 增加编译器 build-dependency；这模糊了源码包与宿主构建边界，也不利于交叉编译。

### 把最小标准库函数先写成 Rust

不采用。它能快速产生可执行文件，却建立第二份公共实现真源，违背既定 `lang-std/koven` 边界，
也无法证明目标语言 frontend、所有权和 codegen 已真正接通。

### 让 codegen 按 `main` 或 bootstrap 函数名选择入口

不采用。它违反 ADR-0010 的显式身份契约，并会在重载、package 与通用入口规则尚未封闭时把
字符串猜测固化为语言行为。

### 本阶段直接发布完整 `kovenc build`

不采用。manifest、多文件 package/import、输出布局和用户入口仍有独立 guide 门禁。SPEC-0042
只建立仓库拥有的单文件 bootstrap 闭环，不把内部验收 driver 冒充完整用户 CLI。

## 后果

收益：

- 标准库首次由真实 Koven 源码经过完整已实现流水线构建和运行，不依赖 Rust 镜像；
- frontend、codegen、CLI 与源码包职责保持单向，后续标准库 Spec 可复用同一 bootstrap driver；
- resolved `SymbolId` 延续 ADR-0010 的显式 entry，不提前定义通用 `main`；
- 内建环境获得单一生产构造入口，CLI 不必复制测试中的 builtin 表。

代价与风险：

- 首版仍是单文件、单目标、单显式 entry 的仓库内部路径，不是可安装的标准库构建系统；
- codegen 需要形成首个最小公共 workspace API，并稳定区分 frontend identity、entry 和 LLVM
  失败；
- bootstrap 验收依赖 ADR-0007 的本机 LLVM 与 ADR-0010 的 `/usr/bin/clang`，其他 target 仍需
  后续 ADR；
- prelude 暂含一个只用于闭环的 smoke callable，SPEC-0043 建立真实 prelude 时需要明确保留、
  替换或删除它。

## 关联

- 相关 Spec：SPEC-0042、SPEC-0043、SPEC-0051
- 相关 ADR：[ADR-0002](./0002-bootstrap-workspace-layout.md)、
  [ADR-0007](./0007-llvm-toolchain-and-first-target.md)、
  [ADR-0008](./0008-internal-value-and-allocation-abi.md)、
  [ADR-0010](./0010-first-native-object-and-linker-contract.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无

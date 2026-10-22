# SPEC-0349: 内建 `Result`/`Ok`/`Err`、`is Case` subject 推断与 `kovenc check`

> **性质**：实施合同 · **状态**：done · **读取时机**：实现或验收内建环境 `Result`、无实参 `is Case`、`kovenc check`/`--help` 时 · **唯一真源**：本片实施范围与验收；语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P0-0349` |
| 所属 Phase | Phase 2（名称与类型）、Phase 6（CLI） |
| 语言规范 | [Guide v0.46 标准环境](../../guide/02-names-files-packages.md#标准环境与允许加载的标准库源)、[Guide 09 `Result`](../../guide/09-nullability-errors.md#resultt-e-与-postfix-)、[Guide 13 只检查与帮助](../../guide/13-program-runtime-standard-library.md#只检查与帮助) |
| 批准依据 | 2026-10-22 用户决定：(1) 用户任一 package 的顶层 `Result`（类型空间）或 `Ok`/`Err`（任一空间）与内建冲突，报既有 L0150，局部与类型参数仍静默遮蔽；(2) 无实参 `is Ok`/`is Err` 从 subject 推断类型实参（并入路线图 L9c）；(3) `kovenc check` 有诊断退出 1，单文件 build/run 诊断退出码由 2 改为 1，usage/operational 保持 2；(4) LSP 不在范围 |
| 基线 | `f5f5b22f`（SPEC-0348 之后的 v0.46 分支栈） |
| 分支 | `feature/spec-0349-builtin-result` |
| 前置 Spec | [SPEC-0348](0348-conformance-corpus.md)（仅分支顺序） |
| 语言语义变更 | 否；落实已启用的 Guide v0.46 内建环境，补 Guide 02 标准源表与 Guide 13 CLI 形式 |

## 1. Goal

- 路线图 E4：`Result<T, E>`、`Ok`、`Err` 在单文件与 project 两种模式下无需 import 即可使用；
  `fun f(): Result<Int, String> = Ok(1)` 直接通过 frontend。
- 路线图 L9c（部分）：`is Ok` / `is Err` 等无实参 case type 从被测 subject 推断 root 实参。
- `kovenc check` 只运行 frontend gate；`kovenc --help` 输出完整用法。

## 2. 设计

- 标准源：新增 `crates/lang-std/koven/core/result.ko`（`package koven.core`，
  `enum class Result<T, E> { Ok(success: T), Err(error: E) }`）。`prelude.ko` 删除自己的
  `Result`，bootstrap 函数改用内建名称。
- 授权：`standard_sources::append_standard_sources` 在 `ranges.ko` 之后追加 `result.ko`
  （ranges 的 descriptor 位置不变），并调用新增的 `NameEnvironment::authorize_builtin_source`
  按 SourceId 授权。用户文件声明 `package koven.core` 或使用同名路径都不取得该身份。
- 名称解析（`name_resolution/compilation_unit/names/builtin.rs`）：`UnitResolver` 构造时从授权
  source unit 取 `Result` 声明及其 `Ok`/`Err` case 的值与类型 symbol；授权存在但缺失导出是
  内部错误 `CompilationUnitNameError::MissingBuiltinExport`。内建绑定是最低优先级：只在
  `lookup_file_binding` 未决定时用于普通引用（含 Value→Type 回退）与相对静态路径首段
  （`Result.Ok(1)`）。词法局部与类型参数先于文件绑定，因而静默遮蔽。
- 冲突：自动绑定建立后，对授权源以外任一 package 的顶层 `Result`（类型空间）与 `Ok`/`Err`
  （任一空间）报 L0150，消息为 ``top-level declaration `X` conflicts with the built-in environment name``，
  label 指向内建 `Result`。
- 发布：`CompilationUnitNames::builtin_result_declaration()` 与
  `ValidatedCompilationUnitNames::builtin_result_declaration()`，供后续 `?`（N08）按身份识别。
- 类型检查：`resolve_type_test_ref` 接收 subject 类型（`when` subject 或 `is` 左操作数）。无实参
  case type 若 subject 去掉 nullable（及已有 case refinement）后是同一 enum 的实例，就用其实参；
  只有引用在该 enum 声明体内时才回退到声明自身的类型参数；否则报既有实参个数诊断。
  显式 `is Ok<Int, String>` 不变。
- CLI：`unit_build::check_unit_frontend` 拆出 names、types、final type admission、ownership 与
  validation 全部 gate，`emit_unit_object` 复用后再选入口与生成 object；`bootstrap::prepare_single_source`
  与 `project_build::prepare_project_unit` 拆出 source 准备。新增 `check_command.rs`：
  `kovenc check <source.ko> [--source-root <dir>]` 与 `kovenc check --project <project.toml>`，
  诊断渲染与 build 相同；退出 0/1/2。单文件 build/run 的 frontend 诊断退出码改为 1。
  `kovenc help`/`--help`/`-h` 把完整用法写到 stdout 并退出 0；全局 usage 与未知命令消息列出
  `check` 与 `help`。

## 3. 非目标

- postfix `?`（N08）、`Result` must-use（L17）、`main` 返回 `Result`（L21）。
- `Pair`/`to`（S6）不进入内建环境。
- L9c 的 L0086→L0084 诊断码统一。
- LSP 不注入标准源，也不提供内建 `Result`。
- String 载荷 enum 的 native lowering（conformance q08、p16b）。

## 4. 验收标准

- [x] `Result`、`Ok`、`Err` 在单文件与 project 中无需 import 解析为授权源的声明与 case。
- [x] `Result.Ok(1)` 通过相对静态路径首段解析。
- [x] 局部 `val Ok` 与类型参数 `Result` 静默遮蔽内建名称。
- [x] 用户任一 package 的顶层 `Result`/`Ok`/`Err` 报 L0150；值空间 `fun Result()` 与 enum case 不冲突。
- [x] 未授权时 `Ok` 仍为 L0080；用户 `package koven.core` 不取得内建身份。
- [x] `when` 与 `if` 中无实参 `is Ok`/`is Err` 从 subject 推断实参；显式实参仍可用。
- [x] `kovenc check` 单文件/project：干净 0、诊断 1（渲染同 build）、usage/operational 2；`--help` 输出到 stdout。
- [x] 单文件 build/run 的 frontend 诊断退出 1；受影响测试与教程合同同步。
- [x] conformance `p16_result_q`、`p16b_result_string`、`p37_is_generic_case` 删除自声明 `Result`。

## 5. 实际验收（2026-10-23）

工具链：云端 Linux x86_64，Rust stable，LLVM 21.1.8，`CARGO_PROFILE_DEV_DEBUG=0`。按用户要求只运行受影响目标，未跑全量 workspace。

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | 通过 |
| `lang-frontend`：`builtin_result`、`multifile_name_resolution`、`multifile_type_checking`、`name_resolution`、`final_type_admission` | 7、13、153、15、40 项通过 |
| `lang-cli`：`check_cli`、`single_unit_cli`、`single_file_analysis_cli`、`native_cli`（`builtin_result`、`final_type_admission`）、`project_cli` `final_type_admission` | 通过 |
| `lang-cli --bins` | 61 项中仅 `unreadable_directory_and_source_report_io_operations` 失败；云端以 root 运行，属环境原因 |
| `lang-codegen --lib -- enum` | 75 项通过 |
| `lang-std`、`lang-lsp`（构建与测试） | 1、47 项通过 |
| `scripts/check_docs.py`、`gen_spec_dag.py`、`gen_roadmap.py`；`test_tutorial_contracts`、`test_preview_host` | 通过 |

受影响测试的处理：`final_type_admission` 的 p16 探针原本自声明 `Result`，现会命中 L0150；新增
`p16_result_q_builtin.ko`（去掉自声明）作为该探针的输入，原 fixture 仍供不加载标准源的 frontend `final_type_admission` 测试使用。

未运行：全量 workspace、clippy、远端 CI、macOS、sanitizer。

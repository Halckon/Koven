# SPEC-0039：本机目标文件、显式入口与首个链接链路

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P4-039` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.28 Phase 4](../guides/v0.34-pre-restructure/06-roadmap.md#phase-4llvm-代码生成) 与 [`error()` abort 契约](../guides/v0.34-pre-restructure/01-design-decisions.md#3-error-与空安全相关运算符) |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |
| 前置 Spec | SPEC-0035、SPEC-0038 `done`；SPEC-0033/0034 verified SSA/LLVM 前置链已完成 |
| 前置 ADR | [ADR-0007](../../adr/accepted/0007-llvm-toolchain-and-first-target.md)、[ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md)、[ADR-0010](../../adr/accepted/0010-first-native-object-and-linker-contract.md) `accepted` |
| 阻塞项 | 无；源码 entry 选择和标准库 `error()` identity 尚未发布，按 ADR-0010 明确排除，不阻塞显式 verified SSA entry 的后端验收 |
| 影响范围 | `lang-codegen` LLVM module/object emission、entry wrapper、测试；`lang-cli` linker driver 与测试；Architecture |
| 语言语义变更 | 否；实现已有 abort 语义和 ADR-0010 私有/平台 ABI，不新增源码入口选择规则 |

## 2. Goal

完成后，编译器能够把单 module verified SSA 和显式 `() -> Unit` entry 通过同一 LLVM 21
TargetMachine 生成 AArch64 Mach-O object，以唯一 C ABI `main` wrapper 链接为可执行文件并真实
运行；正常返回为退出码 0，SSA abort 非零终止，所有失败保持结构化且不按源码名称猜测语义。

## 3. 范围与需求

### 3.1 LLVM module 与 entry wrapper

- 把现有 verified SSA→LLVM 构造从“只返回文本”收敛为一个内部 module lowering 入口；文本和
  object emission 必须复用同一产物、target triple、DataLayout 与 verifier，不建立第二套 lowering。
- object 模式必须接收显式 `FunctionId` entry，并在构造 LLVM 前验证它属于唯一 module、存在、
  entry block 无参数且返回类型为空（Koven `Unit`）。错误 module/owner、未知 entry、参数化或
  非 Unit entry 必须在 object 写盘前返回结构化 `LlvmAdapterError`。
- 生成唯一 external C ABI `i32 @main()`，顺序调用指定 Koven function 后返回 0。普通 Koven
  function 使用内部 linkage；已有外部 runtime symbols 继续保持声明，不导出第二个 entry。

### 3.2 Object emission

- 使用 ADR-0007 的 `TargetMachine::write_to_file(..., FileType::Object, path)` 直接写 Mach-O object；
  禁止把 LLVM IR 交给 clang 再编译。
- API 只接受显式输出路径；父目录、临时目录和覆盖策略由调用方负责。写盘、target 或 verifier
  失败分别保留结构化错误，不 panic、不吞掉 LLVM 消息。
- 重复输入的 LLVM IR 与符号集合必须确定；object 测试锁定 Mach-O arm64 格式与唯一 `_main`
  外部符号，不把平台时间戳或二进制 hash 承诺为语言语义。

### 3.3 CLI linker driver 与运行验收

- `lang-cli` 建立只负责单 object→executable 的 linker driver，按 ADR-0010 直接执行
  `/usr/bin/clang <object> -o <executable>`；参数通过 `std::process::Command` 传递，不经 shell。
- 启动失败和非零退出分开建模；非零退出保留 status 与 stderr 附件，但不解析本地化 stderr、
  不生成 frontend 类型诊断。driver 不创建或删除调用方文件。
- codegen 验收用 test-only orchestration 把真实 Koven object 交给相同参数契约链接并运行；CLI
  driver 单测锁定参数、成功/失败边界。完整 `.ko`→CLI 流水线等待源码 entry facts，不在本 Spec
  伪造。
- 正常显式 entry 运行退出码为 0；包含 SSA `Abort` 的 entry 被 signal/非零状态终止，不生成
  unwind cleanup。后续标准库稳定 identity 可直接 lower 到同一 `Abort` primitive。

## 4. 非目标

- 不定义或实现按源码名称、重载、package、可见性选择 `main`；不接受参数化、异步或多入口。
- 不按字符串 `error` 特判 callable，不实现 String/message 输出、标准库、prelude identity 或
  `println` runtime；它们由 Phase 5 和对应 frontend→SSA 接线承接。
- 不支持 ELF/COFF、x86_64、交叉链接、raw `ld`、linker 搜索配置、静态 runtime、增量缓存、
  安装布局、manifest/package CLI 或可复现二进制 hash。
- 不实现 DWARF、public FFI、优化 pipeline、LTO、strip、codesign、universal binary 或发布打包。
- 不把 test-only linker orchestration变成 `lang-codegen` 生产职责，也不新增 crate 或依赖。

## 5. 验收标准

- [x] module lowering 文本/object 共用同一 verified LLVM module；损坏 SSA 和错误 entry 在写盘前
      被拒绝，失败路径不产生 object。
- [x] 显式零参数 Unit entry 生成唯一 `i32 @main()` wrapper，普通 Koven function 为 internal；
      参数化、非 Unit、foreign/unknown entry 正反矩阵通过。
- [x] TargetMachine 生成的文件被识别为 arm64 Mach-O object，并包含唯一外部 `_main`；不调用
      clang 生成 object。
- [x] CLI linker driver 不经 shell，成功、driver missing、linker non-zero 三类结果可区分，且
      不删除输入 object 或失败输出。
- [x] 真实 Koven object 经 `/usr/bin/clang` 链接并执行：正常 entry 返回 0；SSA Abort 非零终止；
      LLVM/object/link/run 全链路没有 unwind landing pad 或按名称特判 `error`。
- [x] `lang-codegen`、`lang-cli` 窄测及 workspace 五项标准基线通过；production 文件遵守 1000
      行软上限，Spec/Architecture/roadmap 只记录实际完成事实。

## 6. 技术方案与边界

- `llvm::adapter` 继续拥有 SSA→LLVM module 构造；将上下文、LLVM module 和 TargetMachine 的
  生命周期封装在一次闭包式 helper 中，避免返回借用 LLVM context 的 module或复制 lowering。
- entry wrapper 与 object emission 各放独立 LLVM 子模块；adapter 只暴露已声明 function map
  所需的最小入口，不继续扩大接近软上限的主文件。
- `lang-cli` linker module 只接受路径和值类型配置，不依赖 frontend/SSA 内部类型；未来 CLI
  pipeline 可直接复用，不为本 Spec 建立 facade 或通用 build framework。
- 测试临时目录使用 `std::env::temp_dir()` 下的进程/计数器专用路径并显式清理自己的文件；不
  删除仓库或调用方路径。测试命令只接受测试生成的固定路径。

## 7. 实施计划

1. [x] 收敛共享 LLVM module lowering并生成显式 entry wrapper → 验证：IR wrapper、entry
   signature/identity 正反矩阵与 LLVM verifier。
2. [x] 使用同一 TargetMachine 生成 object → 验证：object 存在、Mach-O arm64、符号与失败不落盘。
3. [x] 实现 CLI linker driver 与 test-only object/link/run orchestration → 验证：正常/abort、
   driver missing/link failure 和文件所有权矩阵。
4. [x] 运行 workspace 基线、同步 Architecture/roadmap/Spec 并审查 staged diff → 验证：实际退出
   状态、文件规模、文档与实现一致。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 共享 module lowering、显式 entry 校验与 C ABI wrapper | `feat(codegen): generate explicit native entry wrapper (SPEC-0039)` |
| 2 | TargetMachine object emission 与 object 正反验收 | `feat(codegen): emit native object files (SPEC-0039)` |
| 3 | CLI linker driver、真实 link/run、Architecture 与 done 验收 | `feat(cli): link native executables (SPEC-0039)` |

## 9. 未决问题

- 无。源码 entry 和标准库 identity 是已登记的权限边界而非本 Spec 未决设计；若实现必须通过
  名称选择才能继续，应停止并先补 guide，而不是扩大本 Spec。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | SPEC-0035/0038 `done`；ADR-0007/0008/0010 `accepted`；本机 `arm64`，`/usr/bin/clang` 为 Apple Clang 21 |
| `cargo test -p lang-codegen --all-targets`（entry slice） | 通过 | 84 项；显式 Unit entry wrapper 与参数化/非 Unit entry 正反矩阵通过，既有 LLVM 文本 ABI 断言同步锁定 internal Koven functions |
| `cargo clippy -p lang-codegen --all-targets -- -D warnings`（entry slice） | 通过 | 无 warning；`llvm/adapter.rs` 1000 行，entry 验证/wrapper 位于独立模块 |
| `cargo test -p lang-codegen llvm::object_tests` | 通过 | 2 项；锁定 arm64 Mach-O header、唯一 external `_main`、Koven internal symbol 隐藏、invalid entry 与缺失父目录失败不落盘 |
| `cargo clippy -p lang-codegen --all-targets -- -D warnings`（object slice） | 通过 | 无 warning；共享 module lowering 提取后 `llvm/adapter.rs` 969 行 |
| `cargo test -p lang-codegen --all-targets`（link/run slice） | 通过 | 87 项；真实 Koven normal/Abort object 均经 clang 链接并运行，SSA Abort lower 为 C `abort` + `unreachable` |
| `cargo test -p lang-cli --all-targets` | 通过 | 8 项；链接成功、driver missing、linker non-zero 与带空格路径均通过 |
| `cargo clippy -p lang-codegen -p lang-cli --all-targets -- -D warnings` | 通过 | 无 warning；production 文件均未超过 1000 行软上限 |
| `cargo fmt --all -- --check` | 通过 | workspace 格式基线 |
| `cargo check --workspace --all-targets` | 通过 | workspace 全 target 构建检查 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | workspace 零 warning |
| `cargo test --workspace --all-targets` | 通过 | workspace 全量测试，无失败/忽略；包含真实 object/link/run |
| `cargo build -p lang-cli` | 通过 | `kovenc` binary 构建成功 |
| production 文件规模 | 通过 | `llvm/adapter.rs` 965 行、`llvm/runtime.rs` 681 行、`llvm/object_tests.rs` 207 行、`lang-cli/linker.rs` 136 行 |

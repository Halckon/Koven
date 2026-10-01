# SPEC-0228: Linux x86_64 本机目标与基线验收

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施或验收 Linux 本机目标时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-228` |
| 所属 Phase | Phase 4；必要的 CLI Phase 6 编排适配 |
| 语言规范 | [现行 v0.38](../../guide/README.md)、[Phase 边界](../../guide/15-conformance-and-staging.md#phase-边界) |
| 批准依据 | 2026-10-01 用户确认修复基线问题，并授权“是的，如果新增一个linux taget不麻烦，可以新增一个” |
| 前置 Spec | SPEC-0034/0039/0040 `done` |
| 前置 ADR | ADR-0007/0010/0011/0026 `accepted` |
| 关联 ADR | [ADR-0026](../../adr/accepted/0026-linux-x86-64-native-host.md) |
| 阻塞项 | 严格 Clippy 与 5 项既有 frontend 测试仍失败；macOS 未执行，分支交付未完成 |
| 影响范围 | workspace Inkwell feature、`lang-codegen` LLVM/native 与测试、`lang-cli` linker 与测试、必要基线修复、文档 |
| 语言语义变更 | 否 |

## 1. Goal

在保留 AArch64 macOS 本机路径的前提下，使 x86_64 Linux + glibc 宿主能通过同一 Koven
编译流水线生成、链接并运行 ELF64 x86_64 程序，并得到真实、可追溯的目标与基线验收结果。

## 2. 背景

当前首目标约束来自 ADR-0007/0010；仅在 Linux 安装 LLVM 不会自动建立 Linux object 与
系统链接支持。新增宿主的长期决定由 ADR-0026 封闭，本 Spec 负责实现、失败回归、工具链
验证和证据记录。工作分支为 `feature/spec-0228-linux-native`，从 `main` 建立。

## 3. 范围与需求

- 支持 `aarch64-apple-darwin` 和 `x86_64-unknown-linux-gnu` 两个 host-native 组合。
  host 选择统一决定 LLVM triple、TargetMachine、DataLayout 和 object machine；其他宿主
  返回结构化 `Target` 错误，禁止静默回退。
- 保持 LLVM 21.1.x / Inkwell 0.10.0 版本，在既有 feature 中追加 `target-x86`，不启用
  `target-all`。新增宿主重新执行工具链、模块 verifier 与 object/run 验收。
- 生产链接保持 LLVM 直接生成 object，再由 CLI 使用系统 C driver：macOS
  `/usr/bin/clang`、Linux `/usr/bin/cc`。保留启动失败/非零退出等结构化错误与原子发布。
- 测试按宿主断言真实 Mach-O AArch64 或 ELF64 x86_64 产物。Linux 需处理 LLVM IR 的计数/插桩
  测试使用 `LLVM_SYS_211_PREFIX/bin/clang`，工具缺失时的 PATH `clang` 也须兼容 LLVM 21 IR，
  不误用生产 C driver 作为 IR reader。
- 保留现有 macOS LLDB 源码断点测试；Linux 检查真实 ELF 的 DWARF 行表及 native 运行。
  使用 `LLVM_SYS_211_PREFIX/bin/llvm-dwarfdump` 21.1；明确 Linux 行表证据和调试器断点证据的区别。
- 仅修复本次已确认的三类基线问题：`parser_declaration.rs` markers match 漏 `Item::Deinit`
  导致的 E0004、`ownership_checking/checker.rs` 对 Option 使用 `mem::replace` 的 Clippy
  失败（改用 `Option::replace`），以及 16 个文件的既有格式偏差。具体失败、最小修复和
  重跑结果登记在 §10；不增加 blanket lint allow，不掩盖零命中、跳过或未运行项。
- README 两语言版本、Architecture、测试前提、Spec/ADR 索引及冻结 inventory 同步。

## 4. 非目标

- 不增加 `--target`、交叉编译、sysroot 或通用 linker discovery；不支持 Linux musl、Linux
  AArch64、macOS x86_64、Windows 或其他新增宿主。
- 不改变 Koven v0.38 语言语义、公开 ABI、runtime ownership 规则或依赖版本。
- 不增加完整变量/类型调试信息，不把 Linux 行表自动测试扩称为 debugger 断点/单步验收。
- 不以新增 Linux 支持为由重跑 frontend 全量套件或修改无关代码。
- 本轮本地实现不自动授权 commit、push、PR 或合并；按仓库交付流程另行完成后才可归档。

## 5. 验收标准

- [x] 新增测试先证明旧硬编码目标/平台测试在 Linux 的失败或错误预期，修复后通过相同选择。
- [x] 宿主选择矩阵精确接受两个受支持组合，拒绝其他组合；triple、DataLayout 与 object
  machine 一致，unsupported host 得到结构化 `Target` 错误。
- [x] 实际 LLVM 21.1.x / Inkwell / llvm-sys 编译与动态链接成功，合法模块通过 verifier、
  非法模块被拒绝，选定宿主生成确定性 LLVM/目标产物。
- [x] Linux 产物确认为 ELF64 x86_64；系统 `/usr/bin/cc` 链接、正常返回、stdout、argv 与
  Abort 正反例覆盖通过，已有输出在 backend/link/发布失败时保留。
- [x] Linux LLVM IR 插桩测试使用匹配 Clang 21，相关 runtime 分配/释放或输出断言实际通过。
- [x] Linux DWARF 文件/行表通过匹配 LLVM 21.1 的 `llvm-dwarfdump` 验收；macOS LLDB 测试保留，实际执行情况单列。
- [x] 授权内基线 lint/测试问题的失败与重跑结果已登记，未静默扩大语言或 ABI 边界。
- [ ] 按影响面执行 fmt、严格 clippy、定向 codegen/native/CLI 与 workspace 编译检查；
  实际命中数、filtered、ignored、未运行与平台缺口均已记录。
- [x] Architecture 与 README 和实际实现相符；文档检查、检查器测试与 diff 检查通过。
- [ ] 按仓库分支交付流程完成 PR/CI 与验收账本，再迁为 `done` 并归档；当前保持 active。

## 6. 技术方案与边界

LLVM host 选择及 target 初始化只存在于 `lang-codegen` adapter，frontend 与 typed SSA
保持 target-independent。生产链接选择在 CLI 外围，不把 linker path 或 LLVM 类型传播到
语言事实。测试 helper 可以选择宿主工具，但不能形成第二条生产 LLVM IR→object 路径。

依赖版本与现有内部 ABI 保持；新增 feature、支持矩阵、失败模型和 Linux 工具前提以
[ADR-0026](../../adr/accepted/0026-linux-x86-64-native-host.md) 为长期真源。

## 7. 实施计划

1. [x] 补充宿主/目标和现有平台假设的失败回归 → 验证：§10 的失败证据。
2. [ ] 接入最小 host-native 选择、Linux driver 与测试工具适配，修复已确认基线 → 验证：
   §10 的定向行为与严格门禁。
3. [x] 同步 Architecture、文档索引与实际验收 → 验证：文档门禁与实现核对。
4. [ ] 获得相应交付授权后按分支流程完成 PR/CI、归档与合并 → 验证：实际交付记录。

## 8. 提交计划

2026-10-01 用户已补充授权本地提交，提交信息必须带 SPEC-0228；不自动 push 或创建 PR。
Rust 变更保持可构建、可测试，已知严格门禁缺口仍按 §10 报告；最终归档与 frozen inventory
迁移随交付验收同步完成。

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 宿主目标、Linux 链接/测试适配、必要基线修复与验收文档 | `feat(codegen): support Linux x86_64 native host (SPEC-0228)` |

## 9. 未决问题

无待决定的语言语义或目标范围问题。§10 的既有 frontend 测试失败和严格 Clippy 问题尚未
扩大修复；macOS 回归与最终分支交付仍待完成，不能将本 Spec 标为 `done`。

## 10. 验证记录

本表仅记录实际执行结果；预定但未执行的检查保持“未执行”。所有 Cargo 检查由同一执行者
串行运行，文档工作不并发启动 Cargo。平台或工具缺失不得用零命中/跳过冒充通过。

工具链为工作区本地 Rust 1.96.0、LLVM 21.1.8 及匹配的 Clang 与 llvm-dwarfdump；
`LLVM_SYS_211_PREFIX` 指向该安装。以下 Rust 结果来自同一轮串行执行及其日志核对。

| 验收项 / 命令（目标与过滤器） | 结果（实际测试数） | 未运行原因 / 复用证据 |
|---|---|---|
| Red：`cargo test -p lang-codegen --lib linux_host_renders_x86_64_gnu_target` | 0 passed、1 failed、476 filtered、0 ignored | 旧实现仍生成 Darwin triple，按预期失败 |
| Green：`cargo test -p lang-codegen --lib llvm::tests::` | 7 passed、471 filtered、0 failed/ignored | 宿主矩阵、两个 backend 的 64 位布局、Linux triple、确定性模块与 LLVM verifier 正反例 |
| `cargo test -p lang-codegen -p lang-cli --all-targets --no-fail-fast` | codegen 478 passed；CLI 48 单元 + 3 format + 8 native + 5 project passed；全部 0 failed/ignored/filtered | 覆盖 object/runtime、匹配 Clang 插桩、Linux DWARF、单文件/多文件 build/run/argv 与失败保留输出；不等于 frontend 全量 |
| `cargo test -p lang-codegen -p lang-cli --no-fail-fast` | 542 单元/集成 + 4 codegen doc-tests passed，0 failed/ignored/filtered | 同轮补充运行，包含 native compile-fail 文档契约；合计 546 项 |
| `cargo test -p lang-frontend --test lexer --test parser_declaration --test parser_class_family --test parser_expression --test type_checking --test multifile_type_checking --test ownership_checking --no-fail-fast` | 334 passed、5 failed、0 ignored/filtered | lexer 20、parser_declaration 23、parser_class_family 23、parser_expression 57、type_checking 82、ownership_checking 30 全通过；multifile_type_checking 99 passed、5 failed |
| 干净 HEAD `41adfdf`：`cargo test --manifest-path <baseline>/Cargo.toml --target-dir <shared-target> -p lang-frontend --test multifile_type_checking` | 99 passed、同样 5 failed、0 ignored/filtered | 从 `git archive` 的独立基线目录强制重新编译，日志确认 Compiling 来源为该目录，证明五项失败既已存在 |
| `cargo clippy --workspace --all-targets` | 退出 0，仍有 139 条 warning | frontend lib-test 134、codegen 5，重复输出不另计；不是严格 lint 通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 失败 | 退出 101；139 条诊断全部位于本切片未修改文件，原 Option `mem::replace` lint 已修复；未扩大修复或加入 blanket allow |
| `cargo fmt --all -- --check` | 通过，退出 0 | 16 个文件的基线格式已修复，最终状态已核验 |
| `cargo check --workspace --all-targets` | 通过，退出 0 | 涵盖新增 workspace target feature 与直接消费者 |
| `cargo build -p lang-cli` | 通过，退出 0 | 实际构建 Linux CLI |
| `kovenc build hello-linux.ko -o <new-path>`、`file <new-path>`、直接执行及 `kovenc run hello-linux.ko` | 通过 | 产物为 ELF64 LSB PIE x86-64 GNU/Linux，含 debug_info；两种执行均输出 `Linux native OK` |
| macOS AArch64 回归 / LLDB | 未执行 | 当前是 Linux；macOS 测试由 cfg 不纳入本轮二进制，不是 ignored，也不能计作通过 |
| `python3 scripts/gen_spec_dag.py` | 通过 | 2 份 live + 213 份 archive，四份依赖图同步生成 |
| `python3 scripts/check_docs.py` | 通过，385 Markdown | 包括 Spec/ADR inventory、链接、入口预算、迁移账本与依赖图一致性 |
| `python3 -m unittest discover -s scripts/tests -v` | 21 passed | frozen inventory 变更的检查器回归 |
| `git diff --check` | 通过 | 本轮文档及当前工作树无 whitespace error |
| PR / CI / 归档 | 未执行 | 当前保留本地未提交变更，状态 active / in-progress |

### 失败与修复边界

- 首次 native 全目标检查为 codegen 477 passed、1 failed；
  `native::unit_tests::unit_object_atomically_replaces_links_and_runs_across_packages` 仍硬编码
  Mach-O magic，实际已生成 ELF。将断言接入宿主 object 检查后，相同全目标命令重跑得到上表
  478 passed；不把这次夹具遗漏记为 runtime 或源语言失败。
- 已授权的 markers match 缺少 `Item::Deinit` 导致 E0004；补齐该分支后
  `parser_declaration` 的 23 项通过。`Option::replace` 修复原 Clippy 问题；16 个文件只做
  既有格式修正，不修改语言规则。
- 五项 `multifile_type_checking` 失败在干净基线与本轮完全一致，仍待独立处理：
  `companion_constant_initializers_publish_stable_ordinary_typed_facts`、
  `top_level_initializers_publish_stable_cross_file_symbol_and_expression_types`（typed validate）；
  `deferred_explicit_constructor_type_arguments_publish_no_construction_fact`（MissingDeclarationSymbol）；
  `cross_file_when_diagnostics_cover_shape_order_coverage_and_branch_join`（L0112 预期差异）；
  `unit_lambda_diagnostics_stop_jumps_and_returns_at_callable_boundary`（L0084 数量差异）。
- Linux DWARF 证据来自
  `llvm::debug_tests::elf_line_table_records_koven_source_locations_and_runs`：检查真实 ELF object、
  `.ko` 文件及行 4 / 列 5，再经 `/usr/bin/cc` 链接并验证退出 0。保留 macOS LLDB 测试，
  本轮没有 Linux 调试器源码断点或单步证据。

本轮未运行 frontend 全量、macOS 本机回归或 CI。严格门禁和上述既有失败尚未关闭，因此
只报告 Linux native 与所列定向检查结果，不宣称全仓全绿或 Spec 完成。最终命令状态为
fmt 0、strict clippy 101、普通 clippy 0、workspace check 0、CLI build 0、定向 frontend 101。

### 2026-10-01 同步 main 后复验

按用户本次“拉取一下最新的提交，合并一下”授权，将当前 feature 分支从 `41adfdf` 快进到
`origin/main` 的 `e52acaf`，恢复全部未提交 Linux 改动；未创建新提交、push 或 PR。
上游包含 Actions 版本升级（checkout v7、paths-filter v4、setup-python v7）、
`toml` 1.1.4 → 1.1.6 及 frontend 格式修复。该依赖版本变化来自上游同步，不属于 Linux
目标的依赖扩展。唯一文本冲突位于 `Item::Deinit` 的 receiver mode 保存语句：保留本地
`Option::replace` 修复；上游格式修复已吸收，不再作为重复的本地 diff。

以下均在同步后的最终 Rust 源码和上游 lockfile 上重新执行；Cargo 命令串行运行并使用
`--locked`（fmt 除外），工具链仍为 Rust 1.96.0 / LLVM 与 Clang 21.1.8。

| 验收项 / 命令 | 实际结果 |
|---|---|
| `cargo fmt --all -- --check` | 通过，退出 0 |
| `cargo check --locked --workspace --all-targets` | 通过，退出 0 |
| `cargo test --locked -p lang-codegen -p lang-cli --no-fail-fast` | 546 passed、0 failed/ignored/filtered：codegen 478、CLI 48 单元 + 3 format + 8 native + 5 project、codegen doc-tests 4 |
| `cargo test --locked -p lang-frontend --test parser_declaration --test ownership_checking --no-fail-fast` | 53 passed、0 failed/ignored/filtered，分别 23 和 30 项；覆盖冲突处所有权逻辑与仍保留的 Deinit markers 修复 |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | 失败，退出 101；仍为 frontend lib-test 134 与 codegen 5 条既有诊断，重复目标不重复计数 |
| `cargo clippy --locked --workspace --all-targets` | 通过，退出 0；仍有上述 139 条 warning，不等于严格 lint 通过 |
| `cargo build --locked -p lang-cli` | 通过，退出 0 |
| `python3 scripts/check_docs.py`、`python3 -m unittest discover -s scripts/tests -v`、`git diff --check` | 通过；385 Markdown、21 项检查器测试 |

上游 CI 仍在 `macos-14` 执行编译、严格 Clippy 与测试；没有 Linux 本机 job，且严格 Clippy
失败会阻止测试 job。本轮没有运行远程 CI 或 macOS，也未重跑 frontend 全量或此前五项
`multifile_type_checking` 失败套件；相关 Rust 文件与同步前字节一致，既有失败记录保持，
不能称为已经修复。当前 Spec 继续保持 `in-progress`。

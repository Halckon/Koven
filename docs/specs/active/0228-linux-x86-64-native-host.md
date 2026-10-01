# SPEC-0228: Linux x86_64 本机目标与基线验收

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施或验收 Linux 本机目标时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-228` |
| 所属 Phase | Phase 4；必要的 CLI Phase 6 编排适配及授权的 Parser Phase 1 基线修复 |
| 语言规范 | [现行 v0.38](../../guide/README.md)、[Phase 边界](../../guide/15-conformance-and-staging.md#phase-边界) |
| 批准依据 | 2026-10-01 用户确认修复基线问题，并授权“是的，如果新增一个linux taget不麻烦，可以新增一个” |
| 前置 Spec | SPEC-0034/0039/0040 `done` |
| 前置 ADR | ADR-0007/0010/0011/0026 `accepted` |
| 关联 ADR | [ADR-0026](../../adr/accepted/0026-linux-x86-64-native-host.md) |
| 阻塞项 | 两项 Parser 计数失败已修复；另有 5 项多文件与 3 项 call-argument 既有失败未纳入本次语义修复；实际远程结果以 PR 对应 head 检查为准，分支交付未完成 |
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
- 测试按宿主断言真实 Mach-O AArch64 或 ELF64 x86_64 产物。两个平台需处理 LLVM IR 的计数/插桩
  测试使用 `LLVM_SYS_211_PREFIX/bin/clang`，工具缺失时的 PATH `clang` 也须兼容 LLVM 21 IR，
  不误用生产 C driver 作为 IR reader。
- 保留现有 macOS LLDB 源码断点测试；Linux 检查真实 ELF 的 DWARF 行表及 native 运行。
  使用 `LLVM_SYS_211_PREFIX/bin/llvm-dwarfdump` 21.1；明确 Linux 行表证据和调试器断点证据的区别。
- 最初批准的三类基线修复为：`parser_declaration.rs` markers match 漏 `Item::Deinit`
  导致的 E0004、`ownership_checking/checker.rs` 对 Option 使用 `mem::replace` 的 Clippy
  失败（改用 `Option::replace`），以及 16 个文件的既有格式偏差。具体失败、最小修复和
  重跑结果登记在 §10；不增加 blanket lint allow，不掩盖零命中、跳过或未运行项。
- 2026-10-01 用户进一步批准修复阻塞 PR CI 的 139 条严格 Clippy 诊断。仅做等价清理：
  frontend 测试移除冗余引用、合并回放状态参数、简化匹配/迭代/Map 入口和复杂类型；codegen
  简化 Option 判断与嵌套条件。保留全部行为断言、门禁和语言/ABI 边界。
- 2026-10-01 用户继续批准局部优化 Parser 重复扫描并验证错误恢复，使 CI 往下运行。
  仅复用 block/lambda dispatch 当前 lexeme、合并分号消费路径；保留阈值、诊断、Span
  与 hard-stop 优先级，不扩大到软关键字或其他语义修复。
- 2026-10-01 用户批准继续修复 PR CI 至通过：五处 LLVM IR 插桩调用统一使用匹配
  LLVM 21 的 Clang，保留普通 object 链接与生产 CLI driver；不改变测试断言或忽略项。
- README 两语言版本、Architecture、测试前提、Spec/ADR 索引及冻结 inventory 同步。

## 4. 非目标

- 不增加 `--target`、交叉编译、sysroot 或通用 linker discovery；不支持 Linux musl、Linux
  AArch64、macOS x86_64、Windows 或其他新增宿主。
- 不改变 Koven v0.38 语言语义、公开 ABI、runtime ownership 规则或依赖版本。
- 不增加完整变量/类型调试信息，不把 Linux 行表自动测试扩称为 debugger 断点/单步验收。
- 不以新增 Linux 支持为由重跑 frontend 全量套件或修改无关代码。
- commit、push 与 PR 按后续用户明确授权执行；本次 CI 修复继续现有 draft PR，不包含合并、标记 ready 或扩大语义修复。

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

2026-10-01 用户已补充授权本地提交，提交信息必须带 SPEC-0228；随后批准 push 与创建
[PR #5](https://github.com/Halckon/koven/pull/5)，并先后批准继续修复 Clippy 与 Parser 重复扫描使 CI 往下执行。
本次沿用 `feature/spec-0228-linux-native`，不合并 `main`，保持 draft。
Rust 变更保持可构建、可测试，已知严格门禁缺口仍按 §10 报告；最终归档与 frozen inventory
迁移随交付验收同步完成。

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 宿主目标、Linux 链接/测试适配、必要基线修复与验收文档 | `feat(codegen): support Linux x86_64 native host (SPEC-0228)` |

## 9. 未决问题

本次授权内的两项 Parser 性能计数失败已修复，CI 同款本地命令均通过。§10 登记的五项
frontend 多文件失败及本轮对照确认的三项 call-argument 既有失败均未纳入本次语义修复；
它们不在 CI 的 frontend `--lib` 选择内。远程 CI 必须核对 PR 的实际 head，不能把本地通过
等同于 macOS 通过；当前保持 draft，尚未完成最终分支交付，Spec 不标为 `done`。

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


### 2026-10-01 PR 严格 Clippy 修复

原 PR head `ef79a2b9f9b3021f521ee93fb945b3499ff40504` 的
[CI 36841645711](https://github.com/Halckon/koven/actions/runs/36841645711) 已实际失败：
文档、格式与 Cargo Check 通过，严格 Clippy 失败，Targeted Tests 被前置失败阻止。
本次用户明确批准继续修复后，按同一 Rust 1.96.0 工具链复现 139 条诊断：frontend lib-test
134 条、codegen 5 条（重复目标不重复计数）。

修复限于两份 frontend 测试文件及三份 codegen 文件。frontend 的 121 处冗余引用只移除
自动解引用产生的额外借用；测试辅助结构 `ReplayInstances` 打包同次入边读取的三张只读
map，39 个调用保持原数据对应关系与所有断言；Vacant entry 保证原先的“缺席后插入”条件。
其余为 `matches!`、`next_back`、枚举既有两分支、局部类型别名及 `slice::from_ref`。
codegen 的 `is_some_and` 和 let-chain 保留短路顺序、loop statement 匹配与 loan 消费时机。
未改公开接口、生产 frontend 逻辑、语言/ABI、依赖、CI 条件或测试期望，未增加 lint allow。

以下为本次源码的实际串行本地验证；远程修复后结果应以
[PR #5 检查页](https://github.com/Halckon/koven/pull/5/checks)的对应 head 为准，不将本地通过
提前等同于 macOS CI 通过。

| 验收项 / 命令 | 实际结果 |
|---|---|
| `cargo fmt --all -- --check` | 通过，退出 0 |
| `cargo check --locked --workspace --all-targets` | 通过，退出 0 |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | 通过，退出 0，无 warning |
| `cargo test --locked -p lang-frontend --lib ownership_checking::checker::drop_planner:: -- --nocapture` | 96 passed、0 failed/ignored、81 filtered；包含改动的 iteration、snapshot 及独立实例回放矩阵 |
| `cargo test --locked -p lang-codegen --lib sequential_for_lowering_tests` | 7 passed、0 failed/ignored、471 filtered |
| `cargo test --locked -p lang-codegen --lib lowers_while_loop_break_continue_and_nested_loop_targets` | 1 passed、0 failed/ignored、477 filtered |
| `cargo test --locked -p lang-frontend --lib` | 175 passed、2 failed、0 ignored/filtered；失败为下述两项未修改 parser 测试 |
| `cargo test --locked -p lang-codegen -p lang-cli -p lang-lsp --no-fail-fast` | 572 passed、0 failed/ignored/filtered：codegen 478、CLI 64、LSP 26、codegen doc-tests 4 |
| `python3 scripts/check_docs.py`、`git diff --check` | 通过；385 Markdown |

frontend 全量集成测试未运行；五项已登记的 `multifile_type_checking` 既有失败不在 CI 的
frontend `--lib` 选择内，不能因本次 Clippy 或 CI 通过而称为已经修复。Spec 保持
`in-progress`，尚未归档。


CI 同款 frontend `--lib` 的两项失败为：
`parser::engine::tests::block_dispatch_legal_error_and_nested_families_stay_linear`
（2633 > 68 × 32）和
`parser::engine::tests::lambda_body_legal_unsupported_and_poison_families_stay_linear`
（2777 > 68 × 34）。失败来自单独 Parser 的 significant raw visits 计数，不是运行耗时阈值。
本次未修改 parser、放宽计数上限或屏蔽测试；Clippy 通过不等于测试门禁通过。


已从未修改的 PR head `ef79a2b` 通过 `git archive` 建立独立基线目录，强制重新编译
frontend（编译日志确认来源为基线目录），复用同一 Cargo target 串行执行
`cargo test --locked --manifest-path <baseline>/Cargo.toml --target-dir <shared-target> -p lang-frontend --lib families_stay_linear`：
1 passed、同样 2 failed、174 filtered、0 ignored，计数及失败位置完全一致。因此两项 parser
问题在本次 Clippy 修复前已存在，仍待独立修复与所需的 parser 契约回归。


### 2026-10-01 Parser dispatch 重复扫描修复

[CI 36843140596](https://github.com/Halckon/koven/actions/runs/36843140596) 在 head
`6f2fa4ba40727807d2471473ccec47f706b11a9a` 实际通过文档、格式、workspace check 与严格
Clippy，但 frontend `--lib` 为 175 passed、2 failed，因此 Targeted Tests 与总门禁失败。
本轮先在该 head 重现相同 2633 / 2777 次 significant raw visits，再做授权内局部修复。

生产改动仅位于 `parser/engine/block.rs` 与 `expression.rs`：每轮只读取一个当前 lexeme
判断自身 closer、caller hard stop（包含 EOF）与分号，并在 dispatch 的只读判别中复用它；
消费分号后继续循环，不生成节点，也不增加 dispatch iteration。退出时复用未消费的边界
lexeme；保留 block/lambda 各自原有的 lexical poison 与 terminal-owner 诊断抑制规则。
没有修改全局 cursor、公开 AST/API、语义、测试预算、CI 条件或依赖。

新增三项测试覆盖空 body 的连续分号、注释/trivia、真实 closer、EOF 空诊断 Span，以及
lambda/嵌套 block 遇到外层 `)`、`]` 时不吞 delimiter，body Span 止于最后实际消费分号。
相关测试通过已有 parse-twice helper 同时验证确定性与公开产物不变量。

以下均在 Rust 1.96.0、LLVM/Clang 21.1.8 的 Linux 宿主串行执行，未并发争用 Cargo target。
远程结果应核对 [PR #5 检查页](https://github.com/Halckon/koven/pull/5/checks)的实际 head；
此表记录本地证据，不提前声称 macOS CI 通过。

| 验收项 / 命令 | 实际结果 |
|---|---|
| Red：`cargo test --locked -p lang-frontend --lib families_stay_linear`，未改 head `6f2fa4b` | 1 passed、2 failed、174 filtered、0 ignored；同 CI 两项计数失败 |
| Green：相同 `families_stay_linear` 命令 | 3 passed、0 failed/ignored、174 filtered；原 32 / 34 倍计数预算与增长断言均未修改 |
| `cargo test --locked -p lang-frontend --test parser_block --test parser_lambda --no-fail-fast semicolon_runs` | 新增 3 passed、44 filtered、0 failed/ignored；block 1、lambda 2 |
| `cargo test --locked -p lang-frontend --no-fail-fast --test parser_block --test parser_lambda --test parser_control_flow --test parser_local_destructuring --test parser_call_argument --test parser_trailing_lambda` | 98 passed、3 failed、0 ignored/filtered；block 29、lambda 18、control 7、destructuring 14、trailing lambda 5 均通过；call argument 25 passed、3 既有失败 |
| 干净 `6f2fa4b` archive：`cargo test --locked --manifest-path <baseline>/Cargo.toml --target-dir <shared-target> -p lang-frontend --test parser_call_argument` | 25 passed、同样 3 failed、0 ignored/filtered；强制重编译且日志确认编译来源为 baseline，失败名、诊断与 Span 完全一致 |
| `cargo test --locked -p lang-frontend --no-fail-fast --test parser_entry_adversarial --test parser_entry_prefix_truncation_matrix --test parser_entry_trivia_invariance_matrix --test parser_entry_line_break_boundary_matrix --test parser_entry_lexical_poison_insertion_matrix --test parser_stress_matrix --test parser_standalone_poison_stress_matrix` | 9 passed、0 failed/ignored/filtered；覆盖所有公开入口的 adversarial、UTF-8 prefix、trivia、换行、poison 插入与 4096 元素合法/错误/poison 流 |
| CI 原样：`cargo test -p lang-frontend --lib` | 177 passed、0 failed/ignored/filtered |
| CI 原样：`cargo test -p lang-codegen` | 478 单元 + 4 compile-fail doc-tests passed、0 failed/ignored/filtered；含实际 native/object/runtime 与 Linux DWARF |
| CI 原样：`cargo test -p lang-cli` | 48 单元 + 3 format + 8 native + 5 project passed、0 failed/ignored/filtered |
| CI 原样：`cargo test -p lang-lsp` | 26 passed、0 failed/ignored/filtered |
| `cargo fmt --all -- --check`、`cargo check --workspace --all-targets`、`cargo clippy --workspace --all-targets -- -D warnings` | 全部通过、退出 0；strict Clippy 无 warning |
| `python3 scripts/check_docs.py`、`git diff --check` | 通过；385 Markdown，当前 diff 无 whitespace error |

额外确认的三项 `parser_call_argument` 失败保持原样，留待独立语义任务：
`call_only_ampersand_is_not_a_prefix_operator_or_lexer_error`（`f(inout input)` 得 L0034，
原测试预期 L0033）、`l0033_expected_value_preserves_committed_prefix_and_empty_error_value`
（`f(borrow,)` 得 L0036，原测试预期 L0033）、
`empty_recovery_children_do_not_extend_parent_spans_across_trivia`（函数类型错误 TypeRef
Span 为 `(10, 16)`，原测试预期 `(27, 27)`）。这些源码均不经过本次改动的 block/lambda
循环。未改测试预期，不以性能修复顺带决定软关键字语义。

未运行 frontend 全量与此前五项多文件失败套件；本机未运行 macOS/LLDB。选定契约通过不
代表 frontend 全量通过，也不改变 Spec 的 `in-progress` 状态或 PR 的 draft 状态。

### PR CI LLVM IR 工具版本修复（2026-10-01）

[PR run 36844836811](https://github.com/Halckon/koven/actions/runs/36844836811) 针对
`d8ce3d782680c2400a5a76e4440a652a45026c28` 的 macOS 结果：文档、fmt、workspace check、
严格 Clippy 与 frontend `--lib` 的 177 项通过；codegen 为 463 passed、14 failed、1 ignored。
CLI/LSP 因前序命令失败未运行，汇总失败。14 项失败均在五处测试调用中将 LLVM 21 的 `.ll`
交给系统 `/usr/bin/clang`，其 reader 拒绝 `captures(none)`、GEP `nuw` 或 `memory(none)`。
本机未重跑 macOS 基线，不将这些日志表述为已完成基线对照。

用户授权继续修复该 PR 至 CI 通过；本轮新增 `test_support::ir_clang`，在两个平台优先选用
`LLVM_SYS_211_PREFIX/bin/clang`，仅将五处 `.ll` 调用接入它。原 `clang` object helper 在
macOS 仍选 `/usr/bin/clang`，Linux 的原选择不变；生产 CLI 与所有断言、忽略项保持原样。
既有 LLDB 测试的 ignore 不算通过。

| 验收项 / 命令 | 实际结果 |
|---|---|
| Red：上述 macOS PR CI | 14 项 LLVM IR reader 失败，覆盖全部五处受影响调用 |
| `cargo fmt --all -- --check` | Linux 通过，退出 0 |
| `cargo check --workspace --all-targets` | Linux 通过，退出 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | Linux 通过，退出 0，无 warning |
| `cargo test -p lang-codegen` | Linux 478 单元 + 4 compile-fail doc-tests passed；0 failed/ignored/filtered |

本轮 Linux Rust 1.96.0 / LLVM、Clang 21.1.8 的选定 codegen 契约已通过；未再次运行输入
未变化的 frontend/CLI/LSP 本地测试，复用上一节证据。新 head 的 macOS PR CI 待实际运行，
不以 Linux 结果或 push 事件的轻量检查代替。

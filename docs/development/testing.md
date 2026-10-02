# 测试与分层验收

> **性质**：工程规则 · **状态**：current · **读取时机**：选择、运行或报告验证时 · **唯一真源**：本页

验收以受影响契约为单位。每个 Spec 只维护一张“验收项 → 测试目标/过滤器 → 实际结果”表；
实施步骤引用该表，不再重复执行同一门禁。语言行为覆盖正反例、诊断码/Span；native 行为必须实际运行。

## 选择最小充分检查

| 变化 | 必需证据 | 按影响追加 |
|---|---|---|
| 纯文档、候选 guide、Spec 状态 | `python3 scripts/check_docs.py`、`git diff --check` | 检查器变更才运行其测试 |
| 单阶段内部实现 | fmt、受影响 crate clippy、直接行为测试 | 修改共享 helper 时追加调用方测试 |
| 公开类型或阶段产物 | 上一行及直接消费者契约测试 | 跨 crate API 追加 workspace check |
| SSA/LLVM/runtime | 定向 model/verifier/lowering 测试 | 相关 native 正反例；CLI 编排变化才追加 CLI build/run |
| workspace/依赖或无法界定的影响面 | 先列出受影响目标并扩大检查 | 说明仍未覆盖的范围与扩大理由 |

命令形态（占位符必须替换为仓库中存在的目标）：

```bash
cargo fmt --all -- --check
cargo clippy -p <affected-crate> --all-targets -- -D warnings
cargo test -p <affected-crate> --test <suite> <filter>
cargo test -p <affected-crate> --lib <module-or-test-filter>
```

跨 crate 检查使用 `cargo check --workspace --all-targets`。不固定要求每个切片运行整个 `--lib`、
workspace check 或 CLI build；每项追加检查必须对应实际影响。

## 规范示例的分阶段门禁

`bash scripts/check_guide_litmus.sh` 执行文档结构检查及 Guide Litmus、两个相关 ownership
套件。Litmus 源码直接来自 Guide，known-gap 固定阶段、诊断码和范围，不能用 ignore/skip
或修改规范要求来取得通过；缺口行为变化时要核验实现，再同步
[覆盖账本](../architecture/guide-conformance.md)。此门禁还执行 SPEC-0241 的 Litmus4
单文件 native 与 enum-tag/SSA 定向回归，以及 SPEC-0240 的 Litmus12 两条 native 入口。
因此需要受支持的 LLVM/Clang；仍不等同于其他 native、全部语言功能或 frontend 全量验收。
两个 CI 宿主复用同一脚本，不另维护源码副本。

## 双宿主 CI

`.github/workflows/ci.yml` 配置 macOS 14 AArch64 / Ubuntu 24.04 x86_64 的 check、严格
clippy、核心测试、`check_stage_integration.sh` 和 `check_guide_litmus.sh`；fmt 只运行一次。
Rust 固定 1.96.0，check/clippy/test 使用 `--locked`。Linux 安装 LLVM 官方 Noble 21 签名源
中的固定 21.1.8 包，macOS 保留 `brew install llvm@21`；版本或工具缺失直接失败。

PR 的普通文档变更仍仅跑文档门禁；Guide10/11/13/15 的 Litmus 输入、门禁脚本、workflow/LLVM action
变更触发 Rust 门禁。main 与 workflow_dispatch 强制执行全部配置，feature/fix push 保持
仅文档/fmt 的现有成本策略；完整矩阵在 PR 执行。最终汇总拒绝 changes 失败或必需 job 跳过。

完整 codegen/CLI 测试保留 ELF、DWARF 行表、真实 link/run 和内存计数边界；macOS 已存在的
LLDB ignore 不扩大到 Linux。定向 frontend 仍不代表全量通过，known-gap 不代表功能完成。
本地与远端实际运行情况见 [SPEC-0239](../specs/active/0239-linux-ci-gates.md)，不能由配置存在
推导 CI 已通过。

## 控制 frontend 成本

默认不运行 `cargo test -p lang-frontend`、frontend `--tests` 或
`cargo test --workspace --all-targets` 等会纳入 frontend 全量套件的命令。
Guide 启用或 Spec 完成状态本身不触发全量 Rust 测试；相关实现按上表验收。

Lexer/Parser/Span/AST/诊断/harness 等共享路径变化时，逐项选择受影响的普通套件与
matrix/stress/large 契约测试，不因它们耗时而省略必要覆盖，也不凭文件名建立永久快速白名单。
影响面仍无法界定时，记录缺口，不宣称完成验收；全量 frontend 仅在用户明确要求时运行。
Release 也必须公开所选覆盖范围与缺口，不能把定向结果称为全量通过。

已知测试名或已确认的过滤器可直接运行，并核对输出中的实际命中数；不确定过滤范围时，
先用同一目标和过滤器追加 `-- --list`。零命中不算通过，不为已知选择重复启动测试二进制。
修改新测试后先记录失败证据，实施后运行同一选择，按契约风险扩大到所在套件。

## 编译与测试并行

- 默认串行执行 Cargo 命令，让一个 Cargo 进程管理编译并行；同一切片多个 integration suite
  用重复 `--test <suite>` 合并到一次调用，避免多个进程争抢同一 target 锁。
- 多个互相独立的定向套件合并运行时使用 `--no-fail-fast`，一次收集各目标结果，避免首个失败
  阻止其余套件执行。存在前置依赖的验证仍分开安排。
- 单个测试二进制内部使用 libtest 默认并行；仅在内存、进程或 native 工具资源紧张时，
  用 `-- --test-threads=<N>` 限制并行，并记录所用值。不要默认强制单线程。
- 文档检查、diff 检查可与 Cargo 检查并行。依赖前序产物的验证等待前序成功后运行。
- 不为并行测试复制多个 target 目录，也不同时运行多个全量 Cargo 命令；需要特殊隔离的
  测试遵循其已有资源约束。并行任务逐项收集退出码与结果，不能只报告最后完成的一项。

## 记录与复用

同一源码、依赖、feature 和工具链状态下，覆盖相同契约的成功结果可以复用；记录命令、
目标/过滤器、实际测试数和结果。相关输入变化后重跑对应检查，不按每个文档勾选重复执行。
报告区分 passed、filtered、ignored、未运行、timeout；未运行项写明原因。

## 本机目标与工具前提

受支持宿主与目标选择以 [ADR-0026](../adr/accepted/0026-linux-x86-64-native-host.md) 为准：
AArch64 macOS 与 x86_64 Linux + glibc，仅编译并运行宿主目标，不提供交叉编译。

- 先检查 `LLVM_SYS_211_PREFIX/bin/llvm-config --version` 为 21.1.x；LLVM 开发库、动态库
  和所需 AArch64/X86 backend 必须可用。记录实际工具版本，不以 Rustc 自带 LLVM 替代。
- 生产与 CLI linker 测试通过宿主 C driver 链接：macOS `/usr/bin/clang`、Linux
  `/usr/bin/cc`。Linux 需安装 glibc 开发文件和系统 linker；无需固定某一种 `ld` 实现。
- 两个平台的 LLVM IR 插桩/计数测试都需要匹配 Clang 21，优先使用
  `LLVM_SYS_211_PREFIX/bin/clang`；缺失时 PATH 中的 `clang` 也必须兼容 LLVM 21 IR。
  不能把系统 C driver 能链接 object 当作它能读取 LLVM IR 的证据；macOS 普通 object
  链接测试仍使用系统 `/usr/bin/clang`。
- Linux DWARF 自动检查使用 `LLVM_SYS_211_PREFIX/bin/llvm-dwarfdump` 21.1（缺失时回退到
  PATH 中的匹配工具），验证实际 ELF 中的文件和源码行映射，并保留 native 执行断言；macOS 原有 `/usr/bin/lldb --batch`
  断点/运行验收保持。Linux 行表检查不证明调试器断点、单步或变量查看。
- 新宿主至少要有目标选择正反例、triple/DataLayout/object machine 一致性、真实 object/link/run、
  runtime 正反例与失败保留输出的证据。宿主条件过滤后的零命中不算该平台通过；只有 Linux
  环境时明确登记 macOS 未运行，不把保留 macOS 代码路径表述为完成回归。

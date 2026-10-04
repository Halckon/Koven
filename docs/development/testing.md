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

`check_stage_integration.sh` 还完整选择普通unit的 `owned_compilation_unit_view` 与
`owned_unit_view_compile_contracts` 两个integration targets；selection policy要求各出现一次。
它们分别验证工厂身份/只读事实与外部构造、能力、生命周期边界。
同一脚本也完整选择 `unit_name_snapshot` 与 `unit_name_snapshot_compile_contracts`，selection
policy要求各恰一次且不带过滤器：前者与旧手工名称前缀比较完整facts/诊断及canonical来源，
后者以外部rustc正反例验证owner封闭与inputs生命周期；接线存在不代表该head已运行。

同一 stage 完整选择 `basic_unit_ownership` 与 `basic_unit_ownership_compile_contracts`，
policy要求各恰一次、无过滤器：前者与手工 basic validation/checker 比较完整事实和 provenance，
后者验证封闭字段、能力错配以及按值 Outcome 跨输入借用期的合法持有；不为其虚构借用限制。
CLI完整测试自动包含 `project_basic_ownership_cli` 的旧主干 human/JSON 全输出 oracle；
LSP完整测试保留冻结 manual 链与 publications/UTF-16/raw事实差分，不用新门面循环自证。

同一stage完整选择 `single_file_analysis` 与 `single_file_analysis_compile_contracts` 各恰一次、
无过滤器，分别锁固定五gate/typed observer顺序、原事实与身份，以及封闭字段/只读/HRTB
借用逃逸和raw能力边界。CLI完整测试包含旧生产捕获的单文件全输出golden，LSP完整测试
保留旧手工链、完整publication、UTF-16 definition及legacy特有版本/发送事务边界。

stage还完整选择 `const_owned_compilation_unit_view`、`const_owned_unit_view_compile_contracts`
及 `multifile_constant_ownership` 各恰一次，无filter。codegen完整测试含
`const_native_view_compile_contracts` 的实际公开native API正负编译对照；选择不等于执行。

## 双宿主 CI

`.github/workflows/ci.yml` 配置 macOS 14 AArch64 / Ubuntu 24.04 x86_64 的 check、严格
clippy、核心测试、`check_stage_integration.sh` 和 `check_guide_litmus.sh`；fmt 只运行一次。
同一双宿主 test job 通过 `check_integration.sh` 完整执行 `cargo test --locked -p lang-frontend --test ownership_iteration`，
不加测试名过滤器或细分路径条件，随现有Rust PR/main/dispatch门禁执行；不是frontend全量。
Rust 固定 1.96.0，check/clippy/test 使用 `--locked`。Linux 安装 LLVM 官方 Noble 21 签名源
中的固定 21.1.8 包，macOS 保留 `brew install llvm@21`；版本或工具缺失直接失败。

PR 的普通文档变更仍仅跑文档门禁；Guide10/11/13/15 的 Litmus 输入、门禁脚本、workflow/LLVM action
变更触发 Rust 门禁。main 与 workflow_dispatch 强制执行全部配置，feature/fix push 保持
仅文档/fmt 的现有成本策略；完整矩阵在 PR 执行。最终汇总拒绝 changes 失败或必需 job 跳过。

独立的[手写 Rust 尺寸护栏](rust-size-policy.md)在所有上述事件运行，包含policy测试和基于
明确Git base的增长检查；纯文档PR也不能跳过它。其失败/取消/缺失/跳过均使汇总失败。

完整 codegen/CLI 测试保留 ELF、DWARF 行表、真实 link/run 和内存计数边界；macOS 已存在的
LLDB ignore 不扩大到 Linux。定向 frontend 仍不代表全量通过，known-gap 不代表功能完成。
本地与远端实际运行情况见 [SPEC-0239](../archive/specs/0239-linux-ci-gates.md)，不能由配置存在
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

## 本机恢复后的有界组合

`bash scripts/check_integration.sh`依次选择core、ownership_iteration、stage、尚未覆盖的guide_litmus、
真实CLI教程。独立stage/Guide入口保留，不增加任意skip开关。core包含lang-std源码资产，
组合为77个唯一frontend integration和10次Cargo调用；接线与失败传播由Python合同测试核验。
直接依赖job无条件运行，required summary拒绝失败/取消/跳过/缺失；仅验证五成员四条直接声明边，
不推定第三方transitive、patch/config或lock新鲜度。editors与tutorial输入触发Rust矩阵。
编辑器语法变化另需 `npm ci --prefix editors/tree-sitter` 后执行
`npm run generate --prefix editors/tree-sitter` 与 `npm test --prefix editors/tree-sitter`。
前者必须与提交的生成产物一致；后者执行真实 CLI 完整 corpus 及 Python XML 树断言。
CI 独立 editor job 固定 lockfile 中 CLI 0.26.12，按 editor/对应门禁/词法调用规范输入触发，
main/manual 强制执行；失败或不合法 skip 均阻止 required summary。Rust fixture 交叉检查
不能替代此 CLI 验收。
本机结果见[恢复账本](recovery-local-delivery.md)，不能由接线推定实际CI通过。

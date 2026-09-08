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

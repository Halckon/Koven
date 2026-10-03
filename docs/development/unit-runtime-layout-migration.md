# P2 unit runtime layout 职责拆分验收

> **性质**：有界生产职责迁移验收 · **状态**：本地验收通过；独立源码及验收review通过，精确head双宿主CI待后继 · **读取时机**：复核或回退runtime layout拆分时 · **唯一真源**：本页与[逐项证据](evidence/unit-runtime-layout-migration.json)；范围从属[已批准计划](engineering-governance-plan.md)

## 范围、基底与提交

2026-10-03 从本地已验收的 const 交接 head
`2ec24abc8ca899bef07e00f3bcd1fc23c908fcbd`（tree `f1d3a4901ddec3c54fad776a0e0276eb68571956`）
建立独立 worktree 与 `feature/spec-p2-runtime-layout` 分支。本片仅是批准 P2 的生产职责拆分，
所属 Phase 4；不新增功能、可观察诊断、语言规则、长期架构或 ABI 决策，因此使用本验收账本，
不另建 Spec/ADR。基底0254仍待独立精确head双宿主CI，不由本片本地绿灯替代。

纯源码移动 commit `d79a6929d6f67f8286553c3dfc44ed3c9365fa97`，
tree `2eacebfa4c44ede540b1441c77e6f46862a0c62b`；只有两个生产文件变化。
文档与单项baseline收紧另行提交；本片只本地交付，没有远端引用、PR或其他远端写入。
后续组合发布、精确head双宿主CI与合并由上层交付另行处理，不预称已发布或P2整体完成。

## 模块边界与原合同

| 文件 | PLOC | 职责 |
|---|---:|---|
| `crates/lang-codegen/src/ssa/unit_plan.rs` | 3035 → 2658 | 原planner入口、实例/路由/recipe、concrete type替换及共享模型保持 |
| `crates/lang-codegen/src/ssa/unit_plan/runtime_layout.rs` | 398 | runtime demand升级、存储依赖递归、exact owner字段布局消费及其recipe许可检查 |

完整迁移以下八个函数，原连续正文379行，另移除一行块间空白：

- `classify_runtime_type_demands`：只对父模块开放 `pub(super)`
- `upgrade_runtime_demands`、`runtime_storage_depends_on`、`require_exact_runtime_field_layout`
- `resolve_nominal_runtime_field_types`：保持 `pub(crate)`，父模块重导出原路径
- `supported_nested_runtime_field_recipe`、`supported_nested_runtime_field_recipe_with`、`direct_owner_type_parameter`

新模块私有，内部显式窄引用原 `contains_type_parameter`、`resolve_concrete_type`、
`span_contains`、`unit_callable_signature` 与错误构造器；这些helper与
`UnitRuntimeTypeDemand`、`UnitInstancePlan` 均未搬迁或改写。
单文件和unit的同名 `resolve_concrete_type` 不等价，本片不合并、不进入P4共享内核。

原路径继续供 planner 的运行时需求、委托字段与recipe检查、`unit_plan/deinit.rs` 的resource查询、
`unit_lower/aggregate.rs` 的构造/投影，以及 `unit_lower/type_lower.rs` 的nominal存储消费。
所有调用方字节未变，没有提升为跨crate公开API或新增框架。

保留的关键顺序与拒绝边界：

- 每实例先callable参数/返回，再本source/span内expression；最后才要求所有runtime demand的exact layout
- BTree确定次序、`InstanceKeyOnly` 到 `RuntimeLayoutRequired` 的单向最强需求升级不变
- 存储依赖递归的visited/cycle错误与 `found || ...` 短路原样保留，不提前访问先前不会访问的分支
- exact descriptor先核owner identity/arity，再核declaration/arguments/字段数，逐字段核symbol/template/span/具体类型存在后才查recipe许可
- required布局缺失继续fail closed；仅原nested候选之外的closed/direct参数规则可fallback，不猜测缺失事实
- 错误kind、Span、顺序和unsupported范围不变；未修改测试、fixture、assertion、cfg、ignore或语言行为

## 等价与身份保全

八函数块仅归一 `classify` 的可见性与块尾分隔空白，UTF-8字节全部相等。
保留末尾一个LF的块 SHA-256 为
`c4e079da94ba4b9d479a8d6cf7afbff6b70a75acf5fab18c929a8235cb578f5c`。
将该块插回原位置、撤销三行mod/use/re-export接线，父文件与基底逐字相等；比token等价更强。
证据JSON记录每个完整函数块hash、其余210个codegen受保护文件hash及48个完整plan测试名。

七域仍为 entry_identity3、instances4、delegation_routes13、owner_recipes5、recipe_cycles8、
error_order10、layout_demand5；入口与所有helper字节不变，无身份映射变更。
前后完整library清单均748项且逐项相同；同worktree `cargo metadata` 完整JSON逐字相等，
共142 targets。代码移动之外的Cargo manifest、lockfile、toolchain和CI配置未改。

在本片checkout可复核最强的完整块与父文件逆向重建证据：

```python
from pathlib import Path
import subprocess
base = "2ec24abc8ca899bef07e00f3bcd1fc23c908fcbd"
p = Path("crates/lang-codegen/src/ssa/unit_plan.rs")
old = subprocess.check_output(["git", "show", base + ":" + str(p)]).decode()
new = Path("crates/lang-codegen/src/ssa/unit_plan/runtime_layout.rs").read_text()
a = old.index("fn classify_runtime_type_demands(")
b = old.index("fn span_contains(", a)
chunk = new[new.index("pub(super) fn classify_runtime_type_demands("):]
chunk = chunk.replace("pub(super) fn classify_runtime_type_demands(",
                      "fn classify_runtime_type_demands(", 1)
assert old[a:b].rstrip() + "\n" == chunk
restored = p.read_text().replace("mod runtime_layout;\n", "", 1)
restored = restored.replace("use runtime_layout::classify_runtime_type_demands;\n"
    "pub(crate) use runtime_layout::resolve_nominal_runtime_field_types;\n", "", 1)
restored = restored.replace("fn span_contains(", chunk + "\nfn span_contains(", 1)
assert restored == old
```

## 本地实际验收

Linux x86_64，Rust/Cargo1.96.0、LLVM/Clang21.1.8；既有共享target、`CARGO_INCREMENTAL=0`，
Cargo串行，无clean或复制target。以下Cargo命令均带 `--locked --offline`（fmt除外），退出码均0。

| 命令/证据 | 实际结果 |
|---|---|
| `cargo test -p lang-codegen --lib unit_plan_tests` | 前后各48 passed，0 failed/ignored，700 filtered；含exact descriptor、10 error_order、5 layout_demand |
| `cargo test -p lang-codegen` | 完整748 lib + 2 integration + 4 doc-tests；各0 failed/ignored/filtered |
| native inherited owner recipe roundtrip | 完整命令中 `native::unit_tests::dependent_class_inherited_owner_recipe_value_roundtrip_links_and_runs` 恰一次ok，真实link/run |
| `cargo check --workspace --all-targets` | 通过，覆盖共享resolver下游与workspace编译 |
| `cargo clippy -p lang-codegen --all-targets -- -D warnings` | 通过 |
| `cargo check -p lang-codegen --release` | 普通非test release检查通过；不是release tests |
| `cargo fmt --all -- --check` | 通过 |
| `python3 -m unittest discover -s scripts/tests -v` | 102项通过 |
| `python3 scripts/check_rust_sizes.py --base 2ec24abc8ca899bef07e00f3bcd1fc23c908fcbd` | 708手写Rust、48超千行、0生成物，通过 |
| `python3 scripts/check_docs.py`、`git diff --check` | 479 Markdown结构与whitespace通过 |

policy只有原 `unit_plan.rs` 的baseline从3035收紧至2658，其余baseline、三个既有例外与
生成物登记逐值不变，无新例外。原文件仍有2658行历史欠账，不靠分片或压行声称清零。
未运行macOS、精确head远端CI、frontend/workspace全量tests、整套stage/Guide、完整CLI/LSP、
release tests；本片未改变对应宿主或frontend实现，不以check冒充其测试执行。

## 有界成本样本与限制

同一worktree/工具链，前后各两次只touch `unit_plan.rs` 的dependency-warm lib compile/link，
JSON artifact核实只有 `lang_codegen` nonfresh；另各三次no-op及脱离Cargo直接执行48项。
计时用Python monotonic与RUSAGE_CHILDREN，峰值RSS单位KiB；原始值在证据JSON。
基线采样后、后置对照采样前登记本片调查阈值：rebuild中位耗时增长超过max(25%,1秒)，
或RSS增长超过max(15%,128MiB)；不声称在全部采样之前预注册。

| 样本 | 前 | 后 |
|---|---|---|
| compile/link两次秒 | 14.010、14.566 | 14.175、14.673 |
| compile/link两次max RSS KiB | 1623712、1583172 | 1535460、1525604 |
| no-op三次秒 | 0.059、0.061、0.063 | 0.064、0.061、0.060 |
| 48项直接执行三次秒 | 0.064、0.061、0.057 | 0.050、0.058、0.054 |

两项调查阈值均未触发；小样本只排查明显退化，不证明性能等价或提速。
首次新worktree编译30.249秒含frontend重编，明确不纳入对照。未测干净机器冷构建、
release运行成本或完整benchmark，不能把本地重编次数或尺寸下降解释为用户性能收益。
最初尝试 `/usr/bin/time` 因工具不存在以127退出，Cargo未启动；改用上述Python计时。

独立源码与最终文档/验收证据review均为Approve。回退只撤销本片源码与配套文档/policy，
不触碰0254及其既有交接实现；后续仍需精确最终head的双宿主CI。

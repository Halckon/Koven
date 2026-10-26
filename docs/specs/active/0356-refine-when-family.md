# SPEC-0356: E3 `when` 家族迁入所有权细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3/N02 的 `when` lowering 时 · **唯一真源**：本片目标、范围与验收；语义以 Guide 为准，长期决定以 ADR-0032 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0356` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.46 `when`](../../guide/06-blocks-control-flow.md#when)、[`when` 条件、覆盖域与重复](../../guide/06-blocks-control-flow.md#when-条件覆盖域与重复) |
| 批准依据 | 2026-10-26 用户选择：E3 下一片为 `when` 家族；PR #172 第 9 个源码的阻塞以“细化层支持无 subject 的 when”解决，#172 依赖本片 |
| 基线 | main `adddef8d` |
| 分支 | `feature/spec-0356-refine-when`（步 1）、`feature/spec-0356-when-subject`（步 2，基线 main `86f795e0`） |
| 前置 | [SPEC-0350](../../archive/specs/0350-ownership-refinement-nullable-pilot.md)（细化层首片）、[ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

`when` 在细化层里按 Guide 语义 lowering：条件按源码顺序逐个测试，首个命中的 entry 进入 body；
每个命中边消费 `WhenAlternativeMatch` 析构点，每个 body 消费其 `BranchExit`，语句位置无 `else`
的 `when` 在落空边消费 `BranchExit(entries.len())`。分步交付：

| 步 | 范围 | 关闭的用例 |
|---|---|---|
| 1 | 无 subject、全部条件为 Boolean 表达式的 `when`（语句与值位置，含逗号多条件）；同时补齐 #172 需要的 `loop`、`break`、`continue` 与全分支发散的 `if`；Copyable Borrow nullable 参数的 null 比较 | PR #172 `later_argument_transfer_…` 第 9–12 个源码 |
| 2 | 有 subject：整数/Boolean 等值条件、`in`/`!in` 整数 Range | p08、v16、v19（p41 见 §2 事实缺口） |
| 3 | enum subject 的 `is`/`!is` 与无 payload case 等值，含 enum 方法内 `when (this)` | p50；E2 的 enum when 回退 |

## 2. 设计约束

- 只消费既有事实：条件的 typed 结果、null 比较与非空 use、ownership drop 点；不从 AST 拼写推断。
- 条件仍走细化层的分支形式（`&&`、`||`、`!`、null 比较）；首步不在 `when` 条件上建立 MoveOnly
  proof view，含此类条件的函数留在旧路径。
- 值位置的 `when` 必须有 `else`（封闭域穷尽随步 3 一起做）；分支值按 frontend 批准的 `T → T?` 适配后
  汇入结果槽。
- 不新增按“类型 × 控制流”拆分的模块；子集外的 `when` 继续由现有路径处理，直到对应步骤完成。
- `break`/`continue` 先消费其 `ControlTransfer` 析构点，再结束本轮循环里打开的 proof view；外层
  view 跨越循环保持有效。`LoopExit` 析构在每条出口边上各自发出（条件为假的边与每个 `break`），
  因为同一绑定可能在循环体内的某条路径上已经析构；构建器按路径记录已析构的命名 owner，
  同一路径不重复析构。
- 有 subject 时 subject 只求值一次；等值条件逐个与之比较，`in`/`!in` 先按源码顺序求两端，消费
  Range 表达式的 `AfterExpression` 析构点，再测下界与上界（`..` 用 `<=`，`..<` 用 `<`），`!in`
  交换命中与落空目标。
- **事实缺口（p41）**：MoveOnly subject（String）当前不进入子集。ownership 事实把 subject 绑定
  的最后一次使用记在 subject 表达式之后（liveness 不让 subject 跨越后续条件测试存活），
  且 `when` 条件里的 MoveOnly 候选临时值（如 `"a"`）没有任何析构点。细化层不自造析构点，
  需另开 frontend Spec 补齐这两项事实后再接入。

## 3. 验收标准

- [x] 步 1：`ssa/refine_tests.rs` 的三项 native 运行——语句 `when` 后经 proof view 的调用、值位置
  `when` 的逗号多条件与语句 `when` 落空边、外层 proof view 下的 `loop`/`while` 与 `break`/`continue`；
  `refine_subset_tests` 把无 subject `when` 与循环跳转列为进入子集，有 subject `when` 与 lambda 列为子集外。
- [x] 步 1：PR #172 的 `later_argument_transfer_ends_earlier_temporary_borrow_before_drop` 在合入本片后
  15 个源码、30 个 root 全部通过（本地 Linux）。
- [x] 步 2：p08、v16、v19 在单文件与 project 两种模式通过，矩阵无回退、两模式差异为 0；
  `refine_tests` 的有 subject `when` native 运行（整数等值、`in`/`!in`、Boolean subject、语句位置
  混合分支类型），`refine_subset_tests` 把 nullable/String subject 与无 `else` 的值位置 `when`
  列为子集外。
- [ ] 步 3：p50 通过；E2 的 enum when 回退项关闭或逐项记录剩余原因。
- [ ] 受影响目标定向测试、fmt、clippy、尺寸护栏、`check_docs.py` 通过；双宿主 CI 以 PR 为准。

步 1 更新的既有测试（仅 IR 形状，行为断言不变）：

| 测试 | 原断言 | 更新原因 |
|---|---|---|
| `while_flow_tests::readonly_nested_contract` | 内层 `while` 继承外层 proof、只有一个 `NullableBranch` | 两种布局都进入细化层；内层条件重新测试并建立新 view，改为断言外层 view 沿每条出口边带过内层循环并支撑最后一次读取 |
| `while_flow_tests::direct_contract`（第 3 例 `break`） | Boolean flag loan 沿 header/body/absent 传递 | 该例现由细化层 lowering，同第 4 例：未用的 flag 留在入口，无回边的 header 并入入口块 |

步 2 把路由门从“含 nullable”放宽为“含 nullable 或已迁入家族的构造（`when`）”，不含 nullable 的
`when` 函数从此进入细化层。更新的既有测试（仅 IR 形状，行为断言不变）：

| 测试 | 原断言 | 更新原因 |
|---|---|---|
| `comma_when_cleanup_tests::comma_when_real_facts_and_ssa_preserve_common_owner_and_clean_first_match` | 首个条件与 `probe` 结果的分支 origin 为整个 entry 的 span | 细化层的条件分支以条件自身 span 为 origin；单次析构、析构所在边、owner 沿各边的传递与调用参数断言不变 |
| `direct_call_prefix_tests` 两项矩阵（Commit 列） | Boolean `when` 的判定分支 origin 等于整个 `when` 的 span | Commit 列现由细化层 lowering，判定分支 origin 为其中的 `true` 条件；改为在 `when` span 内查找唯一的条件分支，各边上 owner/loan 的传递与路径断言不变 |

步 2 同时收紧子集：析构点为 `ControlTransfer`、目标为临时值的事实（跳转跳出调用参数，放弃前面
参数的临时值及其 loan）留在旧路径；`direct_call_prefix_tests` 的 TemporaryBorrow-Return 计划红因此保持不变。

## 4. 非目标

- subject 为 MoveOnly 的消费式匹配（路线图 L7）、`when` 封闭域穷尽之外的新诊断；
- Elvis、safe call、`as?`；frontend 新事实。若发现事实缺口，停下另开 frontend Spec。

## 5. 实施记录

| 步 | 提交 | 内容 |
|---|---|---|
| 1 | `4a2e90bf` 与后续提交 | `refine/build/when.rs`；子集接受无 subject `when` 并放行 `WhenAlternativeMatch` 析构点；`loop`、`break`、`continue` 与全分支发散的 `if`；Copyable Borrow nullable 参数的 null 比较先经 loan 读出副本 |
| 2 | 本分支提交 | 有 subject `when`：整数/Boolean 等值与 `in`/`!in` 整数 Range；路由门加入 `migrated`；`ControlTransfer` 临时值析构留在旧路径；String subject 记为 frontend 事实缺口 |

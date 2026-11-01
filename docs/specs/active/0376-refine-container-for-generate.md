# SPEC-0376：顺序容器 for 遍历与运行时长度构造迁入所有权细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的顺序容器 `for` 遍历与运行时长度构造（`Array<T>(size, init)` / `List<T>(size, init)`）细化层迁移时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0034](../../adr/accepted/0034-refine-closures-and-callable-views.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0376` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 集合与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户确认合并 `main` 后按推荐顺序依次实施 SPEC-0376 与 SPEC-0377 |
| 基线 | `feature/spec-0375-refine-sequential-containers` `964ed6d7`（含 `main` `df07778a`） |
| 分支 | `feature/spec-0376-refine-container-for-generate` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0034](../../adr/accepted/0034-refine-closures-and-callable-views.md)、[SPEC-0375](0375-refine-sequential-containers.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将顺序容器的运行时长度构造（`Array<T>(size, init)`、`List<T>(size, init)`）以及具名顺序容器的 `for` 遍历（`for (x in xs)`、`for (_ in xs)`、`for ((a, b) in xs)`，含 `break`、`continue`、`return` 及嵌套遍历退出清理）迁入所有权细化层（`ssa::refine`）。

## 2. 实施内容

1. **细化层 IR 与发射（`refine/body.rs`、`refine/emit.rs`）**：
   - 新增 `ROp::ContainerGenerateBorrowed { container, length, initializer }` 与 `RAbort::NegativeSize`（发射为 `"negative size"` abort），在 `emit.rs` 中映射为 `Operation::ContainerGenerateBorrowed`。
2. **运行时长度容器构造（`refine/subset/container.rs`、`refine/build/container.rs`）**：
   - 预检并构建 `ContainerConstructionKind::RuntimeLength`（`Array<T>(size, init)` 与 `List<T>(size, init)`）：借用并读取 `size`、生成 `length < 0` 负长度守卫分支，在非负分支中为 `initializer`（`Borrow` callable view 参数、局部 callable 绑定、lambda 字面量或返回具体 callable 的直接调用）建立 shared loan，发射 `ContainerGenerateBorrowed` 后按逆序结束临时借用并消费 `AfterExpression` / `CallReturn` 析构事实。
3. **顺序容器 `for` 遍历与有序退出清理（`refine/subset.rs`、`refine/subset/inout.rs`、`refine/subset/container.rs`、`refine/build.rs`、`refine/build/container.rs`）**：
   - 支持具名 `Owned`/`Shared` 顺序容器（`Array<T>`、`List<T>`、`MutableList<T>`）的 `Discard`、`Name` 与 `Destructure`（Copyable/MoveOnly `value class` 组件投影借用 `SharedFieldLoan`）三种绑定形态。
   - 在 `Builder::drops` 中消费前端 `iteration_cleanup_at(point)` 发布的有序 `Drop`、`EndBinding`、`EndElement`、`FinishProvider` 与 `EndSource` 动作序列，在正常回边、显式 `continue`、`break`、`return` 与耗尽出口上精确释放元素借用、源容器借用与局部 owner。

## 3. 验收标准

- [x] `refine_tests::container_for_and_runtime_generate_lower_in_refine` 验证 `Array<T>(size, init)`、`List<T>(size, init)`（含 view 参数与捕获闭包）以及顺序容器 `for` 遍历（含 `continue`、`break` 与 `value class` 解构遍历）经 `ssa::refine` lowering 并通过原生执行。
- [x] `unit_lower_container_tests`（15/15，含 144 项 `runtime_generator_unit_matrix`）与 `sequential_for_lowering_tests`（27/27，含 `borrowed_source_cfg_does_not_end_the_callers_source_loan` 与嵌套遍历确定性测试）全部保持通过。
- [x] `cargo fmt --all -- --check`、`cargo clippy`、`scripts/check_docs.py`、`scripts/check_rust_sizes.py --base main` 通过。

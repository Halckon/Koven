# SPEC-0375：顺序容器（Array/List/MutableList）迁入所有权细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的顺序容器（`Array`/`List`/`MutableList`）细化层迁移时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0375` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 集合与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户确认按推荐优先级依次完成 E3 后续切片 |
| 基线 | `feature/spec-0374-refine-ownership-primitives-const` `fe686b21` |
| 分支 | `feature/spec-0375-refine-sequential-containers` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[SPEC-0374](0374-refine-ownership-primitives-const.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将核心顺序容器（`Array<T>`、`List<T>`、`MutableList<T>`）的存储准入、字面量与空容器构造（`arrayOf`、`listOf`、`mutableListOf`、`MutableList<T>()`）、长度读取（`.size`）、元素下标读取与借用实参（`xs[i]`、`f(xs[i])`）、元素赋值替换（`xs[i] = v`）以及 `MutableList<T>` 可变操作（`add`、`clear`、`removeAt`、`removeFirst`、`removeLast`、`insertAt`）迁入所有权细化层（`ssa::refine`）。

## 2. 实施内容

1. **细化层 IR 与发射（`refine/body.rs`、`refine/emit.rs`）**：
   - 新增 `ROp::ContainerConstruct`、`ROp::ContainerLength`、`ROp::ContainerElementPlace`、`ROp::ContainerAppend`、`ROp::ContainerClear`、`ROp::ContainerRemoveAt`、`ROp::ContainerRemoveFirst`、`ROp::ContainerRemoveLast`、`ROp::ContainerInsertAt` 与 `ROp::ContainerReplace`，并在 `emit.rs` 中一对一映射为对应的 SSA `Operation`。
2. **容器存储类型准入与需求拓扑（`refine/subset.rs`、`refine/subset/container.rs`、`refine/subset/drops.rs`）**：
   - 当 `Array<T>`、`List<T>`、`MutableList<T>` 的元素类型 `T` 为细化层支持的非 `Unit` 存储类型时，准入为细化层存储类型（含 `Borrow` 参数及 nullable `view_inner`），并在 `type_demands` 中递归先登记内层元素类型与 `Int`。
   - 对元素赋值 `xs[i] = v` 在 `Checker::element_replacements` 中登记并放行 `AfterReplacement` / `ReplacedElement` 析构事实。
3. **容器操作细化构建（`refine/build/container.rs`、`refine/build/expression.rs`）**：
   - 支持 `ListForm` 与 `EmptyMutableList` 构造、`.size` 长度读取、Copyable 元素下标读取 `xs[i]`、MoveOnly 元素借用实参 `f(xs[i])`、元素赋值替换 `xs[i] = v` 以及 `MutableList<T>` 六项变更操作（`add`/`clear`/`removeAt`/`removeFirst`/`removeLast`/`insertAt`）。

## 3. 验收标准

- [x] `refine_tests::sequential_containers_lower_in_refine` 验证 `Array`/`List`/`MutableList` 构造、`.size`、下标读取与借用、元素赋值及 `MutableList` 六项变更操作经 `ssa::refine` lowering 并通过原生执行。
- [x] `unit_lower_container_tests`（15/15）、`unit_lower_container_element_tests`（2/2）、`unit_generic_container_tests`（4/4）、`container_operation_tests`（10/10）、`sequential_for_lowering_tests`（27/27）与 `borrowed_container_lowering_tests`（2/2）全部保持通过。
- [x] `cargo fmt --all -- --check`、`cargo clippy`、`scripts/check_docs.py`、`scripts/check_rust_sizes.py --base main` 通过。

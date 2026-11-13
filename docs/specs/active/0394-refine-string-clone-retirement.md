# SPEC-0394：直接 `String.clone()` 旧路径退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `String.clone()`（`unit_lower/scalar.rs::lower_string_operation`）旧路径退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0394` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认继续推进 E3；SPEC-0393 范围收缩时暂留的 `String.clone()` 旧路径由本片退役；用户选择把钉住旧 SSA 形状的测试“按细化层形状重写” |
| 基线 | `feature/spec-0393-refine-range-borrow-result-rc-closure-retirement` `68930ddd` |
| 分支 | `feature/spec-0394-refine-string-clone-retirement` |
| 前置 | [SPEC-0371](0371-refine-integer-range-text.md)、[SPEC-0387](../../archive/specs/0387-refine-string-clone-box-value-field-when-retirement.md)、[SPEC-0393](0393-refine-range-borrow-result-rc-closure-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

删除 `unit_lower/scalar.rs::lower_string_operation`，把直接 `String.clone()` 计入“只在细化层降低”的构造（`subset/range_borrow.rs::expr_requires_refine`），使含 `String.clone()` 的函数整体进入细化层；旧路径对 `String.clone()` 确定性返回 `UnsupportedNode`。细化层的 `String.clone()` 降低已由 SPEC-0371/0387 提供，本片不新增降低能力。

## 2. 测试改写原则

含 `String.clone()` 的函数离开旧路径后，按旧 SSA 形状钉住的测试按细化层的等价形状重写；断言的对象（身份、所有权、清理顺序、native 输出与分配计数）不变：

- **只在细化层成立的形状差异**（逐条改写，不放宽身份检查）：
  - 循环耗尽边不再搬运已死的游标、`Borrow` 参数在最后一次使用后不再搬运（`births`、`scope_future_observer`、`owner_update_observer`、`same_guard`）；
  - 循环体必然 `return` 时不形成回边，header 折叠进 `first` 所在块（`control_flow` 的 `ordinary-neighbor-*-temporary-owned-string`）；
  - 值经 join 参数到达使用点：`definition` 透过“所有入边定义相同”的块参数（`map continue/return structure`）；
  - `if (removed != null)` 在细化层只证明一次并把视图借给两次调用，null 边进入 `else`（原为每次调用各自证明并在 null 时 `Abort`）（`resource_map_loop`）；
  - `Map.contains` 的键是从元素 loan 读出的值（`same_guard`）。
- **原本按旧路径边界写的期望**：独立可空比较（`nullableValue(item) != null`）随函数进入细化层后可降低，断言改为其在自身源区间产生一次 `nullable.is_null`；不含 Map 的 `no-map` 变体仍走旧路径并保留原边界断言（`operand_last_use`）。
- **强制旧路径的测试**（`with_per_family_lowering` 或旧路径私有检查点）：旧路径已无 `String.clone()`，源码中的 `"text".clone()` 由 `test_support::per_family_source` 改写为 `("text" + "")`，或在测试源中直接写成拼接；不改共享 fixture 文件。
- `refine_tests::control_flow_tests` 的“外层证明视图 + 循环内重赋值”从“不得准入细化层”改为“视图在循环前结束、native 输出与分配计数精确”。

## 3. 验收标准

- [x] 删除 `lower_string_operation`，`String.clone()` 计入细化层必需构造；`refine_tests::map_condition_tests::retired_borrow_result_rc_operation_string_clone_and_captured_closure_boundary_tests` 恢复 `s.clone()` 一项：强制旧路径时在 `s.clone()` 处 `UnsupportedNode`。
- [x] lang-codegen 全目标 2664 通过、0 失败；门禁 138/154、0 回退，expectations 无需更新；fmt、clippy、尺寸护栏、`check_docs.py` 通过（云端工作区实测）。
- [ ] 双宿主 CI 以 PR 为准。

## 4. 实施记录

| 提交 | 内容 |
|---|---|
| 本分支首提交 | 退役 `String.clone()` 旧路径；按第 2 节改写测试 |

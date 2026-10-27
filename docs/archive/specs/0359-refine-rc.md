# SPEC-0359: E3 `Rc` 构造、`share()` 与 `Rc.value` 投影迁入所有权细化层

> **性质**：实施合同 · **状态**：done · **读取时机**：评审或实施路线图 E3/N07 的 `Rc` lowering 时 · **唯一真源**：本片目标、范围与验收；语义以 Guide 为准，长期决定以 ADR-0015/0032 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0359` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.46 Runtime 与标准库](../../guide/13-program-runtime-standard-library.md) |
| 批准依据 | 2026-10-27 用户选择：N07 先做 `Rc`，`Box.value`/`unbox()` 待 L8 的 Guide 措辞确定后另开 Spec |
| 基线 | `feature/spec-0358-refine-assignment-inout` `f2711869` |
| 分支 | `feature/spec-0359-refine-rc` |
| 前置 | [SPEC-0358](0358-refine-assignment-inout.md)、[ADR-0015](../../adr/accepted/0015-shared-owner-runtime-abi.md)、[ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

细化层 lowering payload 为标量或普通 class 的 `Rc<T>`：构造（`SharedAllocate`）、`share()`
（`SharedRetain`）、Copyable payload 的 `value` 读取，以及 `rc.value.f` 经 payload place 的 shared
loan 读取字段。p22 在单文件与 project 两种模式通过，矩阵无回退、两模式差异为 0。

## 2. 设计约束

- 只消费既有事实：`IntrinsicRc` construction 与 shared-owner root obligation、`rc_operation`
  与唯一匹配的 `rc_effects`（Retain / BorrowPayload）、owner 的 drop 点；receiver 必须是 place 类别。
- owner 为已知局部或 Value 参数的名称；Borrow 参数、nullable `Rc` 与临时 receiver 留在子集外。
- 路由门：`Rc` 构造与 `Rc` 操作计入“已迁入家族”。
- `Box.value`/`unbox()` 不在本片（frontend 尚无事实，L8 待定）。

## 3. 验收标准

- [x] p22 两模式通过；`refine_tests::rc_construction_share_and_value_projections_release_once` native
  运行覆盖构造、`share()`、Copyable payload 的 `value`、`rc.value.f` 与 own `Rc` 参数，分配计数 3
  （两个 shared owner 与一个 Data payload 各释放一次）。
- [x] `refine_subset_tests` 锁定子集边界：String payload、nullable `Rc`、Borrow `Rc` 参数在子集外。
- [x] 矩阵：154 probe 能力通过 131/154（+1），回退 0、两模式差异 0、内部错误 0（本地 Linux）。
- [x] lang-codegen 全量测试（2448 项）、fmt、clippy、尺寸护栏、`check_docs.py` 通过（本地 Linux，stable 1.97）；
  双宿主 CI 以 PR 为准。

更新的既有测试（仅 IR 形状，行为断言不变）：

| 测试 | 原断言 | 更新原因 |
|---|---|---|
| `unit_lower_rc_tests::lowers_cross_file_rc_retain_copyable_read_transfer_and_drop_deterministically` | `Rc(number)` 先 `Copy` Copyable 参数再交付 | 该函数现由细化层 lowering，直接交付不可变的 SSA 值；`SharedAllocate` 交付、retain、payload place 与析构顺序断言不变 |

Borrow `Rc` 参数留在子集外：LLVM adapter 把 shared-owner 操作的 loan operand 当作 owner 指针本身，
而 root loan 指向 owner 的存储；统一这一约定另行处理。

## 4. 非目标

- `Box<T>` 的 `value`/`unbox()`（L8、N07 Box 部分）；`Rc` 的 nullable/泛型 payload；
- frontend 新事实。若发现事实缺口，停下另开 frontend Spec。

## 5. 实施记录

| 提交 | 内容 |
|---|---|
| 本分支提交 | `build/rc.rs`、`subset/rc.rs`；`SharedAllocate`/`SharedRetain`/`SharedPayloadPlace` 槽位操作；Borrow 参数接受普通 class（只经 loan 借出） |

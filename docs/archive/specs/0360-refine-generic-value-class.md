# SPEC-0360: E3 泛型实例与 value class 迁入所有权细化层

> **性质**：实施合同 · **状态**：done · **读取时机**：评审或实施路线图 E3 的泛型实例与 value class lowering 时 · **唯一真源**：本片目标、范围与验收；语义以 Guide 为准，长期决定以 ADR-0028/0032 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0360` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.46 类型与泛型](../../guide/03-types-generics.md)、[布局与构造](../../guide/11-copyability-layout-construction.md) |
| 批准依据 | 2026-10-27 用户排定 E3 后续顺序：第 4 项为“泛型 / value class（p40、t03）” |
| 基线 | `feature/spec-0359-refine-rc` `44d1e11c` |
| 分支 | `feature/spec-0360-refine-generic-value-class` |
| 前置 | [SPEC-0359](0359-refine-rc.md)、[ADR-0028](../../adr/accepted/0028-frontend-generic-body-type-normalization.md)、[ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

细化层接受泛型函数实例：typed 事实里的模板类型一律按实例的类型替换与 static `Self` 解析为
frontend 已发布的具体身份后再使用；并迁入 Copyable、字段全为标量的 `value class`（构造、按拷贝
读取字段、Value/Borrow receiver 方法）。t03 在单文件与 project 两种模式通过，矩阵无回退、两模式
差异为 0。p40 依赖容器家族（`MutableList.add` 与容器字段），不在本片。

## 2. 设计约束

- 具体化只经 `RefineInputs::concrete`（复用 `resolve_concrete_type`，不在 codegen 新建类型身份）；
  子集与构建器读取的表达式类型、symbol 类型以及描述符携带的类型都先具体化；无法解析的类型保持原样，
  子集据此拒绝、构建器以 MissingFact 失败。
- value class 首片只含 Copyable、字段全为标量者；字段读取对 owner 值做 `AggregateProject`，Borrow
  参数与 receiver loan 先按拷贝读出。
- 路由门：value class 的构造与字段读取计入“已迁入家族”；泛型实例与非泛型函数用同一路由门。

## 3. 验收标准

- [x] t03 两模式通过；`refine_tests` native 运行覆盖泛型 value class 方法、非泛型 value class 的
  构造/字段/隐式 `this`、泛型函数实例中的 `when`。
- [x] 非泛型函数的具体化为恒等：既有细化层测试不变；唯一改动的测试是 `unit_lower_aggregate_tests` 中原锁定“泛型 value class receiver 不支持”的边界测试，现改为正向渲染并通过 SSA 验证。
- [x] 受影响目标定向测试、fmt、clippy、尺寸护栏、`check_docs.py` 通过；双宿主 CI 以 PR 为准。

## 4. 非目标

- p40（容器家族）、MoveOnly 或含非标量字段的 value class、泛型 class 的堆布局；
- frontend 新事实。若发现事实缺口，停下另开 frontend Spec。

## 5. 实施记录

| 提交 | 内容 |
|---|---|
| 本分支首提交 | 具体化 typed 事实、Copyable 标量字段 value class（构造、`AggregateProject`/`SharedFieldLoan` 字段读取、Value/Borrow receiver）；t03 晋升 run_ok（132/154，0 回退） |

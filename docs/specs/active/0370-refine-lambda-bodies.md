# SPEC-0370：lambda 体（thunk）与局部 callable 绑定迁入所有权细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或修改 lambda thunk 体与局部 callable 绑定的细化层 lowering 时 · **唯一真源**：本片目标、范围与验收；语义以 Guide 07 为准，长期决定以 ADR-0032/0034 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0370` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.46 调用、Lambda 与 Closure](../../guide/07-calls-lambdas-closures.md) |
| 批准依据 | 2026-10-31 用户要求继续实施 E3 第 6 项：lambda 体迁入细化层并删除被替代的旧路径代码 |
| 基线 | main `0bb16e2a` |
| 分支 | `feature/spec-0370-refine-lambda-bodies` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0034](../../adr/accepted/0034-refine-closures-and-callable-views.md)、[SPEC-0364](../../archive/specs/0364-refine-closures.md)、[SPEC-0365](../../archive/specs/0365-closure-return-facts.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

按 [ADR-0034 第 1 节与第 5 节](../../adr/accepted/0034-refine-closures-and-callable-views.md) 完成 E3 第 6 项：

1. 将 lambda 体（thunk）迁入 `ssa::refine`，使 lambda 体与普通函数共用同一套子集预检、槽位 CFG 构建与 SSA 发射。
2. 在 `ssa::refine` 中补齐局部 callable 绑定（lambda 字面量初始化、局部移动别名、工厂返回绑定）、`CallableInvoke` 调用与向 `Borrow` 函数参数借出 view（`CallableViewOf`），以及闭包体内对捕获/借用 `String` 和类 `String` 字段的只读借用（`SharedHeapFieldLoan`）。
3. 删除 `unit_lower` 中已被细化层替代的 thunk 入口/返回捕获清理与函数参数单态化遗留实现（`bind_callable_entry`、`end_thunk_capture_views`、`CallableAbi::parameter_type`）。

## 2. 规则

- **thunk 入口与捕获投影**：有捕获 lambda 的首个入口参数为环境的 shared loan；`refine/build` 按捕获顺序发出 `SharedFieldLoan`（`Shared` 捕获紧跟 `SharedReferenceFollow`），将各捕获以只读借用登记到 `borrowed`，并消费 `LambdaEntry` 析构点。
- **thunk 返回清理**：在 `Return` 终结指令前按建立逆序发出捕获借用的 `BorrowEnd`（follow 子借用先于环境字段父借用结束）；`Abort` 分支与成功分支共享相同的活跃槽位边参数，不在 `Abort` 块内展開捕获借用。
- **局部 callable 绑定与调用**：`val f = { ... }`、`val moved = f` 及调用返回具体 callable 的绑定在 `refine` 中保持其具体 `FunctionPointer` 或 `ConcreteClosure` 类型；直接调用发出 `CallableInvoke`，传给 `Borrow` 函数参数时经 `RootPlace` + `BorrowBegin` + `CallableViewOf` 借出 view。
- **旧路径收敛**：`unit_lower` 的 `finish_thunk` 仅保留无捕获、无参数 `() -> Unit` 且含未迁移 Map 迭代家族的回退分支；删除 `bind_callable_entry`、`end_thunk_capture_views` 与 `CallableAbi::parameter_type`。

## 3. 验收标准

- [ ] `refine_closure_tests` 中所有 lambda `.thunk` 函数及工厂调用方 `main` 均断言经 `ssa::refine` 路由，新增 `local_callable_bindings_and_views_lower_in_refine` 覆盖局部 callable 绑定、移动别名、`CallableInvoke` 与借出 view。
- [ ] `unit_lower_closure_tests`、`unit_lower::closure::runtime_tests`、`unit_lower::closure::generic_runtime_tests`、`unit_lower::closure::runtime_slots_tests` 全量通过。
- [ ] `check_rust_sizes.py`、`check_docs.py`、fmt、clippy 通过。

## 4. 非目标

- `q03_fn_ref`（frontend 重载选择缺口 `FE:L0165`）另开 frontend Spec。
- Value 模式（`own`）函数参数与将闭包存入字段或容器，按 ADR-0034 第 4 节留到 `T1` 之前再定。
- 容器构造与 Map 迭代家族迁入细化层前，外层含容器/Map 迭代的函数体仍保留 `unit_lower` 的闭包形成入口。

## 5. 实施记录

| 提交 | 内容 |
|---|---|
| 本分支首提交 | `RefineBody::Lambda` 与 thunk 捕获投影/清理；局部 callable 绑定与 `CallableInvoke`；删除 `bind_callable_entry`、`end_thunk_capture_views`、`CallableAbi::parameter_type` |

# SPEC-0364：E3 闭包家族迁入所有权细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3/N04 的闭包 lowering 时 · **唯一真源**：本片目标、范围与验收；语义以 Guide 07 为准，长期决定以 ADR-0009/0032/0034 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0364` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.46 调用、Lambda 与 Closure](../../guide/07-calls-lambdas-closures.md) |
| 批准依据 | 2026-10-28 用户确认 ADR-0034（“按建议的方式来，然后要设计和做好逃逸分析”） |
| 基线 | `feature/spec-0363-closure-scope-escape` `bdccaace` |
| 分支 | `feature/spec-0364-refine-closures` |
| 前置 | [ADR-0034](../../adr/accepted/0034-refine-closures-and-callable-views.md)、[SPEC-0363](../../archive/specs/0363-closure-scope-escape.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

按 ADR-0034 让 p17、p17b、q02、p18 在单文件与 project 两种模式通过：lambda 体作为细化层 thunk，
closure 形成显式化，Borrow 函数参数使用 callable view，函数可返回 move closure。矩阵无回退、两模式
差异为 0，native 运行 ASan/LSan 干净。q03（frontend L0165）不在本片。

## 2. 分步

1. **SSA 层**：`CallableView` loan target 类型、`CallableViewOf`、`CallableInvokeView`；verifier 的
   类型、操作合同、reborrow 依赖与放置规则；LLVM 描述符、按签名的 function pointer trampoline；
   SSA 层逃逸/生命期反例。
2. **细化层 thunk 与形成**：lambda 体经同一 builder 生成 thunk（环境 shared loan、capture 只读、
   `LambdaEntry` 事实）；无捕获 `FunctionAddress`、有捕获 `ClosureConstruct`；shared capture loan
   在 closure drop 之后结束；Borrow 函数参数的实参经 `CallableViewOf` 传入、callee 经
   `CallableInvokeView` 调用（p17b、p17、q02）。
3. **closure 返回**：函数返回 concrete callable，调用方对 owned callable `CallableInvoke`（p18）。
4. 被取代的旧路径闭包模块随 E3 第 6 项删除。

## 3. 验收标准

- [x] 步 1：`verify_callable_view_tests` 覆盖 function pointer 与 concrete closure 两种 view 的
  verify + LLVM、源 loan 先结束、源 loan 结束后使用、非 callable 源、view 作 value 参数、view 作返回、
  view 存入 aggregate/inline nullable/owned capture、捕获 view 的 closure 逃逸，拒绝原因逐项断言。
- [ ] 步 2/3：p17、p17b、q02、p18 两模式通过；`refine_tests` native 运行覆盖直接实参 lambda、
  有/无捕获、view 继续下传、borrowed closure 捕获 view、move closure 返回与 owned String capture 析构。
- [ ] 受影响目标定向测试、fmt、clippy、尺寸护栏、`check_docs.py` 通过；双宿主 CI 以 PR 为准。

## 4. 非目标

- Value 模式函数参数、closure 存字段/容器、跨 layout join（ADR-0034 第 4 节）；递归 closure 环境
  （ADR-0025）；mutating closure；q03 的 overload 选择。

## 5. 实施记录

| 提交 | 内容 |
|---|---|
| 步 1 | SSA/LLVM callable view 与 verifier 反例 |

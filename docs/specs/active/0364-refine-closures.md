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
- [x] 步 2：p17、p17b、q02 两模式通过（门禁 136/154，0 回退）。规划器取消 Borrow 函数参数的回调
  单态化：callee 参数为 view，调用方的具体 callable 记入 `borrowed_arguments`；旧路径调用点经
  `CallableViewOf` 传 view，运行时长度构造器可直接调用 view 参数。`refine_closure_tests` native 运行覆盖
  无捕获/shared/owned 捕获的直接实参 lambda、view 转发与嵌套调用，分配计数确认 owned capture 随
  closure 析构恰好释放一次。
- [x] 步 3：p18 两模式通过（门禁 137/154，0 回退）。frontend 闭包返回来源事实由
  [SPEC-0365](../../archive/specs/0365-closure-return-facts.md) 提供；规划器把返回的 move closure 冻结为
  工厂的返回 ABI（concrete closure 类型），细化层直接返回 lambda 形成的 closure。`refine_closure_tests`
  native 运行覆盖返回捕获 String 与 class 的 move closure，分配计数确认 class 捕获随 closure 析构恰好释放一次。
- [ ] 受影响目标定向测试、fmt、clippy、尺寸护栏、`check_docs.py` 通过；双宿主 CI 以 PR 为准。

### 改动的既有测试（步 2）

ADR-0034 取消 Borrow 回调单态化后，下列测试原先断言“每种回调一个实例”，现改为断言单实例 + 每个
实参各自冻结的 callable：`unit_plan_tests/callable_identity` 中 8 项（三环境、别名/递归转发、
三层转发、具名实参、两项工厂返回、已知函数引用、冻结路由）；`generic_runtime_tests` 的 key 断言；
`unit_lower_container_tests` 原“view 参数初始化器不支持”的边界测试改为正向测试，float 元素边界单列。

## 4. 非目标

- Value 模式函数参数、closure 存字段/容器、跨 layout join（ADR-0034 第 4 节）；递归 closure 环境
  （ADR-0025）；mutating closure；q03 的 overload 选择。

## 5. 实施记录

| 提交 | 内容 |
|---|---|
| 步 1 | SSA/LLVM callable view 与 verifier 反例 |
| 步 2 | 规划器 view 化、旧路径调用点 view 实参、细化层 view 调用与 lambda 形成；p17/p17b/q02 |
| 步 3 | 合并 SPEC-0365；规划器冻结闭包返回 ABI；细化层返回 lambda；p18 |

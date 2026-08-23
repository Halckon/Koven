# SPEC-0176：迁移 borrow-default 参数契约

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `approved` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.26 callable 参数契约](../guide/05-grammar-calls-lambda.md#callable-参数契约与调用匹配) |
| 前置 Spec | SPEC-0012、SPEC-0067、SPEC-0173、SPEC-0028 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 参数 AST / Parser / strict trial、typed parameter facts、预声明 callable、既有所有权检查、fixture、Tree-sitter grammar、Architecture |
| 语言语义变更 | 否；实施 v0.26 已封闭的参数声明与调用契约 |
| 批准依据 | 用户明确启用 v0.26 并采用 borrow-default、声明侧显式 `own`、MoveOnly 调用点隐式移动；当前持续 Goal 的站立授权有效 |

## 2. Goal

在不增加第四种 typed 参数模式、不引入调用点 `own` 的前提下，把具名函数、函数类型、lambda
expected contract、预声明 callable 与既有所有权检查统一迁移到 v0.26：声明侧 `own` 规范化为
`ParameterMode::Value`，无 marker 或显式 `borrow` 规范化为 `Borrow`，`inout` 继续规范化为
`Inout`；向 `Value` 参数传递 MoveOnly place 时调用点保持无 marker 并隐式移动。

## 3. 范围与需求

- 参数源码 AST 接受 `own` / `borrow` / `inout` 三种声明 marker；marker 缺失表示 `Borrow`。
  `ParameterModeMarker::Own` 只保存真实源码 token 与 `Span`，进入 typed facts 时规范化为既有
  `ParameterMode::Value`，不得新增 `ParameterMode::Own`。
- 具名函数参数、函数类型参数、正式 TypeRef parser 与 strict typed-call trial 使用同一 marker
  字母表及恢复边界；重复 marker 继续产生 L0039，并覆盖 `own` / `borrow` / `inout` 的组合。
- 调用点语法不变：无 marker 可匹配 `Value` 或 `Borrow`，显式 `borrow` 只匹配 `Borrow`，`&`
  只匹配 `Inout`；调用点 `own` / `inout` 继续非法。`Value` 对 Copyable 实参交付 owned copy，
  对 MoveOnly place 隐式移动。
- named signature 与函数类型保留 `Value` / `Borrow` / `Inout` 三态身份；模式仍不参与 overload
  shape。显式 `borrow` 与无 marker Borrow 产生相同 typed contract，不能据此重载。
- lambda 源码参数仍不书写 marker；成功采用唯一 expected function type 后，继续按稳定
  `SymbolId` 发布其最终三态参数事实。
- 预声明 consuming callable（包括 intrinsic `Box`、列表式容器构造、`MutableList.add`、
  `thread` 与 `Sender.send`）登记 `Value`；只读输入登记 `Borrow`。调用源码不增加消费 marker。
- class-family 主构造器的 `val` / `var` 是直接建立存储的字段声明，继续按既有 Value 交付并且
  不接受 callable parameter marker；本 Spec 不引入 `own val` / `own var`。
- 既有所有权检查只把规范化后的 `Value` parameter 视为 owned binding，并只在已选 `Value`
  argument 上执行 copy/move；无 marker Borrow parameter 不得被误建 owner 或在调用时移动实参。
- Tree-sitter parameter mode、生成产物与 corpus 同步接受声明侧 `own`；argument mode 仍精确为
  `borrow` / `&`。TextMate 已把 `own` 分类为 ownership keyword，不改变其词法 scope。

## 4. 非目标

- 不实现调用期 shared/exclusive loan、借用冲突、L0133–L0135 或 ASAP drop facts；这些属于
  SPEC-0029。
- 不决定 instance member / interface delegate 的隐式 receiver mode，不实现顺序容器 index
  place、closure capture、move closure、`Transferable` 或跨调用 borrow。
- 不增加调用点 `own` / `move`，不把 `own` 变成 Pratt prefix，不新增引用类型、生命周期、
  `val` / `var` callable 参数模式或第四种 typed 参数模式。
- 不改写 SPEC-0012、SPEC-0067、SPEC-0173、SPEC-0027 或 SPEC-0028 的历史验收事实。

## 5. 验收标准

- [ ] 具名函数与函数类型 compile-pass 覆盖无 marker Borrow、显式 `borrow`、显式 `own`、
      `inout`、nested / move function type 与 strict typed-call；AST marker 和参数 `Span` 精确。
- [ ] declaration / TypeRef compile-fail 覆盖三个 marker 的重复与错误位置，L0039、恢复后的首个
      marker、后续参数及正式 Parser / strict trial 一致性均保持稳定。
- [ ] named 与 lambda 参数 typed facts 精确查询得到 `Value` / `Borrow` / `Inout`；无 marker 与
      显式 `borrow` 等价，`own` 不产生第四种模式，模式不参与 overload shape。
- [ ] 调用矩阵证明无 marker Borrow 不移动 MoveOnly 实参；显式 `own` callee 的无 marker 调用对
      Copyable 复制、对 MoveOnly 移动并由既有 L0131 拒绝后续使用；调用点 `own` 继续拒绝。
- [ ] 预声明 consuming / read-only callable、class constructor field 例外与结构投影所有权回归
      符合 v0.26，既有 L0119–L0132 含义和源码顺序不漂移。
- [ ] Tree-sitter corpus 接受声明侧 `own` 且仍拒绝调用点 `own`；生成 parser / node types 与生产
      Parser 交叉验收通过，TextMate keyword corpus 无回归。
- [ ] 受影响 frontend / editor 窄测和一次 workspace 标准基线通过；Architecture、roadmap、本
      Spec 验收与验证记录只陈述实际事实。

## 6. 技术方案与边界

- 扩展现有源码 `ParameterModeMarker`，并集中修改唯一 marker-to-`ParameterMode` 规范化函数；
  具名函数、函数类型、lambda 与 ownership checker 继续复用 `TypedFile` 参数事实，不回读源码
  拼写，也不建立第二套 callable contract。
- 调用选择继续复用 SPEC-0067 的映射和 marker compatibility；本次只改变 callee 声明如何产生
  typed mode，以及环境预声明的 mode 数据，不按函数名在调用点猜测消费行为。
- 所有源码顺序、AST identity、诊断聚合和生成 grammar 保持确定性；不新增依赖、crate、日志
  框架、LLVM 类型或全局可变状态。

## 7. 实施计划

1. [ ] 扩展声明 / 函数类型参数 marker 与 strict trial → 验证：Parser AST、L0039 与恢复窄测。
2. [ ] 集中迁移 typed mode 规范化、lambda expected facts 与预声明 callable → 验证：type/callable
       mode matrix。
3. [ ] 迁移既有所有权入口和 fixture → 验证：Borrow 不移动、Value copy/move 与 L0131/L0132。
4. [ ] 更新 Tree-sitter grammar / 生成产物 / corpus → 验证：editor 窄测与生产 Parser 交叉验收。
5. [ ] 同步 Architecture/roadmap/验收并运行 workspace 基线 → 验证：实际退出状态。
6. [ ] 暂存本 Spec 独立范围并审查 staged diff → 验证：无 SPEC-0029 loan/drop 实现或无关改动。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 参数源码/typed/ownership 契约迁移、测试、Tree-sitter、Architecture 与完成记录 | `feat(frontend): adopt borrow-default parameter contracts (SPEC-0176)` |

## 9. 未决问题

- 无；receiver、index 与 capture 已明确留给后续 Spec，不阻塞本次 callable 参数迁移。

## 10. 验证记录

尚未实施；不得预填通过。

# SPEC-0208：常量重新物化与所有权事实

> **性质**：draft Spec · **状态**：draft（blocked by implementation dependencies） · **读取时机**：实施或评审 v0.36 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P3-208` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.36 §36](../../../guide/05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值) |
| 批准依据 | 2026-09-12 用户明确启用 v0.36 并要求分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0028、0029 `done`；SPEC-0026 待完成 |
| 前置 ADR | 无 |
| 阻塞项 | SPEC-0026 `done` |
| 影响范围 | `lang-frontend` ownership/liveness/drop facts 与测试；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.36 materialization 契约 |

## 2. Goal

完成后，ownership checker 消费 SPEC-0026 facts，把 scalar/Char const use 视为 Copyable inline
value，把每个 String const use 视为从编译期 UTF-8 bytes 新建的普通 String temporary owner，
不移动常量声明、不产生全局 owner 或 capture obligation。

## 3. 范围与需求

- constant declaration 是 compile-time identity，不进入 runtime owner state、loan graph、closure
  environment 或 drop plan；任何 use 都不能把 declaration 标为 moved。
- constant initializer 内的 dependency use 只由 SPEC-0026 evaluator 消费，不产生 runtime
  materialization；本 Spec 只为普通运行时 expression 中的 constant use 发布 facts。
- Boolean/integer/Char use 是 Copyable value；String use 每次独立 materialize，按普通 String
  literal的 Value/Borrow/return/ASAP drop 规则交付，多个 use 不共享 owner。
- String materialization 的 temporary/root/drop facts 绑定具体 use expression；失败 analysis 不
  发布半成品 facts，顺序与既有 expression evaluation 一致。
- object/companion 只贡献关联命名空间；无 singleton owner、init guard、exit drop 或 hidden retain。

## 4. 非目标

- 不生成 SSA/LLVM，不实现 runtime global、跨文件分析、associated function 或 object receiver。

## 5. 验收标准

- [ ] scalar/Char const 可重复 Value-deliver 且没有 move/drop/loan；声明永不进入 moved state。
- [ ] 同一 String const 的重复 use、Borrow call、concat/equality、return 与分支产生独立 owner，
  每个正常路径精确 drop 一次。
- [ ] closure 引用 const 不产生 capture；object/companion 不产生 owner/init/drop facts。
- [ ] return/abort/branch/loop 下 String temporary 的 liveness/drop 与普通 literal 一致。
- [ ] validated marker、determinism 与现有 ownership/drop suite 回归。
- [ ] Architecture 与实现事实同步。

## 6. 技术方案与边界

在 ownership expression dispatch 最前消费 constant-use descriptor，复用现有 String literal
temporary/drop 机制；不把 constant symbol伪装成 local variable，也不建立第二套 String owner。

## 7. 实施计划

1. [ ] 建立 const-use materialization ownership facts → 验证：scalar/String model 测试。
2. [ ] 接 liveness/drop/capture → 验证：控制流与交付矩阵。
3. [ ] 同步验收与 Architecture → 验证：按[分层验收](../../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | ownership facts、测试与完成文档 | `feat(frontend): own constant materialization (SPEC-0208)` |

## 9. 未决问题

- 无；状态门禁由元数据表达。

## 10. 验证记录

实施前按[分层验收](../../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | 当前 constant initializer 仍走普通 runtime ownership flow；constant use 也没有 String temporary/materialization facts |

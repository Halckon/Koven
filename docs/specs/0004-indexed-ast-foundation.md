# SPEC-0004: 建立索引式 AST 基础

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P0-004` |
| 所属 Phase | Phase 0 |
| 语言规范 | [`agent-language-design-guide-v0.4.md`](../agent-language-design-guide-v0.4.md) |
| 前置 Spec | SPEC-0001、SPEC-0002 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无；仅因前置 Spec 未完成而暂不可实施 |
| 影响范围 | `lang-frontend`、Architecture |
| 语言语义变更 | 否 |

## 1. Goal

完成后，frontend 提供可人工构造、由 typed ID 访问且保留 `Span` 的索引式 AST 存储，后续
parser 可直接复用而无需持有自引用树。

## 2. 背景

现行 guide 已规定 AST 使用索引式节点。Phase 0 应先锁定节点身份、存储和范围不变量，具体
语言节点则由 Phase 1 的对应语法 Spec 按需增加。

## 3. 范围与需求

- 为 item、statement、expression、type reference 等类别建立不可混用的 typed ID。
- 使用拥有节点的 arena / table 保存数据，父子关系只保存 typed ID 或连续 ID slice。
- 每个 AST 节点都携带合法 `Span`；一个 AST file 明确关联其 `SourceId`。
- 索引访问经过受检 API；越界 ID 返回内部错误，不从用户输入路径触发无说明 panic。
- 为人工构造的最小 AST file 提供只读遍历和确定性 debug / test 表示。
- 节点数据不包含 LLVM 类型、codegen handle、名称解析结果或隐式全局状态。

## 4. 非目标

- 不提前定义 Phase 1 全部语法节点、Pratt precedence、错误恢复或 parser API。
- 不建立 HIR / MIR、visitor 框架、通用图数据库或跨 arena 抽象层。
- 不为未来增量编译固定稳定磁盘 ID 或序列化格式。

## 5. 验收标准

- [ ] 不同节点类别的 ID 在 Rust 类型层面不可混用。
- [ ] 人工构造的父子节点可按 ID 稳定读取，并保留原 `SourceId` / `Span`。
- [ ] 越界 ID 被受检 API 拒绝；同一节点类别的 ID 不承诺携带 arena identity。
- [ ] debug / test 表示不包含机器路径、地址或随机顺序。
- [ ] AST 模块不依赖 codegen、LLVM 或外围工具 crate。
- [ ] 受影响 crate 的窄测试及 workspace fmt、check、Clippy、test 基线通过。
- [ ] Architecture 记录 AST 数据所有权与阶段边界。

## 6. 技术方案与边界

优先采用薄 typed newtype + `Vec` 存储，不引入第三方 arena 依赖，直到具体性能或生命周期
问题出现。Phase 0 只提供节点存储骨架和最小 AST file；节点 enum 随 Phase 1 Spec 增量增加，
避免为尚未实现的语法制造大而空的模型。

## 7. 实施计划

1. [ ] 实现 typed ID、arena 和受检索引 API → 验证：编译期类型约束与边界单测
2. [ ] 实现最小 AST file、带 Span 节点和只读遍历 → 验证：人工 AST 单测
3. [ ] 审计依赖与确定性表示 → 验证：依赖图、golden / debug 断言
4. [ ] 更新 Architecture 和 Spec 验收记录 → 验证：全 workspace 基线

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 索引式 AST 存储、测试与 Architecture | `feat(frontend): add indexed AST foundation (SPEC-0004)` |

## 9. 未决问题

- 具体节点枚举、parser 恢复节点以及 AST 到语义模型的边界由相应 Phase 1 / 2 Spec 决定。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 〈实施时填写〉 | 未执行 | 当前仅完成 Draft Spec |

# SPEC-0197：跨文件类型检查

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P2-197` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 v0.31；候选 v0.32 §32 |
| 批准依据 | 无；v0.32 尚未启用 |
| 前置 Spec | SPEC-0020、0021、0174、0177 `done`；SPEC-0025 待完成 |
| 前置 ADR | ADR-0020 待接受 |
| 阻塞项 | v0.32 启用；SPEC-0025 `done`；ADR-0020 `accepted` |
| 影响范围 | `lang-frontend` compilation-unit type environment/facts、fixtures；Architecture/Roadmap |
| 语言语义变更 | 否 |

## 2. Goal

完成后，所有 source unit 的公开/内部签名与 body 在同一声明身份图上完成类型检查，跨文件
call、nominal、generic、constructor、enum 与 interface 引用发布可供 ownership 消费的确定 typed facts。

## 3. 范围与需求

- 分离 unit-wide signature collection 与 per-body checking，支持合法递归引用且不依赖文件顺序。
- 所有跨文件类型和 callable target 通过 `DeclarationId` 解析，复用现有 overload、generic
  instance、constructor 和 flow typing 规则。
- 同一输入顺序置换产生相同 type/declaration/instance identity 与诊断。
- 任一文件有名称或签名错误时，不向 ownership 发布伪完整 typed unit。

## 4. 非目标

- 不新增类型语义，不实现跨文件所有权、codegen、manifest 或 LSP 生命周期。

## 5. 验收标准

- [ ] 正例覆盖跨文件函数、名义类型、泛型、constructor、enum/interface 与同 package 递归签名。
- [ ] 反例覆盖不可见/未解析目标后的级联抑制、签名冲突、body 类型错误与精确跨文件关联 Span。
- [ ] 文件顺序置换和单文件回归 suite 通过。
- [ ] frontend/workspace 基线与 Architecture 同步。

## 6. 技术方案与边界

消费 SPEC-0025 的完整 name product，构造 unit-wide `TypeEnvironment` 和每声明 typed body；不在
类型阶段重新展开 import。既有单文件 API 包装为单 source-unit 输入。

## 7. 实施计划

1. [ ] 收集跨文件签名与类型 identity → 验证：递归/冲突矩阵。
2. [ ] 接 body、overload/generic/constructor facts → 验证：跨文件正反 suite。
3. [ ] 同步 Architecture 并跑 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | unit-wide signatures 与 body typed facts | `feat(frontend): type check compilation units (SPEC-0197)` |

## 9. 未决问题

- 无。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap 审计 | 通过 | 补齐 0025 与 ownership/codegen 之间缺失的 Phase 2 层 |

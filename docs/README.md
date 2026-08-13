# Koven 文档

Koven 以版本化语言规范为语义基础，并通过 Spec、ADR 和 architecture 推进实现。详细规则见
[`AGENTS.md`](./AGENTS.md)。

## 当前入口

- [语言设计指南 v0.7](./agent-language-design-guide-v0.7.md)：当前语言语义及其中强制实现、
  Phase 边界的真源。
- [Specs 与路线图](./specs/)：单次功能或行为变更的范围、Goal、计划、依赖和验收标准。
- [ADR](./adr/)：长期架构选择及其理由、替代方案与代价。
- [Architecture](./architecture/)：仓库当前已实现架构的最新快照。

## 工作流

```text
现行语言规范
    ↓
Draft Spec：定义做什么与如何验收
    ↓（存在长期架构选择时）
ADR：在批准 Spec 前记录为什么这样选
    ↓
实现与测试
    ↓
Architecture：同步最终已落地事实
    ↓
Spec：验收完成并标记 done
    ↓
独立提交：关联 SPEC-NNNN
    ↓
Goal：提交成功后标记完成
```

当前仓库已完成 Phase 0 并进入 Phase 1；SPEC-0006 已建立确定性 Lexer，SPEC-0007 已建立
独立表达式 Parser。声明至完整文件的 Parser 仍按后续 Specs 推进。
各编译阶段的实际状态见 [架构快照](./architecture/README.md)。

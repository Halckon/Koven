# Koven 文档

Koven 以版本化语言规范为语义基础，并通过 Spec、ADR 和 architecture 推进实现。详细规则见
[`AGENTS.md`](./AGENTS.md)。

## 当前入口

- [语言设计指南 v0.25](./guide/00-index.md)：当前语言语义及其中强制实现、Phase 边界的
  多文档真源。
- [历史单文件 guide](./agent-language-design-guide-v0.12.md)：v0.12 历史候选及更早版本的
  不可变历史快照；v0.11、v0.12 仅用于验证已合入 v0.14 的内容，不参与现行语义优先级。
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

当前仓库已完成 Phase 0 与 Phase 1，并以 SPEC-0018 进入 Phase 2；SPEC-0006 已建立确定性 Lexer，SPEC-0007 与
SPEC-0008 已分别建立独立表达式和声明 Parser，SPEC-0009 已建立 block / statement 序列及
函数 block body Parser，SPEC-0010 至 SPEC-0014 已实现 lambda literal、具名
函数隐式 `Unit` 返回标注、callable 参数 marker / typed call argument，以及 block /
lambda body 内的局部 `val` 解构、完整文件组合与跨声明恢复、Kotlin 风格的
`package` / `import` 文件头和 control-flow。其余语法仍按后续 Specs 推进。
各编译阶段的实际状态见 [架构快照](./architecture/README.md)。

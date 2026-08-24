# Koven 文档

Koven 以版本化语言规范为语义基础，并通过 Spec、ADR 和 architecture 推进实现。详细规则见
[`AGENTS.md`](./AGENTS.md)。

## 当前入口

- [语言设计指南 v0.27](./guide/00-index.md)：当前语言语义及其中强制实现、Phase 边界的
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

当前仓库已完成 Phase 0、Phase 1 与无 guide 门禁的 Phase 2 主线，并已进入 Phase 3：整变量
use-after-move、条件复制、消费式解构、禁止结构分量部分移动、v0.26 borrow-default
参数契约、调用期 loan 与 owned-value ASAP 析构点已经实现；顺序容器 element place 的核心
读取/借用/替换所有权也已实现；Phase 5 容器 relocation API、closure capture 与
`Transferable` 仍待后续 Spec。Phase 6 已独立提供
TextMate 与 Tree-sitter grammar。各编译阶段的准确状态见[架构快照](./architecture/README.md)。

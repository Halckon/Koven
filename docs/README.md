# Koven 文档

Koven 以版本化语言规范为语义基础，并通过 Spec、ADR 和 architecture 推进实现。详细规则见
[`AGENTS.md`](./AGENTS.md)。

## 当前入口

- [语言设计指南 v0.32](./guide/00-index.md)：当前语言语义及其中强制实现、Phase 边界的
  多文档真源；v0.32 已由用户明确启用并取代 v0.31。
- [历史单文件 guide](./agent-language-design-guide-v0.12.md)：v0.12 历史候选及更早版本的
  不可变历史快照；v0.11、v0.12 仅用于验证已合入 v0.14 的内容，不参与现行语义优先级。
- [Specs 与路线图](./specs/)：单次功能或行为变更的范围、Goal、计划、依赖和验收标准。
- [ADR](./adr/)：长期架构选择及其理由、替代方案与代价。
- [Architecture](./architecture/)：仓库当前已实现架构的最新快照。

## 设计审计（非规范）

以下文档用于评估现状和规划后续工作，不启用语言语义，也不替代 guide、Spec、ADR 或
Architecture：

- [Koven 语言设计审计（v0.32）](./language-design-audit-v0.32.md)：现状、关键风险、设计门禁与
  编译流水线结论；
- [Kotlin 语法语义与 Rust 所有权对照清单](./kotlin-rust-design-matrix-v0.32.md)：逐项说明已实现、
  部分实现、候选、延后和不支持能力；
- [v0.32 后续语言与编译器开发路线](./post-v0.32-development-roadmap.md)：系统编程、FFI、标准库、
  并发/异步、优化、元编程和自举的依赖波次与验收门禁。

这三份材料是 2026-08-30 的冻结审计快照，不随每个 Spec 持续改写；实时状态与实施顺序仍以
guide、Specs/ADR 和 Architecture 为准。现行 guide 版本变化后，应把本组标为历史审计或由新版
审计取代。

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

当前仓库已完成 Phase 0、Phase 1 与无 guide 门禁的 Phase 2/Phase 3 主线：整变量
use-after-move、条件复制、消费式解构、禁止结构分量部分移动、v0.26 borrow-default
参数契约、调用期 loan 与 owned-value ASAP 析构点已经实现；顺序容器 element place 的核心
读取/借用/替换所有权也已实现；v0.27 的简化 closure capture、逃逸/owned capture 诊断、
capture loan/drop、结构化 `Transferable` 与 compiler-bound 跨线程 effect 也已实现。Phase 4
已完成 typed SSA/verifier、标量 frontend→SSA→AArch64 LLVM IR，以及聚合/heap-owner、系统
allocation、递归 drop/free、顺序容器连续缓冲区/checked-index/drop 后端基元，以及真实
LLDB Koven 源码断点命中；SPEC-0183/0188/0184 已完成源码 constructor 的 typed、ownership 与
SSA/LLVM native 闭环。Phase 5 已发布标准 `error()` Abort、首个 literal-only
`println(String)` stdout/Hello World 闭环，以及目标语言 `Pair` / `Result` 的条件复制、构造、
投影与解构验收；一般 String runtime 已由 SPEC-0192 完成，容器 relocation API 仍待后续
Spec。Phase 6 已提供
公开单文件显式 entry 与零参数 conventional `main` 的 `kovenc build/run`、机器可读诊断、单文档 LSP diagnostics/definition、
formatter，以及 TextMate 与 Tree-sitter grammar。各编译阶段的准确状态见
[架构快照](./architecture/README.md)。

进入 typed SSA 前所需的泛型 callable 实例化与 overload-lambda 隔离，已分别由
[SPEC-0177](./specs/0177-generic-callable-instantiation.md) 和
[SPEC-0174](./specs/0174-overload-lambda-candidate-isolation.md)。v0.29 已解除两者的 guide
门禁并完成实施。

v0.29 constructor 主线已拆分并完成为
[SPEC-0183](./specs/0183-constructor-typed-facts.md) typed facts、
[SPEC-0188](./specs/0188-constructor-ownership-effects.md) ownership effects 与
[SPEC-0184](./specs/0184-nominal-construction-lowering.md) SSA/LLVM lowering。

Phase 4 的独立架构门禁已由
[ADR-0006](./adr/0006-typed-ssa-block-parameters.md) 封闭：自建 typed SSA 使用 IR-local type、
block parameters 与显式 ownership effects。对应
[SPEC-0033](./specs/0033-typed-ssa-ir-verifier.md) 与后续 SPEC-0034/0035/0036 均已完成实施；后续
Phase 4 Goal 仍按各自 guide、Spec 与 ADR 门禁推进。

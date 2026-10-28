# SPEC-0363：borrowed closure 的词法 scope 逃逸诊断

> **性质**：实施合同 · **状态**：done · **读取时机**：评审或修改 compilation-unit 所有权检查中 closure loan 与词法 scope 的交互时 · **唯一真源**：本片目标、范围与验收；语义以 Guide 07/10 为准，长期决定以 ADR-0034 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P3-0363` |
| 所属 Phase | Phase 3（ownership checking） |
| 语言规范 | [Guide v0.46 调用、Lambda 与 Closure](../../guide/07-calls-lambdas-closures.md)、[所有权、借用与析构](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 2026-10-28 用户确认 ADR-0034 并要求“设计和做好逃逸分析”；本片是 ADR-0034 第 6 节第一层的漏报修复 |
| 基线 | main `e0127ffb` + ADR-0034 |
| 分支 | `feature/spec-0363-closure-scope-escape` |
| 前置 | [ADR-0034](../../adr/accepted/0034-refine-closures-and-callable-views.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否（补齐 Guide 已规定的 L0135：loan 存活期间来源不能被析构） |

## 1. Goal

borrowed closure 的 shared capture loan 持续到 closure 的 ASAP drop point。closure 经赋值或 block
结果流到外层 binding 后，被它借用的 scope 内 binding 在 scope 结束时就要析构，loan 因此比来源活得
更久。实测 main 接受这类源码（例如 `loop { val local = "x"; saved = { read(local) }; break }` 之后
`saved()`），drop 事实在 `break` 处析构 `local`，属于 use-after-free。本片让 compilation-unit
所有权检查报 L0135。

## 2. 规则

- 每个词法 scope（block、control body，含 lambda 体）结束时，对正常出口、`break`、`continue`
  三条流分别检查：外层 binding（非本 scope 声明的 binding）或 scope 结果表达式所持有的 closure，
  以及经 capture 可达的 closure，若仍持有对本 scope binding 的 shared capture loan，报 L0135。
- primary 是 capture 引用，label 指向被借 binding 的声明。
- 直接持有的 closure 的逃逸是根因；只有不存在直接逃逸时，才报经 capture 间接可达的 loan。
  每处逃逸只报一次，报出后该 loan 不再约束这条流，避免级联诊断。

## 3. 验收标准

- [x] `crates/lang-frontend/tests/closure_scope_escape.rs`：6 个拒绝（`break`、`continue`、`if` 分支、
  block 结果、外层 closure 借用 scope 内 closure、move closure 间接携带）与 4 个合法邻居
  （scope 内使用、捕获外层来源、move 捕获、scope 结束前替换）全部通过，拒绝项恰好一条诊断且位置正确。
- [x] lang-frontend 全量测试通过；conformance 无回退；fmt、clippy、尺寸护栏、`check_docs.py` 通过。

## 4. 非目标

- 旧 single-file checker（D10 退役中）不补同样的规则；codegen 不推断逃逸。

## 5. 实施记录

| 提交 | 内容 |
|---|---|
| 本分支首提交 | `dataflow/scope_escape.rs`：scope 出口检查；接入 block 与 control body；回归矩阵 |

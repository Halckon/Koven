# Spec 依赖图

> **性质**：生成物（勿手改） · **状态**：current · **读取时机**：追溯 Spec 依赖拓扑时 · **唯一真源**：各 Spec 正文

由 `scripts/gen_spec_dag.py` 生成；只画拓扑结构，不含验收状态；状态见[README](README.md)。
重建时机：guide 版本启用或新增/迁移 draft Spec。
SVG 版本：[dependency-graph.svg](dependency-graph.svg)。

```mermaid
flowchart TD
ARCH(("已完成<br/>archive 215 份"))
subgraph Gactive["现行 active"]
  S0182["S0182<br/>顺序容器 for frontend→SSA→native 集成"]
  S0228["S0228<br/>Linux x86_64 本机目标与基线验收"]
  S0229["S0229<br/>扩展数值字面量值的端到端闭合"]
  S0230["S0230<br/>递归 Box enum 的 native 构造与析构"]
  S0231["S0231<br/>上下文 TypeRef 与严格调用试探一致性"]
  S0232["S0232<br/>原子置换原语的可信类型事实"]
  S0233["S0233<br/>Parser 工程合同的保全文档迁移"]
  S0234["S0234<br/>普通 block 的换行表达式边界"]
  S0235["S0235<br/>三项批准规则在真实 clone-first 基线启用"]
  S0236["S0236<br/>String.clone 显式深拷贝端到端"]
  S0237["S0237<br/>八阶段本地整合与交叉契约验证"]
  S0238["S0238<br/>Guide 勘误与可执行 Litmus 前端门禁"]
  S0239["S0239<br/>Linux CI 与双宿主定向回归门禁"]
  S0240["S0240<br/>整数具名位运算与取反端到端执行"]
  S0241["S0241<br/>Return 控制表达式操作数与单文件 enum 条件"]
  S0242["S0242<br/>调用点自动借用迁移"]
  S0245["S0245<br/>具体普通 class 的资源析构闭环"]
end
ARCH --> S0182
ARCH --> S0228
ARCH --> S0236
ARCH --> S0245
```

## 节点链接

| 节点 | 分区 | 文档 |
|---|---|---|
| SPEC-0182 | active | [0182-sequential-for-lowering.md](active/0182-sequential-for-lowering.md) |
| SPEC-0228 | active | [0228-linux-x86-64-native-host.md](active/0228-linux-x86-64-native-host.md) |
| SPEC-0229 | active | [0229-extended-numeric-literal-values.md](active/0229-extended-numeric-literal-values.md) |
| SPEC-0230 | active | [0230-recursive-boxed-enum-native.md](active/0230-recursive-boxed-enum-native.md) |
| SPEC-0231 | active | [0231-contextual-type-ref-trials.md](active/0231-contextual-type-ref-trials.md) |
| SPEC-0232 | active | [0232-ownership-primitive-type-facts.md](active/0232-ownership-primitive-type-facts.md) |
| SPEC-0233 | active | [0233-parser-compiler-contracts.md](active/0233-parser-compiler-contracts.md) |
| SPEC-0234 | active | [0234-block-newline-continuation.md](active/0234-block-newline-continuation.md) |
| SPEC-0235 | active | [0235-approved-language-rules.md](active/0235-approved-language-rules.md) |
| SPEC-0236 | active | [0236-explicit-string-clone.md](active/0236-explicit-string-clone.md) |
| SPEC-0237 | active | [0237-local-integration.md](active/0237-local-integration.md) |
| SPEC-0238 | active | [0238-guide-litmus-gate.md](active/0238-guide-litmus-gate.md) |
| SPEC-0239 | active | [0239-linux-ci-gates.md](active/0239-linux-ci-gates.md) |
| SPEC-0240 | active | [0240-integer-bitwise-execution.md](active/0240-integer-bitwise-execution.md) |
| SPEC-0241 | active | [0241-return-control-operands.md](active/0241-return-control-operands.md) |
| SPEC-0242 | active | [0242-automatic-borrow-call-migration.md](active/0242-automatic-borrow-call-migration.md) |
| SPEC-0245 | active | [0245-resource-deinit.md](active/0245-resource-deinit.md) |
| 已完成 Spec（215 份） | archive | [archive/specs/README.md](../archive/specs/README.md) |

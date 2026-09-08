# AGENTS.md — lang-codegen

本 crate 消费 frontend 已验证事实，负责 typed SSA、LLVM 与 native 产物；不得自行补语言语义。

## 按任务读取

| 修改内容 | 必读文档（每行最多四份） |
|---|---|
| unit planning / 通用 lowering | [SSA 架构](../../docs/architecture/ssa-codegen-runtime.md)、[guide 索引](../../docs/guide/README.md)；从索引只追加一个直接相关领域页 |
| SSA model / verifier | [SSA 架构](../../docs/architecture/ssa-codegen-runtime.md)、[所有权规则](../../docs/guide/10-ownership-borrowing-drop.md)、[ADR-0006](../../docs/adr/accepted/0006-typed-ssa-block-parameters.md) |
| 名义布局 / drop | [SSA 架构](../../docs/architecture/ssa-codegen-runtime.md)、[布局与构造](../../docs/guide/11-copyability-layout-construction.md)、[所有权](../../docs/guide/10-ownership-borrowing-drop.md)、[ADR-0008](../../docs/adr/accepted/0008-internal-value-and-allocation-abi.md) |
| closure | [SSA 架构](../../docs/architecture/ssa-codegen-runtime.md)、[Closure 规则](../../docs/guide/07-calls-lambdas-closures.md)、[ADR-0009](../../docs/adr/accepted/0009-concrete-closure-internal-abi.md) |
| Rc | [SSA 架构](../../docs/architecture/ssa-codegen-runtime.md)、[Runtime 规则](../../docs/guide/13-program-runtime-standard-library.md)、[ADR-0015](../../docs/adr/accepted/0015-shared-owner-runtime-abi.md) |
| Borrow / receiver | [SSA 架构](../../docs/architecture/ssa-codegen-runtime.md)、[所有权](../../docs/guide/10-ownership-borrowing-drop.md)、[成员规则](../../docs/guide/08-class-family-members.md)、[ADR-0016](../../docs/adr/accepted/0016-interprocedural-borrow-abi.md) |
| nullable | [SSA 架构](../../docs/architecture/ssa-codegen-runtime.md)、[空安全](../../docs/guide/09-nullability-errors.md)、[ADR-0017](../../docs/adr/accepted/0017-nullable-handle-ssa-abi.md) |
| String / argv | [SSA 架构](../../docs/architecture/ssa-codegen-runtime.md)、[Runtime 规则](../../docs/guide/13-program-runtime-standard-library.md)、[ADR-0018](../../docs/adr/accepted/0018-string-owner-runtime-abi.md)、[ADR-0019](../../docs/adr/accepted/0019-parameterized-process-entry-bridge.md) |
| target / object / link | [流水线](../../docs/architecture/pipeline-and-workspace.md)、[工具架构](../../docs/architecture/tooling.md)、[ADR-0007](../../docs/adr/accepted/0007-llvm-toolchain-and-first-target.md)、[ADR-0010](../../docs/adr/accepted/0010-first-native-object-and-linker-contract.md) |
| debug info | [工具架构](../../docs/architecture/tooling.md)、[ADR-0011](../../docs/adr/accepted/0011-first-dwarf-line-mapping.md) |

## 边界与验证

- frontend facts 是 lowering 的输入合同；缺失 descriptor 必须 fail loud，不从 AST 猜测。
- typed SSA 保持 IR-local 类型、显式 block parameter 和 ownership effects；先通过 verifier 再生成 LLVM。
- LLVM 类型和 ABI 细节不得泄漏到 frontend；目标布局检查在 LLVM 复合类型构造前完成。
- 最近测试位于 `src/ssa/**/*tests*`、`src/llvm/**/*tests*` 和 `native_tests`。改 native 行为时除 IR
  断言外还要执行对应 build/run；目标工具缺失时报告阻塞而不伪造通过。

# AGENTS.md — lang-std

`koven/**/*.ko` 是标准库公共实现真源；Rust library 只维持 Cargo package 和源码资产测试。

## 按任务读取

| 修改内容 | 必读文档 |
|---|---|
| Koven prelude / API | [Runtime 与标准库语义](../../docs/guide/13-program-runtime-standard-library.md)、[SSA/Runtime 事实](../../docs/architecture/ssa-codegen-runtime.md) |
| 所有权相关 API | [Runtime 与标准库语义](../../docs/guide/13-program-runtime-standard-library.md)、[所有权规则](../../docs/guide/10-ownership-borrowing-drop.md)、[所有权事实](../../docs/architecture/ownership.md) |
| String runtime 边界 | [Runtime 语义](../../docs/guide/13-program-runtime-standard-library.md)、[SSA/Runtime 事实](../../docs/architecture/ssa-codegen-runtime.md)、[ADR-0018](../../docs/adr/accepted/0018-string-owner-runtime-abi.md) |
| Rc runtime 边界 | [Runtime 语义](../../docs/guide/13-program-runtime-standard-library.md)、[SSA/Runtime 事实](../../docs/architecture/ssa-codegen-runtime.md)、[ADR-0015](../../docs/adr/accepted/0015-shared-owner-runtime-abi.md) |

## 边界与验证

- 标准库 API 必须先由现行 guide 授权；不因 runtime 已有 helper 就扩张语言表面。
- Koven 源码遵循 Koven 命名和所有权规则；不得用 Rust shim 替代应由 `.ko` 实现的公共逻辑。
- runtime intrinsic 身份与普通同名源码声明严格区分。
- 修改 prelude 后运行 `lang-std` 源码资产测试以及相关 CLI/codegen native 正反例；未实现候选能力
  不得通过占位 API 伪装可用。

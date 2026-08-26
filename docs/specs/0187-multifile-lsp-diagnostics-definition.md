# SPEC-0187：跨文件 LSP 诊断与跳转定义

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P6-187` |
| 所属 Phase | Phase 6 |
| 语言规范 | 现行 v0.31；候选 v0.32 §32 |
| 批准依据 | 无；v0.32 尚未启用 |
| 前置 Spec | SPEC-0055、0056 `done`；SPEC-0025、0197、0198 待完成 |
| 前置 ADR | ADR-0020 待接受 |
| 阻塞项 | v0.32 启用；0025/0197/0198 `done`；ADR-0020 `accepted` |
| 影响范围 | `lang-lsp` workspace/source-set state，frontend API integration，LSP tests；Architecture/Roadmap |
| 语言语义变更 | 否 |

## 2. Goal

完成后，LSP 对显式打开的 source-set compilation unit 发布跨文件 package/import、类型和所有权
诊断，并可从 import、限定名和普通引用跳转到其他文件的精确声明 Span。

## 3. 范围与需求

- LSP 复用 frontend compilation-unit products；打开 buffer 以内存文本覆盖同 logical source unit。
- 文件 open/change/close 后重建一致 unit snapshot，清除失效诊断并发布确定结果。
- definition 使用 `DeclarationId -> SourceId/Span`，覆盖 exact/alias/wildcard、限定名和同 package 引用。
- URI/position 转换继续复用现有 UTF-16/SourceMap 边界，不按路径字符串重新解析 package。

## 4. 非目标

- 不实现 manifest discovery、依赖下载、增量数据库、rename/references/completion 或跨依赖项目跳转。

## 5. 验收标准

- [ ] 多文件 open/change/close 测试覆盖诊断新增、迁移、清除与确定排序。
- [ ] definition 覆盖 exact alias、wildcard、限定名、同 package 及 private/inaccessible 反例。
- [ ] LSP 不包含第二套 import resolver；lang-lsp/workspace 基线与 Architecture 同步。

## 6. 技术方案与边界

在现有单文档 store 上增加显式 source-set snapshot adapter；语义查询全部来自 frontend unit
product。项目 manifest discovery 由后续 SPEC-0052/0054 提供，不是本 Spec 的隐式输入。

## 7. 实施计划

1. [ ] 建立 source-set snapshot 与 buffer overlay → 验证：open/change/close 测试。
2. [ ] 接跨文件 diagnostics/definition → 验证：LSP 正反矩阵。
3. [ ] 同步 Architecture 并跑 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 跨文件 diagnostics 与 definition | `feat(lsp): resolve multifile definitions (SPEC-0187)` |

## 9. 未决问题

- 无。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap 审计 | 通过 | 依赖已扩展为完整 name/type/ownership frontend 链 |

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
| 阻塞项 | v0.32 启用；0025/0197/0198 `done`；ADR-0020 `accepted`；完整 base source-set provider 契约待决 |
| 影响范围 | `lang-lsp` workspace/source-set state，frontend API integration，LSP tests；Architecture/Roadmap |
| 语言语义变更 | 否 |

## 2. Goal

完成后，LSP 对显式打开的 source-set compilation unit 发布跨文件 package/import、类型和所有权
诊断，并可从 import、限定名和普通引用跳转到其他文件的精确声明 Span。

## 3. 范围与需求

- LSP 复用 frontend compilation-unit products；推荐由显式完整 base source set 加打开 buffer overlay
  形成 unit，close 后回退到 base text，而不是把“当前打开的文件集合”误作完整 package。
- snapshot 共同拥有一个 `SourceMap` 与 name/type/ownership products；文件 open/change/close 后整体
  替换，绝不混用新旧 `map_id` 的 Span。失败的内部分析保留 last-good snapshot 并记录内部错误。
- 每次 unit 变化可影响所有 source；按稳定 source key 重新发布/清除所有受影响 URI 的诊断。
- definition 使用 `DeclarationId -> SourceId/Span`，覆盖 exact/alias/wildcard、限定名和同 package 引用。
- definition query 以 `(SourceUnitId, byte offset)` 查 reference fact；exact import 的 terminal/alias、
  普通与限定引用跳转到声明。wildcard 的 `*` 与纯 package segment 不提供定义，实际使用名跳转到目标。
- URI/position 转换继续复用现有 UTF-16/SourceMap 边界，不按路径字符串重新解析 package。
- diagnostic adapter 按 primary `SourceId` 分组到 URI，related location 分别按自己的 SourceId 映射；
  映射不完整时不得发布一个看似完整的部分结果。

## 4. 非目标

- 不实现 manifest discovery、依赖下载、增量数据库、rename/references/completion 或跨依赖项目跳转。

## 5. 验收标准

- [ ] base+overlay provider 确定后，多文件 open/change/close 测试覆盖诊断新增、迁移、清除与确定排序。
- [ ] definition 覆盖 exact alias、wildcard、限定名、同 package 及 private/inaccessible 反例。
- [ ] LSP 不包含第二套 import resolver；lang-lsp/workspace 基线与 Architecture 同步。

## 6. 技术方案与边界

以 unit snapshot store 取代“每 URI 一个独立 frontend 分析”的语义 store；语义查询全部来自
frontend unit product。项目 manifest discovery 由后续 SPEC-0052/0054 提供，不是本 Spec 的
隐式输入。在 provider 决定前，本 Spec 只记录推荐的 base+overlay 模型，不进入 approved。

## 7. 实施计划

1. [ ] 建立 source-set snapshot 与 buffer overlay → 验证：open/change/close 测试。
2. [ ] 接跨文件 diagnostics/definition → 验证：LSP 正反矩阵。
3. [ ] 同步 Architecture 并跑 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 跨文件 diagnostics 与 definition | `feat(lsp): resolve multifile definitions (SPEC-0187)` |

## 9. 未决问题

- 完整 base source set 由项目 manifest、LSP 初始化参数还是独立 host provider 给出；必须保证
  open/change 是 overlay、close 可回退，且不读取未授权文件系统。该选择需要在批准本 Spec 前
  由 SPEC-0052 或独立 ADR 封闭。
- 首版是否只支持一个 compilation unit 与 `file:` URI；untitled/non-file URI 的 source key 和
  unit membership 也必须在批准前明确，不能按 URI 字符串临时猜测。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap 审计 | 通过 | 依赖已扩展为完整 name/type/ownership frontend 链 |

# SPEC-0060：发布版本化机器可读诊断

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P6-060` |
| 所属 Phase | Phase 6 |
| 语言规范 | 现行 [`guide/00-index.md`](../guide/00-index.md) v0.28；[`guide/06-roadmap.md`](../guide/06-roadmap.md) Phase 6 |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |
| 前置 Spec | SPEC-0003、SPEC-0055 `done` |
| 前置 ADR | [ADR-0014](../adr/0014-versioned-machine-diagnostics.md) `accepted` |
| 关联 ADR | [ADR-0003](../adr/0003-diagnostic-architecture.md)、[ADR-0004](../adr/0004-source-span-position-model.md) |
| 阻塞项 | 无 |
| 影响范围 | `lang-cli` machine renderer、全局参数与 formatter 诊断出口；Architecture、Roadmap |
| 语言语义变更 | 否；只增加显式选择的 CLI 诊断展示协议 |

## 1. Goal

完成后，工具可通过 `kovenc --message-format=json format ...` 在 stderr 逐条消费 schema v1
JSON Lines 诊断，同时默认的人类文本、formatter stdout、退出码和 frontend/LSP 边界保持不变。

## 2. 背景

SPEC-0003 已建立完整诊断模型与人类 renderer，SPEC-0055 已验证同一诊断集合可以经独立
LSP adapter 发布。候选 0060 的代码前置已经闭合；ADR-0014 进一步确定首个公共机器 schema，
因此本 Spec 可以在不等待多文件 package/import、build CLI 或新 guide 语义的情况下实施。

`serde_json` 已作为 workspace dependency 并由 `Cargo.lock` 固定为 1.0.151；本次只让
`lang-cli` 直接复用它，不改变 frontend 依赖方向，也不增加新的传递依赖集合。

## 3. 范围与需求

- 新增职责单一的 machine renderer，把完整 `SourceMap + Diagnostic[]` 先校验、排序并编码为
  ADR-0014 v1 JSON Lines；任一错误不得返回部分字节。
- 每条记录完整保留 severity、`Ldddd`、消息、主 location 和有序 label/note/help；location
  同时包含半开 UTF-8 byte range 与 1-based Unicode-scalar line/column。
- CLI 在命令前接受至多一次 `--message-format=json`；默认或显式
  `--message-format=human` 使用现有 renderer。缺值、未知值、重复选项或放在 command 参数中的
  全局选项是稳定 usage error，不执行 formatter。
- formatter 只有在产生 frontend `FormattingError::Diagnostics` 时选择 renderer；合法源码
  stdout、`--check` 0/1、其他 operational error 的 stderr 与退出码保持原行为。
- pure renderer 测试锁定空集合、完整详情、Unicode/CRLF/EOF、raw source name JSON escaping、
  输入/source load order 确定性、foreign span fail-loud；CLI 单元和真实进程测试锁定显式选择、
  默认兼容、错误选项与 stdout/stderr 分离。

## 4. 非目标

- 不新增 `check` / `build` 命令、完整源码编译 CLI、颜色、自动修复、增量事件或通用 build
  event envelope。
- 不把 usage、文件 I/O、UTF-8、内部错误或 write failure 伪装成 frontend 诊断或 JSON event。
- 不改变 `Diagnostic`、错误码、排序规则、SourceMap 位置语义或 LSP UTF-16 adapter。
- 不读取、规范化或重映射 source path，不嵌入源码文本。
- 不引入 Serde derive、日志、CLI parser 框架或新的 workspace member。

## 5. 验收标准

- [ ] v1 每条 JSON Line 包含固定 schema/version、完整主诊断和生产者顺序 detail，精确范围矩阵
      通过；空集合为空，重复运行和 source load/input order 不改变字节。
- [ ] foreign/invalid span 返回具体内部错误且不产生部分输出；JSON 文本与 source name 只由
      `serde_json` 合法转义。
- [ ] 默认 `kovenc format` 人类诊断快照不变；显式 machine mode 的 stderr 每行可独立解析且
      stdout 为空、退出码 2。
- [ ] formatter 成功与 `--check` 继续使用原 stdout/0/1；usage/I/O/UTF-8/internal error 保持
      非协议 stderr 和退出码 2。
- [ ] `serde_json` 只进入 `lang-cli` 展示边界，frontend 和 LSP 依赖/协议不改变。
- [ ] `lang-cli` 窄测试与 workspace 五项标准基线通过；Architecture、Roadmap、ADR/Spec 索引
      只记录实际完成事实。

## 6. 技术方案与边界

- `diagnostic_renderer` 保持人类文本职责；新增 `machine_diagnostic_renderer`，共享
  `ordered_diagnostics` 与 `SourceMap::position`，但不为两种小型 wire format 提取通用 facade。
- machine renderer 先构造所有 `serde_json::Value` 并序列化到临时 `String`，成功后才返回；
  adapter error 明确区分诊断 span 和 JSON encoding。
- `main` 只解析全局 message format，再把枚举传给 `format::execute`；`format` 不读取进程环境，
  测试继续使用纯 `CommandOutput`。

## 7. 实施计划

1. [ ] 接受 ADR-0014 并建立 Spec/索引 → 验证：相对链接、术语和 diff 自检。
2. [ ] 实现 machine renderer 与范围/详情/确定性/错误矩阵 → 验证：renderer 窄测。
3. [ ] 接入全局 CLI 选择并扩展单元/真实进程测试 → 验证：`cargo test -p lang-cli --all-targets`。
4. [ ] 同步 Architecture、Roadmap 与验收记录 → 验证：文档和代码事实一致。
5. [ ] 运行 workspace 五项标准基线并独立提交 → 验证：全部实际退出码为 0。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | ADR、批准 Spec 与候选队列 | `docs(spec): define machine diagnostics protocol (SPEC-0060)` |
| 2 | renderer、CLI 接线、测试、Architecture/Roadmap 与 done 验收 | `feat(cli): emit machine-readable diagnostics (SPEC-0060)` |

## 9. 未决问题

- 无。完整 machine event stream 与 build command 是明确非目标，不影响当前诊断记录协议。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | SPEC-0003/0055 `done`；ADR-0003/0004 `accepted`；当前 formatter 已是唯一公开结构化 CLI 诊断出口 |
| `cargo tree -p lang-cli` | 待执行 | 确认新增直接 serde_json 边界和实际 lockfile 图 |
| workspace 基线 | 未执行 | 实现完成后执行 |

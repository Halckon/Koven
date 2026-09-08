# SPEC-0056：单文档语义跳转定义

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P6-056` |
| 所属 Phase | Phase 6 |
| 语言规范 | 现行 [`guide/00-index.md`](../guides/v0.34-pre-restructure/00-index.md) v0.28；[`guide/06-roadmap.md`](../guides/v0.34-pre-restructure/06-roadmap.md) Phase 6 |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |
| 前置 Spec | SPEC-0055、SPEC-0018–0023、SPEC-0067 `done` |
| 前置 ADR | 无；复用标准 LSP definition request 与既有单文档分析边界 |
| 关联 ADR | [ADR-0003](../../adr/accepted/0003-diagnostic-architecture.md)、[ADR-0004](../../adr/accepted/0004-source-span-position-model.md) |
| 阻塞项 | 无；跨文件 definition 明确排除，不依赖候选 SPEC-0025 |
| 影响范围 | `lang-lsp` analysis/definition/position adapter、server capability/request/lifecycle 测试；Architecture、Roadmap |
| 语言语义变更 | 否；只展示既有名称与类型产物已经确定的源码 identity |

## 1. Goal

完成后，LSP 客户端可在已打开 `.ko` buffer 内按 UTF-16 cursor 请求
`textDocument/definition`，获得由现有 `NameResolution` / `TypedFile` 事实确定的同文档声明
位置；未解析、外部环境或不在 Identifier 上的位置返回 `null`，不使用文本同名搜索。

## 2. 背景与前置审计

候选队列原先把全部跳转定义依赖在多文件 SPEC-0025 上，但单文件名称解析已经为真实
Identifier 保存 `ReferenceTarget`，每个源码 `Symbol` 已保存声明名称 `Span`。成功 overload、
实例 member call 和普通字段投影还分别由 `CallDescriptor` / `AggregateProjectionDescriptor`
保存唯一源码 target。SPEC-0055 已提供 open/change/close buffer 状态和完整 frontend 流水线，
因此同文档闭合切片不需要 import 展开、磁盘读取或 package visibility。

本 Spec 只把上述事实建立为 LSP 查询索引；跨文件目标仍必须等待 SPEC-0187 及其
SPEC-0197/0198 前置，不能把 URI、文件名或限定名称拼写当作 package identity。

## 3. 范围与需求

- `Analysis` 保留本次名称/类型产物派生的 definition index；索引只保存 source-local 引用
  `Span → target Span[]`，不持有 LSP URI，不反向修改 frontend 模型。
- 普通唯一 `Symbol` 引用跳到声明名称；源码声明名称跳到自身；`LaterLocal` 跳到已知的稍后
  声明；external/unresolved 不生成目标。
- 未唯一选择的源码 overload/case payload candidates 返回按声明位置稳定排序并去重的 location
  数组；成功 typed call 必须用唯一 `CallableTarget::Source/StructuralComponent` 覆盖同一引用的
  宽候选，成功普通字段投影同样跳到唯一 field symbol。
- LSP 位置适配集中支持现有 UTF-16 capability：source `Span` 转 0-based UTF-16 range；cursor
  line/character 反向转换为 UTF-8 byte offset，拒绝 surrogate pair 中间位置，越界位置返回无
  目标。CRLF、emoji、EOF 和 Identifier 半开末端有精确测试。
- server 声明 definition provider；只为当前打开 URI 服务。open/change 成功分析后原子替换
  文档 analysis，close 后请求返回 `null`；畸形 params 返回 JSON-RPC invalid-params，后续有效
  请求仍可处理。
- 对带普通 frontend 诊断但仍有有效引用事实的文档继续提供可证明目标；definition request
  不发布额外诊断、不读取磁盘、不改变文档版本。

## 4. 非目标

- 不展开 package/import，不读取未打开文件，不实现跨 URI 跳转、workspace symbol、find
  references、rename、hover、completion、semantic token 或增量 text sync。
- 不为 external standard identity 构造虚假 `.ko` 声明，不从文本相等、函数名、成员名或文件
  路径猜测目标。
- 不定义 overload UI 排序的新语言语义；数组只保留现有源码 symbol 的稳定声明顺序。
- 不让 LSP 类型泄漏到 frontend，不增加 frontend 公共 API、诊断码或 workspace 依赖。
- 不实现尚处于 deferred 的普通非调用 member access、safe-call、callable reference 或跨 package
  名称事实；只有现有 typed/name facts 明确发布 target 时才返回位置。

## 5. 验收标准

- [x] pure definition index 覆盖声明自身、local/parameter/type/enum 引用、LaterLocal、唯一 typed
      overload/member call、字段投影、宽候选排序去重及 external/unresolved null。
- [x] UTF-16↔UTF-8 adapter 精确覆盖 emoji surrogate、LF/CRLF、EOF、行/列越界、surrogate 中间
      和 Identifier 半开末端；诊断 range 既有矩阵不回归。
- [x] 初始化 capability 只新增 definition provider；打开文档的真实 request 返回同 URI 精确
      range，change 后使用新 analysis，close/unopened/outside 返回 `null`。
- [x] 畸形 definition params 返回 invalid-params 且不终止会话；未知 request 继续 method-not-found，
      notification/diagnostic 生命周期不回归。
- [x] `lang-lsp` 窄测试和 workspace 五项标准基线全部通过；production 文件遵守 1000 行软上限。
- [x] Architecture、Roadmap 和候选队列只把单文档跳转标为已实现，跨文件能力继续依赖
      SPEC-0025。

## 6. 技术方案与边界

- 新建 `definition` 模块，把名称引用和 typed 精确 target 合成为稳定 index；先加入名称层宽
  targets，再用同 span 的成功 call/projection 覆盖，不为仅一次查找额外扩张 frontend API。
- 新建/提取 `position_adapter`，供诊断和 definition 共用 `span → Range`；反向 cursor 映射只
  接收打开 buffer 的原 `SourceMap`，不建立独立 line index 真源。
- `OpenDocument` 保存 version 与完整 `Analysis`。full change 先构造下一份 analysis 并成功发布
  diagnostics，再原子替换旧状态，内部失败不留下 text/analysis 不一致。
- definition request 返回 `Option<GotoDefinitionResponse>`；一目标用 scalar，多目标用 array，
  无目标用 JSON `null`。

## 7. 实施计划

1. [x] 建立 definition index 与名称/typed target 测试 → 验证：`cargo test -p lang-lsp definition`。
2. [x] 提取双向 UTF-16 position adapter 并回归诊断映射 → 验证：position/diagnostic adapter 窄测。
3. [x] 接入 capability、request 与原子 document analysis 生命周期 → 验证：memory connection
   open/change/close/malformed request 测试。
4. [x] 同步 Architecture、Roadmap、Spec 验收 → 验证：文档与实现事实一致。
5. [x] 运行 workspace 五项标准基线并创建独立提交 → 验证：全部实际退出码为 0。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | approved Spec 与候选依赖修正 | `docs(spec): define single-document definitions (SPEC-0056)` |
| 2 | index、position/server 接线、测试与 done 文档 | `feat(lsp): resolve single-document definitions (SPEC-0056)` |

## 9. 未决问题

- 无。跨文件目标、deferred member forms 和 external standard source mapping 都是明确非目标。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | SPEC-0055/0018–0023/0067 `done`；NameReference/Symbol、CallDescriptor 与 AggregateProjectionDescriptor 已公开所需 identity/span |
| `cargo test -p lang-lsp --all-targets` | 通过 | 11 tests passed；覆盖 index、双向 position adapter 与 memory connection 生命周期 |
| `cargo fmt --all -- --check` | 通过 | 退出码 0 |
| `cargo check --workspace --all-targets` | 通过 | 退出码 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | 退出码 0 |
| `cargo test --workspace --all-targets --quiet` | 通过 | 全部 workspace targets 通过，退出码 0 |
| `cargo build -p lang-cli` | 通过 | 退出码 0 |
| production 文件行数 | 通过 | 新增 `definition.rs`、`position_adapter.rs` 与修改后的 `server.rs` 均未超过 1000 行软上限 |

# SPEC-0187：跨文件 LSP 诊断与跳转定义

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P6-187` |
| 所属 Phase | Phase 6 |
| 语言规范 | 现行 v0.32 §32 |
| 批准依据 | 2026-08-30 当前持续 Goal 授权继续按现行 guide 与已解锁 Spec 分阶段实施 |
| 前置 Spec | SPEC-0025、0055、0056、0197、0198 `done` |
| 前置 ADR | ADR-0020、[ADR-0021](../adr/0021-lsp-explicit-source-set-protocol.md) `accepted` |
| 阻塞项 | 无 |
| 影响范围 | `lang-lsp` workspace/source-set state，frontend API integration，LSP tests；Architecture/Roadmap |
| 语言语义变更 | 否 |

## 2. Goal

完成后，host 可通过 ADR-0021 的版本化 initialization option 显式提供固定、完整的 base
source set；LSP 对该 unit 发布跨文件 package/import、类型和所有权诊断，并可从 import、限定名
和普通引用跳转到其他 base 文件的精确声明 Span。

## 3. 范围与需求

- 严格解析 `koven.lsp.source-set` version 1；缺席时精确保留现有 legacy 单文档模式，非法 schema、
  root/source key、logical path 或 URI 在 initialize 阶段返回 `InvalidParams`。
- LSP 复用 frontend compilation-unit products；由 immutable base source set 加打开 buffer overlay
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
- 初始化成功后等到 `initialized` 再发布 base diagnostics；每次成功 snapshot 重建都按 source key
  重发所有 URI（包括空集合）。open overlay 带版本，base-only/close 后版本为 `None`。
- source-set unit mode 的 unknown/duplicate open、unopened/stale/partial change 与 unknown close
  只记录协议日志并保持旧状态；legacy mode 精确保留 SPEC-0055/0056 的既有生命周期。
  internal analysis/mapping 失败保留 last-good overlays、snapshot 与 definition facts。

## 4. 非目标

- 不实现 manifest discovery、磁盘读取/监听、动态 base membership、多个 compilation unit、依赖
  下载、增量数据库、rename/references/completion 或跨依赖项目跳转。

## 5. 验收标准

- [x] version 1 初始化正反矩阵覆盖 schema/version、重复 root/key/URI、未知 root、非法路径/URI，
  并证明 server 不读取 URI 指向的磁盘内容。
- [x] 多文件 open/change/close 测试覆盖诊断新增、迁移、清除、base 回落、version 与确定发布顺序。
- [x] definition 覆盖 exact alias、wildcard、限定名、同 package 及 private/inaccessible 反例。
- [x] 跨 source primary/related URI、UTF-16/CRLF/空 Span 和 internal failure last-good 原子性通过。
- [x] sourceSet 缺席时 SPEC-0055/0056 的 legacy suite 精确回归。
- [x] LSP 不包含第二套 import resolver；lang-lsp/workspace 基线与 Architecture 同步。

## 6. 技术方案与边界

以 ADR-0021 的 immutable base、overlay map 与 unit snapshot store 取代“每 URI 一个独立 frontend
分析”的多文件语义 store；候选 overlays、共同 `SourceMap`、frontend facts 和全部 publish payload
先完整构造，再替换 live state。filesystem manifest discovery 由 SPEC-0052 提供给 project CLI、
由 SPEC-0054 消费，不是本 Spec 的隐式输入或依赖。

## 7. 实施计划

1. [x] 解析 version 1 source set 并保留 legacy fallback → 验证：initialize 正反矩阵。
2. [x] 建立 immutable base、buffer overlay 与原子 unit snapshot → 验证：open/change/close、
   stale/internal-failure 测试。
3. [x] 接跨文件 diagnostics/definition → 验证：多 URI、UTF-16 与 frontend 正反矩阵。
4. [x] 同步 Architecture 并跑 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | source-set version 1 初始化协议 | `feat(lsp): validate source-set initialization (SPEC-0187)` |
| 2 | base/overlay snapshot 与跨文件 diagnostics | `feat(lsp): analyze multifile source sets (SPEC-0187)` |
| 3 | 跨文件 definition 与完成同步 | `feat(lsp): resolve multifile definitions (SPEC-0187)` |

## 9. 未决问题

- 无；host wire、单 unit、任意合法绝对 URI、固定 membership 与 legacy fallback 由 ADR-0021
  封闭。状态门禁仍由元数据中的前置 Spec 表达。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap 审计 | 通过 | 依赖已扩展为完整 name/type/ownership frontend 链 |
| 2026-08-26 source-set provider 审计 | 通过 | ADR-0021 已物化显式 initialization wire；该次记录时尚未接受或实施 |
| 2026-08-26 ADR 接受审计 | 通过 | 统一 SourceUnitId 查询边界，严格事件规则仅作用于 source-set mode；ADR 已接受，Spec 仍保持 draft |
| 2026-08-30 实施前审计 | 通过 | 0025/0197/0198 与 0055/0056 均已 `done`，ADR-0020/0021 `accepted`；按初始化、snapshot/diagnostics、definition 三个可独立验证切片推进 |
| `cargo test -q -p lang-lsp source_set_initialization --locked --offline` | 7 passed | strict schema/version/field、root/key/URI/path、确定排序、不读取 URI、legacy 缺席、valid session 与 initialize `InvalidParams` |
| `cargo clippy -p lang-lsp --all-targets --all-features --locked --offline -- -D warnings` | 通过 | 第一切片只跑 LSP 窄测与受影响 crate 静态门禁，不运行 `lang-frontend` 全量 |
| 独立 fresh-context 评审 | 通过 | 复核 ADR-0021 strict wire、无关 option、路径/URI/排序、不读磁盘、InvalidParams 与 legacy fallback；修正 Specs 总路线图状态漂移后无 P1/P2/P3 |
| `cargo test -q -p lang-lsp server::tests --locked --offline` | 9 passed | initial base、全 URI stable-order publish、跨文件诊断新增/清除、overlay version/base 回落、协议忽略、internal failure last-good、parser/type/ownership validation gate 与 legacy lifecycle |
| `cargo test -q -p lang-lsp diagnostic_adapter::tests --locked --offline` | 2 passed | primary source 分组、cross-source related URI/UTF-16 range，并回归 CRLF、空 Span 与单文档映射 |
| `cargo test -q -p lang-lsp source_set_ --locked --offline` | 11 passed | 第二切片的聚合窄测入口；覆盖初始化、unit lifecycle、诊断门禁与 last-good，不运行 `lang-frontend` 全量 |
| `cargo clippy -p lang-lsp --all-targets --all-features --locked --offline -- -D warnings` | 通过 | 第二切片复用 LSP scoped tests + 受影响 crate 静态门禁，不运行 `lang-frontend` 全量 |
| 第二切片独立 fresh-context 复审 | 通过 | 确认 snapshot 保留同源 name/type/ownership recovery products、三阶段诊断门禁与 Box 后原子 commit 语义；无 P1/P2/P3 |
| `cargo test -q -p lang-lsp source_set_ --locked --offline` | 13 passed | 第三切片聚合窄测；新增 exact terminal/alias、wildcard use、qualified/same-package、private/unresolved 与 definition last-good，不运行 `lang-frontend` 全量 |
| `cargo test -q -p lang-lsp definition --locked --offline` | 7 passed | 跨文件与既有单文档 definition 一并回归；direct unit typed-facts 用例锁定 overload 收敛/失败候选、member call 与 field projection |
| `cargo clippy -p lang-lsp --all-targets --all-features --locked --offline -- -D warnings` | 通过 | 第三切片受影响 crate 静态门禁；Cargo target 共享命令保持串行，其余文档/复审可并行 |
| `cargo check --workspace --all-targets --all-features --locked --offline` | 通过 | workspace 编译基线；不执行耗时的 `lang-frontend` 全量测试 |
| 第三切片独立 fresh-context 复审 | 通过 | 初审 P2/P3 测试缺口已由 direct unit typed-facts、unknown URI 与 surrogate 反例关闭；复审确认 identity guard 与原子语义，无新 P1/P2/P3 |

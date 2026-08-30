# SPEC-0054：无依赖本地 project build/run

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `approved` |
| Goal ID | `KOV-P6-054` |
| 所属 Phase | Phase 6 |
| 语言规范 | 现行 v0.33 §33 |
| 批准依据 | 2026-08-30 用户明确要求在 v0.32 完成后启用 v0.33 并继续分阶段实施 |
| 前置 Spec | SPEC-0052、0060、0190、0193、0194、0199 `done` |
| 前置 ADR | ADR-0010、0019、0020、0022 `accepted` |
| 阻塞项 | 无；与 SPEC-0213/0214 独立，按路线图顺序实施 |
| 影响范围 | `lang-cli` project build/run、entry selection、产物提交、CLI integration tests；Architecture/Roadmap |
| 语言语义变更 | 否；实施现行 v0.33 的公开工具契约 |

## 2. Goal

完成后，用户可通过显式 `project.toml` 和绝对 package-qualified entry 构建或运行一个无依赖、
多文件 Koven compilation unit，获得单一独立 native executable；现有单文件命令保持兼容。

```text
kovenc build --project <project.toml> --entry <qualified-name> -o <executable>
kovenc run --project <project.toml> --entry <qualified-name> [-- <program-arg>...]
```

## 3. 范围与需求

- 实施 §33 的固定参数顺序；`--project` 显式选择 project mode，缺失/重复/未知选项、非法 selector
  或缺少 `--entry`/`-o` 是 usage exit 2，不按文件/目录形状猜模式。
- 消费 SPEC-0052 的完整 base snapshot，在同一 `SourceMap` 上运行 0025→0197→0198 validated
  frontend 链；任一源码诊断先于 entry selection，并复用 unit human/JSON Lines renderer。
- 在 typed unit package index 上把 selector 解析为 `DeclarationId + process shape`。目标只能是有
  body、顶层、非泛型、`public`/`internal` 函数，允许 `() -> Unit` 或 Borrow
  `(Array<String>) -> Unit`；private/member/local/external/prelude target 均不可用。
- 选择矩阵精确区分 missing、inaccessible、invalid-shape 与 ambiguous operational failure；
  过滤后唯一合法 shape 可与任意非法 overload 共存。entry 名称不要求 `main`，不执行全 unit
  conventional lookup。
- 把显式 `DeclarationId`/shape 交给 SPEC-0199 unit object API，复用 ADR-0019 的 argv wrapper 和
  ADR-0010 linker；CLI/backend 不按字符串、package 文本或文件顺序重新选择 entry。
- `build` 在读取项目之前检查最终 output 已存在及明显路径重合；object/linker executable 都写
  输出目录内的唯一 sibling temporary。清理 object 后，以原子 no-replace commit 发布最终
  executable；commit 前失败保证 final 不存在并 best-effort 清理，不覆盖竞态中新建的路径。
- provider 完成后、frontend 前继续拒绝 final/object/temp 与 manifest 或任一 source 的路径重合；
  不允许构建写入破坏下一次 source discovery 的输入。
- `run` 使用唯一临时目录，成功 build 后原样转交 `--` 后 `OsString` 参数并保留程序 stdout、
  stderr 与可表示退出状态。cleanup 失败不得抹掉程序输出；程序成功时转为 operational exit 1，
  程序已非零时保留其状态并追加 cleanup 文本。
- 错误优先级固定为 CLI/output preflight → manifest/provider → frontend diagnostics → entry →
  codegen/link/commit → launch/cleanup。project operational error 不分配 `Ldddd`，即使全局选择
  JSON 也不伪造成 diagnostic JSON。

## 4. 非目标

- 不实现 dependency/registry/download/lock、dependency-aware build、跨 compilation-unit ABI。
- 不增加 manifest target/entry/default output，不省略 `--entry`，不做全局 `main`、private/file-
  qualified entry、多 target、library artifact、安装/发布或 cross target。
- 不实现每文件 object、cache/watch/增量构建，不覆盖 existing output。
- 不改变单文件显式 `--entry` 的零参数-only 契约或单文件 conventional main 行为。

## 5. 验收标准

- [ ] CLI 正反矩阵覆盖固定 project build/run、所有缺失/重复/未知参数、selector grammar、`--`
  分隔与单文件形式回归。
- [ ] 真实多文件、多 package project 通过 exact/alias import 分别运行零参数和 argv entry，stdout、
  stderr、参数顺序、退出状态与最终 executable 均正确。
- [ ] selector 矩阵覆盖默认/具名 package、任意函数名、public/internal/private、无 body、generic、
  两种合法 shape、非法 overload 混合及 missing/inaccessible/invalid/ambiguous 分类。
- [ ] 跨文件 frontend human/JSON Lines 诊断按 source key 排序并先于 entry error；project operational
  error 保持单条 stderr 且没有伪造 `Ldddd`。
- [ ] 注入 object/link/commit/cleanup failure，证明 build 不覆盖 existing/racing output、不发布
  部分 executable、拒绝 manifest/source 路径重合、临时产物按契约清理，run 不丢失程序结果。
- [ ] `lang-cli`/workspace 五项基线、Architecture/Guide/Roadmap 同步完成。

## 6. 技术方案与边界

- 新增独立 project orchestration，不循环调用单文件 `BootstrapTarget`，也不复用现有
  `BootstrapEntry::Explicit` 的字符串/`SymbolId` selector。unit entry resolver 只消费 validated
  declaration/type facts 并返回 `DeclarationId` 与既有两种 `NativeEntry` shape。
- `lang-cli` 继续拥有 manifest IO、临时路径、link、launch 与 commit；`lang-codegen` 只生成显式
  entry 的 object，不读取 project。no-replace commit 必须针对当前支持平台选择真实原子 primitive；
  若需新依赖/`unsafe`，实施前按依赖准入和最小安全边界审计，不能用 `exists()+rename` 冒充。
- 本 Spec 不新增依赖解析组件；ADR-0022 version 1 中出现 target/entry/dependency 字段仍由
  SPEC-0052 严格拒绝。

## 7. 实施计划

1. [ ] 扩展 project CLI parser 与错误优先级 → 验证：参数/单文件兼容矩阵。
2. [ ] 编排 snapshot→validated unit 与 project entry resolver → 验证：frontend/selector 正反矩阵。
3. [ ] 接 unit object、link、no-replace commit 与 run → 验证：真实 executable/argv/故障注入。
4. [ ] 同步 Architecture/Spec 验收并跑 workspace 五项基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | project CLI、entry resolver、native build/run 与完成文档 | `feat(cli): build local projects (SPEC-0054)` |

## 9. 未决问题

- 无；manifest target/default、dependency build 和单文件 entry 统一明确留给后继，不阻塞本 Spec。
  实施顺序由现行 guide 与前置 Spec 表达。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 project-entry 审计 | 通过 | CLI/selector/output/error 边界已物化；当时因候选 guide/前置链未生效保持 draft |

# SPEC-0101: 强化独立 Parser 入口 trivia 产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-101` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并简化验收环节；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0077、SPEC-0091、SPEC-0093–0100 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 独立入口 trivia matrix、共享 entry / trivia test support、Architecture |
| 语言语义变更 | 否；只强化 trivia 词法分段与 Parser 产物验收 |

## 1. Goal

完成后，753 个独立入口 trivia 变体会精确验证每个插入区域由预期 `TriviaKind + spelling`
片段覆盖，并直接复用两次已验证 Parser 产物的无 Span shape，不再为 shape 额外执行第三次解析。

## 2. 范围与需求

- 把 tab、无换行 block comment、混合 space/tab/comment 三种变体提升为带预期词法片段的静态
  test support；允许 whitespace lexeme 与原源码相邻 whitespace 合并，但插入区间的 overlap
  必须精确保持预期分类与 spelling。
- 753 个变体的每个单 gap 插入验证一个插入区间；每例 all-gap 插入按累计 byte 偏移验证全部
  code-mode gap，不允许只靠 significant token 过滤掩盖 trivia 分段变化。
- 扩展共享 entry fingerprint，使其包含 root ID 与四张 AST table payload discriminant；两次
  解析先完成 source / AST / diagnostic / typed-root 验证，再比较完整 fingerprint。
- trivia 矩阵直接取得共享的第一次 shape，删除每个 baseline / 变体的第三次 Parser 调用；
  753 个变体从 2,259 次降为 1,506 次解析，同时保持两次确定性证据。
- 保持 12-case corpus、240 个 token、239 个 code-mode gap、210 / 330 / 213 变体计数及
  significant token / shape / 两阶段零诊断断言不变。
- 不增加语料、依赖、生产公开 API、新诊断或合法语法。

## 3. 非目标

- 不在 string / interpolation / char / comment 内插入 trivia。
- 不加入结构性换行 carrier；其语义由 SPEC-0100 验收。
- 不要求插入前后的绝对 Span 相等，也不改变 Parser 或 Lexer 行为。

## 4. 验收标准

- [x] 三种 trivia variant 具有单一 `TriviaKind + spelling` 分段表。
- [x] 753 个变体的全部单 gap / all-gap 插入区间均精确验证词法 overlap。
- [x] 共享 entry 两次产物均包含并比较同一无 Span shape。
- [x] trivia 矩阵不再执行未复用结构化验证的第三次解析。
- [x] 既有 significant token、shape、零诊断与计数断言保持通过。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关矩阵与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增 `parser_trivia_variants` support，以 lexeme 与插入 `[start, end)` 的交集切片验证合并
whitespace 的真实归属；`parser_entry_matrix` 的 fingerprint 增加共享 `EntrySyntaxShape`，并提供
同时返回 diagnostic count 与 shape 元组的单一双解析入口；既有仅需计数的调用方读取首项，
避免在各 integration test crate 中保留未使用的包装 API 或结果字段。

## 6. 实施计划

1. [x] 审计 entry trivia matrix → 验证：确认 trivia 精确分段缺失，且 shape 来自第三次解析。
2. [x] 增加 trivia overlap 合约并复用双解析 shape → 验证：753 变体全部通过且无第三次解析。
3. [x] 运行直接相关窄验收 → 验证：7/7，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0101`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | trivia 分段、共享 entry shape、Architecture 与完成记录 | `test(frontend): strengthen entry trivia invariants (SPEC-0101)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 7 个 `parser_entry_*` mutation / trivia integration tests | 通过 | 7/7；trivia 753 个变体、1,506 次生产解析 |
| 同一组 7 个 integration tests 的窄 Clippy | 通过 | `-D warnings`；0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

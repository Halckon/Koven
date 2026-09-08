# SPEC-0113: 强化独立 Parser 入口 trivia Lexer 确定性

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-113` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0077、SPEC-0091、SPEC-0101、SPEC-0103–0112 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 独立 Parser 入口 trivia integration test、共享 entry / Lexer test support、Architecture |
| 语言语义变更 | 否；只补齐既有独立入口 trivia corpus 的 Lexer 重复产物验收 |

## 1. Goal

完成后，独立入口 trivia 矩阵实际执行的 12 个 token/gap 建模基线、12 个 Parser 基线与
753 个 mutation 均运行两次生产 Lexer，同时保持既有双 Parser、精确 trivia 分段及结构等价证据。

## 2. 范围与需求

- 保持 12-case corpus、240 个 token、239 个 code-mode gap、三种 trivia variant 和
  210 / 330 / 213 mutation 分组不变。
- 覆盖矩阵全部 777 个 Lexer source case：12 个只用于 token/gap 建模的基线，以及 765 个进入
  Parser 的基线或 mutation；每例执行两次 Lexer，共验收 1,554 个 Lexer 产物。
- 两次 Lexer 均验证 source identity、连续完整 byte 覆盖、唯一末尾 EOF、diagnostic primary /
  label Span 与完整 `Debug` 确定性；全部源码继续要求零 Lexer 诊断。
- 首个确定 Lexer 产物继续精确验证单 gap / all-gap 插入区间的 `TriviaKind` 与 spelling；完整
  `Debug` 相等保证第二个产物具有相同分段。
- 765 个 Parser source case 继续各执行两次对应入口 Parser，共验收 1,530 个 Parser 产物及其
  AST、diagnostic、typed root、syntax shape 与完整公开产物确定性。
- 双 Lexer 逻辑复用 SPEC-0103 的共享 helper，不增加语料、依赖、生产 API 或语言语义。

## 3. 非目标

- 不改变 trivia、lexical mode、Parser 恢复、AST 或 grammar。
- 不在 string / interpolation / char / comment 内增加插入位置，也不加入结构性换行 carrier。
- 不合并既有 token/gap 建模与 Parser baseline 路径；本 Spec 只强化其确定性证据。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 12-case、240-token、239-gap、三 variant 与 753-mutation 计数保持固定。
- [x] 1,554 个 Lexer 产物全部满足覆盖、EOF、source、diagnostic Span、零诊断与确定性不变量。
- [x] 单 gap / all-gap 的两次 Lexer 均保持精确 trivia 分段。
- [x] 1,530 个 Parser 产物继续满足 AST、diagnostic、typed root、shape 与确定性不变量。
- [x] 全部 777 个 Lexer source case 与 765 个 Parser source case 无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

`parse_clean` 与独立 token/gap baseline 都复用 `lex_source_twice` 创建 source map、执行并完整验证
两次 Lexer，再仅把首个确定产物交给现有分段检查与双 Parser。共享 entry support 导出其既有
`validate_lexed` 回调，避免复制产物不变量或在 integration crate 中重复加载 assertion 模块。

## 6. 实施计划

1. [x] 审计 entry trivia 矩阵 → 验证：确认 777 个 Lexer source case 均为单次，765 个 Parser source case 已双解析。
2. [x] 接入共享双 Lexer helper → 验证：777 个源码、3,084 个前端产物通过。
3. [x] 运行直接相关窄验收 → 验证：1/1，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0113`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 独立入口 trivia 双 Lexer、Architecture 与完成记录 | `test(frontend): strengthen entry trivia lexer invariants (SPEC-0113)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_entry_trivia_invariance_matrix --locked --offline` | 通过 | 1/1；777 个源码、3,084 个前端产物与 trivia 精确分段 |
| `cargo clippy -p lang-frontend --test parser_entry_trivia_invariance_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

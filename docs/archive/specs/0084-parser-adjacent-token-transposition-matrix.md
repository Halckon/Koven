# SPEC-0084: 建立 Parser 相邻 token 交换矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-084` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0073、SPEC-0080、SPEC-0082、SPEC-0083 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer / 完整文件 Parser 局部错序恢复测试、必要边界修复、Architecture |
| 语言语义变更 | 否；只锁定现行相邻合法 token 被交换后的阶段总性与恢复边界 |

## 1. Goal

完成后，覆盖现行主要语法 family 的合法完整文件中每一对相邻显著 token 被交换时，生产 Lexer
准确生成交换后的 token 序列，完整文件 Parser 返回确定 AST / 用户诊断而不产生内部错误；交换
未涉及 string/interpolation 或成对 delimiter owner 时，后置合法哨兵声明仍存活。

## 2. 范围与需求

- 复用 SPEC-0079–0083 的 22 个 Lexer / Parser-clean 文件和 396 个原始显著 token slot；每个
  文件枚举 `windows(2)`，共生成 374 个相邻 pair mutation。
- 每个变体用 `space + right-source + space + left-source + space` 替换从 left start 到 right end
  的原区间，以避免交换后意外拼成第三种 token；两端外部源码保持不变并重新调用生产 Lexer。
- pair 不含 string start/text/end 或 interpolation start/end 时，交换区必须在计算出的精确 Span
  分别重新词法分析为原 right / left `TokenKind`；涉及 lexical-mode token 时只锁定总性。
- 每例验证 lexeme 连续覆盖、source identity、非 EOF lexeme 非空、唯一末尾 EOF，以及 Lexer /
  Parser 诊断与四张 AST table Span source-local 且有界。
- 同一 `LexedFile` 重复执行两次完整文件 Parser，要求无 `ParserInternalError` 且公开 `Debug`
  产物一致；lexical-mode、owner-affecting 与可恢复 pair 数必须固定且均有非零证据。
- pair 任一 token 是 lexical owner、`()[]{}<>` delimiter 时只要求总性；其余 pair 的最后一个
  顶层 Item 源码切片必须精确等于 `val sentinel = 0`。
- 不增加依赖、公开 API、新诊断或合法语法；矩阵发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不把交换后的源码解释成某个固定 Parser 诊断，也不要求所有变体必须非法。
- 不保留 left/right 之间原 trivia；corpus 内部没有依赖换行的相邻 pair，统一空格用于隔离交换
  后的 token spelling，不把 Lexer 最长匹配变化混入 Parser 错序判据。
- 不交换 trivia / invalid / EOF，不组合多个交换，也不替代删除、重复、poison 或随机 fuzzing。
- 不固定每个恢复 AST 或完整诊断集合；领域测试与 diagnostic witness 继续负责精确语义。

## 4. 验收标准

- [x] 22 个共享样本、396 个 token 和 374 个相邻 pair 保持固定计数与基线合法性。
- [x] 每个相邻 pair 恰好形成一个交换变体，lexical-mode / 普通 pair 数量固定且非零。
- [x] 全部普通 pair 在交换区精确保留原 right / left `TokenKind` 与计算后的 byte Span。
- [x] 全部 mutation 保持 Lexer 覆盖、有界 Span、Parser 无内部错误和重复产物一致。
- [x] owner-affecting / 非 owner pair 数量固定且非零，全部非 owner mutation 精确保留哨兵。
- [x] 矩阵发现的生产缺陷有最小修复和定向断言，或明确记录未发现生产缺陷。
- [x] 新矩阵及直接相关 Lexer / Parser 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

把 replacement / duplication 重复使用的 lexical-mode token 分类提取为 tests 私有共享模块；
继续复用现有 corpus、token slot、owner 分类、Lexer / Parser 不变量和 sentinel helper。交换函数
直接回切 baseline source 中两个 token 的精确 Span，并返回 mutation 内 right / left 的新 Span
offset，不复制生产 Lexer 的 spelling 或最长匹配表。

owner-affecting 分类只决定能否承诺顶层同步，不改变 Parser 语义。若非 owner pair 丢失 sentinel，
先确认交换后 Lexer token 与 Span，再定位最早越过仍然平衡的顶层 owner 边界的恢复路径。

## 6. 实施计划

1. [x] 审计现有 mutation 矩阵 → 验证：确认缺少相邻合法 token 的局部错序覆盖。
2. [x] 提取共享 lexical-mode 分类并建立 adjacent pair 交换 / 重词法断言 → 验证：374 个 mutation；18 / 356 分层。
3. [x] 验证 Parser 总性、确定性和非 owner sentinel 恢复，最小修复实际缺陷 → 验证：154 / 220 owner 分层，无内部错误或生产缺陷。
4. [x] 运行直接相关窄测试与窄 Clippy → 验证：137/137，0 warnings。
5. [x] 运行一次 workspace 标准基线并同步完成记录 → 验证：标准命令全部成功，418 tests。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0084`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | adjacent token transposition 矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser adjacent token recovery (SPEC-0084)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --lib --test lexer --test lexer_boundary_matrix --test parser_expression --test parser_file --test parser_lexical_owner_matrix --test parser_token_omission_matrix --test parser_lexical_poison_replacement_matrix --test parser_token_duplication_matrix --test parser_lexical_poison_insertion_matrix --test parser_adjacent_token_transposition_matrix --locked --offline` | 通过 | 137/137；374 个 transposition mutation，未发现生产缺陷 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、418 tests、CLI build；0 failed / ignored / measured / filtered |

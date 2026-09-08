# SPEC-0081: 建立 Parser 词法 poison 替换矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-081` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0074、SPEC-0075、SPEC-0080 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer poison / 完整文件 Parser 中间恢复测试、必要边界修复、Architecture |
| 语言语义变更 | 否；只锁定现行 L0001 / L0002 词法根因进入每个既有显著 token 位置时的阶段总性 |

## 1. Goal

完成后，覆盖现行主要语法 family 的合法完整文件在任一显著 token 被非法字符或未来保留字
替换时，生产 Lexer 保留对应词法根因，完整文件 Parser 返回确定 AST / 用户诊断而不产生内部
错误；替换未破坏 string/interpolation 或成对 delimiter owner 时，后置合法哨兵声明仍存活。

## 2. 范围与需求

- 复用 SPEC-0079/0080 的 22 个 Lexer / Parser-clean 文件和 396 个原始显著 token slot。
- 对每个 slot 分别用带分隔空格的 `#` 与 `async` 替换完整原 token Span，共生成 792 个源码；
  每个变体必须重新调用生产 Lexer，不能手工构造 Invalid / Reserved lexeme。
- 对不属于 string start/text/end 或 interpolation start/end 的原 slot，替换区必须分别产生恰好一个
  L0001 / L0002；lexical-owner token 被替换后扫描模式可能改变，只锁定总性而不伪造固定错误集。
- 每例验证 lexeme 连续覆盖、source identity、非 EOF lexeme 非空、唯一末尾 EOF，以及 Lexer /
  Parser 诊断与四张 AST table Span source-local 且有界。
- 同一 `LexedFile` 重复执行两次完整文件 Parser，要求无 `ParserInternalError` 且公开 `Debug`
  产物一致；owner / 非 owner 两类 mutation 数必须固定且非零。
- 原 slot 不是 lexical owner、`()[]{}<>` delimiter 时，最后一个顶层 Item 的源码切片必须精确
  等于 `val sentinel = 0`；owner-affecting slot 只要求总性与确定性。
- 不增加依赖、公开 API、新诊断或合法语法；矩阵发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不把 `async` 或 `#` 变成合法 token，也不改变 L0001 / L0002 的 code、message 或 Span。
- 不要求替换 lexical-owner token 后仍产生原替换片段的独立诊断；其字符可能按新 owner 模式成为
  string text 或触发其他既定词法根因。
- 不固定每个 Parser 恢复 AST 或语法诊断集合；领域测试与 diagnostic witness 继续负责精确语义。
- 不组合多个替换、不替换 trivia / EOF，也不替代随机 fuzzing、omission 或 owner placement。

## 4. 验收标准

- [x] 22 个共享样本和 396 个 token slot 保持 SPEC-0080 计数与基线合法性。
- [x] 两种 poison 分别覆盖全部 slot，共执行 792 个重新词法分析的 mutation。
- [x] 全部非 lexical-owner slot 精确包含一次目标 L0001 / L0002 根因。
- [x] 全部 mutation 保持 Lexer 覆盖、有界 Span、Parser 无内部错误和重复产物一致。
- [x] 全部非 owner mutation 精确保留后置哨兵顶层 Item。
- [x] 矩阵发现的生产缺陷有最小修复和定向断言，或明确记录未发现生产缺陷。
- [x] 新矩阵及直接相关 Lexer / Parser 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

在共享 corpus 模块集中 owner-affecting token 分类，避免 omission 与 replacement 两份规则漂移；
在共享矩阵断言模块提供“最后 root 源码切片等于期望值”的最小 helper。replacement 依据原始
baseline `LexedFile` Span 执行 `prefix + poison + suffix`，随后建立全新 `SourceMap` / `LexedFile`。

目标词法码按原 slot 是否为 lexical-owner token分层：普通 / interpolation expression token 的
replacement 必须由生产 Lexer 明确产生目标根因；替换 lexical-owner token 会改变后续扫描模式，
因此只验证实际产物的结构不变量，不把第三方预期强加给 Scanner。

## 6. 实施计划

1. [x] 审计 token inventory 与 lexical-owner placement → 验证：确认缺少完整语法逐 slot poison 矩阵。
2. [x] 提取共享 owner 分类与 sentinel helper → 验证：SPEC-0080 的 396 / 96 / 300 计数和行为保持不变。
3. [x] 建立 396 × 2 replacement 矩阵 → 验证：792 个 mutation 的目标词法码、总性、确定性和 sentinel 分层断言通过。
4. [x] 运行直接相关窄测试与窄 Clippy，最小修复实际缺陷 → 验证：161/161，0 warnings；修复 nested interpolation 后缀恢复根因。
5. [x] 运行一次 workspace 标准基线并同步完成记录 → 验证：标准命令全部成功，415 tests。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0081`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | poison replacement 矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser lexical poison recovery (SPEC-0081)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --lib --test lexer --test parser_expression --test parser_call_argument --test parser_file --test parser_lexical_owner_matrix --test parser_token_inventory --test parser_token_omission_matrix --test parser_lexical_poison_replacement_matrix --locked --offline` | 通过 | 161/161；792 个 replacement mutation 与一个定向生产回归 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、415 tests、CLI build；0 failed / ignored / measured / filtered |

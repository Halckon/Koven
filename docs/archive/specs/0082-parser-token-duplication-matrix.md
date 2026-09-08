# SPEC-0082: 建立 Parser 单 token 重复恢复矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-082` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0073、SPEC-0080、SPEC-0081 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 完整文件 Parser 额外 token 恢复测试、必要边界修复、Architecture |
| 语言语义变更 | 否；只锁定现行合法 token 被重复插入后的阶段总性与恢复边界 |

## 1. Goal

完成后，覆盖现行主要语法 family 的合法完整文件在任一显著 token 后额外复制一次该 token
时，生产 Lexer 仍按源码准确分类，完整文件 Parser 返回确定 AST / 用户诊断而不产生内部错误；
重复未改变 string/interpolation 或成对 delimiter owner 时，后置合法哨兵声明仍存活。

## 2. 范围与需求

- 复用 SPEC-0079–0081 的 22 个 Lexer / Parser-clean 文件和 396 个原始显著 token slot。
- 每个变体保留原 token，并在其后以单个空格分隔插入一次原 Span 的精确源码切片；插入后再
  加单个空格与原后缀分隔，共生成 396 个重新词法分析的源码，不能手工复制 `Lexeme`。
- 原 token 不是 string start/text/end 或 interpolation start/end 时，原位置和插入位置必须分别
  重新词法分析为相同 `TokenKind`，并锁定精确 byte Span；lexical-mode token 只锁定总性。
- 每例验证 lexeme 连续覆盖、source identity、非 EOF lexeme 非空、唯一末尾 EOF，以及 Lexer /
  Parser 诊断与四张 AST table Span source-local 且有界。
- 同一 `LexedFile` 重复执行两次完整文件 Parser，要求无 `ParserInternalError` 且公开 `Debug`
  产物一致；owner / 非 owner 两类 mutation 数必须固定且非零。
- 原 slot 不是 lexical owner、`()[]{}<>` delimiter 时，最后一个顶层 Item 的源码切片必须精确
  等于 `val sentinel = 0`；owner-affecting slot 只要求总性与确定性。
- 不增加依赖、公开 API、新诊断或合法语法；矩阵发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不把重复 token 解释成新的合法语法，也不固定每个变体必须产生某个 Parser 诊断。
- 不要求 lexical-mode token 重复后仍产生两个同类 token；重复 quote / string text / interpolation
  边界会按生产 Scanner 的真实模式重新解释后续字符。
- 不组合多个重复、不复制 trivia / invalid / EOF，也不替代随机 fuzzing、删除或 poison replacement。
- 不固定每个恢复 AST 或完整诊断集合；领域测试与 diagnostic witness 继续负责精确语义。

## 4. 验收标准

- [x] 22 个共享样本和 396 个 token slot 保持既有计数与基线合法性。
- [x] 每个原始显著 token 恰好形成一个重复变体，owner / 非 owner 两类均非空且数量固定。
- [x] 全部非 lexical-mode slot 在原位置和插入位置保留相同 `TokenKind` 与精确 Span。
- [x] 全部 mutation 保持 Lexer 覆盖、有界 Span、Parser 无内部错误和重复产物一致。
- [x] 全部非 owner mutation 精确保留后置哨兵顶层 Item。
- [x] 矩阵发现的生产缺陷有最小修复和定向断言，或明确记录未发现生产缺陷。
- [x] 新矩阵及直接相关 Lexer / Parser 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

复用现有 corpus、token slot / owner 分类、Lexer / Parser 不变量和 sentinel helper。mutation 依据
baseline `LexedFile` 的原 token Span 执行 `prefix-through-token + space + token-source + space +
suffix`，随后建立全新 `SourceMap` / `LexedFile`。普通 token 的重词法断言使用原 Span 和由插入
长度直接计算的 duplicate Span，不扫描或复制生产 Lexer 的匹配规则。

owner-affecting 分类继续只表达不能普遍承诺顶层同步的已知语法边界，不改变 Parser 语义。若
非 owner 重复丢失 sentinel，先确认重词法结果，再定位最早越过已重新平衡顶层边界的恢复路径。

## 6. 实施计划

1. [x] 审计现有 mutation 矩阵 → 验证：确认缺少逐显著 token 重复插入覆盖。
2. [x] 建立逐 token duplication 与精确重词法断言 → 验证：396 个 mutation；14 / 382 lexical-mode 分层。
3. [x] 验证 Parser 总性、确定性和 sentinel 恢复，最小修复实际缺陷 → 验证：96 / 300 owner 分层，无内部错误或生产缺陷。
4. [x] 运行直接相关窄测试与窄 Clippy → 验证：133/133，0 warnings。
5. [x] 运行一次 workspace 标准基线并同步完成记录 → 验证：标准命令全部成功，416 tests。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0082`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | token duplication 矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser duplicate token recovery (SPEC-0082)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --lib --test lexer --test lexer_boundary_matrix --test parser_expression --test parser_file --test parser_token_omission_matrix --test parser_lexical_poison_replacement_matrix --test parser_token_duplication_matrix --locked --offline` | 通过 | 133/133；396 个 duplication mutation，未发现生产缺陷 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、416 tests、CLI build；0 failed / ignored / measured / filtered |

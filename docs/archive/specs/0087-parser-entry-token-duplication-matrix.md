# SPEC-0087: 建立独立 Parser 入口单 token 重复矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-087` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0069、SPEC-0082、SPEC-0085、SPEC-0086 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` expression / declaration / block 独立 Parser token 重复恢复测试、必要直接修复、Architecture |
| 语言语义变更 | 否；只锁定合法独立语法重复一个显著 token 后的重词法与入口总性 |

## 1. Goal

完成后，三个独立 Parser 入口对代表性合法源码中任意显著 token 被紧邻复制均能返回确定、
有界且 root 可解析的产物；普通 token 的原位置和复制位置均由生产 Lexer 精确恢复为原类型。

## 2. 范围与需求

- 复用 SPEC-0085 / 0086 的 12 个合法独立入口样本、4 / 4 / 4 分层和全部 240 个 token slot。
- 每个 mutation 在原 token 后插入 `space + 原 Span 源码 + space`，重新执行生产 Lexer 与对应
  Parser；固定各入口、lexical-mode / 普通 slot 和总 mutation 数。
- 非 string/interpolation lexical-mode token 必须在原 Span 与计算出的复制 Span 精确产生相同
  `TokenKind`；lexical-mode token 只锁定生产 Scanner 的真实总性，不臆造模式内分类。
- 每个 baseline 保持零诊断；mutation 保持连续 lexeme 覆盖、唯一 EOF、source-local 有界诊断 /
  AST Span、可解析 typed root、两次公开 `Debug` 产物一致且无内部错误。
- 不增加依赖、公开 API、新诊断或合法语法；发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不承诺独立入口的后续 sentinel 同步；其输入按定义只有一个 root。
- 不固定重复后必须出现的诊断 code 或恢复 AST 形态。
- 不复制 trivia、invalid lexeme 或 EOF，不组合多个 mutation，也不覆盖 omission / poison / transposition。

## 4. 验收标准

- [x] 12 个共享样本按 4 / 4 / 4 保持非空 token 集与 baseline 零诊断。
- [x] 240 个显著 token 各生成一个 duplication mutation；66 / 104 / 70，模式分层 20 / 220。
- [x] 220 个普通 slot 的原 token 与 duplicate 保持相同 `TokenKind` 和精确 Span。
- [x] 全部 mutation 保持 Lexer / AST / diagnostic 不变量、root 有效和重复解析确定性。
- [x] control-body 重叠恢复的反向 Span 缺陷已最小修复并有定向回归。
- [x] SPEC-0082、0085、0086 与三个领域入口测试保持通过。
- [x] 直接相关窄 Clippy 通过。
- [x] 一次最终成功的 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新矩阵直接复用 tests 私有 `parser_entry_matrix`、`parser_mutation_tokens`、
`parser_mutation_modes` 和 `parser_mutation_lexemes`。duplicate Span 由原 Span 与插入字符串长度
确定，测试不扫描或复制生产 Lexer 规则；每个 mutation 使用全新 `SourceMap` / `LexedFile`。

## 6. 实施计划

1. [x] 审计完整文件 duplication 与独立入口矩阵 → 验证：确认独立入口缺少逐 token 重复覆盖。
2. [x] 建立 duplication / 精确重词法断言 → 验证：66 / 104 / 70；20 / 220 模式分层。
3. [x] 验证 Parser 总性、root、Span 和确定性并修复直接缺陷 → 验证：240 个 mutation 全部执行，修复一个反向 Span 内部错误。
4. [x] 运行直接相关窄测试与窄 Clippy → 验证：156/156，0 warnings。
5. [x] 同步事实并运行 workspace 标准基线 → 验证：最终五条标准命令成功，422 tests；失败预检与不可验证的长时输出已如实记录。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0087`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 独立入口 token duplication 矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser entry duplicate token recovery (SPEC-0087)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --lib --test lexer --test parser_entry_adversarial --test parser_token_omission_matrix --test parser_token_duplication_matrix --test parser_entry_prefix_truncation_matrix --test parser_entry_token_omission_matrix --test parser_entry_token_duplication_matrix --test parser_expression --test parser_declaration --test parser_block --test parser_control_flow --locked --offline` | 通过 | 156/156；240 个独立入口 duplication mutation 与一个定向生产回归 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 最终 `fmt`、`check`、Clippy、422 tests、CLI build 成功；0 failed / ignored / measured / filtered。首次 `fmt --check` 发现一处机械换行并在其余命令前修正；首次长时全测包装未返回可验证退出码，缓存后重跑取得明确 `exit=0` |

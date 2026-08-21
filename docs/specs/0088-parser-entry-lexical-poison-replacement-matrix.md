# SPEC-0088: 建立独立 Parser 入口词法 poison 替换矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-088` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0069、SPEC-0081、SPEC-0085–0087 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 独立 Parser 入口 Lexer poison 恢复测试、必要直接修复、Architecture |
| 语言语义变更 | 否；只锁定合法独立语法 token 被现行 L0001 / L0002 词法根因替换后的阶段总性 |

## 1. Goal

完成后，三个独立 Parser 入口对代表性合法源码中任意显著 token 被 `#` 或 `async` 替换时，
均保留生产 Lexer 的真实根因并返回确定、有界且 root 可解析的 Parser 产物。

## 2. 范围与需求

- 复用 12 个独立入口样本、4 / 4 / 4 分层与 240 个 token slot；提取三套 entry token mutation
  真实共享的 baseline / slot helper，避免合法性和枚举断言漂移。
- 每个 slot 分别以带隔离空格的 `#` / `async` 替换，生成 480 个重新词法分析的 mutation；固定
  各入口、每种 poison 及 lexical-mode / 普通 slot 计数。
- 普通 slot 的替换区必须恰好产生一个目标 code，且 primary Span 精确等于 poison 文本；
  string/interpolation lexical-mode slot 只锁定 Scanner 真实总性。
- 全部 mutation 验证连续 lexeme 覆盖、唯一 EOF、source-local 有界诊断 / AST Span、typed root
  有效、两次公开 `Debug` 产物一致且无内部错误。
- 不增加依赖、公开 API、新诊断或合法语法；发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不要求 lexical-mode 替换仍产生独立 L0001 / L0002；替换可能改变 Scanner owner 状态。
- 不固定 Parser 诊断 code 或恢复 AST 形态，不承诺独立入口后续 sentinel 同步。
- 不替换 trivia / invalid / EOF，不组合多个 mutation，也不覆盖 omission / duplication / insertion。

## 4. 验收标准

- [x] 12 个共享样本按 4 / 4 / 4 保持非空 token 集与 baseline 零诊断。
- [x] 240 × 2 mutation 全部执行；入口 132 / 208 / 140，poison 240 / 240，模式 40 / 440。
- [x] 440 个普通 case 精确产生一次目标 L0001 / L0002 及 poison primary Span。
- [x] 全部 mutation 保持 Lexer / AST / diagnostic 不变量、root 有效和重复解析确定性。
- [x] 矩阵未发现生产缺陷，无需生产修复。
- [x] SPEC-0081、0085–0087 与三个领域入口测试保持通过。
- [x] 直接相关窄 Clippy 通过。
- [x] 最终成功的 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增 tests 私有 `parser_entry_mutation_support`，组合既有 entry corpus / parser assertion 与 token slot
枚举，只服务 entry mutation tests。replacement 直接按 baseline Span 切片并返回 poison 新 Span；
目标诊断通过 code 与 primary Span 同时匹配，不复制 Lexer 分类规则。

## 6. 实施计划

1. [x] 审计完整文件 poison replacement 与独立入口矩阵 → 验证：确认缺少 240 × 2 覆盖。
2. [x] 提取共享 entry mutation baseline / slots → 验证：SPEC-0086 / 0087 的 240 个 slot 与行为不变。
3. [x] 建立 poison replacement、精确根因与入口总性矩阵 → 验证：480 mutation；40 / 440 模式分层。
4. [x] 运行直接相关窄测试与窄 Clippy → 验证：158/158，0 warnings。
5. [x] 同步事实并运行最终 workspace 标准基线 → 验证：五条标准命令全部成功，423 tests。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0088`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 独立入口 poison replacement 矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser entry lexical poison recovery (SPEC-0088)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --lib --test lexer --test parser_entry_adversarial --test parser_token_omission_matrix --test parser_token_duplication_matrix --test parser_lexical_poison_replacement_matrix --test parser_entry_prefix_truncation_matrix --test parser_entry_token_omission_matrix --test parser_entry_token_duplication_matrix --test parser_entry_lexical_poison_replacement_matrix --test parser_expression --test parser_declaration --test parser_block --test parser_control_flow --locked --offline` | 通过 | 158/158；480 个独立入口 poison replacement mutation，未发现生产缺陷 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、423 tests、CLI build；0 failed / ignored / measured / filtered |

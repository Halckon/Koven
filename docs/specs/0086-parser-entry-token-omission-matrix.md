# SPEC-0086: 建立独立 Parser 入口单 token 缺失矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-086` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0069、SPEC-0080、SPEC-0085 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` expression / declaration / block 独立 Parser token 缺失恢复测试、必要直接修复、Architecture |
| 语言语义变更 | 否；只锁定合法独立语法删除一个显著 token 后的入口总性与公开产物不变量 |

## 1. Goal

完成后，三个独立 Parser 入口对代表性合法源码中任意单个显著 token 缺失均返回确定、有界且
root 可解析的产物，不因中间 owner、delimiter、string/interpolation 或普通 token 缺失产生内部错误。

## 2. 范围与需求

- 复用 SPEC-0085 的 12 个合法独立入口样本与 4 / 4 / 4 分层，提取 tests 私有共享 corpus 和
  入口产物断言，避免前缀与缺失矩阵漂移。
- 生产 Lexer 枚举每个完整样本的全部显著 token；逐一删除其精确 byte Span，重新执行生产 Lexer
  与对应 Parser 入口，并固定各入口及总 mutation 数。
- 每个 baseline 必须 Lexer / Parser 零诊断；每个 mutation 保持 lexeme 连续覆盖、唯一末尾 EOF、
  诊断及四张 AST table Span source-local 且有界。
- 每次解析的 expression / item / statement root 必须可解析；同一 `LexedFile` 重复解析两次，公开
  `Debug` 产物必须一致且无 `ParserInternalError`。
- 不增加依赖、公开 API、新诊断或合法语法；发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不重复 SPEC-0080 的完整文件 sentinel 恢复承诺；独立入口没有后续顶层声明可作为同步哨兵。
- 不固定删除后必须出现的诊断 code 或恢复 AST 形态；领域测试继续负责精确语义。
- 不删除 trivia、invalid lexeme 或 EOF，不组合多个删除，也不覆盖 duplication / poison / transposition。

## 4. 验收标准

- [x] 12 个共享样本按 4 / 4 / 4 保持互异、非空 token 集和 baseline 零诊断。
- [x] 每个显著 token 恰好生成一个删除 mutation；66 / 104 / 70，共 240 个。
- [x] 全部 mutation 保持 Lexer 覆盖、唯一 EOF、source-local 有界诊断 / AST Span。
- [x] 全部对应 root 可解析，重复解析产物一致且无内部错误。
- [x] 矩阵未发现生产缺陷，无需生产修复。
- [x] 新矩阵、SPEC-0080 / 0085 矩阵及三个领域入口测试保持通过。
- [x] 直接相关窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

把 SPEC-0085 内嵌的 `EntryKind`、12-case corpus 与入口分派 / output invariant 提取到 tests 私有
共享模块。新矩阵复用既有 `original_token_slots`，按 token 的真实 Span 切除源码，不复制 Lexer
token 表或 Parser grammar。所有 mutation 都重新词法分析；内部错误携带 entry / case / Span 上下文。

## 6. 实施计划

1. [x] 审计完整文件 mutation 与独立入口矩阵 → 验证：确认缺少独立入口逐 token 删除覆盖。
2. [x] 提取共享独立入口 corpus / assertion → 验证：SPEC-0085 的 747 个前缀计数与行为不变。
3. [x] 建立逐 token 删除矩阵并修复直接缺陷 → 验证：66 / 104 / 70，共 240 个 mutation，未发现生产缺陷。
4. [x] 运行直接相关窄测试与窄 Clippy → 验证：146/146，0 warnings。
5. [x] 同步事实并运行一次 workspace 标准基线 → 验证：五条标准命令各成功一次，420 tests。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0086`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 独立入口 token omission 矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser entry token omission recovery (SPEC-0086)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --lib --test lexer --test parser_entry_adversarial --test parser_token_omission_matrix --test parser_entry_prefix_truncation_matrix --test parser_entry_token_omission_matrix --test parser_expression --test parser_declaration --test parser_block --locked --offline` | 通过 | 146/146；240 个独立入口 deletion mutation，未发现生产缺陷 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、420 tests、CLI build；0 failed / ignored / measured / filtered |

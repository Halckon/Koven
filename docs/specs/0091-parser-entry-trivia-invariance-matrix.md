# SPEC-0091: 建立独立 Parser 入口 trivia 等价矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-091` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0069、SPEC-0077、SPEC-0085–0090 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 独立 Parser 入口 trivia 等价测试、矩阵私有结构指纹、必要直接修复、Architecture |
| 语言语义变更 | 否；只锁定既有非换行 trivia 不改变独立入口 token 序列或 AST 结构的不变量 |

## 1. Goal

完成后，三个独立 Parser 入口的代表性合法源码在每个 code-mode token gap 或全部 code-mode
gap 插入 tab、无换行 block comment 或混合 trivia 时，仍产生相同 significant token 序列与
无 Span AST 结构，且 Lexer / Parser 零诊断、公开产物确定。

## 2. 范围与需求

- 复用 12 个 entry 样本与 240 个 token，按真实 string / interpolation owner 枚举 239 个
  code-mode gap；固定 expression / declaration / block 的 case、token、gap 与变体计数。
- 对每个 code-mode gap 单独插入 `tab`、无换行 block comment、混合 space/tab/comment；每个
  样本还分别把三种 trivia 同时插入全部 code-mode gap。
- 每个变体必须保持 baseline 的完整非 trivia `LexemeKind` 序列，Lexer 与 Parser 均零诊断。
- 矩阵私有入口指纹记录 root ID 和四张 AST table 的节点类别 / 源码顺序，不包含 Span；
  每个变体必须与 baseline 相等，同一源码仍重复解析并保持公开 `Debug` 产物确定。
- 不增加依赖、生产公开 API、新诊断或合法语法；发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不在 string text、char literal、comment 或 UTF-8 scalar 内部插入 trivia。
- 不投放 LF / CRLF、line comment 或含换行 block comment；它们可能具有结构边界语义。
- 不宣称插入 trivia 后 Span 不变，不固定 Parser 内部实现或完整 payload 字段序列化。
- 不替代完整文件 trivia 矩阵、Lexer trivia 分段测试或各语法领域精确 AST 测试。

## 4. 验收标准

- [x] 12 个样本、240 个 token、239 个 code-mode gap 与 4 / 4 / 4 baseline 固定合法。
- [x] 三种 trivia 覆盖每个单 gap 与每例全部 gap，入口 210 / 330 / 213，共 753 个变体。
- [x] 全部变体保持 significant token 序列与无 Span AST 结构指纹不变且两阶段零诊断。
- [x] 每个源码重复解析无内部错误且公开产物确定一致。
- [x] 矩阵未发现生产缺陷，无需生产修复。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 最终一次成功的 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

复用共享 lexical-mode gap 状态机；只选择 `code_mode` gap，并按 byte offset 逆序执行全 gap
插入，避免偏移漂移。复用 tests 私有独立入口 helper 锁定 source-local、typed root 与重复解析，
矩阵另从公开 AST table 构造仅包含 root index 与 payload discriminant 序列的结构指纹。

## 6. 实施计划

1. [x] 审计完整文件 trivia 矩阵与独立入口 corpus → 验证：确认缺少独立入口合法 trivia 等价覆盖。
2. [x] 建立矩阵私有入口结构指纹与 gap / trivia 矩阵 → 验证：753 个目标变体唯一执行。
3. [x] 修复直接缺陷并运行窄测试与窄 Clippy → 验证：3/3 tests、0 warnings，未发现生产缺陷。
4. [x] 同步事实并运行最终一次 workspace 标准基线 → 验证：五条标准命令全部成功，426 tests。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0091`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 独立入口 trivia 等价矩阵、矩阵私有指纹、必要修复、Architecture 与完成记录 | `test(frontend): verify parser entry trivia invariance (SPEC-0091)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_trivia_invariance_matrix --test parser_entry_trivia_invariance_matrix --test parser_entry_token_omission_matrix --locked --offline` | 通过 | 3/3；753 个独立入口 trivia 变体，未发现生产缺陷 |
| `cargo clippy -p lang-frontend --test parser_entry_trivia_invariance_matrix --test parser_entry_token_omission_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、426 tests、CLI build；0 failed / ignored / measured / filtered |

# SPEC-0090: 建立独立 Parser 入口相邻 token 交换矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-090` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0069、SPEC-0084、SPEC-0085–0089 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 独立 Parser 入口相邻 token 交换测试、必要直接修复、Architecture |
| 语言语义变更 | 否；只锁定既有公开入口面对相邻 token 交换时的总性与确定性 |

## 1. Goal

完成后，三个独立 Parser 入口在代表性合法源码的每对相邻显著 token 交换后，Lexer 与 Parser
均返回 source-local、有界、确定且 root 可解析的产物；不涉及 lexical-mode segment 时交换后的 token identity
与 Span 必须精确可证。

## 2. 范围与需求

- 复用 12 个 entry 样本与 240 个 token；枚举每个样本内全部相邻 token pair。
- 交换原 token 源码切片并用空格隔离，避免交换本身之外的 token 粘连。
- 不涉及 lexical-mode segment 的 pair 必须精确重词法化为 right / left 原 `TokenKind` 与计算后的 Span；涉及
  string owner 的 pair 只锁定 Scanner / Parser 总性。
- 全部 mutation 保持连续 lexeme 覆盖、唯一 EOF、source-local 有界诊断 / AST Span、typed root
  有效、两次公开 `Debug` 产物一致且无内部错误。
- 不增加依赖、公开 API、新诊断或合法语法；发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不交换 trivia、非相邻 token 或跨样本 token，不组合多个 mutation。
- 不固定 Parser 诊断集合或恢复 AST 形态，不承诺独立入口后续 sentinel。
- 不重构完整文件交换矩阵或生产 Parser，除非测试暴露直接缺陷。

## 4. 验收标准

- [x] 12 个样本、240 个 token 与 4 / 4 / 4 baseline 保持固定合法。
- [x] 三个入口全部 228 个相邻 pair 均被执行，入口 62 / 100 / 66、模式 27 / 201。
- [x] 201 个不涉及 lexical-mode segment 的 pair 精确产生交换后的两个原 token kind / Span。
- [x] 全部 mutation 保持 Lexer / AST / diagnostic 不变量、root 有效和重复解析确定性。
- [x] 矩阵未发现生产缺陷，无需生产修复。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 最终一次成功的 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

矩阵通过 baseline 显著 token `windows(2)` 枚举相邻 pair；交换函数计算 right / left 在 mutation
中的新 byte 区间。普通模式用共享 exact-token helper 验证重词法化，字符串模式只经共享公开
入口断言验证总性、Span 边界、typed root 与确定性，不复制 Scanner 或 Parser 状态机。

## 6. 实施计划

1. [x] 审计完整文件交换矩阵与独立入口 corpus → 验证：确认缺少 12-case 独立入口 pair 覆盖。
2. [x] 建立独立入口 transposition / lexical-mode 矩阵 → 验证：228 个相邻 pair 唯一执行。
3. [x] 修复直接缺陷并运行窄测试与窄 Clippy → 验证：3/3 tests、0 warnings，未发现生产缺陷。
4. [x] 同步事实并运行最终一次 workspace 标准基线 → 验证：五条标准命令全部成功，425 tests。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0090`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 独立入口相邻 token 交换矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser entry adjacent token recovery (SPEC-0090)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_adjacent_token_transposition_matrix --test parser_entry_adjacent_token_transposition_matrix --test parser_entry_token_duplication_matrix --locked --offline` | 通过 | 3/3；228 个独立入口 mutation，未发现生产缺陷 |
| `cargo clippy -p lang-frontend --test parser_entry_adjacent_token_transposition_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、425 tests、CLI build；0 failed / ignored / measured / filtered |

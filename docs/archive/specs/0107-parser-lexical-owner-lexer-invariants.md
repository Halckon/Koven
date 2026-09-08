# SPEC-0107: 强化 Parser lexical-owner Lexer 确定性

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-107` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0075、SPEC-0095、SPEC-0103–0106 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Parser lexical-owner integration test、共享 Lexer test support、Architecture |
| 语言语义变更 | 否；只补齐既有 lexical-owner corpus 的 Lexer 重复产物验收 |

## 1. Goal

完成后，16 个代表性语法位置与 9 个 lexical owner 组成的 144 个 case 均执行两次生产 Lexer
与两次完整文件 Parser，统一证明两个阶段的完整公开产物结构与确定性。

## 2. 范围与需求

- 保持 16 个位置、4 个可恢复 owner、5 个 terminal owner、64 / 80 分组和 144 总数不变。
- 每个 case 执行两次生产 Lexer，共验收 288 个 Lexer 产物；两次均验证 source identity、连续
  完整 byte 覆盖、唯一末尾 EOF、diagnostic primary / label Span，并比较完整公开 `Debug`。
- 每个 case 继续执行两次完整文件 Parser，共验收 288 个 Parser 产物及其 AST、diagnostic、
  root 与完整公开 `Debug` 确定性。
- 两次 Lexer 继续满足各 owner 的精确词法错误码；64 个可恢复 case 的两次 Parser 继续以
  `val after = 1` 为最后一个完整文件 root，共保留 128 次 sentinel 边界检查。
- 普通用户输入不得导致 Lexer / Parser 内部错误或 panic。
- 双 Lexer 逻辑复用 SPEC-0103 的共享 helper，不增加 corpus、依赖、生产 API 或语言语义。

## 3. 非目标

- 不改变 lexical owner、Parser 恢复、诊断、AST 或 grammar。
- 不扩大 owner / placement corpus，不固定领域精确 AST payload。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 16 × 4 = 64 个可恢复 case 与 16 × 5 = 80 个 terminal case 保持固定。
- [x] 288 个 Lexer 产物全部满足覆盖、EOF、source、diagnostic Span 与确定性不变量。
- [x] 288 个 Parser 产物继续满足 AST、diagnostic、root 与确定性不变量。
- [x] 精确词法错误码与 128 次末尾 sentinel 边界检查保持通过。
- [x] 全部 144 个 case 无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

矩阵 `lex_case` 复用 `lex_source_twice` 创建每个 case 的 source map、执行并验证两次 Lexer，
再把首个确定产物交给既有 `parse_twice`。Parser 双运行、精确 lexical code 与 sentinel 断言
保持不变，不复制生产逻辑或共享测试 helper。

## 6. 实施计划

1. [x] 审计 lexical-owner 矩阵 → 验证：确认 144 个 case 的 Parser 已双运行，Lexer 仍为单次。
2. [x] 接入共享双 Lexer helper → 验证：144 个 case、576 个前端产物通过。
3. [x] 运行直接相关窄验收 → 验证：2/2，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0107`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | lexical-owner 双 Lexer、Architecture 与完成记录 | `test(frontend): strengthen lexical owner lexer invariants (SPEC-0107)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_lexical_owner_matrix --locked --offline` | 通过 | 2/2；144 个 case、576 个前端产物、128 次 sentinel 边界检查 |
| `cargo clippy -p lang-frontend --test parser_lexical_owner_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

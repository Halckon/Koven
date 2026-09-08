# SPEC-0093: 强化独立 Parser 入口对抗产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-093` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0068、SPEC-0069、SPEC-0085–0092 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 独立 Parser 入口对抗矩阵产物断言、必要直接修复、Architecture |
| 语言语义变更 | 否；只强化既有 768-case 对抗矩阵的结构与 Span 验收 |

## 1. Goal

完成后，SPEC-0069 的 768 个 expression / declaration / block 对抗组合除总性和重复解析确定性
外，还逐例证明 Lexer 完整覆盖、唯一末尾 EOF、source-local 有界诊断 / AST Span 与 typed root
有效，避免公开 `Debug` 字符串比较间接掩盖结构不变量缺口。

## 2. 范围与需求

- 保持既有 16 × 16 × 3 corpus、入口分布与 1,536 次解析不变，不新增随机或外部输入。
- 每例使用共享输出断言验证 lexeme 从 byte 0 连续覆盖到源码末尾、唯一 EOF 位于末尾，Lexer
  诊断 Span source-local 且有界。
- 两次入口产物均验证 source identity、四张 AST table 的所有 Span 与合并诊断 Span；各入口
  的 typed root 必须能从对应 table 解析。
- 同一源码两次公开 `Debug` 产物继续完全一致；普通用户输入不得触发内部错误或 panic。
- 不增加依赖、生产公开 API、新诊断或合法语法；发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不固定每个对抗组合的具体诊断 code、恢复 AST payload 或节点数量。
- 不扩大前后缀 corpus，不替代 prefix / token mutation、diagnostic witness 或领域精确测试。
- 不比较不同 `SourceMap` 或 source loading order；本 Spec 只强化单例产物结构契约。

## 4. 验收标准

- [x] 768 个组合与 expression / declaration / block 各 256 个入口执行保持固定。
- [x] 全部 Lexer 产物保持连续覆盖、唯一末尾 EOF 与 source-local 有界诊断。
- [x] 1,536 个 Parser 产物保持 source-local 有界 AST / 诊断和可解析 typed root。
- [x] 每例两次公开产物确定一致且无内部错误。
- [x] 矩阵未发现生产缺陷，无需生产修复。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 最终一次成功的 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

保留现有 prefix/suffix runner，入口由固定 enum 分派；复用 tests 私有 `validate_lexed`、
`validate_ast` 与 `validate_diagnostics`，并在每个分支通过对应 typed table 查询 root。结构断言
应用于首次和重复产物，`Debug` 仍只负责同源码的确定性比较。

## 6. 实施计划

1. [x] 审计 SPEC-0069 与后续 mutation 矩阵 → 验证：确认既有对抗矩阵缺少显式输出不变量。
2. [x] 强化 768-case runner 的 Lexer / AST / diagnostic / root 断言 → 验证：1,536 个产物通过。
3. [x] 修复直接缺陷并运行窄测试与窄 Clippy → 验证：6/6 tests、0 warnings，未发现生产缺陷。
4. [x] 同步事实并运行最终一次 workspace 标准基线 → 验证：五条标准命令全部成功，428 tests。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0093`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 独立入口对抗产物断言、必要修复、Architecture 与完成记录 | `test(frontend): strengthen parser entry adversarial invariants (SPEC-0093)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_entry_adversarial --test frontend_adversarial --test parser_token_inventory --locked --offline` | 通过 | 6/6；768 个独立入口组合，未发现生产缺陷 |
| `cargo clippy -p lang-frontend --test parser_entry_adversarial --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

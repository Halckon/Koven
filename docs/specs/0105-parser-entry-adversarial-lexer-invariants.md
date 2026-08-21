# SPEC-0105: 强化独立 Parser 入口对抗矩阵 Lexer 确定性

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-105` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0069、SPEC-0093、SPEC-0103–0104 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 独立 Parser 入口对抗矩阵、共享 Lexer test support、Architecture |
| 语言语义变更 | 否；只补齐既有对抗 corpus 的 Lexer 重复产物验收 |

## 1. Goal

完成后，16 × 16 个前后缀组合分别进入 expression、declaration、block 的 768 个 entry/case，
每例均执行两次生产 Lexer 与两次 Parser，统一证明两个阶段的完整产物结构和确定性。

## 2. 范围与需求

- 保持既有 16 × 16 × 3 corpus、三个入口各 256 个 case 与 768 总计不变。
- 每个 entry/case 执行两次生产 Lexer，共验收 1,536 个 Lexer 产物；两次均验证 source identity、
  连续完整 byte 覆盖、末尾唯一 EOF、diagnostic primary / label Span，并比较完整公开 `Debug`。
- 既有两次 Parser 继续共验收 1,536 个产物；两次均验证四张 AST table、合并诊断 Span、对应
  typed root 与完整公开 `Debug` 确定性。
- 普通用户输入不得导致 Lexer / Parser 内部错误或 panic；对抗 corpus 不固定具体诊断 code、
  AST payload 或节点数量。
- 双 Lexer 逻辑复用 SPEC-0103 的共享 helper，不增加 corpus、依赖、生产 API 或语言语义。

## 3. 非目标

- 不改变 Parser 恢复、诊断、AST 或 grammar。
- 不比较不同 `SourceMap` 或 source loading order。
- 不替代 token mutation、diagnostic witness、prefix truncation 或领域精确测试。

## 4. 验收标准

- [x] 16 × 16 × 3 = 768 个组合与三个入口各 256 个 case 保持固定。
- [x] 1,536 个 Lexer 产物全部满足覆盖、EOF、source、diagnostic Span 与确定性不变量。
- [x] 1,536 个 Parser 产物继续满足 AST、diagnostic、typed root 与确定性不变量。
- [x] 全部组合无内部错误或 panic，既有对抗恢复行为保持通过。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

矩阵 runner 复用 `lex_source_twice` 创建每个 entry/case 的 source map、执行并验证两次 Lexer，
再把首个确定产物交给既有 `parse_twice`；Parser 的两次结构验证与完整 `Debug` 比较保持不变。

## 6. 实施计划

1. [x] 审计独立入口对抗矩阵 → 验证：确认 Parser 双运行完整、Lexer 仍为单次。
2. [x] 接入共享双 Lexer helper → 验证：768 个组合、3,072 个前端产物通过。
3. [x] 运行直接相关窄验收 → 验证：1/1，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0105`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 独立入口对抗矩阵双 Lexer、Architecture 与完成记录 | `test(frontend): strengthen entry adversarial lexer invariants (SPEC-0105)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_entry_adversarial --locked --offline` | 通过 | 1/1；768 个 entry/case、3,072 个前端产物 |
| `cargo clippy -p lang-frontend --test parser_entry_adversarial --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

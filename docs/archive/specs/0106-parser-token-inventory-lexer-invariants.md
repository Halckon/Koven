# SPEC-0106: 强化 token inventory Lexer 确定性

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-106` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0074、SPEC-0094、SPEC-0103–0105 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` token inventory integration test、共享 Lexer test support、Architecture |
| 语言语义变更 | 否；只补齐既有 token inventory 的 Lexer 重复产物验收 |

## 1. Goal

完成后，token inventory 的分类、四个 Parser 入口和定向 string 恢复回归共执行 701 个
source case；每例执行两次生产 Lexer，并保持既有四入口双 Parser 与精确恢复断言。

## 2. 范围与需求

- 保持 42 个 Keyword、11 个 ReservedWord、43 个 Symbol、13 个 atom、4 类 trivia 与 7 个
  invalid 片段不变，并继续证明 120 个片段互异及 L0001–L0008 全覆盖。
- 分类与全库存检查共执行 220 个 source case、440 个 Lexer 产物，并显式锁定执行数。
- 120 个片段进入 expression、declaration、block、file 四个公开入口，共执行 480 个
  entry/case、960 个 Lexer 与 960 个 Parser 产物。
- 独立 declaration 完整 string 回归执行两个 Lexer 与两个 Parser 产物，继续精确锁定单个
  L0017、完整 source Span 与 `Item::Error` root。
- 合计 1,402 个 Lexer 与 962 个 Parser 产物；Lexer 两次均验证 source identity、连续完整 byte
  覆盖、唯一末尾 EOF、diagnostic primary / label Span，并比较完整公开 `Debug`。
- 不增加 corpus、依赖、生产 API、诊断或语言语义。

## 3. 非目标

- 不改变 Lexer 分类、Parser 恢复、诊断、AST 或 grammar。
- 不扩大 token inventory，也不替代各领域精确 Lexer / Parser 测试。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 120-item inventory 的 42 / 11 / 43 / 13 / 4 / 7 分组与互异性保持固定。
- [x] 220 个分类 source case 显式计数，440 个 Lexer 产物满足完整公开产物不变量。
- [x] 四入口 480 个 entry/case 共验证 960 个 Lexer 与 960 个 Parser 产物。
- [x] 定向 string 回归的两个 Lexer 与两个 Parser 产物保持精确 L0017 / Span / error root。
- [x] 合计 1,402 个 Lexer 与 962 个 Parser 产物无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

分类 helper、四入口矩阵和定向回归统一复用 SPEC-0103 的 `lex_source_twice`。四入口继续复用
既有 `parse_twice`；定向回归补齐第二次 declaration parse、逐次公共产物验证和完整 `Debug`
确定性比较。测试用显式执行计数保护分类与全库存检查，避免未来循环静默漏跑。

## 6. 实施计划

1. [x] 审计 token inventory → 验证：确认 701 个 source 执行仍为单 Lexer，定向回归仍为单 Parser。
2. [x] 接入共享双 Lexer 并强化定向双 Parser → 验证：2,364 个前端产物通过。
3. [x] 运行直接相关窄验收 → 验证：3/3，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0106`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | token inventory 双 Lexer、定向双 Parser、Architecture 与完成记录 | `test(frontend): strengthen token inventory lexer invariants (SPEC-0106)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_token_inventory --locked --offline` | 通过 | 3/3；701 个 source case、2,364 个前端产物 |
| `cargo clippy -p lang-frontend --test parser_token_inventory --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

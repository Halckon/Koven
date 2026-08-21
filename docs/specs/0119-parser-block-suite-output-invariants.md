# SPEC-0119: 强化 Block Parser 核心 suite 重复产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-119` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0009、SPEC-0093、SPEC-0103–0105、SPEC-0115–0118 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` block Parser integration test、共享 Parser test support、Architecture |
| 语言语义变更 | 否；只强化既有 block / statement AST、诊断、恢复与复杂度 suite 的重复产物验收 |

## 1. Goal

完成后，`parser_block` 的全部正常用户源码路径均在同一 source identity 上执行两次生产 Lexer
与两次独立 block Parser，并在既有精确 AST / diagnostic 断言之前验证完整公开产物确定性；
foreign map 与 nesting-limit 内部错误路径继续直接调用入口以锁定错误返回。

## 2. 范围与需求

- 保持现有 22 个 Rust 测试、全部源码 corpus、循环矩阵、statement / item / expression payload、
  精确 Span / diagnostic、owner recovery 与复杂度断言不变。
- 全部正常源码统一经现有 `parsed` / `parsed_ok` / `fingerprints` 路径调用 typed 双前端 helper。
- 每个正常 source 的两次 Lexer 均验证 source identity、连续完整 byte 覆盖、唯一末尾 EOF、
  diagnostic primary / label Span 与完整 `Debug` 确定性。
- 每个正常 source 的两次 Parser 均验证 AST 与 diagnostic Span、block typed root，并比较完整
  `Debug` 产物；首个确定产物继续进入既有领域断言。
- `source_identity_and_nested_block_budget_are_internal_boundaries` 中的 foreign-map case 保留单次
  Parser 调用，避免 helper 先以正确 map 消费其故意构造的无效输入。
- 同一测试中的 nesting-limit case 保留单次 Parser 调用，因为验收目标是精确返回
  `ParserInternalError::NestingLimitExceeded`，不存在可比较的成功产物。
- 共享 Parser support 增加 block typed wrapper，并把三个 typed wrapper 共用的 source lookup /
  双 Lexer 准备步骤收敛为私有 helper；不增加 file 或其他入口抽象。
- 不增加语料、依赖、生产 API、新诊断或语言语义。

## 3. 非目标

- 不改变 block / statement 语法、AST、诊断、恢复、递归预算或测试期望。
- 不把预期内部错误包装成 panic，也不声称失败返回具有成功产物确定性。
- 不在本 Spec 同时迁移其他 Parser feature targets。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 22 个 block Parser 测试全部保持通过，无 ignored / filtered。
- [x] 正常用户源码路径全部经双 Lexer / 双 Parser helper，公开产物不变量与既有领域断言均通过。
- [x] 文件内剩余直接 `lex` / `parse_block` 仅属于 foreign-map 与 nesting-limit 定向路径。
- [x] foreign-map source identity 与 nesting-limit 错误断言保持通过。
- [x] 全部普通用户语法错误形成结构化产物，无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] `parser_block` 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

`parser_test_assertions::parse_block_twice` 复用私有的 source lookup / 双 Lexer 准备入口，在首个
确定 Lexer 产物上运行两次公开 block Parser，逐次验证 AST / diagnostics / typed root 并比较
完整 `Debug`；测试文件现有 `(SourceMap, ParsedBlock)` 领域 helper 形状保持不变。

## 6. 实施计划

1. [x] 审计 block suite → 验证：22 个测试；正常路径集中于 `parsed`，仅两个内部错误 case 直接调用入口。
2. [x] 接入 typed block 双 Parser helper → 验证：正常路径不再直接单次调用。
3. [x] 运行直接相关窄验收 → 验证：22/22，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0119`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | block suite 双前端产物、共享 support、Architecture 与完成记录 | `test(frontend): strengthen block suite invariants (SPEC-0119)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_block --locked --offline` | 通过 | 22 passed；0 failed / ignored / measured / filtered |
| `cargo clippy -p lang-frontend --test parser_block --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

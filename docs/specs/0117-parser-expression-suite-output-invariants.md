# SPEC-0117: 强化表达式 Parser 核心 suite 重复产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-117` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0007、SPEC-0093、SPEC-0103–0105、SPEC-0115–0116 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` expression Parser integration test、共享 Lexer / Parser test support、Architecture |
| 语言语义变更 | 否；只强化既有表达式 AST、诊断、恢复与复杂度 suite 的重复产物验收 |

## 1. Goal

完成后，`parser_expression` 的全部正常用户源码路径均在同一 source identity 上执行两次生产
Lexer 与两次表达式 Parser，并在既有精确 AST / diagnostic 断言之前验证完整公开产物确定性；
foreign map 与 nesting-limit 内部错误路径继续直接调用入口以锁定错误返回。

## 2. 范围与需求

- 保持现有 54 个 Rust 测试、全部源码 corpus、循环矩阵、AST payload、精确 Span / diagnostic、
  source-order 与复杂度断言不变。
- `parse_fingerprints`、`assert_parses`、`parsed_case`、`parsed_case_with_diagnostics` 四条共享路径，
  以及使用自定义 source order / name 的正常定向路径，统一调用 typed 双前端 helper。
- 每个正常 source 的两次 Lexer 均验证 source identity、连续完整 byte 覆盖、唯一末尾 EOF、
  diagnostic primary / label Span 与完整 `Debug` 确定性。
- 每个正常 source 的两次 Parser 均验证 AST 与 diagnostic Span、expression typed root，并比较
  完整 `Debug` 产物；首个确定产物继续进入既有领域断言。
- `parsing_rejects_lexed_files_from_another_source_map` 保留单次 foreign-map Parser 调用，避免 helper
  先以正确 map 消费其故意构造的无效输入。
- 六个 nesting-limit case 保留单次 Parser 调用，因为验收目标是精确返回
  `ParserInternalError::NestingLimitExceeded`，不存在可比较的成功产物。
- 共享 Lexer helper 增加“已加载 source 双运行”入口；新增的 Parser support 只实现当前需要的
  expression typed wrapper，不提前加入声明、block 或 file 抽象。
- 不增加语料、依赖、生产 API、新诊断或语言语义。

## 3. 非目标

- 不改变 Pratt binding power、AST、诊断、恢复、递归预算或测试期望。
- 不把预期内部错误包装成 panic，也不声称失败返回具有成功产物确定性。
- 不在本 Spec 同时迁移其他 11 个 Parser feature targets；它们后续按入口复用已验证 support。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 54 个 expression Parser 测试全部保持通过，无 ignored / filtered。
- [x] 正常用户源码路径全部经双 Lexer / 双 Parser helper，公开产物不变量与既有领域断言均通过。
- [x] 文件内剩余直接 `lex` / `parse_expression` 仅属于 foreign-map 与 nesting-limit 定向路径。
- [x] foreign-map source identity 与六个 nesting-limit 错误断言保持通过。
- [x] 全部普通用户语法错误形成结构化产物，无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] `parser_expression` 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

`lexer_matrix_assertions` 新增接受既有 `SourceMap` / `SourceId` 的 `lex_loaded_source_twice`，原有
创建 source 的 wrapper 继续委托它。`parser_test_assertions::parse_expression_twice` 在该 Lexer
产物上运行两次公开 expression Parser，逐次验证 AST / diagnostics / typed root 并比较完整
`Debug`；测试文件保留现有返回 `(SourceMap, ParsedExpression)` 的领域 helper 形状。

## 6. 实施计划

1. [x] 审计 12 个核心 Parser suites → 验证：共 222 个测试；expression target 最大且可分离正常 / 内部错误路径。
2. [x] 接入已加载 source 双 Lexer 与 typed expression 双 Parser helper → 验证：正常路径不再直接单次调用。
3. [x] 运行直接相关窄验收 → 验证：54/54，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0117`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | expression suite 双前端产物、共享 support、Architecture 与完成记录 | `test(frontend): strengthen expression suite invariants (SPEC-0117)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_expression --locked --offline` | 通过 | 54 passed；0 failed / ignored / measured / filtered |
| `cargo clippy -p lang-frontend --test parser_expression --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

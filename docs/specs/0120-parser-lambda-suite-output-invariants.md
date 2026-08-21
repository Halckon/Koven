# SPEC-0120: 强化 Lambda Parser 核心 suite 重复产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-120` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0010、SPEC-0093、SPEC-0103–0105、SPEC-0115–0119 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` lambda Parser integration test、Architecture |
| 语言语义变更 | 否；只强化既有 lambda AST、上下文判定、诊断、恢复与复杂度 suite 的重复产物验收 |

## 1. Goal

完成后，`parser_lambda` 的全部正常用户源码路径均在同一 source identity 上执行两次生产 Lexer
与两次对应公开 Parser，并在既有精确 AST / diagnostic 断言之前验证完整公开产物确定性；
foreign map 与 nesting-limit 内部错误路径继续直接调用 expression 入口以锁定错误返回。

## 2. 范围与需求

- 保持现有 16 个 Rust 测试、全部源码 corpus、lambda header / body / postfix / interpolation、
  expression / declaration / block 上下文、精确 Span / diagnostic、owner recovery 与复杂度断言不变。
- `parsed_expression`、`parsed_block`、`parsed_declaration` 三条正常路径分别调用现有 typed 双前端
  helper；自定义 source-loading-order 正常路径同样改用 expression helper。
- 小调用栈线程内的正常 lambda source 继续实际执行，且通过同一 helper 验收双 Lexer / 双 Parser。
- 每个正常 source 的两次 Lexer 均验证 source identity、连续完整 byte 覆盖、唯一末尾 EOF、
  diagnostic primary / label Span 与完整 `Debug` 确定性。
- 每个正常 source 的两次 Parser 均验证 AST 与 diagnostic Span、对应 typed root，并比较完整
  `Debug` 产物；首个确定产物继续进入既有领域断言。
- source-loading-order 测试中的 foreign-map case 保留单次 Parser 调用；递归预算测试中的深层
  source 保留单次 Parser 调用并精确断言 `ParserInternalError::NestingLimitExceeded`。
- 不增加共享抽象、语料、依赖、生产 API、新诊断或语言语义。

## 3. 非目标

- 不改变 lambda / block / declaration 语法、AST、诊断、恢复、递归预算或测试期望。
- 不把预期内部错误包装成 panic，也不声称失败返回具有成功产物确定性。
- 不在本 Spec 同时迁移其他 Parser feature targets。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 16 个 lambda Parser 测试全部保持通过，无 ignored / filtered。
- [x] expression / declaration / block 的正常用户源码路径全部经对应双前端 helper。
- [x] source-loading-order 与小调用栈正常路径同样满足双 Lexer / 双 Parser 产物不变量。
- [x] 文件内剩余直接 `lex` / `parse_expression` 仅属于 foreign-map 与 nesting-limit 定向路径。
- [x] foreign-map source identity 与 nesting-limit 错误断言保持通过。
- [x] 全部普通用户语法错误形成结构化产物，无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] `parser_lambda` 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

lambda suite 直接组合 `parser_test_assertions` 已有的 expression / declaration / block typed wrapper；
三个本地领域 helper 仍返回原有产物类型和 `SourceMap`。自定义 source order 路径在既有 map / id 上
调用 expression wrapper，不改其跨加载顺序 shape 比较；两个故意失败的内部边界继续绕过 wrapper。

## 6. 实施计划

1. [x] 审计 lambda suite → 验证：16 个测试；三入口正常路径与两个内部错误 case 边界明确。
2. [x] 接入三种 typed 双前端 helper → 验证：全部正常路径不再直接单次调用。
3. [x] 运行直接相关窄验收 → 验证：16/16，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0120`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | lambda suite 三入口双前端产物、Architecture 与完成记录 | `test(frontend): strengthen lambda suite invariants (SPEC-0120)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_lambda --locked --offline` | 通过 | 16 passed；0 failed / ignored / measured / filtered |
| `cargo clippy -p lang-frontend --test parser_lambda --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

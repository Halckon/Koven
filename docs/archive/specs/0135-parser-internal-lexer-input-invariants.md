# SPEC-0135: 强化 Parser 私有算法测试的 Lexer 输入不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-135` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0093、SPEC-0129、SPEC-0130–0134 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer 测试支持、Parser engine / lambda-header / strict-call-trial 私有单元测试、Architecture |
| 语言语义变更 | 否；只强化 Parser 私有算法测试消费的 Lexer 产物验收 |

## 1. Goal

完成后，Parser 私有 engine、lambda-header trial 与 strict-call trial 单元测试实际执行的 68 条源码
路径均使用双 Lexer 产物；两次产物逐次验证 source identity、连续 byte 覆盖、唯一 EOF 与诊断
Span，并比较全部私有字段确定性，再把首个确定产物交给既有 Parser 内部不变量断言。

## 2. 范围与需求

- 保持现有 12 个 engine、6 个 lambda-header trial、3 个 strict-call trial 与 3 个 diagnostic 单元
  测试不变；本 Spec 只迁移其中实际构造 Lexer 输入的 21 个 Parser 测试。
- engine 测试的 43 条、lambda-header trial 的 8 条和 strict-call trial 的 17 条实际源码路径，
  共 68 条源码，均运行两次生产 Lexer。
- 新增仅在 `cfg(test)` 下编译的 Lexer typed helper；每次产物验证 source identity、连续非空
  lexeme byte 覆盖、唯一末尾 EOF，以及 diagnostic primary / label Span 的 source-local 有界性。
- 两次产物直接比较 `source_id`、完整 `lexemes` 与完整 `diagnostics` 私有字段，不通过格式化字符串
  间接建立确定性证据。
- Parser engine 继续验证词法 owner、恢复、诊断、dispatch 和线性复杂度；trial 测试继续验证缓存、
  递归预算、状态数与 inspection 复杂度。
- Parser 私有测试模块不再直接调用生产 `lex`；统一消费 helper 返回的首个确定产物。
- 不增加依赖、生产公开 API、语料、诊断或语言语义。

## 3. 非目标

- 不改变 Scanner、Parser engine、trial、lambda-header 或恢复实现。
- 不把私有 Parser 算法执行机械改为两次；本 Spec 只封闭其 Lexer 输入确定性。
- 不替代 integration suite 的公开 AST / root / directive 产物验收。
- 不为测试 helper 增加非测试构建可见的 API。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 68 条实际源码路径均经双 Lexer helper，共验证 136 个 Lexer 产物。
- [x] 每个产物满足 source identity、byte coverage、唯一 EOF 与 diagnostic Span 不变量。
- [x] 两次产物的全部私有字段确定一致。
- [x] 三个 Parser 私有测试模块不再直接调用生产 `lex`。
- [x] 24 个 `lang-frontend` library 单元测试全部通过，无 ignored / filtered。
- [x] Parser 内部恢复、缓存、递归预算和线性复杂度断言保持不变。
- [x] 未发现生产缺陷；本 Spec 只修改测试支持、测试调用与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

在 `lexer/test_support.rs` 提供 `cfg(test)` 的 `lex_test_source_twice`。helper 位于 Lexer 模块内，
因此可以直接比较 `LexedFile` 的全部私有字段，同时通过公开 getter 逐项验证产物结构；非测试构建
不会编译或暴露该入口。Parser 的三个私有测试模块只替换 Lexer 构造点，不改后续 Parser 调用、
counter 或断言。

## 6. 实施计划

1. [x] 全量审计 Parser 私有 Lexer 输入 → 验证：43 + 8 + 17 = 68 条实际源码路径。
2. [x] 新增 typed 双 Lexer helper 并迁移调用点 → 验证：三个模块无直接 `lex`。
3. [x] 运行直接相关窄验收 → 验证：24/24，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0135`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 私有 Lexer 输入 helper、调用迁移、Architecture 与完成记录 | `test(frontend): strengthen parser internal lexer inputs (SPEC-0135)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --lib --locked --offline`：24 passed，0 failed / ignored /
  measured / filtered。
- `cargo clippy -p lang-frontend --lib --tests --locked --offline -- -D warnings`：通过，0 warnings。
- workspace 标准基线：`cargo fmt --all -- --check`、`cargo check --workspace --all-targets
  --locked --offline`、`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`、
  `cargo test --workspace --all-targets --locked --offline`、`cargo build -p lang-cli --locked
  --offline` 均通过；全量测试 428 passed，0 failed / ignored / measured / filtered。

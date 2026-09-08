# SPEC-0131: 强化类型检查核心 suite 的前端输入不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P2-131` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0019–0023、SPEC-0067、SPEC-0130 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 类型检查核心 integration test 的 Lexer / Parser 前置流水线、Architecture |
| 语言语义变更 | 否；只强化进入名称解析与类型检查前的公开产物验收 |

## 1. Goal

完成后，`type_checking` 核心 integration suite 实际执行的 49 条源码路径均在名称解析与类型检查前
执行两次生产 Lexer 与两次完整文件 Parser，并验证两阶段完整公开产物确定性、source-local Span、
AST、诊断、文件 roots 与 directive Span；名称解析和类型检查领域断言保持不变。

## 2. 范围与需求

- 保持现有基础类型、局部推导、返回契约、名义 / 泛型 / interface、`when` / smart cast、
  `Copyable`、结构化解构、callable、顺序容器、递归规模与 Phase 2 fixture 断言不变。
- 统一 `parse()` 前置入口复用现有 typed file helper；29 个 Rust 测试实际执行的 49 条源码均运行
  两次 Lexer 与两次完整文件 Parser，向后续阶段返回首个已验证的确定产物。
- 每次 Lexer 产物验证 source identity、连续 byte 覆盖、唯一末尾 EOF、diagnostic primary / label
  Span 与完整 `Debug` 确定性。
- 每次 Parser 产物验证 source identity、AST 全表、diagnostic primary / label Span、全部 file roots、
  package / import directive Span 与完整 `Debug` 确定性。
- 保持进入名称解析与类型检查的 Parser 零诊断门禁。
- 不增加语料、依赖、生产 API、诊断或语言语义。

## 3. 非目标

- 不改变名称解析、类型推导、类型诊断、环境或 deferred 契约。
- 不把全部名称解析或类型检查调用机械改为重复执行；既有专门确定性测试保持原职责。
- 不同时迁移 callable、container 或 copyability 的独立 integration target。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] `type_checking` 的 49 条实际源码路径均经共享双 Lexer / 双 Parser helper。
- [x] 98 个 Lexer 与 98 个 Parser 产物满足公开结构、Span 与确定性不变量。
- [x] 现有 29 个类型检查测试及全部领域断言保持通过，无 ignored / filtered。
- [x] 测试文件不再直接调用 Lexer / Parser。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

`type_checking.rs` 的单一 `parse()` helper 改用
`parser_test_assertions::parse_file_twice`。该共享 helper 已封装双 Lexer、双完整文件 Parser、
逐次结构校验和完整公开产物比较；名称解析与类型检查测试仍只消费首个确定 `ParsedFile`，不改变
生产阶段边界或领域断言。

## 6. 实施计划

1. [x] 审计类型检查前置流水线 → 验证：49 条实际源码路径均汇入单次 Lexer / Parser helper。
2. [x] 迁移至共享 typed file helper → 验证：测试文件无直接 Lexer / Parser 调用。
3. [x] 运行直接相关窄验收 → 验证：29/29，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0131`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 类型检查核心 suite 前置 Lexer / Parser 不变量、Architecture 与完成记录 | `test(frontend): strengthen type checking input invariants (SPEC-0131)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test type_checking --locked --offline` | 通过 | 29 passed；0 failed / ignored / measured / filtered |
| `cargo clippy -p lang-frontend --test type_checking --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

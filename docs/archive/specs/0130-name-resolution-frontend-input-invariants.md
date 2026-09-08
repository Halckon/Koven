# SPEC-0130: 强化名称解析 suite 的前端输入不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P2-130` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0018、SPEC-0093、SPEC-0115、SPEC-0128、SPEC-0129 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 名称解析 integration test 的 Lexer / Parser 前置流水线、Architecture |
| 语言语义变更 | 否；只强化进入名称解析前的公开产物验收 |

## 1. Goal

完成后，`name_resolution` integration suite 的全部 14 条源码路径均在名称解析前执行两次生产
Lexer 与两次完整文件 Parser，并验证两阶段完整公开产物确定性、source-local Span、AST、诊断、
文件 roots 与 directive Span；名称解析领域断言保持不变。

## 2. 范围与需求

- 保持现有名称作用域、双命名空间、前向引用、overload、成员、遮蔽、泛型、object、loop、
  未解析名称、foreign `SourceMap` 与 Phase 2 fixture 断言不变。
- 统一 `parsed()` 前置入口复用现有 typed file helper；每条源码运行两次 Lexer 与两次完整文件
  Parser，向后续名称解析返回首个已验证的确定产物。
- 每次 Lexer 产物验证 source identity、连续 byte 覆盖、唯一末尾 EOF、diagnostic primary / label
  Span 与完整 `Debug` 确定性。
- 每次 Parser 产物验证 source identity、AST 全表、diagnostic primary / label Span、全部 file roots、
  package / import directive Span 与完整 `Debug` 确定性。
- 保持进入名称解析的 Parser 零诊断门禁。
- 不增加语料、依赖、生产 API、诊断或语言语义。

## 3. 非目标

- 不改变名称收集、作用域、遮蔽、解析、诊断或环境契约。
- 不把名称解析本身改为重复执行；其确定性与非修改性继续由既有领域测试验收。
- 不同时迁移其他 Phase 2 类型检查 suite。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] `name_resolution` 的 14 个源码路径均经共享双 Lexer / 双 Parser helper。
- [x] 28 个 Lexer 与 28 个 Parser 产物满足公开结构、Span 与确定性不变量。
- [x] 现有 13 个名称解析测试及全部领域断言保持通过，无 ignored / filtered。
- [x] 测试文件不再直接调用 Lexer / Parser。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

`name_resolution.rs` 的单一 `parsed()` helper 改用
`parser_test_assertions::parse_file_twice`。该共享 helper 已封装双 Lexer、双完整文件 Parser、
逐次结构校验和完整公开产物比较；名称解析测试仍只消费首个确定 `ParsedFile`，不改变生产阶段
边界或领域断言。

## 6. 实施计划

1. [x] 审计名称解析前置流水线 → 验证：14 条源码路径均汇入单次 Lexer / Parser helper。
2. [x] 迁移至共享 typed file helper → 验证：测试文件无直接 Lexer / Parser 调用。
3. [x] 运行直接相关窄验收 → 验证：13/13，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0130`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 名称解析前置 Lexer / Parser 不变量、Architecture 与完成记录 | `test(frontend): strengthen name resolution input invariants (SPEC-0130)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test name_resolution --locked --offline` | 通过 | 13 passed；0 failed / ignored / measured / filtered |
| `cargo clippy -p lang-frontend --test name_resolution --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

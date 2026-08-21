# SPEC-0103: 强化 Lexer 固定词与符号边界矩阵产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-103` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0073、SPEC-0093–0102 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer boundary / frontend adversarial matrices、共享 Lexer test support、Architecture |
| 语言语义变更 | 否；只强化既有固定词、符号和注释边界产物验收 |

## 1. Goal

完成后，2,127 个固定词、符号最长匹配、compound word 与注释优先级 case 均执行两次生产
Lexer，并以共享断言验证完整产物和确定性；完整文件对抗矩阵复用同一双 Lexer helper。

## 2. 范围与需求

- 保持 330 个固定词 mutation、1,596 个固定符号 pair、199 个 compound word boundary 与
  2 个注释 opener case 不变，测试内分别锁定分组计数和总计 2,127。
- 每个 case 执行两次生产 Lexer，共验收 4,254 个产物；两次均验证 `LexedFile.source_id()`、
  lexeme 连续完整 byte 覆盖、唯一且位于末尾的 EOF、diagnostic primary / label Span。
- 全部 2,127 个合法边界源码必须保持零 Lexer 诊断；既有精确首 token kind / Span、单一
  identifier、compound boundary 与注释优先级断言保持不变。
- 同一源码两次完整公开 `Debug` 产物必须一致，覆盖 lexeme kind / Span 与完整诊断细节。
- 双 Lexer helper 由 boundary matrix 与 frontend adversarial matrix 共同复用，不在两处维护副本。
- 不增加 corpus、依赖、生产公开 API、新诊断或合法词法形式。

## 3. 非目标

- 不改变关键字、保留字、固定符号或注释词法规则。
- 不替代 Lexer 领域测试对错误码、字符串模式、Unicode 与恢复边界的精确验收。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 四组 case 计数保持 330 / 1,596 / 199 / 2，总计 2,127。
- [x] 4,254 个 Lexer 产物全部满足完整覆盖、EOF、source 与 diagnostic Span 不变量。
- [x] 2,127 个源码全部零诊断且既有精确分类 / Span 断言保持通过。
- [x] 每个源码的两次完整 Lexer 公开产物确定一致。
- [x] 两个矩阵只维护一份双 Lexer helper。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增 tests 私有 `lexer_matrix_assertions`，负责创建单 source map、对同一 source identity 执行
两次 `lex`、逐次调用由调用方注入的产物验证函数并比较完整 `Debug`，返回首个产物供领域断言
与后续 Parser 使用。完整文件矩阵传入既有共享验证器；boundary matrix 使用职责更窄的完整
Lexer 验证器并统一断言零诊断，不把该约束错误施加到对抗输入。

## 6. 实施计划

1. [x] 审计 Lexer boundary matrix → 验证：确认单次运行、局部弱覆盖和分散 helper 缺口。
2. [x] 提取共享双 Lexer helper 并强化 2,127-case matrix → 验证：4,254 个产物通过。
3. [x] 运行直接相关窄验收 → 验证：6/6，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0103`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 共享双 Lexer helper、边界矩阵产物不变量、Architecture 与完成记录 | `test(frontend): strengthen lexer boundary invariants (SPEC-0103)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test lexer_boundary_matrix --test frontend_adversarial --locked --offline` | 通过 | 6/6；2,127 个边界源码、4,254 个 Lexer 产物 |
| 同一组两个 integration tests 的窄 Clippy | 通过 | `-D warnings`；0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

# SPEC-0095: 强化 Parser lexical-owner 矩阵产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-095` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并简化验收环节；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0075、SPEC-0093、SPEC-0094 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Parser lexical-owner 集成测试、Architecture |
| 语言语义变更 | 否；只强化公开产物与恢复边界验收 |

## 1. Goal

完成后，`parser_lexical_owner_matrix` 除 Lexer 覆盖、词法错误码和重复解析确定性外，还会对
两次完整文件解析的 AST / diagnostic Span、文件根引用与可恢复 case 的末尾 sentinel 边界
执行显式断言，避免仅靠 `Debug` 相等掩盖同样错误的公开产物。

## 2. 范围与需求

- 保持 SPEC-0075 的 16 个位置、4 个可恢复 owner、5 个 terminal owner 和 144 个 case 不变。
- 复用共享前端产物断言，验证 Lexer source identity、连续 byte 覆盖、唯一末尾 EOF 以及
  Lexer diagnostic Span。
- 对每个 case 的两次解析分别验证 AST 全表节点与 Parser diagnostic 主 / label Span 都
  source-local 且不越界，并验证每个文件根都能从 item arena 解引用。
- 对 64 个可恢复 case 的两次解析分别证明 `val after = 1` 是最后一个完整文件根，而非仅在
  任意嵌套或恢复节点中出现同名 token。
- 不增加依赖、公开 API、新诊断或合法语法。

## 3. 非目标

- 不扩大 lexical owner、语法位置或随机输入集合。
- 不改变 Parser 恢复策略、AST 形态、诊断含义或 guide。
- 不重复验证各语法领域的精确 AST payload；本矩阵只锁定通用公开产物与恢复边界。

## 4. 验收标准

- [x] 144 个 case 的 Lexer 产物均满足 source / coverage / EOF / diagnostic Span 不变量。
- [x] 288 次解析产物均满足 AST / diagnostic Span 与文件根可解引用不变量。
- [x] 64 个可恢复 case 的 128 次解析均以精确 sentinel 源码作为最后文件根。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

测试直接复用 `frontend_output_assertions` 与 `parser_mutation_assertions`，不复制生产 Parser
逻辑。两次解析先各自经过结构化不变量验证，再比较公开 `Debug` 产物；sentinel 通过最后
root 的完整 `Span` 切片验证。

## 6. 实施计划

1. [x] 审计现有 lexical-owner 矩阵 → 验证：确认缺少 AST / Parser diagnostic Span、root
   引用及精确末尾 sentinel 边界断言。
2. [x] 强化两次解析的公开产物断言 → 验证：144 case、288 次解析和 128 次 sentinel 边界检查。
3. [x] 运行直接相关窄验收 → 验证：2/2，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0095`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | lexical-owner 产物不变量、Architecture 与完成记录 | `test(frontend): strengthen parser lexical owner invariants (SPEC-0095)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_lexical_owner_matrix --locked --offline` | 通过 | 2/2；144 case、288 次解析、128 次精确 sentinel 边界检查 |
| `cargo clippy -p lang-frontend --test parser_lexical_owner_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

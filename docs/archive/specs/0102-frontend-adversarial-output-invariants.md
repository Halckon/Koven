# SPEC-0102: 强化完整文件对抗矩阵 Lexer / Parser 产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-102` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0068、SPEC-0093–0101 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 完整文件对抗矩阵、Architecture |
| 语言语义变更 | 否；只强化既有对抗 corpus 的 Lexer / Parser 产物验收 |

## 1. Goal

完成后，324 个完整文件对抗组合与一个终止字符定向回归均执行两次 Lexer 和两次 Parser，
并复用共享完整文件产物断言验证完整公开结构，不再由较弱的私有 Span 指纹间接代表确定性。

## 2. 范围与需求

- 保持既有 18 × 18 corpus、324 个组合与定向 `L0007` 回归不变，不增加随机或外部输入。
- 325 个源码分别执行两次生产 Lexer；两次产物均验证 source identity、连续完整 byte 覆盖、
  唯一末尾 EOF、diagnostic primary / label Span，并比较完整公开 `Debug` 产物。
- 325 个源码分别执行两次生产完整文件 Parser；两次产物均验证 source identity、四张 AST table、
  diagnostic primary / label、typed roots 与 package / import 全部子 Span。
- 两次 Parser 完整公开 `Debug` 产物必须一致，覆盖 AST payload、文件头、根、Span 与完整诊断细节。
- 定向回归继续精确锁定 Lexer 与合并 Parser 诊断仅含 `L0007` 及其 message / primary Span。
- 不增加依赖、生产公开 API、新诊断、合法语法或 corpus。

## 3. 非目标

- 不固定 324 个组合各自的诊断 code、AST payload 或节点数量。
- 不替代 token mutation、prefix truncation、diagnostic witness 或领域精确测试。
- 不改变 Lexer、Parser、AST、guide 或用户可观察语义。

## 4. 验收标准

- [x] 18 × 18 corpus、324 个组合与定向回归保持固定。
- [x] 650 个 Lexer 产物全部满足共享结构不变量且逐源码确定一致。
- [x] 650 个 Parser 产物全部满足共享完整文件不变量且逐源码确定一致。
- [x] 定向 `L0007` 的精确诊断与 lexical-owner 恢复保持通过。
- [x] 删除被共享断言完整替代的私有弱指纹，不保留重复测试框架。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

`frontend_adversarial` 增加局部双 Lexer helper，复用 `validate_lexed` 验证两次产物并比较完整
`Debug`；完整文件解析直接复用 SPEC-0099 的 `parse_file_twice`，由共享 helper 验证 AST、诊断、
typed root 与文件头子结构。局部 helper 不进入共享模块，避免其他 integration test crate 出现
未使用 API。

## 6. 实施计划

1. [x] 审计完整文件对抗矩阵 → 验证：确认私有指纹遗漏 label、AST identity、文件头子 Span 与 Lexer 重复确定性。
2. [x] 复用共享 Parser 断言并增加双 Lexer 验收 → 验证：325 个源码、1,300 个前端产物通过。
3. [x] 运行直接相关窄验收 → 验证：2/2，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0102`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 对抗矩阵共享产物断言、Architecture 与完成记录 | `test(frontend): strengthen adversarial output invariants (SPEC-0102)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test frontend_adversarial --locked --offline` | 通过 | 2/2；325 个源码、1,300 个前端产物 |
| `cargo clippy -p lang-frontend --test frontend_adversarial --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

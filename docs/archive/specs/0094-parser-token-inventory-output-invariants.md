# SPEC-0094: 强化 Parser token inventory 产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-094` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0073、SPEC-0074、SPEC-0093 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四入口 token inventory 产物断言、必要直接修复、Architecture |
| 语言语义变更 | 否；只强化既有 480-case 库存矩阵的结构与 Span 验收 |

## 1. Goal

完成后，SPEC-0074 的全部 120 个词法片段在 expression / declaration / block / file 四个入口
中的 480 个组合，除总性与确定性外，还显式证明 Lexer 连续覆盖、唯一 EOF、source-local AST /
诊断、三类 typed root、完整文件 roots 与 package/import directive Span 有效。

## 2. 范围与需求

- 保持 42 个 Keyword、11 个 ReservedWord、43 个 Symbol、13 个 atom、4 类 trivia 与 7 个
  invalid 片段组成的 120-item inventory，以及 480 个入口组合 / 960 次解析不变。
- 库存分类与入口 runner 全部复用共享 Lexer 输出断言，验证连续 byte 覆盖、唯一末尾 EOF 及
  Lexer 诊断 Span source-local 且有界。
- expression / declaration / block 的两次产物均验证 source identity、四张 AST table、合并诊断
  与对应 typed root；file 的两次产物额外验证全部 roots、package/import/alias/segment Span。
- 同一源码两次公开 `Debug` 产物继续完全一致，任何普通库存片段不得触发内部错误或 panic。
- 保留完整 string 独立声明的 L0017 / Error Item 精确回归；不增加依赖或生产公开 API。

## 3. 非目标

- 不固定每个片段在每个入口中的具体恢复 AST、诊断集合或 root 数量。
- 不扩充 token inventory，不替代 Lexer boundary、diagnostic witness 或领域精确测试。
- 不要求 owner-affecting 片段后的 file/block sentinel 必然存活；该契约由 lexical-owner 矩阵负责。

## 4. 验收标准

- [x] 120 个互异片段与 480 个入口组合保持固定，960 次解析全部执行。
- [x] 所有 Lexer 产物连续覆盖源码、唯一末尾 EOF 且诊断 Span source-local 有界。
- [x] 720 个独立入口产物保持有效 typed root、AST / 诊断 Span 与 source identity。
- [x] 240 个完整文件产物保持有效 roots、AST / 诊断及 package/import directive Span。
- [x] 每例两次公开产物确定一致且无内部错误；既有 L0017 回归保持不变。
- [x] 矩阵未发现生产缺陷，无需生产修复。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 最终一次成功的 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

保留 inventory / wrapper 单一真源，将字符串函数指针改为固定入口 enum。三类独立入口通过
对应 typed table 查询 root；file 分支遍历所有 root，并逐项验证 package/import directive 的
完整、keyword、segment、wildcard 与 alias Span。共享 helper 负责通用 Lexer / AST / diagnostic
边界，`Debug` 只负责同源码确定性。

## 6. 实施计划

1. [x] 审计 SPEC-0074 与 SPEC-0093 → 验证：确认 inventory 缺少显式四入口产物不变量。
2. [x] 强化 480-case runner 的 Lexer / AST / diagnostic / root / directive 断言 → 验证：960 个产物通过。
3. [x] 修复直接缺陷并运行窄测试与窄 Clippy → 验证：33/33 tests、0 warnings，未发现生产缺陷。
4. [x] 同步事实并运行最终一次 workspace 标准基线 → 验证：五条标准命令全部成功，428 tests。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0094`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | token inventory 四入口产物断言、必要修复、Architecture 与完成记录 | `test(frontend): strengthen parser token inventory invariants (SPEC-0094)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_token_inventory --test parser_entry_adversarial --test frontend_adversarial --test parser_file --locked --offline` | 通过 | 33/33；480 个四入口组合，未发现生产缺陷 |
| `cargo clippy -p lang-frontend --test parser_token_inventory --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

# SPEC-0099: 强化完整文件恢复矩阵共享产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-099` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并简化验收环节；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0079–0084、SPEC-0093–0098 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 六个完整文件恢复矩阵的共享测试断言、Architecture |
| 语言语义变更 | 否；只强化两次完整文件 Parser 产物验收 |

## 1. Goal

完成后，前缀截断、token 删除 / 重复 / 相邻交换及 lexical poison 替换 / 插入六个完整文件
恢复矩阵，会通过同一共享入口对两次 Parser 产物分别验证 AST、诊断、文件 roots 与 directive
Span，而不再只结构化验证第一次产物后依赖 `Debug` 相等推断第二次合法。

## 2. 范围与需求

- 强化 `frontend_matrix_assertions::parse_file_twice`，覆盖其全部六个调用方，不复制矩阵逻辑。
- 两次产物分别验证 source identity、AST 四张表、diagnostic 主 / label Span、全部文件 roots，
  以及 package / import / segment / wildcard / alias Span。
- 保持两次完整公开 `Debug` 产物确定性比较，并继续返回第一次产物供各矩阵执行领域断言。
- 保持 22-file corpus、1,373 个 UTF-8 前缀、396 个删除、792 个 poison 替换、396 个重复、
  836 个 poison 插入与 374 个相邻交换 case 不变；共 4,167 个变异 / 前缀 case、8,334 次解析。
- `prefix_ends` 继续证明空前缀、每个 Unicode scalar 结束位置与完整源码，无效 UTF-8 不进入
  SourceMap。
- 不增加语料、依赖、公开 API、新诊断或合法语法。

## 3. 非目标

- 不改变各 mutation 的生成、lexical-owner 分类、sentinel 或精确 poison Span 断言。
- 不固定每个 EOF 截断位置的诊断码或 AST 恢复形态。
- 不改变 Lexer / Parser 恢复语义、资源预算或 guide。

## 4. 验收标准

- [x] 六个共享调用方的两次产物都验证 AST / diagnostic Span 与 source identity。
- [x] 两次产物的所有文件 roots 与实际 directive Span 均有效。
- [x] 4,167 个变异 / 前缀 case 的既有领域断言与确定性保持通过。
- [x] 1,373 个 prefix case 继续覆盖全部 UTF-8 scalar 边界。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 六个直接相关矩阵测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

把完整文件产物的 source / AST / diagnostic / root / directive 验证集中在
`frontend_matrix_assertions` 私有 helper；仅把通用 `validate_span` 提升为 integration test
support 范围可见，不改变生产 crate API。所有矩阵继续使用各自现有领域断言。

## 6. 实施计划

1. [x] 审计 prefix truncation 与共享入口 → 验证：scalar 边界已完整，第二次文件产物及
   roots / directive 缺少结构化验证，且同一 helper 有六个调用方。
2. [x] 强化共享完整文件产物断言 → 验证：两次产物均经过相同结构校验。
3. [x] 运行六个直接相关窄验收 → 验证：8/8，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0099`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 共享文件产物不变量、Architecture 与完成记录 | `test(frontend): strengthen file mutation invariants (SPEC-0099)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_prefix_truncation_matrix --test parser_token_omission_matrix --test parser_lexical_poison_replacement_matrix --test parser_token_duplication_matrix --test parser_lexical_poison_insertion_matrix --test parser_adjacent_token_transposition_matrix --locked --offline` | 通过 | 8/8；4,167 个主要变异 / 前缀 case、8,334 次解析 |
| `cargo clippy -p lang-frontend --test parser_prefix_truncation_matrix --test parser_token_omission_matrix --test parser_lexical_poison_replacement_matrix --test parser_token_duplication_matrix --test parser_lexical_poison_insertion_matrix --test parser_adjacent_token_transposition_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |

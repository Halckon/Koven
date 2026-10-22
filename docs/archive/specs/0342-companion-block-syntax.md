# SPEC-0342: companion 块语法

> **性质**：实施合同 · **状态**：done · **读取时机**：实现或验收 `companion { }` 语法时 · **唯一真源**：本片实施范围与验收；语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-0342` |
| 所属 Phase | Phase 1（Parser），外加编辑器语法 |
| 语言规范 | [Guide v0.46 Companion 与关联成员](../../guide/08-class-family-members.md#companion-与关联成员) |
| 批准依据 | 2026-10-22 用户同意 D13（companion 语法改为 `companion { }`）并要求按计划开始实施 |
| 基线 | main `3b202f86`（PR #120 合并）之上的 Guide v0.46 启用提交 |
| 分支 | `feature/spec-0342-companion-block` |
| 前置 Spec | 无 |
| 语言语义变更 | 否；落实已启用的 Guide v0.46 规则 |

## 1. Goal

Parser 接受 `companion { ... }` 作为唯一 companion 形式；`companion object` 以 L0077 拒绝，诊断
落在 `object` 关键字上并给出删除 `object` 的提示，同时继续恢复 body。仓内源码、测试与编辑器
语法同步迁移，Guide Litmus 5、12 恢复通过。

## 2. 范围

- `parse_companion_object`：`companion` 后直接是 `{` 时正常解析，`object_span` 为 body 起点的
  空范围；后面是 `object` 时在该关键字上报 L0077（消息
  ``write `companion { ... }`; `companion object` is not Koven syntax``），之后照常解析 body。
  两者都不是时沿用原 body 缺失的 L0077。
- AST 不改结构；`CompanionObject.object_span` 的文档改为“仅在 L0077 恢复时为真实范围”。
- 全部仓内 Rust 测试、`.ko` fixture 与 tree-sitter fixture 中的 `companion object {` 迁移为
  `companion {`；依赖源码字节或 token 数的固定统计按新语料更新。
- tree-sitter：`companion_object` 规则去掉 `"object"`，重新生成 `src/`。

## 3. 非目标

- 不改 companion 的语义、名称解析、类型检查或代码生成。
- 不新增诊断码；不改具名 `object` 声明。
- 不改 `docs/archive` 中的历史 Guide 与 Spec 文本。

## 4. 验收标准

- [x] `companion { ... }` 无诊断解析，`companion_span`、空 `object_span` 与 body 成员正确。
- [x] `companion object { ... }` 恰好一个 L0077，Span 精确覆盖 `object`，body 仍被恢复。
- [x] 原有拒绝用例（`companion object Named {}`、block 中的 `companion object`）仍被拒绝。
- [x] Guide Litmus 5、12 与其派生用例通过。
- [x] tree-sitter 语法重新生成，语料与契约测试通过。
- [x] 工程检查见 §5。

## 5. 实际验收（2026-10-22）

工具链：云端 Linux x86_64，Rust stable，LLVM 21.1.8，`CARGO_PROFILE_DEV_DEBUG=0`。

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | 通过 |
| `cargo test --locked --no-fail-fast -p lang-frontend` | 变基前除下条外全部通过 |
| `multifile_type_checking::assignments::assignment_publishes_plain_storage_facts_and_keeps_compounds_deferred` | 在 PR #120 合并前的头 `3f0d6fb2` 上同样失败；main `3b202f86` 已修复，变基后复跑通过 |
| 解析器变异矩阵（token/scalar 插入、删除、替换、转置、截断、trivia 不变性等 26 个目标） | 固定统计按删除一个 `object` token 后的语料更新后全部通过 |
| `npm ci`、`npm run generate`、`npm test`（editors/tree-sitter） | 通过；`src/` 已重新生成 |
| `cargo test --locked --no-fail-fast --workspace` | 变基前 188 个测试目标中 3 个各有失败：上条；`lang-codegen --lib` 中 companion 边界测试的固定 Span 随源码变化，按新源码更新后 `cargo test -p lang-codegen --lib companion` 3 项通过；另两项见下 |
| `lang-codegen` `nullable_field_access_without_current_root_fact_stops_in_frontend` | 变基前失败；main `3b202f86` 已修复该测试，变基后复跑通过 |
| `lang-cli` `unreadable_directory_and_source_report_io_operations` | 失败；云端以 root 运行，目录权限不生效，属环境原因 |

未运行：远端 CI、macOS、sanitizer。

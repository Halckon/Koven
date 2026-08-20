# SPEC-0014: 组合完整文件并跨声明恢复

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-014` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.15](../guide/00-index.md)：[完整文件契约](../guide/04-grammar-declarations-blocks.md#10-spec-0014-完整文件声明分隔与跨声明恢复) |
| 批准依据 | 用户明确启用 v0.15，并在当前持续 Goal 中授权继续分阶段实施 Specs |
| 前置 Spec | SPEC-0011、SPEC-0013 `done` |
| 前置 ADR | 无 |
| 影响范围 | `lang-frontend` 文件 Parser、fixture、Architecture 与文档状态 |
| 语言语义变更 | 否；实施 v0.15 已批准契约 |

## 1. Goal

新增 `parse_file` / `ParsedFile`，按源码顺序组合既有顶层声明；遇局部错误时保留下一最外层
声明 starter，并在不改变独立入口的前提下提供确定、owner-aware、线性的跨声明恢复。

## 2. 范围

- 空文件及零个或多个 `val`、`var`、`const val`、`fun` 根 Item。
- 无换行/分号分隔；仅 owner baseline 的 `val`/`var`/`const`/`fun` 是 soft boundary。
- 文件未知区复用 L0017 与 `Item::Error`；Lexer poison 不重复诊断。
- 顶层解构继续 L0043，但保留下一个声明；现有声明内部诊断不改义。
- `ParsedFile` 暴露同源 AST、有序 `ItemId` 根切片和合并诊断。
- 不增加依赖、错误码、AST table 或根 Item。

## 3. 非目标

- 不实现 module/import、control-flow、class-family、类成员或 Phase 2/3 检查。
- 不改变 `parse_expression`、`parse_declaration`、`parse_block` 的行为或返回类型。
- 不把换行、注释或分号定义为声明分隔符。

## 4. 验收标准

- [x] 空文件、trivia-only 与多声明文件产生准确的有序 roots。
- [x] 四类 starter 自分隔；无体、表达式体和 block body 函数可与下一声明组合。
- [x] 普通未知区形成单一 L0017/Error Item，Lexer poison 不重复分类。
- [x] L0043、缺 initializer 与 terminal string 错误均保留下一声明。
- [x] nested delimiter/call/parameter/string owner 内 starter 不提升为文件根。
- [x] 独立声明入口继续发 L0013 trailing token。
- [x] multi-source identity、长文件与 fixture 锁定同源、顺序和单调前进。
- [x] 受影响窄测与 workspace fmt/check/Clippy/test/build 全部通过。
- [x] Architecture、guide/roadmap、Spec 索引与验证记录同步完成。
- [x] 独立提交成功。

## 5. 实施计划

1. [x] 新增 `ParsedFile` / `parse_file` 与有序根循环。
2. [x] 增加 declaration soft-stop，并接入 owner-aware 恢复。
3. [x] 增加专测及真实 pass/fail fixture。
4. [x] 跑全基线、同步事实文档并提交。

## 6. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_file --locked --offline` | 通过 | 14 passed |
| file fixture 定向测试 | 通过 | 1 passed；真实 pass/fail 各一份 |
| `cargo fmt --all -- --check` | 通过 | 无格式差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | workspace 全 target |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 合计 267 passed；0 failed / ignored / measured / filtered |
| `cargo build -p lang-cli --locked --offline` | 通过 | dev build 成功 |
| `cargo tree -p lang-frontend --edges all --locked --offline` | 通过 | 仅 `lang-frontend` 自身；未新增依赖 |

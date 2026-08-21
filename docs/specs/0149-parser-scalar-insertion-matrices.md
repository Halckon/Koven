# SPEC-0149: 建立 Parser UTF-8 scalar 插入矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-149` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0068–0069、SPEC-0082–0083、SPEC-0087、SPEC-0089、SPEC-0099、SPEC-0111、SPEC-0114、SPEC-0148 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 完整文件及独立 Parser 入口公开集成测试、Architecture |
| 语言语义变更 | 否；只锁定固定单-scalar 字母表逐 UTF-8 边界插入后的总性与确定性 |

## 1. Goal

完成后，在现有 22 个完整文件与 12 个独立入口合法 corpus 的每个 UTF-8 scalar 边界分别
插入 13 个代表性 scalar 时，生产 Lexer 与对应 Parser 入口都必须重复返回有效、source-local、
确定的公开产物；原字符重复、token gap poison 和特定换行矩阵未覆盖的任意 scalar 插入得到
确定、可计数的验收。

## 2. 范围与需求

- 复用 SPEC-0148 固定的 13-scalar 字母表：`a`、`0`、`#`、`\"`、`'`、`\\`、`$`、`/`、
  `*`、`{`、`}`、LF 与 `界`，不维护第二份字符表。
- 完整文件复用 `GRAMMAR_CASES` 的 22 个互异零诊断 baseline；枚举含源码起点和 EOF 在内的
  1,373 个 UTF-8 scalar 边界，每个边界执行全部 13 项，精确产生 17,849 个真实变异。
- 独立入口复用 `ENTRY_CASES` 的 expression / declaration / block 各 4 个零诊断 baseline；分别
  枚举 195 / 350 / 202 个边界，执行 2,535 / 4,550 / 2,626 个变异，合计 9,711 个。
- 插入 offset 必须是 UTF-8 scalar 边界；结果必须仍是有效 UTF-8、长度增加插入 scalar 的
  byte length 且不同于 baseline，不添加额外分隔符。
- 每个变异源码重新加载独立 `SourceMap` 并运行生产 Lexer 两次，验证完整字节覆盖、唯一 EOF、
  source identity、diagnostic Span 与完整公开产物确定性。
- 完整文件或对应独立入口对同一 `LexedFile` 解析两次，验证 AST table Span、root 可解析、
  diagnostic Span 与完整公开产物确定性；普通变异不得产生内部错误或 panic。
- 不增加生产依赖、公开 API、新语法或诊断；发现缺陷时只修复直接根因并增加定向断言。

## 3. 非目标

- 不替代原 scalar 重复、token gap poison insertion 或 line-break boundary 的精确行为断言。
- 不组合多个插入位置，也不固定每个变异的具体 token、恢复 AST 或诊断集合。
- 不穷举 Unicode scalar 空间，也不构造无效 UTF-8 byte sequence。

## 4. 验收标准

- [x] 复用的 13 个插入项互异、各含一个 scalar，并固定其顺序与文本。
- [x] 22 个完整文件 baseline 互异且零诊断，精确识别 1,373 个边界和 17,849 个变异。
- [x] 12 个独立入口 baseline 按 4 / 4 / 4 分布，精确识别各入口边界与变异数量。
- [x] 27,560 个真实变异全部保持双 Lexer 覆盖、Span、唯一 EOF 与确定性不变量。
- [x] 每个变异对应入口双解析无内部错误，AST / root / diagnostic / source identity 均有效确定。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 两个新矩阵及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增两个 integration test target，直接复用合法 corpus、SPEC-0148 的固定字母表、UTF-8 scalar
边界枚举以及共享双 Lexer / 双 Parser output assertion。使用
`source[..offset] + scalar + source[offset..]` 构造变异源码，不复制生产 Lexer、Parser 或 AST
遍历逻辑。

## 6. 实施计划

1. [x] 审计插入矩阵覆盖 → 验证：现有矩阵只覆盖原 scalar、特定 poison 或换行插入。
2. [x] 建立完整文件 / 独立入口矩阵 → 验证：精确执行 17,849 / 9,711 个真实变异。
3. [x] 运行直接相关窄测试和窄 Clippy → 验证：两个新 target 通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0149`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 两个 scalar 插入矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser scalar insertion recovery (SPEC-0149)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --test parser_scalar_insertion_matrix --test parser_entry_scalar_insertion_matrix --locked --offline`
  首次通过：2 passed，0 failed / ignored / measured / filtered；精确执行 27,560 个真实插入。
- `cargo clippy -p lang-frontend --test parser_scalar_insertion_matrix --test parser_entry_scalar_insertion_matrix --locked --offline -- -D warnings`
  通过：0 warnings。
- `cargo fmt --all -- --check` 通过。
- `cargo check --workspace --all-targets --locked --offline` 通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过：0 warnings。
- `cargo test --workspace --all-targets --locked --offline` 通过：448 passed，0 failed / ignored /
  measured / filtered。
- `cargo build -p lang-cli --locked --offline` 通过。

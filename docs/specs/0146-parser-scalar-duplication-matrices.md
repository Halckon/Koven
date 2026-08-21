# SPEC-0146: 建立 Parser UTF-8 scalar 重复矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-146` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0068–0069、SPEC-0082、SPEC-0087、SPEC-0099、SPEC-0111、SPEC-0114、SPEC-0145 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 完整文件及独立 Parser 入口公开集成测试、Architecture |
| 语言语义变更 | 否；只锁定逐 UTF-8 scalar 原位重复后的总性与确定性 |

## 1. Goal

完成后，现有 22 个完整文件与 12 个独立入口合法 corpus 的每个 UTF-8 scalar 被原位重复时，
生产 Lexer 与对应 Parser 入口都必须重复返回有效、source-local、确定的公开产物；完整 token
重复矩阵无法进入的 lexeme、字符串、插值和注释边界由此获得逐 scalar 验收。

## 2. 范围与需求

- 完整文件复用 `GRAMMAR_CASES` 的 22 个互异零诊断 baseline；每个 scalar 原位重复一次，精确
  执行 1,351 个变异。
- 独立入口复用 `ENTRY_CASES` 的 expression / declaration / block 各 4 个零诊断 baseline；按
  191 / 346 / 198 精确执行 735 个变异。
- 重复范围必须来自相邻 UTF-8 scalar 边界；新源码应在原 scalar 后立即插入同一完整 scalar，
  不添加空格或构造无效 UTF-8。
- 每个变异源码重新加载独立 `SourceMap` 并运行生产 Lexer 两次，验证完整字节覆盖、唯一 EOF、
  source identity、diagnostic Span 与完整公开产物确定性。
- 完整文件或对应独立入口对同一 `LexedFile` 解析两次，验证 AST table Span、root 可解析、
  diagnostic Span 与完整公开产物确定性；普通变异不得产生内部错误或 panic。
- 不增加依赖、公开 API、新语法或诊断；发现缺陷时只修复直接根因并增加定向断言。

## 3. 非目标

- 不替代完整 token duplication 的精确重词法与 sentinel recovery 断言。
- 不组合多个 scalar 重复，也不固定每个变异的具体 token、恢复 AST 或诊断集合。
- 不枚举 UTF-8 code unit 内部的非法切点；`SourceMap` 继续负责无效 UTF-8 输入。
- 不把资源预算耗尽视为普通语法恢复。

## 4. 验收标准

- [x] 22 个完整文件 baseline 互异且零诊断，精确执行 1,351 个 scalar 重复。
- [x] 12 个独立入口 baseline 按 4 / 4 / 4 分布且零诊断，精确执行 735 个 scalar 重复。
- [x] 2,086 个变异全部保持双 Lexer 覆盖、Span、唯一 EOF 与确定性不变量。
- [x] 每个变异对应入口双解析无内部错误，AST / root / diagnostic / source identity 均有效确定。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 两个新矩阵及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增两个独立 integration test target，复用现有合法 corpus、UTF-8 scalar 边界枚举以及共享的
双 Lexer / 双 Parser output assertion。对每对相邻边界 `[start, end)` 取完整 scalar，并构造
`source[..end] + source[start..end] + source[end..]`；不复制 Lexer、Parser 或 AST 遍历逻辑。

## 6. 实施计划

1. [x] 审计 duplication 覆盖 → 验证：只有加空格的完整显著 token 重复，没有 lexeme 内 scalar 重复。
2. [x] 物化完整文件 / 独立入口矩阵 → 验证：精确执行 1,351 / 735 个变异。
3. [x] 运行直接相关窄测试和窄 Clippy → 验证：两个新 target 通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0146`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 两个 scalar 重复矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser scalar duplication recovery (SPEC-0146)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --test parser_scalar_duplication_matrix --test parser_entry_scalar_duplication_matrix --locked --offline`
  通过：2 passed，0 failed / ignored / measured / filtered；精确执行 2,086 个 scalar 重复。
- `cargo clippy -p lang-frontend --test parser_scalar_duplication_matrix --test parser_entry_scalar_duplication_matrix --locked --offline -- -D warnings`
  通过：0 warnings。
- workspace 标准基线全部通过：
  - `cargo fmt --all -- --check`
  - `cargo check --workspace --all-targets --locked --offline`
  - `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`
  - `cargo test --workspace --all-targets --locked --offline`：442 passed，
    0 failed / ignored / measured / filtered。
  - `cargo build -p lang-cli --locked --offline`

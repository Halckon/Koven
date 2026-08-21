# SPEC-0145: 建立 Parser UTF-8 内部区间删除矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-145` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0068–0069、SPEC-0079–0080、SPEC-0085–0086、SPEC-0099、SPEC-0111、SPEC-0114、SPEC-0144 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 完整文件及独立 Parser 入口公开集成测试、Architecture |
| 语言语义变更 | 否；只锁定 UTF-8 scalar 对齐内部区间删除后的总性与确定性 |

## 1. Goal

完成后，现有 22 个完整文件与 12 个独立入口合法 corpus 的任意非空 UTF-8 scalar 对齐内部
区间被删除时，生产 Lexer 与对应 Parser 入口都必须重复返回有效、source-local、确定的公开
产物；此前只由前缀、后缀和单 token 删除间接覆盖的内部缺失组合得到穷举验收。

## 2. 范围与需求

- 完整文件复用 `GRAMMAR_CASES` 的 22 个互异零诊断 baseline；从每个源码的内部 scalar 边界
  选择严格递增的起止点，精确执行 44,969 个删除，且始终保留非空前缀和非空后缀。
- 独立入口复用 `ENTRY_CASES` 的 expression / declaration / block 各 4 个零诊断 baseline；按
  4,453 / 15,634 / 4,656 精确执行 24,743 个内部区间删除。
- 删除以 byte offset 实施，但起止点必须来自 UTF-8 scalar 边界；不得构造无效 UTF-8。
- 每个变异源码重新加载独立 `SourceMap` 并运行生产 Lexer 两次，验证完整字节覆盖、唯一 EOF、
  source identity、diagnostic Span 与完整公开产物确定性。
- 完整文件或对应独立入口对同一 `LexedFile` 解析两次，验证 AST table Span、root 可解析、
  diagnostic Span 与完整公开产物确定性；普通变异不得产生内部错误或 panic。
- 不增加依赖、公开 API、新语法或诊断；发现缺陷时只修复直接根因并增加定向断言。

## 3. 非目标

- 不重复前缀、后缀或空区间删除，也不替代单 token omission 的精确 token / recovery 断言。
- 不固定每个删除结果的精确恢复 AST 或诊断集合；领域测试与 diagnostic witness 继续负责语义。
- 不枚举 UTF-8 code unit 内部的非法切点；`SourceMap` 继续负责无效 UTF-8 输入。
- 不把资源预算耗尽视为普通语法恢复。

## 4. 验收标准

- [x] 22 个完整文件 baseline 互异且零诊断，精确执行 44,969 个内部区间删除。
- [x] 12 个独立入口 baseline 按 4 / 4 / 4 分布且零诊断，精确执行 24,743 个内部区间删除。
- [x] 69,712 个删除全部保持双 Lexer 覆盖、Span、唯一 EOF 与确定性不变量。
- [x] 每个删除对应入口双解析无内部错误，AST / root / diagnostic / source identity 均有效确定。
- [x] 缺陷有最小修复与定向回归证据。
- [x] 两个新矩阵及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增两个独立 integration test target，直接复用现有合法 corpus、UTF-8 scalar 边界枚举以及
共享的双 Lexer / 双 Parser output assertion。对内部边界数组枚举 `start < end`，用
`source[..start] + source[end..]` 构造变异源码；不复制 Lexer、Parser 或 AST 遍历逻辑。

## 6. 实施计划

1. [x] 审计删除矩阵覆盖 → 验证：只有前缀、后缀和逐 token 删除，没有内部 scalar 区间穷举。
2. [x] 物化完整文件 / 独立入口矩阵 → 验证：精确执行 44,969 / 24,743 个删除。
3. [x] 运行直接相关窄测试和窄 Clippy → 验证：两个新 target 通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0145`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 两个内部区间删除矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser interior deletion recovery (SPEC-0145)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --test parser_class_family malformed_companion_constant_preserves_string_owner_and_both_closers --locked --offline -- --exact`
  通过：1 passed，0 failed / ignored / measured，12 filtered out。
- `cargo test -p lang-frontend --test parser_interior_deletion_matrix --test parser_entry_interior_deletion_matrix --locked --offline`
  通过：2 passed，0 failed / ignored / measured / filtered；精确执行 69,712 个内部区间删除。
- `cargo clippy -p lang-frontend --lib --test parser_class_family --test parser_interior_deletion_matrix --test parser_entry_interior_deletion_matrix --locked --offline -- -D warnings`
  通过：0 warnings。
- workspace 标准基线全部通过：
  - `cargo fmt --all -- --check`
  - `cargo check --workspace --all-targets --locked --offline`
  - `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`
  - `cargo test --workspace --all-targets --locked --offline`：440 passed，
    0 failed / ignored / measured / filtered。
  - `cargo build -p lang-cli --locked --offline`

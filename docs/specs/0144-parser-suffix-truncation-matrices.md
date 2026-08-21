# SPEC-0144: 建立 Parser UTF-8 后缀截断矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-144` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0068–0069、SPEC-0079、SPEC-0085、SPEC-0099、SPEC-0111、SPEC-0114、SPEC-0143 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 完整文件及独立 Parser 入口公开集成测试、Architecture |
| 语言语义变更 | 否；只锁定逐 UTF-8 后缀作为不可信源码时的总性与确定性 |

## 1. Goal

完成后，现有 22 个完整文件与 12 个独立入口合法 corpus 不再只验证“保留开头”的逐 scalar
前缀；删除任意 UTF-8 scalar 前缀后留下的全部 2,120 个后缀也必须由生产 Lexer 完整覆盖，并由
对应 Parser 入口重复返回有效、source-local、确定的公开产物。

## 2. 范围与需求

- 完整文件复用 `GRAMMAR_CASES` 的 22 个互异零诊断 baseline；枚举每个 scalar 起点及 EOF 空
  后缀，共固定执行 1,373 个后缀。
- 独立入口复用 `ENTRY_CASES` 的 expression / declaration / block 各 4 个零诊断 baseline；按
  195 / 350 / 202 固定执行 747 个后缀。
- 每个后缀重新加载独立 `SourceMap` 并执行生产 Lexer 两次，验证完整字节覆盖、source identity、
  唯一 EOF、diagnostic Span 与确定性。
- 完整文件或对应独立入口对同一 `LexedFile` 解析两次，验证 AST table Span、root 可解析、
  diagnostic Span 与完整公开产物确定性；普通后缀不得产生内部错误或 panic。
- 完整 baseline 必须保持 Lexer / Parser 零诊断；后缀不固定具体恢复 AST 或诊断集合。
- 不增加依赖、公开 API、新语法或诊断；发现缺陷时只修复直接根因并增加定向断言。

## 3. 非目标

- 不重复固定每个后缀的精确诊断或 AST；领域测试与 diagnostic witness 继续负责语义。
- 不枚举 UTF-8 code unit 内部的非法切点；`SourceMap` 继续负责无效 UTF-8 输入。
- 不替代前缀、token omission / duplication / transposition、poison 或 trivia 矩阵。
- 不把资源预算耗尽视为普通语法恢复。

## 4. 验收标准

- [x] 22 个完整文件 baseline 互异且零诊断，精确执行 1,373 个 UTF-8 后缀。
- [x] 12 个独立入口 baseline 按 4 / 4 / 4 分布且零诊断，精确执行 747 个 UTF-8 后缀。
- [x] 2,120 个后缀全部保持双 Lexer 覆盖、Span、唯一 EOF 与确定性不变量。
- [x] 每个后缀对应入口双解析无内部错误，AST / root / diagnostic / source identity 均有效确定。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 两个新矩阵及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增两个独立 integration test target，直接复用前缀矩阵已经验证的 corpus、UTF-8 scalar 边界
枚举和 output assertion。边界序列改作 suffix start，并取 `&source[start..]`；不复制 Lexer、
Parser 或 AST 遍历逻辑。

## 6. 实施计划

1. [x] 审计截断矩阵方向覆盖 → 验证：仅有逐 scalar 前缀，没有逐 scalar 后缀。
2. [x] 物化 Spec 与完整文件 / 独立入口矩阵 → 验证：复用 22-file / 12-entry corpus。
3. [x] 执行 2,120 个后缀并定位最早失败 → 验证：双 Lexer / 双 Parser 产物不变量。
4. [x] 运行直接相关窄验收与窄 Clippy → 验证：两个新 target 通过，0 warnings。
5. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0144`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 两个后缀截断矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser suffix recovery (SPEC-0144)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --test parser_suffix_truncation_matrix --test parser_entry_suffix_truncation_matrix --locked --offline`
  通过：2 passed，0 failed / ignored / measured / filtered。
- `cargo clippy -p lang-frontend --test parser_suffix_truncation_matrix --test parser_entry_suffix_truncation_matrix --locked --offline -- -D warnings`
  通过：0 warnings。
- workspace 标准基线全部通过：
  - `cargo fmt --all -- --check`
  - `cargo check --workspace --all-targets --locked --offline`
  - `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`
  - `cargo test --workspace --all-targets --locked --offline`：437 passed，
    0 failed / ignored / measured / filtered。
  - `cargo build -p lang-cli --locked --offline`

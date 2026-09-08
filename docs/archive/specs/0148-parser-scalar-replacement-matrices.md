# SPEC-0148: 建立 Parser UTF-8 scalar 替换矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-148` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0068–0069、SPEC-0081、SPEC-0088、SPEC-0099、SPEC-0111、SPEC-0114、SPEC-0147 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 完整文件及独立 Parser 入口公开集成测试、Architecture |
| 语言语义变更 | 否；只锁定固定单-scalar 字母表逐位置替换后的总性与确定性 |

## 1. Goal

完成后，现有 22 个完整文件与 12 个独立入口合法 corpus 的每个 UTF-8 scalar 分别被 13 个
代表性 scalar 原位替换时，生产 Lexer 与对应 Parser 入口都必须重复返回有效、source-local、
确定的公开产物；完整 token poison replacement 无法进入的 lexeme 与 lexical-owner 内部得到
确定、可计数的替换验收。

## 2. 范围与需求

- 共享替换字母表固定为 `a`、`0`、`#`、`"`、`'`、`\`、`$`、`/`、`*`、`{`、`}`、LF 与
  `界`；分别代表 identifier、number、poison、string / char、escape、interpolation、comment、
  delimiter、line boundary 与多字节 UTF-8。
- 完整文件复用 `GRAMMAR_CASES` 的 22 个互异零诊断 baseline；枚举 17,563 个候选，排除 123 个
  相同 scalar no-op，精确执行 17,440 个真实变异，并固定各替换项计数。
- 独立入口复用 `ENTRY_CASES` 的 expression / declaration / block 各 4 个零诊断 baseline；枚举
  2,483 / 4,498 / 2,574 个候选，排除 15 / 29 / 42 个 no-op，执行 2,468 / 4,469 / 2,532 个变异。
- 被替换范围必须来自相邻 UTF-8 scalar 边界；结果必须仍是有效 UTF-8 且不同于 baseline。允许
  replacement 改变源码 byte length，不添加额外分隔符。
- 每个变异源码重新加载独立 `SourceMap` 并运行生产 Lexer 两次，验证完整字节覆盖、唯一 EOF、
  source identity、diagnostic Span 与完整公开产物确定性。
- 完整文件或对应独立入口对同一 `LexedFile` 解析两次，验证 AST table Span、root 可解析、
  diagnostic Span 与完整公开产物确定性；普通变异不得产生内部错误或 panic。
- 不增加生产依赖、公开 API、新语法或诊断；发现缺陷时只修复直接根因并增加定向断言。

## 3. 非目标

- 不替代完整 token poison replacement 的精确 L0001 / L0002 与 sentinel recovery 断言。
- 不组合多个位置或多个 replacement，也不把相同 scalar replacement 计作有效变异。
- 不固定每个变异的具体 token、恢复 AST 或诊断集合。
- 不穷举 Unicode scalar 空间，也不构造无效 UTF-8 byte sequence。

## 4. 验收标准

- [x] 13 个替换项互异、各含一个 scalar，并固定其顺序与文本。
- [x] 22 个完整文件 baseline 互异且零诊断，精确识别 17,563 / 123 / 17,440 个候选 / no-op / 变异。
- [x] 12 个独立入口 baseline 按 4 / 4 / 4 分布，精确识别各入口候选、no-op 与变异数量。
- [x] 26,909 个真实变异全部保持双 Lexer 覆盖、Span、唯一 EOF 与确定性不变量。
- [x] 每个变异对应入口双解析无内部错误，AST / root / diagnostic / source identity 均有效确定。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 两个新矩阵及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

在 tests support 中集中维护 13 项 replacement 单一真源，新增两个 integration test target，复用
现有合法 corpus、UTF-8 scalar 边界枚举以及共享的双 Lexer / 双 Parser output assertion。替换
使用 `source[..start] + replacement + source[end..]`，相同 scalar 只计入 no-op 审计。

## 6. 实施计划

1. [x] 审计 replacement 覆盖 → 验证：只有加空格的完整 token poison replacement。
2. [x] 建立共享字母表与完整文件 / 独立入口矩阵 → 验证：精确执行 17,440 / 9,469 个真实变异。
3. [x] 运行直接相关窄测试和窄 Clippy → 验证：两个新 target 通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0148`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | scalar 替换字母表、两个矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser scalar replacement recovery (SPEC-0148)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 首次窄测试在编译阶段失败：两个新矩阵把 `str` 直接与 `&str` 比较；补齐切片引用后完整
  重跑，未进入或修改生产逻辑。
- `cargo test -p lang-frontend --test parser_scalar_replacement_matrix --test parser_entry_scalar_replacement_matrix --locked --offline`
  重跑通过：2 passed，0 failed / ignored / measured / filtered；精确执行 26,909 个真实替换并
  排除 209 个 no-op。
- `cargo clippy -p lang-frontend --test parser_scalar_replacement_matrix --test parser_entry_scalar_replacement_matrix --locked --offline -- -D warnings`
  通过：0 warnings。
- `cargo fmt --all -- --check` 通过。
- `cargo check --workspace --all-targets --locked --offline` 通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过：0 warnings。
- `cargo test --workspace --all-targets --locked --offline` 通过：446 passed，0 failed / ignored /
  measured / filtered。
- `cargo build -p lang-cli --locked --offline` 通过。

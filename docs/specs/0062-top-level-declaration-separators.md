# SPEC-0062: 修正顶层声明换行与分号分隔

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-062` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.16](../guide/00-index.md)：[词法固定符号](../guide/02-lexical-spec.md#7-固定运算符标点与最长匹配)、[完整文件声明分隔](../guide/04-grammar-declarations-blocks.md#10-spec-0014-完整文件声明分隔与跨声明恢复) |
| 批准依据 | 用户明确启用 v0.16，并要求修复“换行可分隔声明、同行必须写分号”的设计意图；当前持续 Goal 的站立授权继续有效 |
| 前置 Spec | SPEC-0014 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer、完整文件 Parser、诊断、fixture、Architecture |
| 语言语义变更 | 否；实现已经批准并启用的 v0.16 增量契约 |

## 1. Goal

完成后，完整文件的顶层声明只能由实际换行或 `;` 分隔；同行缺少 `;` 时产生稳定诊断并
保留后一声明，而 block 与独立声明入口不获得分号分隔语义。

## 2. 背景

SPEC-0014 按 v0.15 实现了声明 starter 自分隔，因而接受 `val a = 1 fun f() {}`。用户确认
该行为违背预期，并启用 v0.16 取代它。0015–0061 已由路线图预留，因此本增量使用下一个
未占用编号 0062，不改号或改写已完成 Spec。

## 3. 范围与需求

- Lexer 增加 `Symbol::Semicolon`，精确覆盖 `;` 且不再产生 L0001。
- 完整文件在两个顶层声明之间接受至少一个实际 LF / CRLF，或一个 `;`；terminated block
  comment 内的 LF / CRLF 同样计入。
- 同行声明之间只有 space、tab 或无换行注释时产生 L0047，主 Span 覆盖后一声明 starter，
  AST 仍按源码顺序保留两个根。
- 最后一个声明后允许一个 `;`；前导或连续 `;` 不产生空声明，复用 L0017 与 Error Item。
- `;` 与声明 starter 只在文件 owner baseline 作为恢复边界；嵌套 delimiter、string 与
  interpolation 内不得提前结束当前声明或提升内部 starter。
- 保持单调文件游标与 `O(n)` dispatch / 恢复边界，不新增依赖或 AST table。

## 4. 非目标

- 不把 `;` 或换行定义为 block element、expression 或独立声明的通用结束符。
- 不实现 module/import、control-flow、class-family 或 Phase 2/3 语义。
- 不改写 SPEC-0014 的历史状态、验收记录或实施时 v0.15 引用。
- 不引入空声明、前导分号或连续分号语义。

## 5. 验收标准

- [x] Lexer 将 `;` 识别为精确固定符号，并保持完整覆盖与确定性诊断。
- [x] LF、CRLF、显式 `;`、换行加 `;`、block comment 内换行及可选尾随 `;` compile-pass。
- [x] 同行无 `;`、无换行注释、前导 / 连续 `;` compile-fail；断言 L0047 / L0017 与精确 Span。
- [x] 同行缺分隔时保留后一声明及根顺序，分号 / starter 不越过 nested lexical 或 delimiter owner。
- [x] 独立声明继续对 `;` 发 L0013，block 中 `;` 仍被拒绝且不产生 Lexer L0001。
- [x] 长换行 / 分号声明序列保持完整 roots、单调前进和线性实现证据。
- [x] 受影响窄测及 workspace fmt/check/Clippy/test/build 全部通过，无 ignored / skipped / filtered。
- [x] Architecture、guide/roadmap、Spec 索引、fixture 计数和验证记录同步为实现后的事实。
- [x] 独立提交成功。

## 6. 技术方案与边界

`Symbol::Semicolon` 只扩充 Lexer 固定符号字母表。文件 Parser 保留 starter 作为恢复 stop，并在
每个根结束后检查当前 raw lexeme 区间：分隔区出现 LF（包括 terminated block comment 的
源码范围）或消费一个 `;` 即合法；若下一 token 是声明 starter 且分隔区二者皆无，则发
L0047 后继续。file stop 集合增加 owner-baseline `;`，嵌套入口继续移除 file stop，因此不把
分号泄漏成表达式或 block 语义。

## 7. 实施计划

1. [x] 扩充 Lexer Symbol、scanner 与诊断目录 → 验证：Lexer 和 diagnostic 窄测
2. [x] 实现文件分隔检查、L0047 与 owner-aware `;` stop → 验证：`parser_file` 窄测
3. [x] 迁移 fixtures、同步 Architecture 与验收记录 → 验证：fixture 窄测和 workspace 基线

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Lexer、Parser、测试、fixture、Architecture 与完成状态 | `fix(frontend): enforce declaration separators (SPEC-0062)` |

## 9. 未决问题

- 无

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_file --test parser_block --test lexer --test diagnostic_model --test fixtures --locked --offline` | 通过 | parser_file 19、parser_block 21、lexer 19、diagnostic 9、fixture harness 23；0 failed / ignored / measured / filtered |
| `cargo fmt --all -- --check` | 通过 | 无格式差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | workspace 全 target |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 合计 273 passed；0 failed / ignored / measured / filtered |
| `cargo build -p lang-cli --locked --offline` | 通过 | dev build 成功 |
| `cargo tree -p lang-frontend --edges all --locked --offline` | 通过 | 仅 `lang-frontend` 自身；未新增依赖 |
| 修改文档 Markdown 相对链接检查 | 通过 | 12 份相关文档的本地目标均存在 |
| `git diff --check` | 通过 | 无空白错误 |

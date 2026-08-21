# SPEC-0058: 提供 TextMate grammar 与回归 fixture

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P6-058` |
| 所属 Phase | Phase 6 |
| 语言规范 | [现行 v0.25 词法规范](../guide/02-lexical-spec.md)与[工具链路线图](../guide/06-roadmap.md#phase-6工具链完善) |
| 批准依据 | 用户在当前持续 Goal 中要求继续分阶段实施 Specs，并授权简化重复验收环节 |
| 前置 Spec | SPEC-0014、SPEC-0015 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `editors/textmate`、`lang-frontend` 回归测试、Architecture |
| 语言语义变更 | 否；grammar 只呈现现行 Lexer 已定义的拼写分类 |

## 1. Goal

完成后，TextMate 兼容编辑器可按 Koven `.ko` 文件的现行词法契约提供基础语法高亮，仓库内
存在由生产 Lexer 校验的高亮语料与 scope 覆盖回归。

## 2. 范围与需求

- 提供独立的 JSON TextMate grammar，声明 `source.koven` 与 `.ko` 文件类型。
- 覆盖注释、字符串与 `${...}` 插值、字符、数值、42 个硬关键字、11 个未来保留字、内建
  类型、声明名称、annotation、固定运算符与标点。
- 保留 Lexer 的 ASCII 标识符、非嵌套 block comment、单行 string、最小数值后缀和最长符号
  匹配边界，不从 Kotlin 或未来 Koven 版本补入额外形式。
- 用真实 `.ko` corpus 和 scope expectation sidecar 建立可重复回归；生产 Lexer 必须证明
  正常 corpus 无词法诊断，reserved corpus 精确产生 L0002。

## 3. 非目标

- 不创建 VS Code extension、package manifest、主题或安装流程。
- 不提供语义 token、LSP 高亮、交叉文件名称分类或诊断展示。
- 不实现 Tree-sitter grammar；该工作属于 SPEC-0059。
- 不保证 TextMate 正则等价于 Parser 或类型检查器；grammar 是无状态的词法近似层。

## 4. 验收标准

- [x] grammar 是合法 JSON，声明 `source.koven`、`.ko` 和可解析的 repository include。
- [x] grammar 覆盖现行关键字、保留字、字面量、注释、内建类型、声明名称、运算符与标点。
- [x] 正常 corpus 经生产 Lexer 无诊断并覆盖全部主要 token/trivia family。
- [x] reserved corpus 的 11 个词均被标为 illegal scope，生产 Lexer 精确产生 11 个 L0002。
- [x] scope expectation 的每个 scope 在 grammar 中存在，代表性源码片段在 corpus 中存在。
- [x] 窄回归与一次 workspace 标准基线通过；Architecture 与本 Spec 同步。

## 5. 技术方案与边界

grammar 作为仓库级编辑器资产放在 `editors/textmate/syntaxes/`。`lang-frontend` integration
test 只复用公开 Lexer 产物校验 corpus，不把 TextMate 或 JSON 依赖引入生产 crate；JSON
语法由验收命令直接校验，Rust 回归锁定 repository、scope、语料和 Lexer 分类覆盖。

## 6. 实施与提交计划

1. [x] 添加 grammar、正常与 reserved corpus、scope expectation。
2. [x] 添加 std-only Rust 回归并运行窄测试。
3. [x] 同步 Architecture、本 Spec 状态和验证记录，运行一次 workspace 基线。
4. [x] 独立提交：`feat(tooling): add TextMate grammar (SPEC-0058)`。

## 7. 未决问题

- 无。

## 8. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `python3 -m json.tool editors/textmate/syntaxes/koven.tmLanguage.json` | 通过 | grammar JSON 可解析 |
| `cargo test -p lang-frontend --test textmate_grammar --locked --offline` | 通过 | 4 passed；repository、scope、正常与 reserved corpus 回归 |
| workspace Cargo 基线 | 通过 | fmt、check、Clippy `-D warnings`、383 tests、`cargo build -p lang-cli` 全部成功 |

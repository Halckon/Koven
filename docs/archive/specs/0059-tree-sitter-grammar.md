# SPEC-0059: 提供 Tree-sitter grammar 与 corpus

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P6-059` |
| 所属 Phase | Phase 6 |
| 语言规范 | [现行 v0.25 词法规范](../guides/v0.34-pre-restructure/02-lexical-spec.md)、[核心表达式文法](../guides/v0.34-pre-restructure/03-grammar-core.md)、[声明与 block 文法](../guides/v0.34-pre-restructure/04-grammar-declarations-blocks.md)、[call/lambda 文法](../guides/v0.34-pre-restructure/05-grammar-calls-lambda.md) |
| 批准依据 | 用户在当前持续 Goal 中要求继续分阶段实施 Specs，并授权简化重复验收环节 |
| 前置 Spec | SPEC-0014、SPEC-0015 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `editors/tree-sitter`、Architecture、开发依赖治理 |
| 语言语义变更 | 否；grammar 追随现行 Lexer/Parser，不成为新的语言规范 |

## 1. Goal

完成后，Tree-sitter 工具链可确定性生成 Koven `.ko` 的增量 concrete-syntax parser，并用仓库
内 corpus 验证当前 Phase 1 的代表性有效结构与错误恢复。

## 2. 范围与需求

- 提供 `grammar.js`、生成的 C parser/node-types、`tree-sitter.json` 与精确锁定的生成命令。
- 覆盖现行文件头、声明/class-family、block/control-flow、lambda、类型、literal、call 与
  Pratt 运算符层级；保留 `${...}` 字符串插值和 Koven 专用参数 marker。
- corpus 至少覆盖最小文件、声明/class-family、表达式/call/lambda、control-flow/string 及
  一组可恢复错误，并断言 concrete syntax tree。
- grammar 是编辑器增量解析层；生产编译仍只使用 Rust Lexer/Parser，二者发生漂移时以 guide
  和生产前端为准修正 grammar。

## 3. 非目标

- 不替换、生成或复用 `lang-frontend` Parser，不把 Tree-sitter runtime 引入 Cargo workspace。
- 不实现语义高亮、locals/tags query、格式化或 LSP。
- 不接受 Kotlin 独有语法，不提前加入未实施的 Koven 语义。
- 不把 Tree-sitter 的宽松错误恢复当作 Koven compile-pass 判据。

## 4. 依赖准入

- **组件**：官方 `tree-sitter-cli` `0.26.12`，仅为 `editors/tree-sitter` 的精确锁定 devDependency。
- **适配与兼容**：该 CLI 是 grammar 生成与 corpus runner 的直接实现；要求 Node `>=12`，当前
  验收环境 Node `26.7.0` 满足。它不进入任何 Rust target 或最终编译器产物。
- **维护与许可**：官方包由 Tree-sitter 仓库发布，当前版本发布于 2026-08-08，MIT；项目仍
  保持 `publish = false`，不据此虚构 Koven 自身许可证。
- **供应链**：npm tarball SHA-512 与 registry integrity 一致且无传递包；其 `install.js` 会从
  同版本官方 GitHub Release 下载平台 CLI，但不校验下载后二进制摘要。这是已知代价；通过
  精确版本、lockfile、dev-only 边界、提交生成产物及重复生成 diff 检查收敛风险。
- **成本与质量**：只安装一个平台 CLI；生成器不参与运行时。`generate`、`test`、生成确定性
  与 npm audit 均进入验收，不使用浮动 `npx` 或全局工具。

## 5. 验收标准

- [x] `npm ci` 按 lockfile 安装，`npm audit` 无已知漏洞。
- [x] `npm run generate` 成功，连续生成不改变已提交产物。
- [x] `npm test` 执行非零 corpus，所有有效与错误恢复 case 通过且无意外 `ERROR`/`MISSING`。
- [x] 代表性 `.ko` 文件可解析，根节点和主要声明/表达式结构稳定。
- [x] 未引入 Cargo/runtime 依赖；Architecture 与本 Spec 同步。
- [x] Tree-sitter 窄验收与一次 workspace 标准基线通过。

## 6. 技术方案与边界

grammar 使用 JavaScript DSL 作为唯一手写语法源，生成文件保存在同目录 `src/` 并接受 diff
审阅。换行是文件头、顶层声明、block element 与 `when` entry 的显式 separator；space、tab、
CR 和 comment 作为 extras。expression precedence 只在 grammar 的 precedence 常量中定义，
不导出给编译器。外部 C scanner 仅负责排除硬关键字、未来保留字和解构中的单独 `_`，不承载
语法状态。corpus 使用 Tree-sitter 原生 test 格式，另用真实 `.ko` fixture 做 parse smoke
check，并由生产 Rust Lexer/Parser 对同一 fixture 做诊断与恢复交叉验收。

## 7. 实施与提交计划

1. [x] 固定 CLI、lockfile 与 Tree-sitter package metadata。
2. [x] 实现 grammar、外部 identifier scanner 并生成 parser/node-types。
3. [x] 添加 corpus、真实 `.ko` fixture 与生产前端交叉测试，运行窄验收。
4. [x] 同步 Architecture、本 Spec 和路线索引，运行一次 workspace 基线。
5. [x] 独立提交：`feat(tooling): add Tree-sitter grammar (SPEC-0059)`。

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `npm ci`；`npm audit` | 通过 | 精确 lockfile 安装；0 个已知漏洞 |
| `npm run generate`；`npm test` | 通过 | 7/7 corpus 成功，非零 case guard 由 CLI 保证 |
| 连续两次 `npm run generate` 与 SHA-256 | 通过 | 三个生成产物两轮摘要一致 |
| `tree-sitter parse --grammar-path . test/fixtures/representative.ko` | 通过 | 完整树无 `ERROR` / `MISSING`；CLI 仅提示未配置全局 parser 目录 |
| `cargo test -p lang-frontend --test tree_sitter_grammar --locked --offline` | 通过 | 3/3 生产 Lexer/Parser 交叉验收 |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、test、CLI build 完成后统一执行一次 |

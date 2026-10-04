# SPEC-0267: Tree-sitter corpus 解析与必需 CI 门禁

> **性质**：变更合同 · **状态**：done · **读取时机**：实施和验收编辑器修复时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-EDITOR-CORPUS` |
| 所属 Phase | Phase 1 编辑器语法镜像 |
| 语言规范 | [现行 Guide](../../guide/README.md)、[词法](../../guide/01-lexical.md)、[调用参数](../../guide/07-calls-lambdas-closures.md) |
| 批准依据 | 用户授权推进 M0 已确认失败项；0267 为隔离修复分支 |
| 前置 Spec | 无 |
| 前置 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `editors/tree-sitter`、对应 frontend fixture 合同、CI 与文档 |
| 语言语义变更 | 否 |

## 1. Goal

修复 M0 已证实的 Tree-sitter corpus 失败，并由必需 CI 实际执行锁定版本 CLI corpus。

## 2. 背景

M0 基线使用 CLI 0.26.12，10 个 corpus 仅 5 个通过；重新生成的三个产物逐字节一致，
故不是生成物遗漏。外部 identifier 抢先接受参数模式与 loop；named argument 的歧义选择了
assignment。reserved golden 未反映前置合法 value 声明，保留词首部 ERROR 后恢复后缀属于编辑器错误恢复，并非接受完整保留字。
现 Rust fixture 测试调用生产 frontend，不执行 Tree-sitter CLI。

## 3. 范围与需求

- 上下文关键词在指定产生式识别，在参数名、调用、成员和普通值中仍为 identifier。
- 未分组 `name = value` 形成 named argument；分组赋值保留 assignment。
- 硬关键字和未来保留字不能合法成为 identifier；错误必须落在对应词区间，未来保留字后的声明独立恢复，前后缀名称仍可解析。
- corpus 的非法 own 调用保持错误；仅逐项审阅后更新恢复树 golden。
- CI 安装 lockfile 固定的 0.26.12，运行完整 corpus 与语义断言，并把失败/意外 skip 传至汇总。

## 4. 非目标

不改 Guide、生产 Lexer/Parser、TextMate 语义，不做完整语言重写，不引入 Cargo target 或依赖。

## 5. 验收标准

- [x] 保存有效红测，完整 corpus 和新增语义回归通过。
- [x] 关键词语境、名称、整词边界和非法调用恢复均有独立可观察断言。
- [x] 生成产物与锁定 CLI 同步；frontend `tree_sitter_grammar` 定向合同通过。
- [x] CI editor job 路径/事件政策和 required 汇总回归通过。
- [x] Architecture、测试指南与 Spec 账本一致，文档门禁通过。
- [x] 独立复核与实现 head 的 PR CI 通过后归档；归档提交须再次通过 PR 门禁方可合并。

## 6. 技术方案与边界

锁定 CLI 的 reserved word sets 要求 token 在合法产生式中使用，而未来保留字没有合法位置，
因此保留现有 scanner 和单一 reserved 表；不创建虚假的合法产生式。13 个 external token
分别承载 own/borrow/inout、loop、move lambda/type、to/by、in/is/as、!in/!is；用 valid_symbols 与
不改变 token span 的 extras lookahead 识别语境。as? 保留既有复合 token。
词表继续与生产 Lexer 定向交叉验证。命名参数使用明确语法优先级。
恢复 golden 不能替代精确词区间与后续声明断言。CI 使用现有 Python/Node 环境，无额外依赖。

## 7. 实施计划

1. [x] 红测与根因确认 → 完整 corpus、独立树节点/跨度断言。
2. [x] 最小语法修复和生成 → 同一测试全部通过。
3. [x] CI、文档与独立审阅 → required 汇总和文档门禁。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 解析修复、直接回归与必需 CI | `fix(editors): enforce contextual syntax with real corpus CI (SPEC-0267)` |

实现提交 `236625b4c83ad414808de07c2fd68e735d1ef3e7` 经 PR #45 验收；归档提交单独记录最终门禁。

## 9. 未决问题

无语义未决项；实现已通过远端 CI，归档提交仍须通过最终 PR 门禁。

## 10. 验证记录

| 验收项 | 结果 | 边界 |
|---|---|---|
| CLI 0.26.12 `tree-sitter test` 基线 | 10 项，5 通过、5 失败 | 真实 CLI，生成物未漂移 |
| 初始新增 CLI 树回归 | 6 项、20 个失败子情形 | 包含后来审阅为过严的整词 ERROR 断言；参数模式、命名参数和非法调用是真实红测 |
| CLI `tree-sitter test` 修复后 | 10/10，通过 | 只更新 Own 和 reserved 两项错误恢复 golden；其余预期未改 |
| `python3 editors/tree-sitter/test/test_contract.py -v`，PATH 为锁定 CLI | 9/9，通过 | 正常/异常退出、实际节点、词区间、注释 token span、后继声明、关键字前后缀与复合词运算符邻接性 |
| `cargo test --locked --offline -p lang-frontend --test tree_sitter_grammar --test textmate_grammar` | 4/4 + 5/5，通过 | 共享 target 前 touch frontend 入口并实际重编；无忽略/过滤 |
| `python3 -m unittest discover -s scripts/tests -p test_check_ci_results.py -v` | 20/20，通过 | 新增 editor required job 与 CLI 接线 |
| `python3 -m unittest discover -s scripts/tests -v` | 116/116，通过 | 同步两处既有 required 集合断言 |
| 锁定 CLI 再次 generate | 三个生成文件逐字节一致 | node-types.json 无语义/API 漂移 |
| `python3 scripts/check_docs.py` / `git diff --check` | 507 页，通过 / 通过 | Spec DAG 由既有脚本生成 |
| `python3 scripts/check_rust_sizes.py --base HEAD` | 通过，无 Rust 增长 | 45 份旧尺寸欠账保持 |
| 独立审阅 | 修复 !input/!island 同族边界后复核通过，无剩余阻断发现 | 独立隔离 generate、10 corpus、9 XML 回归；附加 when/相邻注释/边界探针 |
| 远端 PR CI | [run 37198642821](https://github.com/Halckon/Koven/actions/runs/37198642821)，11/11 jobs success | 关联实现 head `236625b4c83ad414808de07c2fd68e735d1ef3e7`；两宿主 workspace/clippy 与 Targeted Tests 实际执行 |
| 远端 editor 门禁 | npm ci、generate、生成物 diff、corpus 10/10、CLI 树合同 9/9 全部通过 | 真实 Tree-sitter CLI；不代表完整语言或真实编辑器增量解析验收 |

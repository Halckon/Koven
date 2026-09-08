# SPEC-0071: 执行 TextMate symbol 与 literal 契约

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P6-071` |
| 所属 Phase | Phase 6 |
| 语言规范 | 现行 [v0.25 词法规范](../guides/v0.34-pre-restructure/02-lexical-spec.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收 |
| 前置 Spec | SPEC-0006、SPEC-0058 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | TextMate grammar 测试、`lang-frontend` Lexer 交叉测试、Architecture |
| 语言语义变更 | 否；只执行并锁定现行 symbol/literal 正则边界 |

## 1. Goal

完成后，TextMate grammar 的 operator、punctuation、integer、float、string escape 与 character
正则由零依赖测试实际执行，并与生产 Lexer 复用同一份正反例契约。

## 2. 范围与需求

- 新增仓库私有 TSV，完整列出 33 个 operator spelling、10 个 punctuation spelling，以及
  有效 integer/float/string escape/character 和代表性拒绝边界。
- 零依赖 Node verifier 读取实际 JSON grammar，按 scope 解析并锚定执行相应 regex；每个正例
  只能命中目标 family，每个反例不得完整命中，并锁定 float 规则先于 integer 规则。
- Rust integration test 读取同一 TSV，把全部正例交给生产 Lexer，逐项验证 `Symbol`、
  `IntegerLiteral`、`FloatLiteral` 类别或 string/character 零诊断。
- 提供私有 `package.json` 的 `npm test` 入口；不安装 package、不新增 lockfile 或依赖。

## 3. 非目标

- 不实现完整 TextMate 引擎、scope stack 或 Oniguruma 特有扩展；本 Spec 的现行正则均为
  JavaScript `RegExp` 可等价执行的子集。
- 不改变 grammar、生产 Lexer、token 集合、数值语义或公开 API。
- 不把 Node 引入 Cargo 测试；TextMate 窄验收与 workspace Cargo 基线仍保持分层。

## 4. 验收标准

- [x] 33 + 10 个 `Symbol` 拼写均由 grammar 正确且互斥地分类，并由生产 Lexer 产出单一 Symbol。
- [x] integer/float、8 个 string escape 与代表性 character 正例同时通过 grammar 和 Lexer。
- [x] 非法或非单一 literal 边界不被 TextMate regex 完整误认；float pattern 顺序稳定。
- [x] `npm test`、frontend 窄测试和一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

TSV 每行是 `family<TAB>source spelling`，不转义源码内容；Node 与 Rust 均按 UTF-8 原样读取。
Node 仅从 grammar repository 中按 scope name 选择 regex，并用 `^(?:pattern)$` 检查单 token
边界。Rust 只验证正例，反例是否形成多个合法 token 仍由生产 Lexer 的既有精确测试决定。

## 6. 实施计划

1. [x] 添加共享契约、Node verifier 与 npm script → 验证：TextMate `npm test`。
2. [x] 添加生产 Lexer 交叉测试 → 验证：TextMate Rust integration test。
3. [x] 同步 Architecture、Spec 与索引 → 验证：文档和事实一致。
4. [x] 运行一次 workspace 基线并提交 → 验证：staged diff 单一且提交成功。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 共享契约、双端 verifier、文档与完成记录 | `test(tooling): execute TextMate lexical contract (SPEC-0071)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `npm test`（`editors/textmate`） | 通过 | 实际执行 80 个 grammar regex case；无依赖安装 |
| `cargo test -p lang-frontend --test textmate_grammar --locked --offline` | 通过 | 5/5；共享正例均经生产 Lexer 验证 |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、391 tests、CLI build；0 failed / ignored / measured / filtered |

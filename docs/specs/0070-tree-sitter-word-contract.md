# SPEC-0070: 锁定 Tree-sitter 与 Lexer 词表契约

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P6-070` |
| 所属 Phase | Phase 6 |
| 语言规范 | 现行 [v0.25 词法规范](../guide/02-lexical-spec.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收 |
| 前置 Spec | SPEC-0006、SPEC-0059 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | Tree-sitter external scanner/corpus、`lang-frontend` grammar 交叉测试、Architecture |
| 语言语义变更 | 否；只锁定现行 42 + 11 词表的一致性 |

## 1. Goal

完成后，Tree-sitter external scanner 的不可用 identifier 表由可执行测试精确证明与生产 Lexer
的 42 个硬关键字、11 个未来保留字一致，并验证关键字只是前缀时仍为合法 identifier。

## 2. 范围与需求

- Rust 交叉测试从 `src/scanner.c` 的唯一 `RESERVED_WORDS` 表提取全部拼写，按顺序与现行
  42 + 11 契约精确比较，禁止漏项、额外项、重复项或静默换序。
- 同一测试把完整 53 词交给生产 Lexer：前 42 项必须为 `Keyword`，后 11 项必须为
  `ReservedWord` 并逐项产生精确 `L0002` span。
- Tree-sitter corpus 与共享 `.ko` fixture 增加 `className`、`asyncTask` 边界，证明硬关键字与
  未来保留字的更长前缀拼写仍由 external scanner 产出 identifier。
- 保持 scanner 算法、生产 Lexer、grammar、依赖与生成产物不变。

## 3. 非目标

- 不把 C scanner 表生成化，不扩大公开 Lexer API，也不改变关键字集合。
- 不复制 Tree-sitter runtime 到 Cargo 测试或把 npm 工具变成 workspace 依赖。
- 不替代生产 Lexer 已有的 enum 变体、边界与全 token 分类精确测试。

## 4. 验收标准

- [x] scanner 表精确包含 53 个约定拼写，42/11 分界和顺序稳定。
- [x] 生产 Lexer 对同一词表的 token 类别与 11 个 L0002 span 全部精确通过。
- [x] Tree-sitter corpus 同时拒绝 `value` / `async` 并接受 `className` / `asyncTask`。
- [x] Tree-sitter 与 frontend 窄测试、一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

交叉测试中的词表是验收 oracle，不进入生产路径；它同时对照 production Lexer 运行结果和 C
scanner 的实际初始化表，因而任一侧漂移都会失败。Tree-sitter 原生 corpus 继续负责证明
external token 的真实边界行为，而 Rust 源码检查只负责完整集合一致性。

## 6. 实施计划

1. [x] 增加 53 词 scanner/Lexer 交叉测试 → 验证：frontend grammar 窄测试。
2. [x] 扩展 identifier 边界 corpus / fixture → 验证：`npm test` 与生产 Parser 交叉测试。
3. [x] 同步 Architecture、Spec 和索引 → 验证：文档与实现一致。
4. [x] 运行一次 workspace 基线并提交 → 验证：staged diff 单一且提交成功。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 词表交叉测试、corpus、文档与完成记录 | `test(tooling): lock Tree-sitter word contract (SPEC-0070)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `npm test` | 通过 | 7/7 corpus；拒绝 exact word 并接受更长 identifier |
| `cargo test -p lang-frontend --test tree_sitter_grammar --locked --offline` | 通过 | 4/4；53 词、11 个 L0002 span 与共享 fixture |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、390 tests、CLI build；0 failed / ignored / measured / filtered |

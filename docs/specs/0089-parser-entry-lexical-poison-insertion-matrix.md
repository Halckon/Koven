# SPEC-0089: 建立独立 Parser 入口词法 poison 插入矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-089` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0069、SPEC-0083、SPEC-0085–0088 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 独立 Parser 入口 lexical-mode gap / poison 插入测试、必要直接修复、Architecture |
| 语言语义变更 | 否；只锁定保留原语法 token 时额外 L0001 / L0002 输入与 string text 插入行为 |

## 1. Goal

完成后，三个独立 Parser 入口在代表性合法源码的每个显著 token gap 插入 `#` 或 `async` 时，
生产 Lexer 按真实 lexical mode 给出精确结果，Parser 返回确定、有界且 root 可解析的产物。

## 2. 范围与需求

- 复用 12 个 entry 样本与 240 个 token；枚举每个源码起点和每个 token 末尾，共 252 个 gap。
- 提取完整文件 insertion 矩阵内的 Code / String owner 栈为 tests 私有共享模块，两套矩阵必须
  继续固定各自 token / gap / mode 计数。
- 每个 gap 分别插入带隔离空格的 `#` / `async`，生成 504 个 mutation；code-mode 必须只产生
  目标 L0001 / L0002 及精确 primary Span，string-mode 必须保持 Lexer / Parser 零诊断。
- 全部 mutation 保持连续 lexeme 覆盖、唯一 EOF、source-local 有界诊断 / AST Span、typed root
  有效、两次公开 `Debug` 产物一致且无内部错误。
- 不增加依赖、公开 API、新诊断或合法语法；发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不要求 string-mode poison 形成独立 `StringText` lexeme，只要求内容被无损覆盖且语法保持合法。
- 不在 trivia、char literal或 UTF-8 scalar 内部插入，不组合多个 gap mutation。
- 不固定 code-mode Parser 诊断或恢复 AST 形态，不承诺独立入口后续 sentinel。

## 4. 验收标准

- [x] 12 个样本、240 个 token、252 个 gap 与 4 / 4 / 4 baseline 保持固定合法。
- [x] 两种 poison 分别覆盖全部 gap，共 504 个；入口 140 / 216 / 148，模式 239 / 13。
- [x] 478 个 code-mode mutation 精确产生唯一目标 code / Span；26 个 string-mode mutation 两阶段零诊断。
- [x] 全部 mutation 保持 Lexer / AST / diagnostic 不变量、root 有效和重复解析确定性。
- [x] 完整文件 insertion 矩阵的 396 / 418 / 409 / 9 计数保持不变。
- [x] 矩阵未发现生产缺陷，无需生产修复。
- [x] 直接相关窄测试与 Clippy 通过。
- [x] 最终成功的 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

共享 gap helper 接收 `(TokenKind, end_offset)` 迭代器，不依赖某个 integration test 内实例化的
`MutationSlot` 类型；用 Code / String 栈处理 StringStart、InterpolationStart/End、StringEnd，
baseline 结束必须回到唯一 Code 根。mutation 仍以全新 `SourceMap` 重词法，不复制 Scanner。

## 6. 实施计划

1. [x] 审计完整文件 poison insertion 与独立入口矩阵 → 验证：确认缺少 252 × 2 覆盖。
2. [x] 提取共享 lexical-mode gap 状态机 → 验证：SPEC-0083 的 396 / 418 / 409 / 9 计数不变。
3. [x] 建立独立入口 insertion / mode / root 矩阵并修复直接缺陷 → 验证：504 mutation，239 / 13 gap 分层，未发现生产缺陷。
4. [x] 运行直接相关窄测试与窄 Clippy → 验证：160/160，0 warnings。
5. [x] 同步事实并运行最终 workspace 标准基线 → 验证：五条标准命令全部成功，424 tests。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0089`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 独立入口 poison insertion 矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser entry lexical poison insertion (SPEC-0089)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --lib --test lexer --test parser_entry_adversarial --test parser_token_omission_matrix --test parser_token_duplication_matrix --test parser_lexical_poison_replacement_matrix --test parser_lexical_poison_insertion_matrix --test parser_entry_prefix_truncation_matrix --test parser_entry_token_omission_matrix --test parser_entry_token_duplication_matrix --test parser_entry_lexical_poison_replacement_matrix --test parser_entry_lexical_poison_insertion_matrix --test parser_expression --test parser_declaration --test parser_block --test parser_control_flow --locked --offline` | 通过 | 160/160；504 个独立入口 poison insertion mutation，未发现生产缺陷 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、424 tests、CLI build；0 failed / ignored / measured / filtered |

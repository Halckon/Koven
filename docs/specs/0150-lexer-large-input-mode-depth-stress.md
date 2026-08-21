# SPEC-0150: 建立 Lexer 大输入与深模式压力矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-150` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0073、SPEC-0103、SPEC-0129、SPEC-0149 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer 公开集成测试、Architecture |
| 语言语义变更 | 否；只锁定既有迭代 scanner 在大输入与深模式下的资源边界行为 |

## 1. Goal

完成后，生产 Lexer 对超长最大化 lexeme、4,096 层 string/interpolation mode、16,384 层
interpolation brace、深层未终止 mode 和 4,096 条独立诊断都必须双运行返回完整、确定、
source-local 的公开产物，不因用户输入深度或长度 panic、丢失覆盖或产生级联诊断。

## 2. 范围与需求

- 以 65,536-byte 级 identifier、number、whitespace、line comment、block comment 与多字节
  string text 锁定最大化扫描；每例必须形成预期的单一主体 lexeme 或三段 string token。
- 构造 4,096 层合法嵌套 string/interpolation，精确验证 4,096 个 `StringStart`、
  `InterpolationStart`、`InterpolationEnd` 与 `StringEnd`，以及唯一内层 identifier 和 EOF。
- 构造单一 interpolation 内 16,384 层平衡 brace，精确验证左右 brace 数、owner closer、
  string closer 与 EOF，不借助 Parser 递归预算限制 Lexer 深度。
- 构造 4,096 层未终止 string/interpolation mode；只允许最内层 L0005，primary Span 从最内层
  `${` 到 EOF，不得按外层 owner 数量产生级联诊断。
- 构造 4,096 个连续多字节非法 scalar；每个 scalar 形成一个 L0001 和对应 `Invalid` lexeme，
  诊断按 byte Span 严格递增并在重复运行间完全一致。
- 全部输入运行生产 Lexer 两次，锁定完整连续覆盖、唯一 EOF、UTF-8 byte Span、source
  identity、diagnostic detail Span 与完整公开产物确定性。
- 不增加生产依赖、公开 API、语言语义或人为 wall-clock 阈值；发现缺陷时只修复直接根因。

## 3. 非目标

- 不以固定耗时断言声称跨机器性能，不测 allocator 极限或 OOM 行为。
- 不改变 Parser 的 1,024 递归预算，也不让 Parser 接受超过该实现资源边界的嵌套语法。
- 不穷举 Unicode scalar、词法 token 或所有错误组合；这些由既有矩阵负责。

## 4. 验收标准

- [x] 6 类超长最大化输入精确保持既有 token / trivia 分段与零诊断。
- [x] 4,096 层合法 mode stack 精确闭合，16,386 个 lexeme 连续覆盖源码。
- [x] 16,384 层 brace depth 精确闭合，32,774 个 lexeme 连续覆盖源码。
- [x] 4,096 层未终止 mode 只产生一个 source-local L0005，不 panic 或级联。
- [x] 4,096 个多字节 poison 产生相同数量、严格有序且 byte-accurate 的 L0001。
- [x] 每个压力输入双运行完整公开产物一致，唯一 EOF 与 diagnostic Span 有效。
- [x] 新矩阵测试及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增单一 Lexer integration test target，复用 `lexer_matrix_assertions::lex_source_twice` 与
`lexer_output_assertions::validate_lexed`。测试只通过 `lang_frontend::lexer::lex` 公开入口构造
和检查产物，不暴露 scanner 私有 mode stack，也不新增测试专用生产计数器。

## 6. 实施计划

1. [x] 审计 Lexer / Parser 资源边界测试 → 验证：Parser 深度与长链已有覆盖，Lexer 深 mode /
   大诊断流没有独立验收。
2. [x] 建立 Lexer 压力矩阵 → 验证：最大化 lexeme、mode、brace、terminal 与 diagnostic 五族。
3. [x] 运行直接相关窄测试和窄 Clippy → 验证：新 target 通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0150`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Lexer 大输入 / 深模式压力矩阵、必要修复、Architecture 与完成记录 | `test(frontend): stress lexer depth and large inputs (SPEC-0150)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --test lexer_stress_matrix --locked --offline` 首次通过：5 passed，
  0 failed / ignored / measured / filtered；未发现生产缺陷。
- `cargo clippy -p lang-frontend --test lexer_stress_matrix --locked --offline -- -D warnings`
  通过：0 warnings。
- `cargo fmt --all -- --check` 通过。
- `cargo check --workspace --all-targets --locked --offline` 通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过：0 warnings。
- `cargo test --workspace --all-targets --locked --offline` 通过：453 passed，0 failed / ignored /
  measured / filtered。
- `cargo build -p lang-cli --locked --offline` 通过。

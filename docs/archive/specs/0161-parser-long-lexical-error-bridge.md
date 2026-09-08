# SPEC-0161: 建立 Parser 超长词法错误桥接矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-161` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0095、SPEC-0140–0143、SPEC-0158–0160 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四个公开 Parser 入口的超长 Lexer 错误桥接测试、Architecture |
| 语言语义变更 | 否；只锁定约 65 KiB L0003–L0008 产物进入 Parser 后的既有恢复行为 |

## 1. Goal

完成后，SPEC-0160 的七种超长词法错误分别进入 expression、declaration、block 与 file 四个公开
Parser 入口时，必须保留唯一 Lexer 根因及完整长 Span，形成可解引用的 Error / String AST 与
外层声明结构，不得因 owner EOF、长 lexeme 或入口 wrapper 产生 Parser 级联或内部错误。

## 2. 范围与需求

- 复用 65,536-byte payload 形状，覆盖 unterminated block comment、string、interpolation、
  terminal escape、interior invalid escape、closed invalid char 与 invalid number 七类源码。
- 每类分别投放到 expression 直接入口、变量 declaration initializer、block 局部变量 initializer
  和 file 顶层变量 initializer，共执行 28 个源码。
- terminal case 的 block 不伪造右花括号；必须依赖既有 Lexer EOF ownership 抑制缺 block / string
  closer 级联。可恢复 case 保留真实 block closer。
- 每个 Parser 产物必须恰有一个与生产 Lexer 相同的 L0003–L0008，primary Span 等于原错误范围
  加 wrapper byte offset；不得出现 L0009–L0078。
- comment / char / number 必须形成覆盖全部 payload 的 `Expression::Error`；四种 string owner
  必须形成覆盖全部 payload 的 `Expression::String`，interior L0006 仍保留 Error part。
- declaration、block 与 file 分别保留变量 Item、单一 local element 与单一 file root，并由其
  initializer 指向上述 Error / String expression。
- 28 个源码各执行两次生产 Lexer 与两次对应 Parser，共验证 56 个 Lexer 和 56 个 Parser 产物
  的连续覆盖、唯一 EOF、source-local AST / diagnostic Span、typed root 与完整确定性。
- 不改变 Lexer/Parser 语义、诊断目录、公开 API 或依赖；发现长 Span / owner 恢复缺陷时只修复
  直接根因。

## 3. 非目标

- 不重复 SPEC-0160 的逐 lexeme 分段断言，不把 Parser 测试当作 Scanner 单元测试。
- 不测试多个长错误同源组合、wall-clock / 内存阈值、随机 fuzzing 或语言长度上限。
- 不检查类型、所有权、运行时求值或 diagnostic renderer。
- 不新增共享生产状态、测试钩子或 property-testing 依赖。

## 4. 验收标准

- [x] 七形状 × 四入口的 28 个源码均保留唯一原始 Lexer 诊断及精确偏移 Span。
- [x] 28 个 Parser 产物均无 Parser 级联和内部错误，typed root 可解引用。
- [x] Error / String payload 形态、完整长 Span 及 declaration/block/file 外层数量准确。
- [x] 双 Lexer / 双 Parser 共验证 56 + 56 个公开产物及确定性不变量。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新 bridge target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration target，以 tests 私有 case descriptor 保存源码、错误范围、terminal 标志与
期望 expression kind。四入口 wrapper 只负责偏移和外层合法结构；Lexer / Parser 仍复用
`parser_test_assertions` 的生产双运行入口。测试遍历公开 AST table 和 typed child，不读取
`LexicalRecoveryIndex` 私有 sidecar。

## 6. 实施计划

1. [x] 审计 SPEC-0160 与现有 Parser owner/poison 矩阵 → 验证：长错误桥接缺失。
2. [x] 建立七形状 × 四入口矩阵 → 验证：28 个源码、唯一诊断、Error/String 与外层 AST。
3. [x] 运行新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0161`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 长词法错误桥接、必要修复、Architecture 与完成记录 | `test(frontend): bridge long lexical errors (SPEC-0161)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 新增 `parser_long_lexical_error_bridge` integration target：7 类约 65 KiB 错误分别经过四个
  Parser 公共入口，28 个源码均保留唯一 L0003–L0008、精确 wrapper 偏移 Span、完整 Error /
  String payload 与外层 typed 结构；双运行共覆盖 56 个 Lexer 和 56 个 Parser 产物。
- 未发现生产 Lexer / Parser 缺陷；未修改生产代码、公开 API、诊断目录或依赖。
- `cargo test -p lang-frontend --test parser_long_lexical_error_bridge --locked --offline`：1 passed，
  0 failed，0 ignored，0 filtered out。
- `cargo clippy -p lang-frontend --test parser_long_lexical_error_bridge --locked --offline -- -D warnings`：
  通过，0 warnings。
- workspace 标准基线在实现与 Architecture 同步后仅执行一次：
  - `cargo fmt --all -- --check`：通过。
  - `cargo check --workspace --all-targets --locked --offline`：通过。
  - `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
  - `cargo test --workspace --all-targets --locked --offline`：466 passed，0 failed，0 ignored。
  - `cargo build -p lang-cli --locked --offline`：通过。

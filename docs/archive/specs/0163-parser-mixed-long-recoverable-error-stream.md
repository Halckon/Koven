# SPEC-0163: 建立 Parser 混合超长可恢复词法错误流矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-163` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0075、SPEC-0095、SPEC-0140–0143、SPEC-0155–0162 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四个公开 Parser 入口的混合超长可恢复 Lexer 错误流测试、Architecture |
| 语言语义变更 | 否；只锁定 L0004/L0006–L0008 长错误后的既有 code-mode 与 Parser 恢复行为 |

## 1. Goal

完成后，同一约 320 KiB 闭合 call 中连续出现五类超长可恢复词法错误时，Lexer 必须在 closed
owner 或换行边界后准确返回 code mode，Parser 必须继续识别逗号、合法尾参数、真实 call closer
以及 block / file 后置声明，不得发生长距离游标漂移、诊断级联或 typed AST 丢失。

## 2. 范围与需求

- 固定 call argument 顺序为：长 closed string 内 L0006 invalid escape、长 newline-terminated
  L0004 string、长 newline-terminal L0006 escape、长 closed L0007 char、长 L0008 number，最后
  跟合法名称参数 `sentinel` 和真实 `)`。
- 每种长 argument 使用 65,536-byte 级 payload；换行恢复参数的 String / CallArgument Span 必须
  精确停在换行前，换行只作为 trivia 与 lexical recovery boundary。
- 同一 closed call 分别投放到 expression 直接入口、变量 declaration initializer、block 局部
  变量 initializer 和 file 顶层变量 initializer，共执行 4 个源码。
- 每例必须恰有源码顺序的 L0006、L0004、L0006、L0007、L0008 五条 Lexer 诊断，primary Span
  必须等于各错误相对范围加 wrapper offset，不得出现 Parser 诊断。
- call 必须保留六个 argument：前三项为完整 String，两个 invalid escape 各含一个 Error part；
  中间 char / number 为完整 Error；末项为真实 Name `sentinel`。Call Span 必须包含真实 `)`。
- declaration 保留变量 Item；block 除 result local 外还必须保留后置 `val after = 0` local；file
  除 result root 外还必须保留后置 `val after = 0` root，证明恢复已越过 call 返回外层 code mode。
- 4 个源码各执行两次生产 Lexer 与两次对应 Parser，共验证 8 个 Lexer 和 8 个 Parser 产物的
  连续覆盖、唯一 EOF、source-local AST / diagnostic Span、typed root 与完整确定性。
- 不改变 Lexer / Parser 语义、诊断目录、公开 API 或依赖；发现长恢复流缺陷时只修复直接根因。

## 3. 非目标

- 不重复逐 lexeme 分段断言，不把 Parser 测试当作 Scanner 单元测试。
- 不覆盖 L0003 / L0005 或 EOF terminal owner；其长输入边界由 SPEC-0160–0162 锁定。
- 不声明源码或 token 长度上限，不设置 wall-clock / 内存阈值。
- 不随机排列 argument，不做组合爆炸或 fuzzing。
- 不检查类型、所有权、运行时求值或 diagnostic renderer。
- 不新增共享生产状态、测试钩子或 property-testing 依赖。

## 4. 验收标准

- [x] 四入口源码均保留五条有序 Lexer 根因及精确 wrapper-offset Span。
- [x] 每例零 Parser 诊断，closed call 与六个 typed argument 完整可解引用。
- [x] 三个 String、两个 Error 与合法 Name 的形态、Error part 和长 Span 准确。
- [x] block / file 的后置 `val after = 0` 分别作为第二个 local / root 存活。
- [x] 双 Lexer / 双 Parser 共验证 8 + 8 个公开产物及确定性不变量。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新 recoverable-stream target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration target，以 tests 私有 builder 记录每个 argument 的源码与语法 Span、错误
相对范围和期望 expression kind。四入口 wrapper 只负责偏移与后置声明；Lexer / Parser 仍复用
`parser_test_assertions` 的生产双运行入口。测试只遍历公开 AST table、CallArgument 和 typed
child，不读取 `LexicalRecoveryIndex` 私有 sidecar。

## 6. 实施计划

1. [x] 审计短 owner sentinel 与长单错误 / terminal 混合流证据 → 验证：长可恢复混合流缺失。
2. [x] 建立五错误 + sentinel × 四入口矩阵 → 验证：4 个源码、20 条 Lexer 根因与完整 Call AST。
3. [x] 运行新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0163`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 混合长可恢复错误流、必要修复、Architecture 与完成记录 | `test(frontend): recover mixed long lexical streams (SPEC-0163)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 新增 `parser_mixed_long_recoverable_error_stream` integration target：同一约 320 KiB closed call
  内的五类可恢复长错误分别经过四个 Parser 公共入口，4 个源码均保留有序 L0006 / L0004 /
  L0006 / L0007 / L0008、精确 wrapper-offset Span、六个 CallArgument 与外层 typed 结构；双运行
  共覆盖 8 个 Lexer 和 8 个 Parser 产物。
- 两个 newline owner 的 String / CallArgument 均精确停在换行前；合法 `sentinel` 参数、真实 `)`
  及 block / file 的第二个 `val after = 0` local / root 全部存活，且没有 Parser 诊断。
- 未发现生产 Lexer / Parser 缺陷；未修改生产代码、公开 API、诊断目录或依赖。
- `cargo test -p lang-frontend --test parser_mixed_long_recoverable_error_stream --locked --offline`：
  1 passed，0 failed，0 ignored，0 filtered out。
- `cargo clippy -p lang-frontend --test parser_mixed_long_recoverable_error_stream --locked --offline -- -D warnings`：
  通过，0 warnings。
- workspace 标准基线在实现与 Architecture 同步后仅执行一次：
  - `cargo fmt --all -- --check`：通过。
  - `cargo check --workspace --all-targets --locked --offline`：通过。
  - `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
  - `cargo test --workspace --all-targets --locked --offline`：468 passed，0 failed，0 ignored。
  - `cargo build -p lang-cli --locked --offline`：通过。

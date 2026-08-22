# SPEC-0164: 建立 Parser 超长 UTF-8 换行恢复矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-164` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0002、SPEC-0006–0009、SPEC-0014、SPEC-0078、SPEC-0098、SPEC-0100、SPEC-0110、SPEC-0112、SPEC-0150、SPEC-0160–0163 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四个公开 Parser 入口的超长 UTF-8 string owner 换行恢复测试、Architecture |
| 语言语义变更 | 否；只锁定 L0004 / L0006 在长多字节 payload 与 LF / CRLF 边界的既有恢复行为 |

## 1. Goal

完成后，约 65 KiB 多字节 StringText 后的未终止字符串或 terminal escape 分别遇到 LF / CRLF
时，Lexer 必须在正确 UTF-8 byte boundary 结束诊断和 owner，Parser 必须继续识别逗号、合法
sentinel、真实 call closer 以及 block / file 后置声明，不得把 scalar 列数、CRLF 双字节或 wrapper
偏移误当成 AST / diagnostic byte offset。

## 2. 范围与需求

- payload 固定为 21,845 个 `界`，共 65,535 bytes，确保单个 StringText 同时跨越大量 Unicode
  scalar 和 16-bit 长度边界，但不声明任何语言长度上限。
- owner 形状为 newline-terminated L0004 string 与 newline-terminal L0006 escape，各自分别使用
  LF 和 CRLF，共四类 case；换行后均追加 `, sentinel)`。
- 四类 owner 分别投放到 expression、declaration、block 与 file 四个公开入口，共执行 16 个源码。
- L0004 primary 必须覆盖引号到 CR / LF 前；L0006 primary 只覆盖反斜杠。String、首个
  CallArgument 与 StringText Span 必须同样停在换行前或反斜杠前后的规范边界，全部以 byte
  offset 加 wrapper 偏移计算且落在 UTF-8 boundary。
- 每例必须恰有一个 L0004 或 L0006，无 Parser 诊断；Call 保留 String 与 Name 两个 argument、
  真实 `)` 和完整 Span，Name 源码切片精确等于 `sentinel`。
- block / file 还必须保留后置 `val after = 0` local / root，证明 LF / CRLF 后已返回外层 code mode。
- 16 个源码各执行两次生产 Lexer 与两次对应 Parser，共验证 32 个 Lexer 和 32 个 Parser 产物
  的连续覆盖、唯一 EOF、source-local AST / diagnostic Span、typed root 与完整确定性。
- 不改变 Lexer / Parser 语义、诊断目录、公开 API 或依赖；发现 UTF-8 / CRLF 恢复缺陷时只修复
  直接根因。

## 3. 非目标

- 不重复 LF / CRLF 的全部语法分隔矩阵，也不重复逐 lexeme Scanner 分段断言。
- 不覆盖 EOF terminal owner、插值 owner、裸 CR 或非法 Unicode 编码。
- 不验证 renderer 的行列展示，不设置 wall-clock / 内存阈值。
- 不检查类型、所有权或运行时求值。
- 不新增共享生产状态、测试钩子或 property-testing 依赖。

## 4. 验收标准

- [x] 四 owner/carrier × 四入口的 16 个源码均保留唯一 L0004 / L0006 和精确 byte Span。
- [x] StringText、String、CallArgument、Name 与 Call Span 均落在正确 UTF-8 / CRLF 边界。
- [x] 每例零 Parser 诊断，两个 typed argument 和真实 call closer 完整可解引用。
- [x] block / file 后置 `val after = 0` 分别作为第二个 local / root 存活。
- [x] 双 Lexer / 双 Parser 共验证 32 + 32 个公开产物及确定性不变量。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新 UTF-8 line-recovery target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration target，以 tests 私有 case builder 记录 owner 源码、语法长度、错误相对范围
和 Error part 数量。四入口 wrapper 只负责 byte 偏移与后置声明；Lexer / Parser 仍复用
`parser_test_assertions` 的生产双运行入口。测试遍历公开 AST table、CallArgument、StringPart 与
源码切片，不读取 `LexicalRecoveryIndex` 私有 sidecar。

## 6. 实施计划

1. [x] 审计短 LF / CRLF 矩阵与长 ASCII owner 证据 → 验证：长 UTF-8 × CRLF 交叉边界缺失。
2. [x] 建立四 owner/carrier × 四入口矩阵 → 验证：16 个源码与精确 byte Span / sentinel。
3. [x] 运行新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0164`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 长 UTF-8 换行恢复、必要修复、Architecture 与完成记录 | `test(frontend): recover long utf8 string lines (SPEC-0164)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 新增 `parser_long_utf8_line_recovery` integration target：四种 65,535-byte 多字节 StringText
  owner 分别经过四个 Parser 公共入口，16 个源码均保留唯一 L0004 / L0006、精确 byte Span、
  两个 CallArgument 与外层 typed 结构；双运行共覆盖 32 个 Lexer 和 32 个 Parser 产物。
- StringText 源码切片均精确包含 21,845 个 `界`；LF / CRLF 前的 String / argument、反斜杠
  L0006、后置 `sentinel`、真实 `)` 与 block / file 第二个 `val after = 0` local / root 全部准确，
  且没有 Parser 诊断。
- 未发现生产 Lexer / Parser 缺陷；未修改生产代码、公开 API、诊断目录或依赖。
- `cargo test -p lang-frontend --test parser_long_utf8_line_recovery --locked --offline`：1 passed，
  0 failed，0 ignored，0 filtered out。
- `cargo clippy -p lang-frontend --test parser_long_utf8_line_recovery --locked --offline -- -D warnings`：
  通过，0 warnings。
- workspace 标准基线在实现与 Architecture 同步后仅执行一次：
  - `cargo fmt --all -- --check`：通过。
  - `cargo check --workspace --all-targets --locked --offline`：通过。
  - `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
  - `cargo test --workspace --all-targets --locked --offline`：469 passed，0 failed，0 ignored。
  - `cargo build -p lang-cli --locked --offline`：通过。

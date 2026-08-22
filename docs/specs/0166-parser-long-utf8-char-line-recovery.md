# SPEC-0166: 建立 Parser 超长 UTF-8 Char 换行恢复矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-166` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0002、SPEC-0006–0009、SPEC-0014、SPEC-0073、SPEC-0095、SPEC-0103、SPEC-0150、SPEC-0155、SPEC-0157–0160、SPEC-0163–0165 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四个公开 Parser 入口的超长 UTF-8 invalid Char 换行恢复测试、Architecture |
| 语言语义变更 | 否；只锁定 L0007 在 LF / CRLF 前停止扫描并从 interpolation 返回 Parser 的既有行为 |

## 1. Goal

完成后，约 65 KiB 多字节 payload 的非法 `Char` 遇到 LF / CRLF，或其末尾反斜杠紧邻该换行时，
Lexer 必须产生覆盖引号到换行前的单一 L0007 且不消费换行；Parser 必须在 interpolation 中把该
Invalid 保留为 Error expression，并继续关闭内外两层 call、interpolation 与外层 string。

## 2. 范围与需求

- payload 固定为 21,845 个 `界`，共 65,535 bytes，确保单个 invalid Char 跨越大量 Unicode
  scalar 和 16-bit 长度边界，但不声明任何语言长度上限。
- 四类 carrier 分别为 payload 后直接 LF / CRLF，以及 payload 与 LF / CRLF 之间多一个反斜杠；
  每类均形成从开始单引号到换行前的单一 L0007 primary / Invalid 范围。
- 每个 invalid Char 固定放入 outer string interpolation 的 inner call 首个 argument；换行后必须
  继续识别 `inner_sentinel`、inner `)`、interpolation `}`、outer tail / `"`、`outer_sentinel`
  与 outer `)`。
- 四类 carrier 分别投放到 expression、declaration、block 与 file 四个公开入口，共执行 16 个源码。
- 每例必须恰有一个 L0007，无 Parser 诊断；inner CallArgument 与 `Expression::Error` Span 必须
  等于 L0007 primary，全部以 byte offset 加 wrapper 偏移计算且落在 UTF-8 boundary。
- 外层 String 必须精确保留 head、完整 Interpolation 和 tail 三部分；两层 Call 均保留两个 typed
  argument 与真实 closer，两组 sentinel 源码切片必须准确。
- block / file 还必须保留后置 `val after = 0` local / root，证明换行未被 Char 消费并且 Parser
  已返回最外层 code mode。
- 16 个源码各执行两次生产 Lexer 与两次对应 Parser，共验证 32 个 Lexer 和 32 个 Parser 产物
  的连续覆盖、唯一 EOF、source-local AST / diagnostic Span、typed root 与完整确定性。
- 不改变 Lexer / Parser 语义、诊断目录、公开 API 或依赖；发现 Char 边界缺陷时只修复直接根因。

## 3. 非目标

- 不重复短 LF、闭合多 scalar Char、EOF terminal Char 或大量 sibling L0007 压力矩阵。
- 不覆盖合法 Char、数值形式 Unicode 转义、裸 CR 或非法 Unicode 编码。
- 不验证 renderer 的行列展示，不设置 wall-clock / 内存阈值。
- 不检查类型、所有权或运行时求值。
- 不新增共享生产状态、测试钩子或 property-testing 依赖。

## 4. 验收标准

- [x] 四 Char carrier × 四入口的 16 个源码均保留唯一 L0007 和精确 byte Span。
- [x] L0007、inner CallArgument 与 `Expression::Error` 范围一致且停在 LF / CRLF 前。
- [x] 每例零 Parser 诊断，两层 call、outer string/interpolation、两组 sentinel 与 closer 完整。
- [x] block / file 后置 `val after = 0` 分别作为第二个 local / root 存活。
- [x] 双 Lexer / 双 Parser 共验证 32 + 32 个公开产物及确定性不变量。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新 Char line-recovery target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration target，以 tests 私有 case builder 记录 invalid Char、两层 owner 和各 sentinel
的精确相对 byte offset。四入口 wrapper 只负责统一偏移与后置声明；Lexer / Parser 仍复用
`parser_test_assertions` 的生产双运行入口。测试只遍历公开 AST table、CallArgument、StringPart
与源码切片，不读取 Lexer / Parser 私有恢复状态。

## 6. 实施计划

1. [x] 审计短 Char line recovery、长 L0007 与 nested owner 证据 → 验证：长 UTF-8 × CRLF / backslash × interpolation × 四入口交叉边界缺失。
2. [x] 建立四 Char carrier × 四入口矩阵 → 验证：16 个源码与两层 typed owner / 精确 Span。
3. [x] 运行新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0166`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 长 UTF-8 Char 换行恢复、必要修复、Architecture 与完成记录 | `test(frontend): recover long utf8 char lines (SPEC-0166)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 新增 `parser_long_utf8_char_line_recovery` integration target：四种 65,535-byte 多字节 invalid
  Char carrier 分别经过四个 Parser 公共入口，16 个源码均保留唯一 L0007、精确 byte Span、
  `Expression::Error`、两层 Call 与完整外层 String；双运行共覆盖 32 个 Lexer 和 32 个 Parser 产物。
- L0007、inner CallArgument 与 Error AST 范围一致；直接或反斜杠紧邻的 LF / CRLF 均未被 Char
  消费，换行后的两组 sentinel、所有真实 closer 及 block / file 第二个 `val after = 0` 均存活，
  且没有 Parser 诊断。
- 未发现生产 Lexer / Parser 缺陷；未修改生产代码、公开 API、诊断目录或依赖。
- `cargo test -p lang-frontend --test parser_long_utf8_char_line_recovery --locked --offline`：
  1 passed，0 failed，0 ignored，0 filtered out。
- `cargo clippy -p lang-frontend --test parser_long_utf8_char_line_recovery --locked --offline -- -D warnings`：
  通过，0 warnings。
- workspace 标准基线在实现与 Architecture 同步后仅执行一次：
  - `cargo fmt --all -- --check`：通过。
  - `cargo check --workspace --all-targets --locked --offline`：通过。
  - `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
  - `cargo test --workspace --all-targets --locked --offline`：471 passed，0 failed，0 ignored。
  - `cargo build -p lang-cli --locked --offline`：通过。

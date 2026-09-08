# SPEC-0165: 建立 Parser 超长 UTF-8 嵌套换行恢复矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-165` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0002、SPEC-0006–0009、SPEC-0014、SPEC-0078、SPEC-0098、SPEC-0100、SPEC-0110、SPEC-0112、SPEC-0150、SPEC-0155、SPEC-0160–0164 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四个公开 Parser 入口的超长 UTF-8 nested string/interpolation 换行恢复测试、Architecture |
| 语言语义变更 | 否；只锁定 L0004 / L0006 从嵌套 string 恢复至 interpolation、外层 string 和 code mode 的既有行为 |

## 1. Goal

完成后，约 65 KiB 多字节 StringText 后的内层未终止字符串或 terminal escape 分别遇到 LF /
CRLF 时，Lexer 必须只弹出当前 string mode 并返回 interpolation；Parser 必须继续关闭内层 call、
interpolation、外层 string 与外层 call，且四个公开入口均保持精确 UTF-8 byte Span 和 typed AST。

## 2. 范围与需求

- payload 固定为 21,845 个 `界`，共 65,535 bytes，确保单个内层 StringText 跨越大量 Unicode
  scalar 和 16-bit 长度边界，但不声明任何语言长度上限。
- 完整表达式形状固定为外层 call，其首个 argument 是含 head / interpolation / tail 三部分的
  string；interpolation 根是内层 call，内层首个 argument 分别承载 newline-terminated L0004
  string 与 newline-terminal L0006 escape，并分别使用 LF / CRLF，共四类 case。
- 换行后必须继续识别内层 `inner_sentinel`、内层 `)`、interpolation `}`、外层 tail / `"`、
  外层 `outer_sentinel` 与外层 `)`；四类 case 分别投放到 expression、declaration、block 与 file
  四个公开入口，共执行 16 个源码。
- L0004 primary 必须覆盖内层引号到 CR / LF 前；L0006 primary 只覆盖反斜杠。内层 StringText、
  String 与 CallArgument Span 必须同样停在规范 byte boundary，全部以 wrapper 偏移计算。
- 每例必须恰有一个 L0004 或 L0006，无 Parser 诊断；外层 String 必须精确保留 head、完整
  Interpolation 和 tail 三部分，两层 Call 均保留两个 typed argument 与真实 closer。
- block / file 还必须保留后置 `val after = 0` local / root，证明恢复已依次越过 string、
  interpolation、外层 call 并返回最外层 code mode。
- 16 个源码各执行两次生产 Lexer 与两次对应 Parser，共验证 32 个 Lexer 和 32 个 Parser 产物
  的连续覆盖、唯一 EOF、source-local AST / diagnostic Span、typed root 与完整确定性。
- 不改变 Lexer / Parser 语义、诊断目录、公开 API 或依赖；发现模式栈恢复缺陷时只修复直接根因。

## 3. 非目标

- 不重复短输入 nested owner、EOF terminal owner、深层递归预算或大量 sibling owner 压力矩阵。
- 不覆盖裸 CR、插值自身未终止、外层 string 错误或非法 Unicode 编码。
- 不验证 renderer 的行列展示，不设置 wall-clock / 内存阈值。
- 不检查类型、所有权或运行时求值。
- 不新增共享生产状态、测试钩子或 property-testing 依赖。

## 4. 验收标准

- [x] 四 inner-owner/carrier × 四入口的 16 个源码均保留唯一 L0004 / L0006 和精确 byte Span。
- [x] 内层 StringText / String / CallArgument 与外层 interpolation / String / Call Span 全部准确。
- [x] 每例零 Parser 诊断，两层 call、两组 sentinel 与所有真实 closer 完整可解引用。
- [x] block / file 后置 `val after = 0` 分别作为第二个 local / root 存活。
- [x] 双 Lexer / 双 Parser 共验证 32 + 32 个公开产物及确定性不变量。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新 nested line-recovery target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration target，以 tests 私有 case builder 记录两层 owner 的源码和精确相对 byte
offset。四入口 wrapper 只负责统一偏移与后置声明；Lexer / Parser 仍复用
`parser_test_assertions` 的生产双运行入口。测试只遍历公开 AST table、CallArgument、StringPart
与源码切片，不读取 `LexicalRecoveryIndex` 私有 sidecar。

## 6. 实施计划

1. [x] 审计短 nested line recovery 与长顶层 owner 证据 → 验证：长 UTF-8 × CRLF × 完整嵌套模式栈交叉边界缺失。
2. [x] 建立四 inner-owner/carrier × 四入口矩阵 → 验证：16 个源码与两层 typed owner / 精确 Span。
3. [x] 运行新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0165`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 长 UTF-8 嵌套换行恢复、必要修复、Architecture 与完成记录 | `test(frontend): recover nested long utf8 string lines (SPEC-0165)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 新增 `parser_long_utf8_nested_line_recovery` integration target：四种 65,535-byte 多字节内层
  StringText owner 分别经过四个 Parser 公共入口，16 个源码均保留唯一 L0004 / L0006、精确
  byte Span、两层 Call 与完整外层 String；双运行共覆盖 32 个 Lexer 和 32 个 Parser 产物。
- 换行后 `inner_sentinel`、inner call closer、interpolation closer、outer tail / string closer、
  `outer_sentinel`、outer call closer，以及 block / file 第二个 `val after = 0` local / root 全部
  存活；没有 Parser 诊断。
- 未发现生产 Lexer / Parser 缺陷；未修改生产代码、公开 API、诊断目录或依赖。
- `cargo test -p lang-frontend --test parser_long_utf8_nested_line_recovery --locked --offline`：
  1 passed，0 failed，0 ignored，0 filtered out。
- `cargo clippy -p lang-frontend --test parser_long_utf8_nested_line_recovery --locked --offline -- -D warnings`：
  通过，0 warnings。
- workspace 标准基线在实现与 Architecture 同步后仅执行一次：
  - `cargo fmt --all -- --check`：通过。
  - `cargo check --workspace --all-targets --locked --offline`：通过。
  - `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
  - `cargo test --workspace --all-targets --locked --offline`：通过，0 failed；随后使用 `-- --list`
    只枚举、不重跑，共确认 470 个测试，且仓库无 `#[ignore]`。
  - `cargo build -p lang-cli --locked --offline`：通过。

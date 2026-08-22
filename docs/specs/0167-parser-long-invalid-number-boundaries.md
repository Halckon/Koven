# SPEC-0167: 建立 Parser 超长非法数字边界矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-167` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0002、SPEC-0006–0009、SPEC-0014、SPEC-0073、SPEC-0095、SPEC-0103、SPEC-0150、SPEC-0155、SPEC-0157–0163、SPEC-0165–0166 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四个公开 Parser 入口的超长 L0008 maximal-region / operator-boundary 测试、Architecture |
| 语言语义变更 | 否；只锁定非法数字最大 ASCII 区域在 `+` 前停止并进入 typed Parser 恢复的既有行为 |

## 1. Goal

完成后，约 65 KiB 的四类非法数字候选必须分别形成单一 L0008 / Error，并在真实 `+` 前停止；
Parser 必须保留 `Error + rhs` 二元结构、所在 inner call、outer string interpolation 与 outer call，
证明最大化数字扫描不会吞掉运算符、右操作数或 lexical-owner closer。

## 2. 范围与需求

- 四类候选分别覆盖：长整数核心后的指数尾、长浮点核心后的指数尾、合法 `uL` 后的长非法
  identifier tail，以及 `0x` 后的长非法进制 tail；各非法尾均由 ASCII identifier-continue 字节
  组成，含数字核心与可选小数点的完整候选达到约 65 KiB。
- 每个非法数字固定作为 `invalid + rhs` 的左操作数，并作为 outer string interpolation 中 inner
  call 的首个 argument；数字后必须保留真实 `+`、Name `rhs`、argument comma、`inner_sentinel`、
  inner `)`、interpolation `}`、outer tail / `"`、`outer_sentinel` 与 outer `)`。
- 四类候选分别投放到 expression、declaration、block 与 file 四个公开入口，共执行 16 个源码。
- 每例必须恰有一个 L0008，无 Parser 诊断；L0008 primary 与左侧 `Expression::Error` Span 必须
  精确覆盖全部非法数字，且 `BinaryOperator::Add` 的 operator Span 紧随其后但不重叠。
- inner CallArgument 必须覆盖完整 `Error + rhs`，Binary 与右侧 Name Span 必须准确；外层 String
  精确保留 head、完整 Interpolation 和 tail 三部分，两层 Call 均保留两个 typed argument 与 closer。
- block / file 还必须保留后置 `val after = 0` local / root，证明扫描与解析均返回最外层 code mode。
- 16 个源码各执行两次生产 Lexer 与两次对应 Parser，共验证 32 个 Lexer 和 32 个 Parser 产物
  的连续覆盖、唯一 EOF、source-local AST / diagnostic Span、typed root 与完整确定性。
- 不改变 Lexer / Parser 语义、诊断目录、公开 API 或依赖；发现 maximal-region 缺陷时只修复直接根因。

## 3. 非目标

- 不重复短数字全集、合法数字后缀、range 消歧义、单一长 `…e3` Error 或大量 sibling L0008 压力。
- 不定义指数、进制、下划线或新后缀语义，也不验证数字值、溢出或类型推断。
- 不验证 renderer 的行列展示，不设置 wall-clock / 内存阈值。
- 不检查所有权或运行时求值。
- 不新增共享生产状态、测试钩子或 property-testing 依赖。

## 4. 验收标准

- [x] 四数字候选 × 四入口的 16 个源码均保留唯一 L0008 和精确 maximal-region Span。
- [x] 每个 L0008 与左侧 Error Span 一致，并在真实 `+` operator Span 前停止且不重叠。
- [x] 每例零 Parser 诊断，完整 `Error + rhs`、两层 call、outer string/interpolation 与 closer 存活。
- [x] 两组 sentinel 与 block / file 后置 `val after = 0` 保持精确 typed 结构和源码切片。
- [x] 双 Lexer / 双 Parser 共验证 32 + 32 个公开产物及确定性不变量。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新 invalid-number boundary target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration target，以 tests 私有 case builder 记录四类非法数字与两层 owner 的精确相对
byte offset。四入口 wrapper 只负责统一偏移与后置声明；Lexer / Parser 仍复用
`parser_test_assertions` 的生产双运行入口。测试只遍历公开 AST table、CallArgument、StringPart、
Binary payload 与源码切片，不读取 Lexer / Parser 私有恢复状态。

## 6. 实施计划

1. [x] 审计短数字全集、长 L0008 与 Parser poison 证据 → 验证：四类 maximal candidate × operator × interpolation × 四入口交叉边界缺失。
2. [x] 建立四数字候选 × 四入口矩阵 → 验证：16 个源码与 `Error + rhs` / 两层 typed owner。
3. [x] 运行新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0167`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 长非法数字边界矩阵、必要修复、Architecture 与完成记录 | `test(frontend): preserve long invalid number boundaries (SPEC-0167)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 新增 `parser_long_invalid_number_boundaries` integration target：四种约 65 KiB 的 maximal invalid
  numeric candidate 分别经过四个 Parser 公共入口，16 个源码均保留唯一 L0008、精确 byte Span、
  `Error + rhs` Binary、两层 Call 与完整 outer String；双运行共覆盖 32 个 Lexer 和 32 个 Parser 产物。
- 整数 / 浮点指数尾、合法 `uL` 后长非法尾和非法 `0x` 长尾均由 L0008 / Error 精确完整覆盖，
  真实 `+`、右侧 Name、两组 sentinel、全部 closer 及 block / file 第二个 `val after = 0` 均存活，
  且没有 Parser 诊断。
- 未发现生产 Lexer / Parser 缺陷；未修改生产代码、公开 API、诊断目录或依赖。
- `cargo test -p lang-frontend --test parser_long_invalid_number_boundaries --locked --offline`：
  1 passed，0 failed，0 ignored，0 filtered out。
- `cargo clippy -p lang-frontend --test parser_long_invalid_number_boundaries --locked --offline -- -D warnings`：
  通过，0 warnings。
- workspace 标准基线在实现与 Architecture 同步后仅执行一次：
  - `cargo fmt --all -- --check`：通过。
  - `cargo check --workspace --all-targets --locked --offline`：通过。
  - `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
  - `cargo test --workspace --all-targets --locked --offline`：472 passed，0 failed，0 ignored。
  - `cargo build -p lang-cli --locked --offline`：通过。

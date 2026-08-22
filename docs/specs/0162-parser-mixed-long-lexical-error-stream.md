# SPEC-0162: 建立 Parser 混合超长词法错误流矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-162` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0095、SPEC-0140–0143、SPEC-0155–0161 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四个公开 Parser 入口的混合超长 Lexer 错误流测试、Architecture |
| 语言语义变更 | 否；只锁定 L0003–L0008 混合长错误进入嵌套 call 后的既有恢复行为 |

## 1. Goal

完成后，同一约 256 KiB 源码中的三个可恢复超长词法错误和一个 EOF terminal owner 进入
expression、declaration、block 与 file 四个公开 Parser 入口时，必须保持全部 Lexer 根因的顺序、
精确 Span、四个 call argument 与外层 typed AST；terminal owner 只抑制自己拥有的缺失 closer，
Parser 必须恰好保留独立的 call closer L0010，不得因长距离游标推进或外层 block closer 产生其他
级联或内部错误。

## 2. 范围与需求

- 固定可恢复前缀依次包含长 closed string 内的 L0006 invalid escape、长 closed L0007 char 与长
  L0008 number；三者由真实 call argument comma 分隔。
- terminal argument 分别使用长 L0003 unterminated block comment、L0004 string、L0005
  interpolation 与 L0006 terminal escape，形成四种源码形状。
- 四种形状分别投放到 expression 直接入口、变量 declaration initializer、block 局部变量
  initializer 和 file 顶层变量 initializer，共执行 16 个源码。
- terminal owner 后不伪造 call / block closer；既有 Lexer EOF ownership 只抑制 terminal string /
  interpolation 自身与外层 block 的派生 closer，未闭合 call 必须保留一条独立 L0010。
- 每个产物必须恰有四条源码顺序的 Lexer 诊断：固定 L0006、L0007、L0008 后跟对应 terminal
  L0003 / L0004 / L0005 / L0006；随后恰有一条 EOF 空 Span 的 L0010，其唯一 label 指向真实
  call `(`。全部 Span 必须等于相对范围加 wrapper offset，不得出现其他 Parser 诊断。
- call 必须保留四个 argument：首项为含一个 Error part 的完整 String，中间两项为完整 Error，
  terminal comment 为完整 Error，其余 terminal owner 为完整 String，terminal escape 含一个
  Error part。
- declaration、block 与 file 分别保留变量 Item、单一 local element 与单一 file root，并由其
  initializer 指向上述 Call expression。
- 16 个源码各执行两次生产 Lexer 与两次对应 Parser，共验证 32 个 Lexer 和 32 个 Parser 产物
  的连续覆盖、唯一 EOF、source-local AST / diagnostic Span、typed root 与完整确定性。
- 不改变 Lexer / Parser 语义、诊断目录、公开 API 或依赖；发现长错误流恢复缺陷时只修复直接
  根因。

## 3. 非目标

- 不重复逐 lexeme 分段断言，不把 Parser 测试当作 Scanner 单元测试。
- 不声明源码或 token 长度上限，不设置 wall-clock / 内存阈值。
- 不覆盖多个 terminal owner（EOF 后不存在后续源码）、随机排列或组合爆炸。
- 不检查类型、所有权、运行时求值或 diagnostic renderer。
- 不新增共享生产状态、测试钩子或 property-testing 依赖。

## 4. 验收标准

- [x] 四 terminal owner × 四入口的 16 个源码均保留四条有序 Lexer 根因及精确偏移 Span。
- [x] 每例恰有一条 EOF L0010 及 call opener label，无其他 Parser 级联或内部错误。
- [x] 16 个 Parser 产物的 typed root 均可解引用。
- [x] 四个 call argument、各自 Error / String 形态和完整长 Span 准确。
- [x] declaration / block / file 外层结构与 initializer 连接准确。
- [x] 双 Lexer / 双 Parser 共验证 32 + 32 个公开产物及确定性不变量。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新 mixed-stream target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration target，以 tests 私有 case descriptor 保存 terminal payload、诊断相对范围与
期望 argument kind。固定可恢复 argument 前缀与四入口 wrapper 只负责生成合法偏移；Lexer /
Parser 仍复用 `parser_test_assertions` 的生产双运行入口。测试只遍历公开 AST table、CallArgument
和 typed child，不读取 `LexicalRecoveryIndex` 私有 sidecar。

## 6. 实施计划

1. [x] 审计长错误、同类 poison 压力与 owner 恢复证据 → 验证：异构长错误同源桥接缺失。
2. [x] 建立四 terminal owner × 四入口矩阵 → 验证：16 个源码、64 条 Lexer 根因、16 条 L0010 与完整 Call AST。
3. [x] 运行新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0162`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 混合长错误流矩阵、必要修复、Architecture 与完成记录 | `test(frontend): mix long lexical error streams (SPEC-0162)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 新增 `parser_mixed_long_lexical_error_stream` integration target：四种约 256 KiB 混合错误流分别
  经过四个 Parser 公共入口，16 个源码均保留四条有序 Lexer 根因、唯一 EOF L0010、精确 primary /
  opener label Span、四个 CallArgument 及外层 typed 结构；双运行共覆盖 32 个 Lexer 和 32 个
  Parser 产物。
- 第一次窄测试按初始“terminal owner 抑制全部 closer”假设运行，准确暴露每例存在独立 L0010；
  审计 `parser_expression` 的既有精确回归后确认该 L0010 是外层 call 的预期语法错误，随后修正
  Spec 与测试预期。未修改生产 Lexer / Parser、公开 API、诊断目录或依赖。
- 修正后 `cargo test -p lang-frontend --test parser_mixed_long_lexical_error_stream --locked --offline`：
  1 passed，0 failed，0 ignored，0 filtered out。
- `cargo clippy -p lang-frontend --test parser_mixed_long_lexical_error_stream --locked --offline -- -D warnings`：
  通过，0 warnings。
- workspace 标准基线在实现与 Architecture 同步后仅执行一次：
  - `cargo fmt --all -- --check`：通过。
  - `cargo check --workspace --all-targets --locked --offline`：通过。
  - `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
  - `cargo test --workspace --all-targets --locked --offline`：467 passed，0 failed，0 ignored。
  - `cargo build -p lang-cli --locked --offline`：通过。

# SPEC-0156: 压力验证 Lexer 字符串错误向 Parser 的唯一传播

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-156` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0095、SPEC-0140–0142、SPEC-0155 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四个公开 Parser 入口 owner-rich 压力测试、Architecture |
| 语言语义变更 | 否；只锁定大量可恢复 L0006 owner 进入 Parser 后的既有唯一根因行为 |

## 1. Goal

完成后，expression、declaration、block 与 file 四个公开 Parser 入口都必须保留 4,096 个含可
恢复非法转义的完整 string owner，各 owner 只携带一个 Lexer L0006 与一个 `StringPart::Error`，
不得级联 Parser 诊断、吞 owner 或破坏外层 AST 数量。

## 2. 范围与需求

- 固定 owner 源码为 `"a\qz"`；生产 Lexer 必须把每个 owner 分段为 Text / Error / Text，错误
  Span 精确覆盖两字节 `\q`。
- expression 入口解析含 4,096 个 owner 实参的 call；declaration 入口在变量 initializer 中解析
  相同 call；两者均保留 4,096 个实参。
- block 入口解析 4,096 个以 owner 初始化的局部 `val`；file 入口解析 4,096 个对应顶层 `val`。
- 四个源码分别产生精确 4,096 条 L0006，合计 16,384 条；每份诊断序列 primary Span 严格
  递增，不得出现 L0009、L0010、L0013 或其他 Parser 级联。
- 每个源码保留精确 4,096 个 `Expression::String`，每个 string 恰有一个 Error part；call
  argument、block local element 与 file variable root 数量不变。
- 4 个源码分别执行两次生产 Lexer 与对应 Parser，锁定连续完整覆盖、唯一 EOF、source-local
  AST / diagnostic Span、有效 typed root 与完整公开产物确定性。
- 不改变 Lexer recovery、Parser grammar、诊断目录、公开 API 或依赖；发现缺陷时只修复直接根因。

## 3. 非目标

- 不测试会终止 string owner 的换行 / EOF invalid escape；这些属于 terminal-owner 矩阵。
- 不替代单例精确 byte Span、64-case placement 或 diagnostic-anchor 负向矩阵。
- 不测试 L0001–L0005、L0007–L0008 的大规模传播、OOM 或 wall-clock 阈值。
- 不把 4,096 固定规模声明为语言输入上限。

## 4. 验收标准

- [x] 四入口分别保留 4,096 个 Text/Error/Text string owner 与外层 AST 数量。
- [x] 全部 16,384 条诊断均为源码有序 L0006，无 Parser 级联。
- [x] 四个源码的双 Lexer / 双 Parser 公开产物不变量与确定性验证通过。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 更新后的 owner stress target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

扩展 `parser_owner_stress_matrix`，复用 SPEC-0155 的四入口源码载体、双运行 helper、数量与诊断
顺序断言。新增 AST helper 只遍历公开 expression table，要求每个 string 的 parts 精确为
Text / Error / Text；不读取 `LexicalRecoveryIndex` 私有状态，也不增加生产测试钩子。

## 6. 实施计划

1. [x] 审计 L0006 大规模桥接覆盖 → 验证：现有证据止于单例与 64-case placement 矩阵。
2. [x] 增加四入口 L0006 owner-rich 压力 case → 验证：4 个源码、16,384 个 owner / L0006。
3. [x] 运行直接相关窄测试和窄 Clippy → 验证：更新 target 通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0156`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 四入口字符串 poison 压力矩阵、必要修复、Architecture 与完成记录 | `test(frontend): stress string poison propagation (SPEC-0156)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --test parser_owner_stress_matrix --locked --offline` 通过：3 passed，
  0 failed / ignored / measured / filtered。
- `cargo clippy -p lang-frontend --test parser_owner_stress_matrix --locked --offline -- -D warnings`
  通过：0 warnings。
- `cargo fmt --all -- --check` 通过。
- `cargo check --workspace --all-targets --locked --offline` 通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过：0 warnings。
- `cargo test --workspace --all-targets --locked --offline` 通过：462 passed，0 failed / ignored /
  measured / filtered；随后以同参数 `-- --list` 确认 462 个测试已被枚举。
- `cargo build -p lang-cli --locked --offline` 通过。

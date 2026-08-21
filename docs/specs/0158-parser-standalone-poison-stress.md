# SPEC-0158: 建立 Parser standalone lexical poison 压力矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-158` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0129、SPEC-0140–0143、SPEC-0150–0151、SPEC-0157 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四个公开 Parser 入口 standalone lexical-poison 压力测试、Architecture |
| 语言语义变更 | 否；只锁定大量 L0001 / L0002 / L0007 / L0008 的既有恢复行为 |

## 1. Goal

完成后，expression、declaration、block 与 file 四个公开 Parser 入口面对同一源码中的 4,096 个
L0001、L0002、L0007 或 L0008 standalone poison 时，均必须线性保留全部词法根因、对应
`Expression::Error` 与外层 AST 数量，不得吞诊断、级联 Parser 错误或产生内部错误。

## 2. 范围与需求

- 复用 SPEC-0157 的四类共享 poison：`#`、`async`、`'ab'`、`1e3`，分别对应 L0001、L0002、
  L0007、L0008；不得在新 target 复制第二份 spelling/code 表。
- 每类 poison 分别投放到四个入口：expression 的 4,096 个 call argument、declaration 变量
  initializer 中的相同 call、block 的 4,096 个局部 `val` initializer、file 的 4,096 个顶层
  `val` initializer，共形成 16 个压力源码。
- 每个源码精确产生 4,096 条同类 Lexer 诊断，primary Span 长度等于 poison byte length且起点
  严格递增；不得出现 L0009–L0078 Parser 诊断。
- 每个源码精确保留 4,096 个 `Expression::Error`，其 Span 与 poison 长度一致；call argument、
  block local element 与 file variable root 数量均保持 4,096。
- 16 个源码各运行两次生产 Lexer 与两次对应 Parser，共验证 32 个 Lexer 和 32 个 Parser 产物
  的连续覆盖、唯一 EOF、source-local Span、typed root 与完整公开产物确定性。
- 四类 poison 各覆盖 16,384 条诊断 / Error 节点，合计 65,536 条诊断和 65,536 个 Error 节点。
- 不修改 Lexer 规则、Parser grammar、诊断目录、公开 API 或依赖；发现缺陷时只修复直接根因。

## 3. 非目标

- 不重复 L0006 的 string-owner 压力，也不覆盖终止扫描的 L0003–L0005 / terminal L0006。
- 不把 4,096 固定规模声明为语言输入上限，不设置 wall-clock 或内存阈值。
- 不组合不同 poison、随机化位置或固定 Error 以外的完整 AST table 布局。
- 不新增 property-testing 依赖、生产测试钩子或公开诊断协议。

## 4. 验收标准

- [x] 16 个压力源码分别保留 4,096 条同类、有序且 byte-accurate 的 Lexer 诊断。
- [x] 全部 65,536 条诊断均为 L0001 / L0002 / L0007 / L0008，无 Parser 级联。
- [x] 16 个源码分别保留 4,096 个 Error expression 与外层 AST 数量。
- [x] 双 Lexer / 双 Parser 公开产物不变量、typed root 与确定性验证通过。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新压力 target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration target，复用 tests 私有 `LEXICAL_POISONS` 与 `parser_test_assertions`。源码
载体沿用已证明合法的 call argument、局部变量和顶层变量形状；AST helper 只遍历公开 expression
table 统计 Error 节点并核对 Span，不访问恢复 sidecar 私有状态，也不改生产实现。

## 6. 实施计划

1. [x] 审计 standalone poison 的大规模 Parser 证据 → 验证：现有证据止于单 poison mutation。
2. [x] 建立四 poison × 四入口压力矩阵 → 验证：16 个源码、65,536 条诊断 / Error 节点。
3. [x] 运行新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0158`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | standalone poison 四入口压力矩阵、必要修复、Architecture 与完成记录 | `test(frontend): stress standalone poison recovery (SPEC-0158)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 新 target 首次编译以 E0308 失败：测试把 typed `CallArgument` 直接当作 `ExpressionId`；改为
  读取公开 `CallArgument::value` 并同时核对 argument Span 后通过，未修改生产代码。
- `cargo test -p lang-frontend --test parser_standalone_poison_stress_matrix --locked --offline`
  通过：1 passed，0 failed / ignored / measured / filtered；16 个源码合计验证 65,536 条诊断和
  65,536 个 Error 节点。
- `cargo clippy -p lang-frontend --test parser_standalone_poison_stress_matrix --locked --offline -- -D warnings` 通过：0 warnings。
- 首次 workspace 基线在 `cargo fmt --all -- --check` 停止，仅报告新 helper 签名的 rustfmt 差异；
  执行 `cargo fmt --all` 后从第一项完整重跑，最终结果如下。
- `cargo fmt --all -- --check` 通过。
- `cargo check --workspace --all-targets --locked --offline` 通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过：0 warnings。
- `cargo test --workspace --all-targets --locked --offline` 通过：463 passed，0 failed / ignored /
  measured / filtered。
- `cargo build -p lang-cli --locked --offline` 通过。

# SPEC-0159: 锁定 Parser lexical-owner 递归预算边界

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-159` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0075、SPEC-0095、SPEC-0150、SPEC-0152–0155、SPEC-0158 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Parser 四入口递归预算矩阵、Architecture |
| 语言语义变更 | 否；只锁定深层 string/interpolation owner 的现有资源边界与 EOF 恢复行为 |

## 1. Goal

完成后，完整闭合与 EOF 未终止的嵌套 string/interpolation 必须在 expression、declaration、block
与 file 四个公开 Parser 入口具有精确相邻的最后接受 / 首个拒绝深度；接受侧保留全部 String AST
与唯一 Lexer 根因，拒绝侧稳定返回 `NestingLimitExceeded { limit: 1024 }`，不得发生栈溢出或
边界漂移。

## 2. 范围与需求

- 构造 `"${` 重复 N 层、最内层为 `x` 的 owner；closed 形状追加 N 组 `}"`，terminal 形状在
  `x` 后直接 EOF，由生产 Lexer 只报告最内层一个 L0005。
- expression、declaration 与 file 入口分别接受 511 层、拒绝 512 层；block 入口因外层 block
  自身占用一级预算，接受 510 层、拒绝 511 层。
- 每个入口分别验证 closed / terminal 两种形状和接受 / 拒绝两侧，共新增 16 个边界源码；每份
  源码执行两次生产 Lexer 与两次对应 Parser。
- closed 接受侧必须零诊断；terminal 接受侧必须恰有一个 L0005，其 primary Span 从最内层
  interpolation start 到 EOF，且不得级联 closing / block / file Parser 诊断。
- 八个接受源码的 AST 必须保留与输入深度相同的 `Expression::String` 数量及有效 typed root。
- 八个拒绝源码必须双运行得到相同 `NestingLimitExceeded { limit: 1024 }`；terminal 拒绝源码的
  Lexer 仍必须精确保留唯一 L0005。
- 与 SPEC-0152 既有 18 个边界源码合并后，target 共覆盖 34 个源码、68 个 Lexer 产物和 68 个
  Parser 成功产物或内部错误结果。
- 不改变预算值、Lexer owner 规则、Parser grammar、诊断目录、公开 API 或依赖；发现 off-by-one
  或恢复缺陷时只修复直接根因。

## 3. 非目标

- 不把 Parser 预算声明为语言语义或用户可配置项，也不改变 `NestingLimitExceeded` 的分类。
- 不重复平坦 owner / poison 压力、所有递归语法形状或小调用者栈矩阵。
- 不测试 OOM、wall-clock 阈值、运行时表达式求值或类型检查。
- 不覆盖多段 StringText、多个 sibling interpolation 或混合 operator 的组合深度。

## 4. 验收标准

- [x] closed 与 terminal owner 在四入口分别锁定最后接受 / 首个拒绝深度。
- [x] 八个接受源码保留精确 String 数量；closed 零诊断，terminal 仅有 byte-accurate L0005。
- [x] 八个拒绝源码稳定返回 limit 1024 的 `NestingLimitExceeded`，无栈溢出。
- [x] 更新后的 target 共验证 34 个源码的双 Lexer / 双 Parser 结果与公开产物不变量。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 更新 target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

扩展现有 `parser_recursion_boundary_matrix`，新增 tests 私有 owner shape / entry 枚举和源码 builder。
接受侧复用四个公开入口的双运行 helper 并遍历公开 expression table；拒绝侧复用现有
`assert_parser_error_twice`，不新增生产测试钩子，也不把内部 binding-power 数值复制到测试。

## 6. 实施计划

1. [x] 审计 owner terminal 恢复与递归预算证据 → 验证：浅 placement 与非 owner 边界已覆盖，
   二者交叉缺失。
2. [x] 增加 closed / terminal owner 四入口相邻边界 → 验证：16 个新增源码。
3. [x] 运行更新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0159`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | lexical-owner 递归边界、必要修复、Architecture 与完成记录 | `test(frontend): lock lexical owner recursion boundaries (SPEC-0159)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 新 case 首轮窄测试失败于 declaration closed 载体：`value` 是 Koven 关键字，先产生 L0018；
  将测试变量名改为普通标识符 `result` 后，定向 case 通过，未修改生产代码。
- 窄 Clippy 首轮以 `too_many_arguments` 拒绝 8 参数测试 helper；改为在 helper 内通过
  `SourceMap` 读取源码长度后，接口收敛为 7 参数。
- `cargo test -p lang-frontend --test parser_recursion_boundary_matrix --locked --offline` 最终通过：
  3 passed，0 failed / ignored / measured / filtered；target 共验证 34 个相邻边界源码。
- `cargo clippy -p lang-frontend --test parser_recursion_boundary_matrix --locked --offline -- -D warnings`
  最终通过：0 warnings。
- 首次 workspace 基线在 `cargo fmt --all -- --check` 停止，仅报告新 helper 签名的 rustfmt 差异；
  执行 `cargo fmt --all` 后从第一项完整重跑，最终结果如下。
- `cargo fmt --all -- --check` 通过。
- `cargo check --workspace --all-targets --locked --offline` 通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过：0 warnings。
- `cargo test --workspace --all-targets --locked --offline` 通过：464 passed，0 failed / ignored /
  measured / filtered。
- `cargo build -p lang-cli --locked --offline` 通过。

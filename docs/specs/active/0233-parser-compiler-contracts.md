# SPEC-0233: Parser 工程合同的保全文档迁移

> **性质**：文档治理变更合同 · **状态**：in-progress · **读取时机**：实施或验收 Guide 与 Parser 工程合同的首片分离时 · **唯一真源**：本 Spec 的范围与验收记录

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-DOC-0233` |
| 所属 Phase | 文档治理；不改变任何 Phase |
| 语言规范 | 已启用 [v0.38 Guide](../../guide/README.md) |
| 批准依据 | 2026-10-01 用户要求继续完成演进计划中未完成任务，分阶段实施、验证和提交；计划明确渐进分离 Language Reference 与 compiler contracts |
| 前置 Spec | 无 |
| 前置 ADR | 无 |
| 阻塞项 | 无；不包含语义选择 |
| 影响范围 | Guide 05/06/07、文档路由、`docs/compiler-specs/`、文档结构检查及测试 |
| 语言语义变更 | 否 |

## 1. Goal

为 Parser 内部表示与算法建立可直接查阅的 current 工程合同入口，以逐块原文保全的方式从
Guide 迁出边界明确的九段工程文字；现行语言语义、诊断、源码范围与强制 Phase 权威不变。

## 2. 范围与边界

- 新入口 [Compiler Contracts](../../compiler-specs/README.md) 导航 AST 与算法/资源两页。
- Guide 05 的函数封闭状态、索引 root；06 的 statement table/body；07 的 lambda payload
  字段与 header/body 不变量迁入 AST 合同。
- Guide 07 的 header DFA、共享预索引；05/06 的独立恢复/dispatch 资源约束迁入算法合同。
- 逐段保留正文与空白，只机械更新相对 Guide 链接；最小转交链接取代旧正文，不留重复真源。
- current marker、metadata、递归索引覆盖、四跳可达性、五份必读上限与页面预算纳入检查。
  新合同入口 160 行、合同页 200 行；原有预算及检查保持不变。

## 3. 非目标与保留段落

- 不更改 Rust、grammar、优先级、诊断类别/恢复语义、Span 表、语义示例、Phase 或实现授权。
- Guide 03 strict TypeRef/call trial 决定 typed-call 的接受和回退，不能整体视为实现算法，保留。
- Guide 05 的返回形式诊断、名称 marker/Span 与参数模式仍相互关联，保留；07 的调用参数
  marker/匹配、tail-lambda/newline 规则和 header 失败后的诊断同样保留。
- Guide 15 的 Span/函数模式恢复与 Phase、litmus 不迁移；不把剩余混合段落包装为当前实现事实。
- 不决定 borrow 调用、shift、deinit、Str 等待明确的后续规则，也不顺带修订混合历史表述。
- 不追求一次搬空全部 Guide 工程文字。迁移只改变位置，不将 proposal、计划或代码现状升级为语义。

## 4. 验收与实施

- [x] 两份 current 合同具有清晰权限与唯一入口；Guide 保留最小转交链接。
- [x] 九段来源与目标逐块登记；目标还原相对链接后与基线逐字相等，全部原文只迁一次。
- [x] 检查器覆盖新目录，新增测试先失败；原有检查与预算不降低。
- [x] 文档门禁、完整检查器单元测试、迁移核对与 diff whitespace 检查通过并登记结果。
- [ ] 独立提交、PR CI 通过后按仓库生命周期归档；由负责交付的主任务执行。

原文保全证据见 [迁移账本](../../archive/migrations/v0.38-parser-compiler-contracts.md)。
Architecture 只补充语义/合同入口导航，现有实现事实没有改变，因而不新增代码完成声明。

## 5. 提交计划

| 顺序 | 边界 | 建议提交信息 |
|---|---|---|
| 1 | 九段迁移、单一入口、治理回归与保全账本 | `docs(parser): separate compiler contracts from guide (SPEC-0233)` |
| 2 | PR CI 与最终验收登记、归档及 inventory/DAG 同步 | `docs: archive verified parser contract migration (SPEC-0233)` |

## 6. 验证记录

| 验收项 / 命令 | 结果 | 限制 |
|---|---|---|
| Red：新增八项测试后，`python3 -m unittest discover -s scripts/tests -p 'test_check_docs.py'` | 29 项中 21 passed、6 failed、2 error | 缺失新目录治理，错误为 current marker 检查尚不存在 |
| Red：追加 nested route、五份正例与链接测试后，同一命令 | 32 项中 31 passed、1 failed | 新发现子目录合同页的默认路由尚未纳入；修正为递归检查 |
| Green：同一命令 | 32 passed、0 failed/error/skipped | 原有 21 项与新增 11 项全部通过；不涉及 Cargo |
| `python3 scripts/check_docs.py` | 通过；390 Markdown 文件 | 不将结构检查表述为语义等价证明 |
| 九段基线/目标机械比较与 SHA-256 | 9/9 通过，共 4,667 UTF-8 字节；目标各出现一次，Guide 无原段重复 | 原文仅两处相对链接机械更新 |
| Guide 保留正文对照 | 移除九段/转交链接后，05/06/07 保留内容经空白规范化相等；其余 12 份领域页字节相等 | README 只增加导航 |
| 独立只读迁移审阅 | 无阻断；补充算法路由的 Guide06 入口后再次运行门禁 | 审阅同样核对九段原文及诊断/Span/Phase 保留 |
| `git diff --check` | 通过，退出 0 | 不执行 git 写操作 |
| Cargo / native / frontend 测试 | 未运行 | 本次没有 Rust 或语言行为变更，按文档验收规则不适用 |
| PR CI / push | 未执行 | 等待发布授权；本 Spec 保持 in-progress |

# Unit wrapper来源交付观察

> **性质**：实现事实快照 · **状态**：current · **读取时机**：检查wrapper live-source交付与因果引用时 · **唯一真源**：本页；普通借用规则见[Guide](../guide/10-ownership-borrowing-drop.md)

## 实际来源与范围

真实PendingLoan第三acquisition句柄随PendingForwardTransport交到实际wrapper finish；
BorrowForwardHandoff保留原transaction，独立validator核同catalog、先后、完整Begin token、
source/file及真实入口/完成scope后，同时公开source_begin与wrapper callable。scope仅一次
建立于顶层单Shared参数、raw Name callee加单raw Name实参的直接表达式体；不在finish补推。
原returned/source Eq与retain不变，metadata合流仅保唯一同句柄/scope，失证不回填。
旧forward全家族均从实际handoff记录无条件保序投影，observer-off也保留；显式Return、
Group或控制邻域仍可有原raw事实但无本片因果证据。handoff不是End，不进入cleanup执行。
固定单wrapper双布局true on/off的原SSA/LLVM与新引用已通过：wrapper仍仅BorrowCall加
BorrowReturn，caller原八条不变。source/callable仍Partial，无新Complete、receipt或跨函数LoanId。

## 证据与边界

来源与正常完成的实际移动都沿原planner；窄观察准入失败不会新增语言拒绝或过滤旧事实。
显式Block wrapper仍在正常Unit lower的既有声明gate拒绝；普通FE事实不等于SSA准入。
核心证明只关联同catalog的静态记录，不证明动态唯一消费、全部控制边、权限恢复或absence。

实现入口为frontend `drop_planner/borrow_forward_transport.rs`、`action_source/forward_links.rs`，
公开查询在 `loan_actions.rs`；原样与真off核心证据见[SPEC-0341](../specs/active/0341-shared-lowering-core.md#单表达式体-wrapper-的-live-source-partial-因果合同2026-11-11)。
其他动作家族与source整体边界见[Unit loan动作源](unit-loan-action-source.md)。

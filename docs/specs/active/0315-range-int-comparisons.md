# SPEC-0315: 范围绑定函数内剩余 Int 比较

> **性质**：有界变更合同 · **状态**：in-progress · **读取时机**：实施范围绑定与 Int 比较组合时 · **唯一真源**：本页合同与验收；语言语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0315` |
| 所属 Phase | Phase 4 typed SSA 能力门与 native 验收 |
| 语言规范 | [Guide v0.43](../../guide/README.md)、[表达式](../../guide/04-expressions-operators.md) |
| 批准依据 | 用户持续主线实施及小阶段提交授权；合并仍需具体批准 |
| 前置 Spec | [SPEC-0298](../../archive/specs/0298-range-binding-local-control-flow.md) |
| 基线 | 官方 fetch 确认 actual main `934b65908a7e8a42b3b7d969130aa2c95b191703`；独立 `feature/spec-0315`，不复制未合入0314 |
| 影响范围 | `borrow_result_support::binding` 的 typed 比较白名单及普通源码测试 |
| 语言语义变更 | 否 |

## 1. 冻结合同

仅为已发布 NewRangeDescriptor 绑定的完整函数开放 Int `!= <= > >=`，与既有 `== <`
保持同一合同：两个 operand 均由 typed facts 证明为 Int，表达式结果单独证明为 Boolean。
比较不新增 CFG，不根据语法放行全部 Binary，不重建 loan/来源/最后使用事实。
现有 statement-if 的双正常 arm、同 root 词法 scope 及显式运输原样复用。

不扩短路 `&& ||`、非 Int 比较、String concat、普通 borrow/metadata alias 全函数门、
不同 root、隐藏退出、分支新 binding、when/loop/value-if。Guide、frontend、ABI、verifier、
Resource 合同、LLVM 和 M13 文档陈旧问题均不在本片。

## 2. 验收账本

| 验收项 | 目标/过滤器 | 实际结果 |
|---|---|---|
| 原拒绝复现 | codegen lib 精确 `ssa::range_lowering_tests::local_control::boundary::range_local_control_keeps_unselected_binary_gates` | main源码实际1/1通过；8表达式×3入口，共24次精确UnsupportedNode/Span；新四比较12次拒绝；前后源码hash一致 |
| 新四比较红测 | codegen lib `ssa::range_lowering_tests::local_control::comparisons::` | 实际72/72红；均frontend成功后在比较精确Span以UnsupportedNode拒绝；四op×三位置×literal/constant与single/unit正逆序，0 ignored；生产未改、源码冻结一致 |
| 新四比较真实执行 | 同上 | 待实现；每项负极值/负数/相等/正数/正极值五次输出，SSA核opcode/Int/Boolean/Span与RangeEnd→BorrowEnd→consume |
| statement-if 与同root父子/兄弟 | 普通源码新比较选择矩阵 | 待补；两arm及合流再读，String/MoveOnly/Resource真实内容与析构输出 |
| 旧能力与拒绝 | `ssa::range_lowering_tests::local_control::` 中静审过的普通源码目标 | 待运行；旧==/<18项保持，短路/非Int与其他能力门保持 |
| 静态门禁 | fmt、codegen all-targets严格clippy、docs/diff与尺寸 | 待运行 |
| 远端交付 | Draft PR与双宿主CI、归档、具体合并授权 | 未运行；阶段commit/push不表示完成 |

single 的 literal 与 const fixture 使用既有同一 constant-aware 分析/lowering入口；unit basic
与 validated constant unit 使用各自入口，不把basic成功冒充constant证据。每项通过真实
frontend、SSA verifier、原版LLVM和native；不造坏IR、不插桩或运行聚合discover。
日志、源码冻结与恢复材料留仓外。Rust1.96.0/LLVM21.1.8；baseline/current独立绝对target，
Cargo串行，不clean。Linux本地不替代macOS CI；未运行项不会标绿。

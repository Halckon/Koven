# SPEC-0333: 静态 Map 返回调用的双入口闭合

> **性质**：后继实施合同 · **状态**：approved · **读取时机**：修复已保留的普通静态调用缺口时 · **唯一真源**：本片范围与验收；语义以现行 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | approved |
| Goal ID | `KOV-P4-0333` |
| 批准依据 | 用户要求将0331范围审计确认的独立旧调用缺口登记为明确后继，不扩大F或转交D10 |
| 基线 | main `a2a7e4b6833977fcae26d6420f193ea1ac7d53df`、备份 `60512fda0677b07abb77f10d88242f7a16edd1fb`、验收头 `91e8be08187413afa4c5af9bcb62cff89c70f753` |
| 前置 Spec | [0331 Map provider](0331-map-borrowed-iteration.md)的已交付事实；后续从实际合并main另建 `feature/spec-0333-static-map-callers`，当前未实施 |
| 合同 | [Guide08成员/companion](../../guide/08-class-family-members.md)、[Guide10借用](../../guide/10-ownership-borrowing-drop.md)；不新增语义 |
| 影响范围 | 单文件调用选择/receiver与owned结果、Unit companion可达模板、完整调用/Drop及provider组合回归 |

## 1. 有界归属与原输入

保留[原12源生成器](../../../crates/lang-codegen/src/ssa/map_provider_exits_pipeline_tests/local_reassignment/ordinary_routes.rs)的
`ordinary_route_all_sources`与`ordinary_route_all_controls`；member、companion、override、Borrow-only静态委托
各有noFor、previous、future三份原源。previous在For前以顶层make()替换成员结果，future在For后才调用；
实际被遍历owner均来自make()。原字节与golden保持，不删companion或仅收Unit成功子集。

三快照各24观察、共72观察使用独立Cargo target；每头3条noFor Unit严格native，共9次。
无For的3条Single Call Unsupported、Single companion L0165、Unit companion MissingFact在三头逐字一致；
previous companion的complete Internal也在基线存在。本有界集合未见成功→失败回归，不推论全部语言无回归。
固定91e8观察为9个入口成功、15个入口失败；collector的3次Rust退出0只表示全部观察收齐，不算24全过。

## 2. 具体修复义务

- Single普通class成员及concrete override/Borrow静态委托的实际callee、receiver、owned Map结果交付；
  现value-class Int/Boolean窄准入不足，不以返回类型/方法文本建立特例。
- Single companion的关联函数选择与原诊断事实链；有效源码不得留下MissingRequiredFact Internal。
- Unit把已正确解析的companion Symbol纳入真实可达模板，保持无receiver命名空间合同；
  generic companion不得隐式捕获enclosing类型参数。
- provider出生本身的额外门属于0331，不能转交本片：已支持普通Call形成的当前named owner及temporary
  要保留实际birth/current epoch/全incoming和最终Drop；修共享调用后重验原组合，不用noFor失败豁免For证明。

## 3. 成功标准与非目标

- [ ] 先把原12源×双入口24项恢复为真实成功目标，保存现有15失败的阶段/位置及源码身份。
- [ ] 同分析FE选中callee/参数槽位与mode/receiver/CallReturn清理，SSA完整owner/loan/入边/全部受保护使用可追溯。
- [ ] 24项完整SSA/LLVM及普通native逐项exit0、完整golden、空stderr；再核current-member出生与退出组合。
- [ ] 无owner复活、重复求值、重复Drop；含Resource marker与原参数求值顺序，不以输出替代静态身份。
- [ ] 独立审查、工程门和精确新头双宿主CI；故障注入/校准/合成损坏/sanitizer对照只在CI。

不扩动态interface value、一般Inout/Field native、nullable String ABI、条件借用、Guide或D10。
这是合法源码的既有实现债，不是Guide staging；本登记不表示修复或自动解除0331的其他阻断。

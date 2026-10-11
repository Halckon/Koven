# M2B 首片设计冻结：选择返回访问 mode + 唯一来源

> **性质**：ADR/Spec 前置设计冻结输入 · **状态**：selected / semantics not enabled · **读取时机**：起草 M2B ADR、正式 Spec 与红测时 · **规范性**：本页不启用任何新语法、Guide 或运行时 ABI

## 1. 决定

M2B 首片选择候选 A：**返回访问 mode + 唯一声明来源**。

首片只设计和实现 **Shared borrow result**。声明必须携带唯一来源，来源只能是 receiver 或一个具名 formal parameter；body 的每条正常返回都必须证明结果派生自该来源的当前实例及其 selector 链。

候选 B 的 `Access<T>` / `LookupAccess<T>` 暂不采用。B 并不能消除 provenance、parent continuation、CallReturn handoff 或 SSA/verifier 的新增工作，同时还要求引入“访问能力不可存储”这一独立类型分类、generic containment 检查、handle migration/reborrow 与 target refinement。首片没有足够收益去承担这部分额外表面和类型系统侵入面。

此选择不批准文档中的候选拼写；语法仍由后续 Spec 冻结并经 Guide 启用。

## 2. 为什么 A 的实际侵入面更小

现有实现已经有可复用的三层基础：

- frontend ownership 可表示 place、字段/元素 selector、Shared/Exclusive call loan、overlap 与非 owning 参数限制；
- Copyable 的普通 Value delivery 会按现有 ownership 规则退化为 Read，而 MoveOnly 的 borrowed move 会被拒绝；
- SSA 已有 `EntityType::Loan`、loan id、field/element reborrow、parent/child 依赖与“活跃 child 时不能结束 parent”的 verifier 约束。

但现有能力只覆盖**调用期 loan**。函数返回仍是普通 `SsaTypeId` / `ValueId`，DirectCall/Return 没有借用结果 provenance；CallReturn 会结束本帧建立的 call loan。因而 A/B 都必须新增：

1. 声明级 result contract：result kind、target type、permission、唯一 source identity、存在协议；
2. body producer proof：每条正常返回的 root/current-instance/selector/parent chain 证明；
3. caller substitution：formal source 到实际 owner 或已有 parent access 的映射；
4. CallReturn 原子 handoff：在结束 callee 输入视图前，把 result child 重接到 caller 持有的 protector/parent；
5. SSA result-loan producer/consumer facts 与 verifier 规则；
6. CFG/域退出的 child → protector/parent → owner 结束顺序。

A 只需要在现有 callable/local/return delivery 上增加“access category + provenance”。B 在以上共同成本之外还要新增一种不能按普通 owned type 处理的能力类型系统，因此不进入首片。

## 3. 首片冻结边界

### 3.1 结果权限

首片只有 **Shared** result。

不返回 arbitrary Exclusive capability，不把只读查询升级权限，不允许结果绕过现有 receiver/parameter mode。

### 3.2 允许的来源

首片允许：

- caller 具名 owner 通过 Borrow receiver/parameter 作为来源；
- 已有有效 parent access 通过 Borrow receiver/parameter 继续投影并受控转发。

首片拒绝：

- Own/Value receiver 或参数；
- callee local；
- temporary receiver；
- captured source；
- 多来源选择；
- function-value / dynamic dispatch 下无法由静态声明确定来源的返回。

### 3.3 Inout

**首片不支持 Inout 作为返回访问来源。**

理由不是语义上永远禁止，而是当前模型没有可验证的 continuation protector / parent handoff 原语。现有 Inout call 的 Exclusive loan 在 CallReturn 结束；若此时遗留 Shared child，就会出现“父权限已经恢复但 child 仍存活”的不可验证状态。

在以下事实进入 frontend + SSA/verifier 之前，不得把 Inout 负例改为正例：

- 原子 parent handoff / protector 创建；
- child 存活期间父 Exclusive 冲突权限保持挂起；
- 所有 child 结束后唯一一次恢复；
- 提前 return / break / continue / 可恢复错误上的逐边恢复；
- forged handoff / early restore 的 verifier 拒绝。

后续可以单独做 Inout extension ADR/Spec。

## 4. Missing 与 nullable 的冻结

查询型借用结果采用**独立存在协议**，概念上是：

- `Missing`
- `Found(target)`

其中 `target` 仍是 borrow result，不是 ordinary owned value。

必须区分：

- `Missing`：没有 element/field target loan，不可读取 target；
- `Found(null)`：条目/槽存在，目标是一个存在的 nullable slot；
- `Found(value)`：槽存在，non-null refinement 仍依赖同一 slot/root。

普通 `T?` 继续只表示 nullable owned value，不承担 Missing 语义；不得用 `T??`、null pointer 或 ordinary enum 构造伪造 borrow existence。

首片对 Missing 采用保守策略：结果域仍保持 root protector；Missing 分支不生成 element loan。以后若能证明 Missing 分支可提前释放 protector，可作为优化/扩展，不能作为首片语义前提。

## 5. 局部绑定冻结

首片要求**显式 access binding**，概念拼写沿用 proposal 的 `borrow val`：

```text
borrow val selected = first(names)
inspect(selected)
inspect(selected)
```

关键合同：

- ordinary `val` 不自动推导成 borrow binding；
- access binding 不拥有目标，不运行目标 deinit；
- 多个只读 alias 通过显式 Shared child/reborrow 产生，各自有结束义务；
- binding 的上界先采用最近结构化词法域，并受 source/parent 更短上界限制；
- 不使用 last-use/NLL 自动缩短；
- 不可存入普通 field/container/Box/Rc/Any，不可逃逸 closure/thread/async；
- 受控 wrapper return 只能通过带唯一来源合同的函数返回。

后续 Spec 可以调整最终关键字，但不能取消“局部借用与 ordinary owned local 在语义上显式区分”这一冻结。

## 6. owned copy 的明确触发

首片不允许“同一个普通 `val` 根据 T 是否 Copyable 自动在 borrow/copy 间切换”。

冻结为：**只有显式 owned delivery context 才能从 Shared access 产生独立 copy，并且目标 T 必须是 Copyable。**

Spec 起草时必须给这个动作一个可见、不可歧义的源码触发；优先使用专门的 `copy` 表达式/操作，而不是依赖类型推断偷偷复制，例如概念上：

```text
borrow val selected = first(ints)
val snapshot: Int = copy(selected)
```

要求：

- `copy(access)` 生成现有 Copy/Read ownership fact，并切断结果与 source 的生命周期依赖；
- 对 String/Resource/其他 MoveOnly 目标拒绝；
- 不引入 clone/retain；
- ordinary owned API 若本来就返回 Copyable 值，仍按现有 Value delivery，不经过 borrow-result 协议；
- 在最终语法进入 Guide 前，`copy` 只是 Spec 待绑定的操作名，不是已启用语法。

这比 `val x: Int = selected` 更适合作为首片，因为红测可以直接区分“access reborrow”和“owned snapshot”，也避免未来类型推断变化改变所有权语义。

## 7. selector 与嵌套来源

首片来源是“唯一 root + 有序 selector 链”，不是字符串、hash、槽地址或 API 名。

实现至少要能表达并核验：

- field；
- List element；
- field → element → field 等有序组合；
- query 结果的逻辑 entry selector（Map 自身仍由后续 Map Spec 批准）。

当前 single-file place 的字段/索引组合与 compilation-unit terminal element 表示不足以覆盖 Map → List → field，因此 M2B 实现 Spec 必须先定义统一 selector path；不能用 wrapper 或 runtime pointer 绕过。

所有 key/index operand 只求值一次；wrapper return 不得重新执行 selector 来“重建” provenance。

## 8. CallReturn handoff 的最小可批准模型

Borrow 来源的首片只允许以下两种 caller 侧 continuation：

1. **具名 owner source**：caller 建立 result-domain protector，callee result child 在 CallReturn 前原子重接到该 protector；
2. **已有 parent access source**：result child 重接到 caller 已存在的 parent，且期限不超过 parent。

必须保证：

- 先验证 producer/source/current-instance，再交接；
- 交接成功后才结束 callee 的临时输入视图；
- 不相关 key/argument call loans 正常结束；
- 不允许输出 child 继续指向即将结束的 callee loan；
- return operand Abort 时不生成 result/handoff；
- structured exit 按 child → protector/parent → owner 清理；
- verifier 能拒绝 missing parent、错误 source、wrong current instance、early parent end、重复 end 和 forged result。

在这些事实存在前，任何只在 frontend 记录 pointer、或只延长 `LoanFact.end_span` 的实现都不算完成。

## 9. ADR/Spec 拆分建议

### ADR：M2B borrow-result ABI 与 continuation

必须决定：

- result-loan SSA 表示；
- DirectCall / Return 如何携带 result provenance；
- caller protector / parent handoff 的原子模型；
- tagged Missing/Found 在 SSA/LLVM 层的表示边界；
- 模块序列化与 native/external producer 的信任边界；
- verifier 的 producer/consumer/cleanup 不变量。

ADR 不得顺带开放 Inout。

### Spec 1：M2B Shared borrow result core

先只覆盖具名 Borrow owner / Borrow parent：

- field；
- List element；
- nested selector；
- explicit borrow local；
- static named wrapper return；
- explicit owned copy；
- CFG structured exit；
- forged SSA facts。

### Spec 2：lookup existence protocol

在核心稳定后接：

- Missing；
- Found(nullable slot)；
- Found(non-null refinement)；
- query key/index single evaluation；
- Missing 不生成 element loan；
- root protector 与 mutation/relocation conflict。

Map 类型/API/entry identity 仍由 Map Spec 独立批准；该 Spec 只提供通用 existence/result 机制。

### 后续扩展

- Inout source；
- compatible selector join 的更精细证明；
- short-lived non-escaping closure capture；
- function value / dynamic dispatch；
- multi-source result；
- long-lived stored borrow。

## 10. 首批红测必须先失败

正式实现前至少建立以下失败 oracle：

1. 从 Borrow owner 返回 Shared field access，frontend 尚无 result contract；
2. caller 绑定返回 access，SSA 尚无 result-loan return；
3. wrapper 从声明 source 之外的参数返回，必须拒绝；
4. callee local / temporary / Own 参数作为来源拒绝；
5. Inout 返回 Shared result 明确拒绝；
6. access 存入 ordinary field/container/Any 拒绝；
7. MoveOnly access 走 owned copy 拒绝；
8. Copyable access 未显式 copy 却进入 owned local 拒绝；
9. Missing 分支读取 target 拒绝；
10. Found(null) 与 Missing 被合并拒绝；
11. parent 在 child 前结束的 forged SSA 拒绝；
12. result source/current-instance 被替换的 forged SSA 拒绝；
13. key/index 被 wrapper return 二次求值的行为测试失败；
14. nested field/list/field selector provenance 丢失时拒绝；
15. structured early return 未按 child→parent 清理时 verifier 拒绝。

红测通过前，不修改 Guide 宣称能力可用。

## 11. 完成定义

本设计冻结完成后，M2B 可以进入 ADR/Spec 的条件是：

- A 已选，B 移出首片；
- Missing、局部 binding、owned copy、Inout 边界均有唯一答案；
- frontend/SSA/CallReturn 的复用点与缺口被明确；
- 没有把现有 call-scoped loan 当作 borrow-return 已实现；
- 所有未被现有 verifier 证明的语义保持拒绝；
- 后续红测能直接从本页导出正反 oracle。

此页本身不改变现行语言版本，也不构成任何新语法、Map API 或 ABI 的批准。

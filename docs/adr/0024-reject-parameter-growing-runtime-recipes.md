# ADR-0024：确定性拒绝参数增长型 runtime recipe

## 状态

accepted

## 接受依据

2026-09-01 持续 Goal 已授权继续按现行 guide 与 Phase 依赖推进 Specs 并简化验收。接受前审计
确认本 ADR 只选择 codegen 对无限 concrete specialization 的实现策略，不改变 v0.34 的类型、
所有权、receiver 静态分发语义或公开 ABI。

## 背景

dependent inherited owner recipe 可能把类型参数嵌入下一层同一 nominal declaration，例如
`Grow<T>` 的字段 recipe 指向 `Grow<List<T>>`。若 planner 逐次实例化 concrete argument，
`Grow<Int> → Grow<List<Int>> → Grow<List<List<Int>>> → …` 永远不会重复完整 `UnitTypeId`，因此
按 concrete node 查重或提高递归上限都不能证明终止。

SPEC-0224 已允许非增长的单参数 ordinary-class recipe，并要求 runtime layout 只能消费 exact
frontend descriptor。现行 guide 没有要求参数增长型 recipe 必须具有某种 erased ABI，也没有
授权共享泛型 glue、运行时类型描述符或 GC；需要先固定一个不会让 LLVM 类型构造发散的长期边界。

## 决策

- v0.34 codegen 对参数增长型 runtime recipe 采用确定性拒绝，不生成 type-erased layout、共享
  generic glue 或无限 concrete specialization。
- planner 在展开 concrete type graph 前按 nominal declaration path 检查 recipe。每个 root 先按
  稳定 source identity（root identity、logical path、起始 byte offset）排序；root 内按 field/source
  顺序深度优先，constructor type arguments 从左到右、enum cases/payloads 按源码顺序遍历。第一次遇到
  指向当前 declaration stack 的边就是唯一 witness，以该 field/payload 的 source `Span` 返回
  `UnsupportedNode`。source input 数组顺序不得进入排序键。
- 检查只遍历源码中有限的 declaration/template edges，不为变化后的 concrete arguments 创建新节点；
  当前 path 最多包含可达 nominal declaration 数量。再次进入已访问 declaration 时无论实际参数是否
  相同都立即停止，因此 termination measure 是“尚未进入当前 path 的有限 declaration 数量”，不依赖
  递归深度、实例数量、内存预算或遍历超时。
- 同一规则覆盖 fixed-argument self/mutual SCC 与无重复 concrete node 的参数增长链。它只约束本
  codegen recipe 能力，不把一般源码 nominal 声明判为类型错误，也不改变 frontend facts。
- 非循环的有限 recipe 仍按 SPEC-0224 分类为 `InstanceKeyOnly` 或 `RuntimeLayoutRequired`；后者继续
  强制消费 exact SPEC-0219 descriptor。缺 descriptor、target layout overflow 与 unsupported
  constructor 保持各自既有失败边界，不降级成 growth rejection。
- 检测必须基于有序、有限的 declaration/template graph，并在 LLVM type、drop glue 或 object 生成前
  完成。未来若要开放这类 recipe，必须由新 ADR 取代本记录并同时定义 ABI、终止证明与迁移范围。

## 替代方案

### 按 concrete `UnitTypeId` 建有限图并查找 SCC

拒绝。参数增长链可以永不重复 concrete node，构图本身已经不终止；它只能处理普通递归，不能解决
本决策的核心问题。

### 提高递归深度或实例数量上限

拒绝。任意阈值会把资源策略伪装成语言支持边界，并让结果受实现常量、调用顺序或输入规模影响。

### type erasure 与运行时类型描述符

暂不采用。它会新增 erased value ABI、动态 layout/drop 元数据和间接访问，超出现行静态单态化与
无 RTTI 边界，也没有 guide 授权。

### compilation-unit 内共享泛型 layout/drop glue

暂不采用。单一 LLVM module 内不要求公开跨 crate ABI，但仍需定义内部 metadata/dictionary 的传递
契约、动态字段寻址与 drop metadata，增加运行时间接层，并使优化、DWARF 调试和 verifier 不变量更
复杂；在没有第二个已批准使用场景前成本高于确定性拒绝。

### 只允许 pointer/owner 间接递归

不作为本轮例外。ordinary class 的间接存储可使单个 layout 有限，却不能阻止 inherited owner
specialization key 持续增长；在共享 specialization ABI 未定义前仍无法证明完整 codegen 有限。

## 后果

收益：

- planner 对无限增长链在 LLVM 前、与资源上限无关地终止；
- 保持现有 concrete ABI、exact descriptor、drop glue 与五 crate 边界；
- SPEC-0225 可用窄负矩阵完成，不需要虚构不在 guide 中的 runtime 能力。

代价与风险：

- 一部分 frontend 可类型化、且单个 class layout 通过指针间接有限的程序仍不能 native lowering；
- declaration-path 检查有意比 concrete-node SCC 更保守，参数改变后最终可能收敛的特殊图也会拒绝；
- 若未来需要开放，必须新增 ABI 决策、替换本 ADR，并重做 planner/SSA/LLVM/drop/native 验收。

## 关联

- 相关 Spec：[SPEC-0224](../specs/0224-dependent-inherited-owner-recipes.md)、
  [SPEC-0225](../specs/0225-parameter-growing-runtime-type-cycles.md)
- 相关 ADR：[ADR-0008](./0008-internal-value-and-allocation-abi.md)、
  [ADR-0016](./0016-interprocedural-borrow-abi.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无

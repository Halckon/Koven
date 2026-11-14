# Unit 函数根集合候选：保留 whole-file 身份见证

> **性质**：非规范工程合同候选 · **状态**：未启用；planner 隔离原型已通过两项普通测试 · **唯一真源**：本候选；现行规则仍以 Guide/accepted ADR 为准 · **基线**：`3ce91326` · **读取时机**：评审 D10 旧 whole-file 测试退出路径时

## 问题与范围

此处 root 是函数声明，不是 owner/借用来源或文件系统 root。不涉及 Origin-based
裁决，也不新增语言语法、CLI 多 entry、导出 ABI 或自动全文件根选择。
现有 [退出审计](../reviews/2026-11-14-d10-legacy-consumer-audit.md) 中两项
`lower_frontend_tests.rs` 见证要求多个函数根存在于同一 Program；现有逐根 Unit
对照不能验证跨根共享 generic 实例和同名 overload 的 ID/LLVM 符号互不冲突。

已核实实现：`unit_plan.rs::plan_unit_instances_from_facts` 以单个 DeclarationId
初始化 pending；`unit_lower/orchestrate.rs` 仅调用一次 planner，并从同一
function_ids 表返回一个 entry_id。计划、类型需求、callable ABI、函数发射和最后
verifier 原本已共享，因此不应拼接多个独立 Program 或重映射它们的 ID。

## 推荐的最小工程合同（待确认）

1. 仅增加 crate 内显式函数根集合入口，首片只供上述普通测试使用；现有生产单根
   入口和 driver 选择保持不变。测试入口仍必须消费已封闭的 OwnedCompilationUnitView，
   不提供伪造前端事实或绕过 admission 的接口。
2. 输入是同一已验证 Unit 中非空、无重复的 DeclarationId 集合。空集合、重复根、
   不是普通函数、需要泛型实参或隐式 receiver 的根均结构化拒绝；首片只覆盖原
   scalar fixture 的普通非泛型顶层函数。具体错误分类须实施前与现有错误合同对齐，
   不预先承诺新的用户诊断码。
3. 集合按稳定 DeclarationId 规范化。先验证全部根，再将其 for_entry key 一次性
   放入同一个 pending；模板、planned、runtime_type_demands、CallablePlanner 和
   泛型实例预算均共享。实例预算按唯一实例计数，不能按根重置。
4. 保留一个 Program、一个 function_ids 表；结果返回根声明到 FunctionId 的
   确定性映射，不把某根特设为程序启动入口。根集合排序不改变任何调用绑定。
5. 单根适配器保持原根校验、错误优先级和返回值；不能因抽取共享核心改变当前
   CLI 或 native 入口行为。一次最终 verifier 通过后才提交观察收据，不允许先
   发布部分根、后失败另一个根；全 Unit 声明/global 检查仍在原阶段执行。

上述输入集合与返回映射是新增内部合同，当前任务先保留为候选，不以文档提交
宣布批准或实现。它不自动赋予任意函数、方法、closure 或其他能力作为根的资格。

## 分步实现与验收计划

- 第一片：抽取 root-set planner 核心，单根调用保持兼容；在普通源上核 root 顺序
  交换稳定性、共享实例去重、递归自引用、overload 原声明身份以及全局唯一预算。
- 第二片：共享 orchestrate 组装与根映射，保持一次 callable ABI/布局/SSA 分配；
  不把现有函数复制到第二个 Program，不引入合成 wrapper。
- 第三片：原 generic 源的四个非泛型根共同 lower：四根加两种 identity、两种
  relay 和一个 recurse，共九个函数；unused 泛型不得发射。原 overload 源的
  两个根共同 lower：两个不同 choose 声明和两个 caller，共四个函数，调用目标
  必须不同，LLVM 符号均独立且 verifier 通过。数量只是辅助，必须核原 source/span、
  声明 key、具体类型与实际 DirectCall 身份。
- 验证两种现有 Unit 来源布局，核 arena 不增长、重复运行稳定以及单根回归；原
  fixture 不加 package/wrapper、不拆源码。不把数次独立 lower 的成功相加当作共同图。
- 单个根失败不得返回部分 Program；普通非法源码与内部故障分类分开，所有事实
  变异、foreign identity 与其他故障注入只在既有 CI 规则允许的位置执行。
- 若旧生成名和新 source-qualified 名不同，先记录精确符号映射，保留原旧见证。
  只有明确哪些旧命名属于必须保留的合同，并通过相应差异裁决后才能删除旧测试。

## 停止条件与非目标

若实现发现需改变 receiver、export、ABI、错误优先级或所有权事实权限，停止扩张，
拆出新的候选而非调整断言求通过。此方案不解决 legacy MissingFact、mapper 身份、
旧拒绝边界或条件清理 oracle；即使两项迁移完成，也不能宣称旧 frontend 可全部删除。

## 隔离原型进度（不启用完整候选）

在 `e925eea2` 上推进第一片：共享 planner 内部根集合 worklist，生产入口仍仅传
单根并保留原校验；多根调用仅存在私有测试模块，不提供 driver/lowering 入口。
普通原源核共同计划的九/四实例、根映射、与各单根可达 key 的精确并集关系及逆序根
输入的完整计划相等。该证据不是共同 SSA Program、LLVM 符号或 native 验收。
尚未实现多根 orchestrate/返回 FunctionId 映射；共享预算拒绝、内部非法身份及
失败原子性也未完成动态验收。旧 whole-file 测试全部保留，Origin 与语言合同不变。

当前实跑：rooted/basic validated 来源的两个完整 FQN ordinary exact 各 1/1，
最终源码 no-run 编译通过。独立审查未发现 singleton 校验顺序回归；未执行故障、
全量或 native 测试，未推送。MemoryDocument、共同 SSA/LLVM 和预算边界仍待验收。

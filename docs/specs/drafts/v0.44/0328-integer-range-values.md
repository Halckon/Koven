# SPEC-0328: IntRange / LongRange 普通值首片

> **性质**：实施草案 · **状态**：draft · **读取时机**：准备 v0.44 D 首片接口与普通源码红测时 · **唯一真源**：本 Spec 的实施范围；语言合同以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P2-0328` |
| Phase | 类型与所有权身份先行；SSA、LLVM、native 分片验收 |
| 基线 | 实际 main `5a32550a0ea956c1d66fdb65ae9bf2aeb6681684`，已继承 0325/0326 |
| 分支 | `feature/spec-0328-integer-range-values` |
| 规范 | [Range 身份](../../../guide/03-types-generics.md#简单变量赋值与整数-range-身份)、[操作](../../../guide/04-expressions-operators.md#整数-range-与显式整数转换) |
| 当前阶段 | 仅计划与接口审查；无新测试、生产实现或执行结论 |

## 1. 范围与拆片责任

承接已启用的 IntRange / LongRange 非泛型环境身份、同型端点 `..` / `..<` 构造、
普通 Copyable/Transferable 值存储、`in` / `!in` 及同型序列相等。
不复制 Guide；公开构造器、额外成员、排序、hash、step、切片、用户 provider 均不扩展。
不同 Range 类型与已定型 Int/Long 不隐式转换；既有字面量定型和 Map 包含性保留。

数值 Range 的 `for` 是已启用但本片不交付的独立后继，由本片作者继续认领：
在本片值身份/ABI稳定后，另立实施 Spec，按[独立迭代合同](../../../guide/12-collections-destructuring.md#整数-range-与-map-的独立迭代合同)
闭合无 loan 的值交付、终点判断、continue/break/return 与 body owner 清理。
不以首片 done 宣称 D 全部完成，不预占后继编号。
0327 仍为独立转换交付，本片不依赖其未合入代码；其 CI/归档异常优先处理。

## 2. 已核接口缺口与候选表示

- `type_checking/model.rs` 的 BuiltinType 尚无这两个公开身份；名称环境、single/unit 类型表、
  TypeRef 解析、Copyability/Transferable 分类须一致接线，禁止按用户声明名字冒认。
- single `checker/expression.rs` 与 unit `bodies/checker/operators.rs` 的 Range 运算仍未完成。
  新事实应保留操作种类、真实 operand ID、端点类型及 Range 身份；后端消费事实而非重推 AST。
- 现 `iteration.rs` / `compilation_unit/iteration.rs` 的顺序 provider 是 Borrow 交付；
  `lang-codegen/src/ssa/provider.rs` 以 LoanId 和 length/cursor 表示，不能借它实现数值 Range。
  既有 RangeView / SSA Range carrier 是 View 借用机制，必须保持独立。
- 内部候选为定宽端点与 empty 标记的普通值布局，具体 SSA 类型/ABI 在生产前独审。
  可考虑 `{first, last, empty}`：半开仅在 start < end 后计算 end - 1；闭区间 MAX 不加一。
  相等按空标记或有效端点判断，不逐元素展开、不以 Int/Long length 表示全域基数。
  此候选不是公开字段、构造器或可观察布局合同。
- 先保留现有两个 TypeId 体系；不为此重构所有标量、借用 provider 或容器布局。

## 3. 拟定首批 ordinary frontend exact

拟独立 integration target `integer_range_values`，以下六个函数尚未落测试代码。
helper 仅普通源码经真实 lex/parse/names/raw types/complete API，两入口观察收齐后断言；
无 facts/IR 构造或变异、无语言产物执行。新 API 未存在时先采现有公开查询事实，
不得引用未来 API 产生编译失败后称语言红。

1. `same_type_endpoints_publish_range_identity`：已定型 Int/Long 参数分别构造闭/半开区间，
   用真实 TypeRef/类型表核结果，complete 成功；不能只断无诊断或把两入口 TypeId 互比。
2. `range_values_cross_storage_boundaries`：局部复制再用、普通参数/own 参数与返回、普通字段、
   既有支持的容器 element 路径分别采证；明确每个边界，不能用局部成功概括全部存储。
   示例 `fun make(a: Int, b: Int): IntRange = a..b`；未知类型当前可能先名称拒绝，分账。
   显式查询 Copyable/Transferable 分类并以普通调用/复制验证，不启动线程；own 只是参数采样。
3. `range_membership_requires_matching_endpoint_type`：Int/IntRange、Long/LongRange 的 in/!in
   结果 Boolean；Int/Long 混用、非 Range RHS 与现有 Map 包含性成对控制。
4. `range_equality_preserves_nominal_type_boundaries`：同型闭/半开混合 ==/!= 为 Boolean；
   IntRange/LongRange 不相容，不用静态类型绿冒称序列值比较已执行。
5. `invalid_range_endpoints_preserve_source_diagnostics`：已定型 Int/Long 混用、Byte/Short、四种
   unsigned、Float/Double、Char、Boolean、String、nullable 端点拒绝；端点错型锁 Guide L0085，其他前置诊断先采完整 code/span。
   不放宽 null 推导或 nullable 支持；用户同名类型仍按真实名称解析规则处理。
6. `range_values_do_not_publish_extra_apis`：公共构造调用、first/last/step/size/hash 成员不开放；
   `IntRange<Int>` 非泛型拒绝与 Map key 的 Hashable 约束拒绝另验，缺 hash 成员不等于
   不满足 Hashable。均与合法构造配对，避免以 Range 身份尚不存在误计专属拒绝门通过。

运行模板：`cargo test --locked --offline -p lang-frontend --test integer_range_values NAME -- --exact --nocapture --test-threads=1`。
落 helper 后再冻结 head/tree/输入 SHA 与唯一 exact 清单，独审后按授权执行；串行绝对 target。
本计划不授权宽 target、聚合、故障注入、校准或 fuzz；测试自身编译失败不算语言红。
隐式 `1..2` 表达式事实与显式 IntRange/LongRange 身份分组采证：后者当前名称拒绝可正是
本片缺身份的真实红，必须保留阶段与完整诊断；只将无关源码拼写/解析错误列为 harness 前置失败。

## 4. 后续值语义与资源验收

- 真实源码 FE → ownership → SSA → verify → LLVM；先核无 owner loan、Copyable 身份及
  存储/传参/返回/字段/容器实际支持范围，未支持边界须结构化拒绝并登记，不能静默遗漏。
- 单文件与项目各独立成功 native：动态端点构造、空/单元素/反向/半开、in/!in、==/!=；
  同型不同写法 `1..2 == 1..<3`，不同形态的空区间相等，非空端点不同不等。
- Int/Long MIN/MAX、MAX..MAX、MIN..<MIN、反向 MAX..<MIN、全域闭区间，避免任何
  MAX+1、空区间无条件 end-1 或基数溢出。用显式数学 oracle，不靠实现自身 roundtrip。
- 端点及 membership 两 operand 的带输出普通函数验证源码顺序、各一次；活跃 String owner
  与 pending 参数跨构造/比较后仍可读，正常退出精确清理。有限样例不外推所有析构顺序。
- for 未完成仍拒绝，不把 Range 值装成 View/provider 来凑绿。旧 View、Map 包含性、
  整数比较与存储 ABI 的受影响普通 exact 必须逐 helper 审核后另冻结。

## 5. 交付门

接口与红证明确后再 active 登记、最小实现、逐片绿测；此刻只提交 draft。
新增 target 必须同步 CI 选择与真实新增数量/唯一预算，执行实际 main base 的尺寸检查，
不得抬阈值或挤行。最终工程、双 host CI、归档 head CI 与真实 main 验收分账。
备份用 binary/full-index format-patch 增量及 base/head/tree/SHA/恢复说明；不修改 E 固定包。

## 6. 首批三项采证候选

现仅落 `integer_range_values.rs` 三个 ordinary exact：
`implicit_range_construction_has_resolved_copyable_value`（4源码/8入口）、
`explicit_range_identity_and_capabilities_are_available`（4源码/8入口）、
`range_membership_and_equality_publish_boolean_results`（8源码/16入口）。
每项在断言前收齐所有源码的 single/unit 观察；unit names.validate 拒绝时明确保存名称诊断，
不绕该门调用 types。single 保留真实 recovery raw 与 complete。原始 API 异常另报。
显式身份以本入口真实 TypeRef 对比，Copyable 使用公开查询；Transferable 仅先观察普通
泛型 bound 调用，完整所有权能力查询/存储、拒绝矩阵与值语义仍待后续，并非首三覆盖全 D。
当前只有 tests-only 候选，尚未编译或执行；其余三项与第4节验收保留待采。

首红固定 `86e4623f` / tree `3112d1b0`：三个 exact 各实际1命中且行为红，11输入前后相同，
32个源码/入口观察收齐，无编译/解析前置失败。隐式4源码双入口均 Deferred(Call)，
complete 在 Range 根保留 L0165；显式4源码 single 名称 L0080 并保留未完成 Range，
unit 四例在 names.validate 拒绝，未调用后续 types。这是公开身份未注册的本片真实缺口。
包含/相等8源码双入口父型均 Deferred(ControlJoin)，子 Range 的原诊断未丢失。
不据此声称 Copyable/Transferable、存储或序列值语义已经实现；完整 raw 位于仓外
`spec0328-evidence/red3-receipt.json`（SHA `18e2a3a78b9ca802c30325f82153bd8df1fd78cecb6c1156a4557d9e4d390ad5`）。
仍保持 draft 与无生产变化，后续实现/剩余矩阵须另行按审定范围推进。

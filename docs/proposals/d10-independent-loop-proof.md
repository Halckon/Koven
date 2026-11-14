# D10 独立 loop producer 与 CFG 证明候选

> **性质**：非规范工程候选 · **状态**：未启用、未实现 · **读取时机**：设计 D10 独立 loop producer 与 replay 时 · **唯一真源**：本候选；现行行为仍以 SPEC-0341 与代码为准

状态：架构候选，未实现、未启用。基线为本地 `e24640cc`；不替换现行
SPEC-0341、全 Replace receipt 或生产 selector。本文不裁决 Origin-based。

## 为什么不能继续从日志补出回边

当前真实来源已有条件/body 边界、producer/绑定/写入、borrow pending 移除和
清理事件，但这些是静态 planner 遍历，不是动态执行或实际 SSA predecessor。

- `drop_planner/owner_transport.rs::commit_owner_primitive` 在旧 observation
  inactive 时返回 None；此时 ValueState 中残留 owner 不能作为当前代。
- 同文件结束观察时不一致会置 Unsupported 并清空 definitions/events。
- `owner_transport/loops.rs::owner_while_statement_scoped` 克隆 body_state，
  静态遍历 body 并记录 continuation；成功返回不自动证明 header 方程。
- `lang-codegen/src/ssa/refine/build/control_bridge/nested_body.rs::NestedBody::new`
  要求既有 `nested_normal_body_v1`，并拒绝冲突的观察种类。不能绕过它把 Carry
  old==new 填入要求每根都有新 producer 的旧协议。

因此需要新增独立权威生产者，而非让日志分析器自行铸造已验证事实。

## 最小架构改变

### 1. 独立、每次分析唯一的 catalog

在测试域增加与旧 observation 不共享生命周期的 catalog。仅受限候选分析可开启；
普通生产入口不读此结果。catalog 的 identity 绑定同一次 parsed/named/typed/owned
输入，token 构造器私有。旧 observation Unsupported 既不使它自动成功，也不能给
它提供残留 token。独立 catalog 遇未覆盖操作必须标记失败并禁止导出证书。

它由实际表达式求值、binding、Replace commit 处形成 producer/根环境；不得从
完成后的日志倒填 token。Literal 的表达式 ID 标识定义位置，不是某次运行时分配。

### 2. 显式结构边，不用 source span 排序代替 CFG

实验 producer 必须直接拥有 Entry、Header、Body、Backedge、FalseExit、Resume
等结构节点和有向边。节点只能在实际选定的受限控制路径创建，并绑定同源语法身份。
条件真/假后继和 normal body continuation 各有独立边；错误/return/break 未支持时
整图拒绝。已有 checked arithmetic abort 的终止边及元数据必须保留并由 replay 核对，
不能当未知错误删掉；本候选不新增 abort-drop 承诺。所有边在构图时登记来源，
最终必须完整消费，不允许 verifier 补边。

现有 `WhileTraversal` 不满足这一接口。第一版只选一个正常 while，先验证实际
代码生成后的基本块和 predecessor；父子嵌套在单层闭环验收之后再加。

### 3. 根环境传递使用符号 SSA 值，不模拟动态代数

每条边携带 root → symbolic value 的环境。值为 Initial、Producer 或 HeaderPhi；
必须区分“静态定义一次”和“循环每轮执行产生值”。HeaderPhi 在结构建立时占位，
闭合时必须恰好具有 entry/backedge 两条实际输入；不能用递增整数假装动态 owner。

- Replace：验证当前 root 值存在，incoming producer 已在当前路径形成；产出新根值，
  old-result 明确引用旧值，并与真实清理事实配对。
- Carry：只允许边上传递同一个已知 symbolic value；必须证明该路径没有覆盖外的
  写、move、drop 或借用效果。这里的“同一”是逻辑根经 edge→header 参数映射
  的同一来源，允许 Initial→HeaderPhi 及跨块实体不同，不能要求原始 SSA ID
  相等。值相等本身不是有效 Carry 证据。不同活动根不得别名，Replace 新代
  不得与仍活动的 Carry 代混同；这些约束须由模型检查和最终 replay 分别验证。
- 合流：输入来自实际前驱集合，类型、source identity 与根集合一致；任何遗漏、
  多余或重复前驱都失败。不给缺失值填默认值，不从另一条臂复制缺失状态。

### 4. 独立 emitter 与 replay 的职责分离

emitter 消费成功的实验结构模型，生成真实 SSA blocks、分支、phi 输入与清理，
同时记录实际 emitted block/value 映射。replay 读取最终 SSA，而不是 emitter 自报
“成功”或同一手写拓扑：核 predecessor 集合、phi 输入、Replace old/new、drop 点及
终结出口。模型与 SSA 不一致不得发布任何实验结果，更不能回落成旧 receipt。

前端模型和最终 SSA 必须分别检查；LLVM verifier 是补充，不能代替所有权证明。
验收未完成前没有跨 crate 的生产 capability，不能修改默认准入或旧 oracle。

## 可执行工作顺序与停止条件

1. **catalog 与根环境**：实现独立身份/私有构造，接实际 Literal/Bind/Replace；普通
   无循环正例先闭合。任何当前值不明或清理不匹配即拒绝，保留旧 Unsupported。
2. **单层结构 producer**：从实际控制选取生成条件真/假和 normal backedge；覆盖清单
   完整，不能把未观察的 For/Loop 当作“没有影响”。先只准入已明确定义的正常 while。
3. **真实 emitter/replay**：实际 CFG predecessor/phi 与源模型逐项一致；同源身份、
   两条输入的来源及出口清理均可检查后，才可称单层实验闭环。
4. **Replace/Carry 与嵌套**：加入原候选 a Replace/b Carry，再加入父子层；不能跨过
   前三步直接重用旧全 Replace 接口。条件 break、return、复杂 producer 单独延期。

普通本地验收包括：正常单层、零次路径、真实循环后使用、未覆盖调用/提前退出拒绝，
以及源模型与实际 CFG 的独立一致性检查。故意破坏模型/映射/回边的注入验收仅在 CI；
本地可编译、列举而不执行。普通拒绝测试不等于恶意 corruption 测试。

## 当前交付与不可宣称项

已有真实事件和静态分区作为调试/来源审计材料保留，但不是本 catalog 或证书。
本轮只完成架构接口与实施约束设计，未实现上述独立 catalog、CFG emitter、replay，
没有新增生产能力；不能以此报告 Carry、代际推广或 D10 已完成。

## 第一批实现：无循环 producer catalog（测试域）

后续本地实现独立 per-planner catalog，token 私有构造且以独立 Arc identity 区分
不同 planner；不读取旧 `UnitOwnerId`。实际 Literal、Binding、Replace commit、
Named Drop hooks 直接更新 producer/pending/root 环境：Replace 把旧 token 交给
真实 old-result initializer，新 token 留在根；两者不能误认为同一 producer。

普通源码验证两个 literal producer、一次 Replace、最终根清理及重复分析 token
不相等；同时比较启用前后生产 facts/observations 不变。已观察的 call、move、
branch、while 均置失败，普通源码拒绝测试不做损坏注入。

这是第 1 步的内部状态实现，尚无完整源码覆盖检查、完成证书或导出 API。
`failed == false` 只表示已到达的 hooks 未冲突，绝不等于函数已验证；未观察的语义
仍可能存在。catalog 暂无 loop/header phi、CFG、replay 或生产权限，不可被下游
当 capability 使用。后续必须先补覆盖/闭合检查再考虑成功导出。

当前诊断字段仍可由测试域修改，不是不可伪造资格；Named Drop 暂只更新根环境，
尚未保存 token→具体 DropFact 退休映射，不能宣称完整 old-result 清理证明。

## 第二批实现：覆盖预筛与实际退休配对

catalog 启用前增加保守 AST 变体预筛：单个 Function，仅 Function/Variable item、
Block/LocalVariable statement、纯文本 String/Name 与已检查的单根 Replace 调用。
其他表达式或控制变体立即失败；普通整数 literal initializer 和独立 loop 源码被拒绝。
这只是当前窄形态预筛，不替代动态 hook 消费或语义完成检查。

每次实际 Named Drop 现在保存被移出根环境的 token 与原始完整 UnitDropFact。
正例验收每个 literal token 恰一次退休、pending 与 roots 均耗尽，退休事实顺序
与未启用 catalog 的真实 facts 完全一致。未用跨度猜清理点，也不合成 DropFact。

这些仍是测试诊断和验收断言；没有导出成功证书。尤其 AST 变体白名单不等于
全函数已完成验证，后续须核函数/body 可达覆盖、类型/根角色以及私有闭合接口。

预筛当前允许 Name，不代表已排除 scalar 参数拷贝；Function/Variable 的参数、
泛型、返回形式和声明类型尚待进一步收紧，不能扩大为“所有 scalar 都拒绝”。

## 第三批实现：最窄无循环闭合结果

源码覆盖收紧到一个无参数、无泛型、无 extension 的 implicit-Unit 函数；body
恰为两个局部声明：`var a = "literal"` 与 `val old = replace(&a, "literal")`。
核两个真实符号均为 String、Replace 单完整根指向第一个声明；所有 item、statement、
expression 数量和角色逐一匹配，无额外表达式或未归属 Name。显式返回形式及任何
其他源码形态暂拒绝，不以“同样应该安全”放宽。

catalog 的可变字段已私有化，仅提供只读诊断。私有构造的测试域 Closed 只在
形态匹配、实际 producer/binding 顺序一致、一次 Replace 的旧/新 token 正确、
root/pending 耗尽、每 token 恰一次真实 DropFact 且目标分别对应 a/old 后产生。
它由当次 planner 独立 identity 绑定，不接收旧 token 或外部拼装事实。

这仅闭合上述两声明无循环形态。Closed 没有跨 crate 或生产导出，不代表 CFG、
loop/header phi、Carry 或一般函数准入。新增参数拷贝与额外局部声明负例防止之前
仅按 AST 变体放行的空洞重新出现。普通源码测试不进行损坏注入。

完成检查进一步限定真实清理点：新根为该 Replace 的 CallReturn，旧结果为第二个
声明的 AfterStatement。Closed 是可重复检查的测试域成功标记，不是消费式 emitter
capability。负例采用 implicit Unit 形态，避免只被返回形式门槛遮蔽。

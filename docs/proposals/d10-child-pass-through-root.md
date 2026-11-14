# D10 child 未修改根：最窄隔离候选

> **性质**：非规范工程候选 · **状态**：未启用；首个真实源码探测发现保活前提不成立 · **读取时机**：评审 D10 child carry 扩展时 · **唯一真源**：本候选；现行行为仍以 SPEC-0341 与代码为准 · **基线**：`a78f8bdb`

## 问题与现行证据

现有 `owner_transport/loops/multi_root_normal.rs::transfer` 要求 root/step/producer
数量相同、每根恰退休一次，最终 Header 状态不能成为输出。
`refine/child_cycle_receipt.rs::finish` 要求每根都具有旧/新代实体对，且新代集合
不得与任何旧代实体相交。这些是现行全根 Replace 合同，不是可直接删掉的冗余检查。

本候选只研究两层循环、恰两根独占 String：child 改 a 而不访问 b，parent resume
仍只改 a。所有入口和替换 producer 都是已有 String literal；无 concat、一般调用、
借用源、第三根、闭包、字段、分支体、break/return 或跨轮 loan。checked counter
和 abort 复用原形状。函数正常出口只做原有最终根清理，不返回或打印 owner。

仅用户允许的本地设计/隔离实验范围；不修改 Guide/Spec，不放宽 selector，不发布
生产 capability，不得以候选 shape 匹配冒充已验证来源权限。

## 身份与状态合同草案

每根必须有明确动作，不以“没有找到 Replace”推定允许保留：

- `Replace(root, original_step)`：沿用原真实 producer、call、old-result retirement、
  source identity 及代际证明。
- `Carry(root, original_header_binding)`：必须来自同一已封闭 view、同一父/child
  control 与唯一根槽；没有 producer、没有退休事实，且有全路径无消费证据。

两动作是互斥且覆盖全部两根的总映射；a/b 的原 UnitSymbolId 及控制身份不能靠
名字、类型或 ordinal 替代。编译期来源相同不等于所有 SSA block 的 EntityId 相同：
每块的参数实体可以不同，需用原根槽和真实 edge argument/target parameter 逐边
证明传递，禁止跨 Function/Program 比较裸索引作为身份。

| 边界 | a | b |
|---|---|---|
| 父 header → child entry/header | 当前父代 | 当前父代 |
| child 零轮 false → resume | 原输入代 | 原输入代 |
| child body → checked-success → child header | 本轮 literal Replace 新代 | 同一逻辑输入代，经原槽转发 |
| child 正常 false → parent resume | 最后正常轮当前代 | 未被 child 消费的当前代 |
| parent resume → checked-success → parent header | 父 literal Replace 新代 | 原槽原逻辑代，不新造 owner |

这是部分正确性而非终止证明。b 不随轮次产生新 owner，也不因新建块参数而改变
逻辑代；a 的不变量仍按原 Replace 证明。禁止用 `old==new` 的虚构 Replace 元组
把 Carry 塞进旧 receipt，也禁止将新旧集合相交检查整体移除。

## 退出、清理与无隐藏使用

Carry 的证据必须覆盖真实完整函数与全部相关 CFG 边，不只检查 body 指令列表。
child 循环区域及 parent resume/正常回边区域内 b 不允许成为 Drop、Move、RootPlace、BorrowBegin、Call、Return、
Store 或其他指令的 operand；仅允许对应完整 owner 向量中的 edge forwarding。
每条正常边参数顺序与目标块参数身份匹配，无重复/遗漏/交换；逐角色精确前驱集合
及禁止额外重入条件沿用现行 receipt，child header 允许的恰是 entry 与 backedge，
不得误限为唯一前驱。Replace 新代不得与 Carry 当前代或其它活动根别名。最终 false 出口复用现行清理分区，每根恰一次消费；不得将
child 未消费 b 误报为泄漏，也不得提前清理后在父回边复活。

checked abort 沿用原终止边，不计作正常归纳后继，不能在本候选中新增“abort 必须
执行 drop”承诺。实际 root 向量、operator、前驱和 abort 块形状仍须按原规则核验。

## 分步实验与验收门槛

1. 先在隔离测试域从真实完成检查的源码提取动作候选。保持普通生产路径不选中
   本规则，不发布 initial-base/child-cycle/parent-composition 完成标记。
2. 私有候选模型显式区分 Replace/Carry。核同 view/control/root 身份、入口到父子
   header 的状态，以及上述全部边界；不得以合成 ID 替代真实事实。首次模型通过
   只证明模型，不代表已产出 SSA。
3. 在实验 emitter 中重放真实 CFG/实体/清理；独立终检通过才共同发布实验收据。
   普通用例包含 outer/inner 各 0、1、2 次组合及两个原根声明顺序；有限执行只作
   兼容证据，不替代逐边归纳证明。使用原源码，不加 wrapper 冒充原函数。
4. CI-only 负控至少覆盖 carry/replace 角色互换、遗漏或重复根、foreign view/control、
   b 隐藏消费、交换边参数、出口提前/重复清理、parent 回边复活，以及终检失败后
   无部分发布和同 view 重试。只在 CI 执行；本地编译/列举不能记作动态通过。
5. 按阶段分别记账：来源候选、模型、实验 SSA、普通 LLVM/native、CI 动态、生产
   启用。任何一项未做均明确列未完成。启用须有独立范围裁决，不自动替换旧 oracle。

## 初稿阶段结论

已对照现行 transfer 与 receipt 的全根更新前提，并完成上述有限候选的人工身份、
父子状态、清理和拒绝责任审查。初稿阶段未实现模型或 emitter、未跑新 ordinary/CI/native，
更未宣称 child 未修改根已支持。下一可执行切片原为第 1 步的真实源码隔离候选提取；下述实际探测已阻止按原限定直接形成候选。


## 首个真实源码探测：原限定不能形成 Carry 正例

独立普通源码包含两个 literal 根、child/parent 各一次对 a 的真实 Replace、b
从未使用。完整类型检查后的同次 ownership 事实明确：b 的唯一 Named Drop 是
其原 LocalVariable 声明的 AfterStatement，位置早于两个 While；两个真实
ownership primitive 的 place 均为 a。不是两个声明就有两个活跃循环根。

这与 [Guide 10 ASAP](../guide/10-ownership-borrowing-drop.md#asap-析构)一致：
未使用的纯内存 owner 在 initializer 完成并建立 binding 后立即析构。因此上文
“b 全程无使用但最后才清理”的组合不能作为现行合法 Carry 权限前提。先前状态表
只是假定存在活跃输入根的条件性模型，不是该源码已满足前提的证据。

测试 `ordinary_unused_carry_is_retired_before_parent_entry` 保留此普通真实来源差异。
未构造 Carry 描述符、未更改 ASAP/selector、未发布 receipt；这是提取必须拒绝的
前提失败，不是 CI 故障注入。下一步需先明确循环后真实 borrow/consume 的保活用途
与对应最终清理事实，再提取其合法活跃根；本稿不自行批准或实现该修订。


循环后真实用途的现行参照是 `lang-codegen/src/owner_parent_false_tests.rs` 的
`println(b)` 活跃根分区，符合 Guide 保留未来合法读取/借用的规则；该参照的
child 仍更新所有根，不能据此取得 Carry 权限。若采用真实输出作为后续用途，
须另列变体并核对应最终 use/cleanup，不以无意义临时绑定或人为 keepalive 求绿。
本片普通精确目标 1/1 通过，独审确认前提失败证据有效；不代表提取器完成。

## 隔离用途变体：循环后真实输出 b

另立 ordinary 变体，仅在原两层循环之后增加实际 `println(b)`。这改变初稿的
“出口只清理、不打印 owner”限制，不能覆盖或删除此前未使用 b 的反例，也不是
加入人为 keepalive。输出是源码的实际用途，当前阶段只核 frontend 事实，不宣称
已执行输出或原循环的实验 SSA 已生成。

实测 b 的唯一 Named Drop 变为该真实 println 的 CallReturn；同次 Shared loan
绑定原 b 的 root place，调用源码位置晚于两个 While。原全根更新
nested_normal_body 仍不存在。因此“根确有循环后用途”的前提可成立，但尚无
Carry 归纳/权限；后续提取器须以这些真实 use/cleanup 身份为输入，不能仅检查
语法后缀含 println。新变体仍未扩大生产 selector，不发布 receipt。

该用途变体与原无用途反例 ordinary exact 2/2 通过；frontend all-target 严格
Clippy、fmt、docs955、diff及身份/清理独审通过。未运行 native、故障或全量测试。

### 同次分析的必要证据提取（测试域）

`child_carry_candidate::extract_source_evidence` 目前只返回真实源码的必要证据：
同一 typed/ownership 分析、单函数且无 lambda、两根声明早于 parent、两个嵌套 while、child 内与 child 后且 parent 内各一次且同根的
Replace，以及另一根在 parent 后的唯一 Shared loan 和同一调用的 CallReturn 清理。
根身份从 primitive place 与 loan target 得到，不从 `a`/`b` 拼写或 symbol 下标构造。
没有后续读取、读取 Replace 根、parent 改写另一根、多个后续 loan
均由普通源码回归拒绝；原无读取的 ASAP 证据保留。

这层必要事实本身不是完整候选准入器：AST 白名单、两根类型与初始化限制、
old-result 清理配对由下节独立 shape 层检查，实际 CFG 代际边证据仍未建立。
返回 `CarrySourceEvidence` 只表示这些必要事实存在，不能产生 Carry 权限、
现行 nested-normal observation、selector 放宽或任何生产/SSA 输出。

### 封闭源码形态与 old-result 清理（测试域）

`child_carry_candidate::shape` 在必要事实之后，沿真实 AST body 边核对函数的
五项顺序（Replace 根、Carry 候选根、outer、parent、suffix）、parent 四项和 child 两项结构，
不再仅由 Span 推导语句归属。两根必须是同分析 `String` 的 `var`，初始化与
两次 Replace 新值均为不含插值的字符串字面量。两次 old-result 必须分别是
真实 `val String`，initializer 为对应 primitive，唯一 Named Drop 精确配对
自己的 `AfterStatement`。counter 存储限于 `Int`，赋值和二元操作的两侧限于
`Int`；表达式种类采用封闭白名单，调用仅允许这两个 primitive 与后缀调用。

这些是隔离源码候选约束，不是 CFG 或代际许可：下节源码关系以外的实际 CFG 边与 owner Entity 身份、cleanup lowering 和 native
见证仍待建立。生产观察、selector 和 SSA 均未接入。


### Counter 与后缀目标的源码约束（测试域）

`counter` 按同一分析的 reference→Symbol 配对条件左值、赋值目标及减法左值，
不按变量拼写猜测。两 counter 必须从不同真实 ValueParameter 初始化，分别采用
`counter > 0` 和 `counter = counter - 1`；0/1 仅接受无后缀十进制写法。
改名正例及条件错根、更新错根、右侧错根、>=、加法、步长2、共享输入参数的
普通源码反例覆盖该窄范围。

后缀读取除既有 Shared loan/CallReturn 配对外，必须具有 typed call 的编译器绑定
`prints_line` effect，且非 abort、无 receiver、返回 Unit；不把 `println` 拼写当
可信目标。普通递归调用不具该 effect，不能成为后缀输出候选。
这些约束未产生实际 CFG、运行时终止性证明或 Carry 权限，生产仍未启用。

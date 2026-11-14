# D10 child 未修改根：最窄隔离候选

> **性质**：非规范工程候选 · **状态**：未启用、未实现 · **读取时机**：评审 D10 child carry 扩展时 · **唯一真源**：本候选；现行行为仍以 SPEC-0341 与代码为准 · **基线**：`a78f8bdb`

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

## 本轮结论

已对照现行 transfer 与 receipt 的全根更新前提，并完成上述有限候选的人工身份、
父子状态、清理和拒绝责任审查。未实现模型或 emitter、未跑新 ordinary/CI/native，
更未宣称 child 未修改根已支持。下一可执行切片是第 1 步的真实源码隔离候选提取。

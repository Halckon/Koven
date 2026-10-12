# SPEC-0298: N1a 范围绑定与有界局部控制流

> **性质**：候选变更合同 · **状态**：draft · **读取时机**：准备范围绑定与标量条件组合时 · **唯一真源**：本页的有界候选范围；语义以Guide为准

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P4-0298` |
| 所属 Phase | Phase 3来源/终止事实消费、Phase 4 SSA/native |
| 语言规范 | [Guide v0.43](../../../guide/README.md)、[N1a](../../../guide/12-collections-destructuring.md#n1a-单来源连续范围-carrier) |
| 批准依据 | 2026-10-11确认0297→0300→0299/0298顺序；本页保持draft，由独立实施分支激活 |
| 前置 Spec | [SPEC-0289](../../../archive/specs/0289-n1a-range-carrier.md) |
| 前置 ADR | [ADR-0030](../../../adr/accepted/0030-range-carrier-source-facts.md) |
| 阻塞项 | 冻结有界CFG矩阵并证明逐边carrier/root/loan运输；复杂未证明路径仍拒绝 |
| 影响范围 | 两入口borrow-result能力门、局部CFG lowering与直接测试 |
| 语言语义变更 | 否；若所选程序超出现行Guide，先停下形成独立语义决定 |

## 1. Goal 与重起草来源

让已批准的单来源新范围描述符绑定，与有界标量表达式及局部if组合共存；只开放具有完整
origin、loan continuation/end和SSA运输证明的路径，不直接删除全函数能力门。

2026-10-12原未发布草案文件丢失，本页依据已核Goal/非目标、现行Guide与源码重新起草，
不是原文件的逐字恢复。原审计固定事实见[历史摘要](../../../archive/migrations/pr72-74-followup-audit.md)；
摘要中的旧静态检查不能作为本页或功能验收。

## 2. 起点与选择边界

基线`8c6907c2`的`ssa/borrow_result_support.rs::binding`按包含binding的最小函数Span
扫描，不是只扫描loan活跃区间。除真实Int五种算术及同root范围调用前缀if/return外，
Binary/If/When/Return和while/loop仍可被拒绝。因此binding前、存活期间、已结束后
各位置都需普通源码对照，不能从“不相关语句”直接推定其loan已结束。

首片选择Int比较（含`n == 2`、`n < 3`）、Boolean短路、既有String拼接及其局部if；
它们不能改变range唯一来源或让结果逃逸。类型、求值顺序、短路未执行后缀、String临时owner
清理与来源保护分别证明。普通借用结果/借用已有metadata绑定的原门禁不随新描述符自动放宽。

沿用已发布事实、显式block参数和verifier；不靠语法形状或相同TypeId猜来源，不从AST补loan。
父子/兄弟/元素依赖仍保护根；一条分支结束不能释放另一存活依赖。正常结束先解除相应
结果/元素loan，再解除来源依赖，最后清理root owner；其他存活父/子/兄弟依赖继续保护根。
事实不足时维持带Span拒绝。

## 3. 非目标与仍开放的候选

不做一般CFG/NLL、条件/多来源合流、普通借用结果全面运输、任意when/return、escape、
consume、Clone、N1b、用户扩展或新算法；不以0289归档推定这些能力完成。

while/loop及更广循环组合继续作为后继候选；`break`/`continue`并非语言禁用，但不纳入
本片新增支持。后继须先封闭循环fixed-point/owner版本、目标loop身份、backedge上的根与
父子loan、continue保留provider/source、break结束最近provider及逐边清理，再独立验收。
不能把if正例或移除gate当作循环证明。

## 4. 最小验收与交付

- [ ] 两入口普通源码分别覆盖上述四类表达式在binding前/期间/后的结果；静态选择必须有实际命中。
- [ ] if两支与短路两值有真实SSA/verifier、未改写LLVM及正常native结果，求值各一次、未选后缀不执行。
- [ ] String/MoveOnly/Resource范围保留唯一根、上述结果/元素loan→来源依赖→root清理顺序、父子/兄弟依赖和结束后权限恢复；非法move/变异/逃逸仍拒绝。
- [ ] 0289计数、同root前缀退出及普通borrow/metadata别名回归不退化；未选CFG继续结构化拒绝。
- [ ] 独立分支先红测、最小事实消费、直接消费者回归、Architecture与本页验收；发布按单独交付闭环。

本页只有静态起草；功能测试、Cargo、native、注入与聚合均未运行。起草静态验收见0303恢复记录，
不将历史649/42或0289的功能通过数字复制为本片结果。

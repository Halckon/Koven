# 受限入口 concat 基例

> **性质**：实现合同 · **状态**：current（远端破坏性验收待完成） · **读取时机**：修改受限入口 producer 时 · **唯一真源**：本文及对应 receipt

新增实施切片只选择直接函数入口两个不同的非插值 literal 临时 owner 的总计单次
StringConcat，结果绑定到父循环根。FE 使用独立 concat origin，不能冒充 literal
生产者，也不放宽旧 literal-only 能力；完成 operand 遍历及临时清理后才记录结果。
后端在初始块真实发射时封存 producer ID/operation，终检核同块唯一 concat、
两输入各唯一 literal 定义、只被 concat 读取及随后原块唯一 Drop 使用、无参数或
terminator 逃逸，结果继续使用既有完整入口根向量与父组合合同。
一般调用、clone、借用或具名输入、嵌套 concat 不属于这片。
CI-only 新增 duplicate input、missing Drop、early Drop 三类，并在 concat fixture
复用初始根交换/提前消费两类；使用第三个双宿主精确目标。本地只编译列举。

返回[父子循环入口](nested-owner-normal-body.md)。

FE replay依赖planner发布的完成事实，不独立重读Drop facts；实际清理顺序由SSA终检。

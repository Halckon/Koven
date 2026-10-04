# 当前教程

> **性质**：教程入口 · **状态**：current · **读取时机**：学习可执行的 Koven 示例时 · **唯一真源**：本目录

[当前 Koven tour](koven-tour.md)依据 [Guide v0.40](../guide/README.md)。源码只保存在 Markdown；
`examples.json` 保存完整输出合同，`scripts/check_tutorial.py` 使用真实 CLI 验收。
跨文件合同按路径引用本页 fence ID，自动生成现行 project manifest；源码不在 JSON 重复。
定向验收可重复传入 `--example ID`；CI 默认选择15个正例和4个负例；parameter-report的四组argv共用一份三文件源码，
每组分别执行build、artifact和run，总计22组执行合同。原17项由0262/0268有双宿主证据；
新增5项及当前完整选集的双宿主实际证据见 [SPEC-0271](../archive/specs/0271-tour-combination-coverage.md)；PR49实现head已通过双宿主CI。
两个planned不计通过：线程native未验收；`gap-scope-branch`保留实际 `InvalidSsa` 失败。
编排mock测试本身不作为实际CLI证据。
历史教程保留在 archive，不代表当前实现。planned 示例不计入通过。

## 有限覆盖与仍有的差距

| 维度 | 当前代表示例 | 证据或边界 |
|---|---|---|
| 入门值与调用 | hello/strings/branch/function/constant | 原有真实CLI合同 |
| 数字表达式组合 | [numbers-bitwise](koven-tour.md#numbers-bitwise) | Int radix、分隔符、and、shl计数屏蔽；不覆盖全部宽度与溢出 |
| Borrow与资源作用域 | borrowing/deinit、[scope-cleanup](koven-tour.md#scope-cleanup) | 函数资源逆序清理及caller继续；不证明分支资源缺口或纯内存ASAP |
| 原地置换与参数处理 | root-replace-swap/parameter-report | root与直接字段replace；空、非空、UTF-8和空字符串argv |
| unit/native资源退出组合 | [unit-loop-cleanup](koven-tour.md#unit-loop-cleanup) | 两文件、临时MutableList、Borrow调用、continue/break/return、元素和局部资源清理；一个具体provider |
| 预期结构化拒绝 | reject-typed/reject-ownership、[reject-immutable-place](koven-tour.md#reject-immutable-place)、[reject-iteration-move](koven-tour.md#reject-iteration-move) | 完整JSON类型、移动、不可变place及迭代活跃借用诊断；非法源码无产物 |
| 已知native缺口 | [gap-scope-branch](koven-tour.md#gap-scope-branch) | Mac固定基线实际build失败；planned，不计通过 |
| 未纳入本批 | nullable/Result、enum、闭包、Box/Rc、Map、任意嵌套组合 | 不从上述例子推定全面支持；语义见Guide，实现边界见Architecture |

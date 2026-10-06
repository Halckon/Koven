# 当前教程

> **性质**：教程入口 · **状态**：current · **读取时机**：学习可执行的 Koven 示例时 · **唯一真源**：本目录

[当前 Koven tour](koven-tour.md)依据 [Guide v0.41](../guide/README.md)。源码只保存在 Markdown；
`examples.json` 保存完整输出合同，`scripts/check_tutorial.py` 使用真实 CLI 验收。
跨文件合同按路径引用本页 fence ID，自动生成现行 project manifest；源码不在 JSON 重复。
定向验收可重复传入 `--example ID`；CI 默认选择17个正例和4个负例；parameter-report的四组argv共用一份三文件源码，
每组分别执行build、artifact和run；新增argv-word-frequency的8组后总计31组执行合同。原17项由0262/0268有双宿主证据；
原新增5项及当时22项完整选集的双宿主实际证据见 [SPEC-0271](../archive/specs/0271-tour-combination-coverage.md)；PR49实现head已通过双宿主CI。第23项及本轮23项完整选集已在PR50实现head通过双宿主CI，验收见0273。
argv词频及decimal/非法UTF-8边界已在PR52实现head通过双宿主各37条真实命令；
完整字节账本见[0274验收](../archive/specs/0274-argv-word-frequency.md#9-双宿主实际验收与归档2026-10-05)。
仅线程native仍为planned，不计通过；`gap-scope-branch`由 [SPEC-0273](../archive/specs/0273-control-body-resource-cleanup.md) 修复并进入实际执行合同。
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
| 分支资源退出 | [gap-scope-branch](koven-tour.md#gap-scope-branch) | 同一历史失败源码加入正常CLI合同，核对内层逆序清理与外层存活；验收见0273 |
| argv词频 | [argv-word-frequency](koven-tour.md#argv-word-frequency) | 三文件、首次顺序、精确UTF-8及控制字节；双宿主各37命令已验收，边界见0274 |
| 未纳入本批 | nullable/Result、enum、闭包、Box/Rc、Map、任意嵌套组合 | 不从上述例子推定全面支持；语义见Guide，实现边界见Architecture |

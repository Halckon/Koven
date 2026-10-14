# 范围绑定与有界局部控制流

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改新范围描述符的比较/局部if能力门时 · **唯一真源**：`ssa/borrow_result_support`、两入口borrow-result与普通源码测试

[SPEC-0298](../archive/specs/0298-range-binding-local-control-flow.md)冻结首片只新增真实Int
`==`/`<`与有界statement-if。[SPEC-0315](../archive/specs/0315-range-int-comparisons.md)
补齐`!= <= > >=`；两操作数必须由typed facts证明为Int，比较结果另核Boolean；
五种既有Int算术保持原合同。gate仍扫描包含显式binding的最小完整函数，不限loan活跃区间；
普通borrow与借用既有metadata不随NewRangeDescriptor放宽。

新if必须是直接词法Block中的完整语句、有显式双ControlBody，且两arm只含Owned Unit
调用语句。该Block的全部直接borrow声明先于if，均须有NewRangeDescriptor、相同精确
origin、source continuation以及唯一`AfterStatement(Block)` end。scope选择只限定能力形状，
不从AST或最后一次read重算loan；构造处仍执行原call/argument/origin校验。

整个if子树只接Name、literal、Group、Prefix、普通Member、已放行Int Binary与纯文本String；
call结果另核Owned和Int/Boolean/Unit。Nothing、嵌套控制、lambda、赋值、nullable断言、
传播、索引、cast、safe member与插值不由首片开放；新if也不能位于循环或lambda内。
这不改变原来同root范围调用前缀的if/return支持。

真实CFG继续由原single linear-binding slots与unit carried-control运输descriptor Value、
protecting Loan、source loans及root owner，分支各自重绑定，合流显式核对slots。
verifier逐边核对精确descriptor/source配对和实际root能力；不存在按同类型猜来源的捷径。
结束消费原end facts：对应RangeEnd在其保留source BorrowEnd之前，root消费在全部活跃
依赖结束之后。root-flat child/sibling保留独立根能力，不依赖父metadata永久存活。

普通源码定向测试覆盖single及unit provider正/逆序，0315另比较literal/const fixture与
独立validated constant unit入口。比较在binding前/
期间/已结束后、双arm与合流继续读取，以及String/MoveOnly/Resource父子兄弟同根。
测试消费真实SSA/verifier与未改写LLVM/native，显式核对入边身份、结束顺序和正常析构输出；
可观察stdout不等同于完整泄漏或内存安全证明。直线源码的最后一次read后，即便仍在同一
词法块，已有受限last-use事实也可先结束descriptor及source；0315只消费该事实，不新增
终止规则，也不把statement-if所需的scope-end证明套给所有比较。

公共project CLI另覆盖六种Int比较；0315四比较各有basic/validated constant路径、三位置与
相等/正负极值的真实build/run。单文件CLI不加载range std，不以single codegen证据宣称
该CLI入口已支持同一导入源码。

短路、String拼接及复杂RHS、value-if、分支新binding、条件/多来源、一般NLL、借用存储/
逃逸和更广循环组合仍是开放后继。0299专用nullable proof suffix、0300 Map owner carrier与
frontend/LLVM ABI未改；冻结首片不宣称整个0298广义目标完成。

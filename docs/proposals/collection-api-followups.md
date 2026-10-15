# 集合剩余 API 与迭代合同（候选）

> **性质**：非规范候选设计 · **状态**：未启用 · **读取时机**：评审未选顺序集合与 Map API 时 · **唯一真源**：本页的剩余 API 范围；来源核心与算法分别由链接候选维护

本页接收原[M3A](../archive/development/sequential-collections-spec-draft.md)和
[M3B](../archive/development/map-collections-spec-draft.md)未承接范围。0275/0276/0279–0285、
0287/0288及0295的选定能力不重新列待实施；现行API以[Guide12](../guide/12-collections-destructuring.md)为准。

## 尚未选定的范围

- 显式容量控制（如预留容量）及真实程序需要的其他基础操作；capacity不是已批准公共属性。
- Map更广查询、遍历、输出顺序与provider失效；词频程序重扫argv不证明开放Map迭代。
- 每操作封闭receiver/参数mode、求值顺序、提交点、旧/新值owner、活跃loan与relocation、
  大小/布局溢出、分配失败、用户代码调用及drop次序，不套用其他语言API。
- String、Resource、嵌套容器和零大小布局按真实可构造类型选范围；内部ZST不冒充源语言支持。

原C01–C08及K矩阵未选部分保留：空/边界/增长或删除、Copyable/MoveOnly正反例、前缀return
与正常提交、借用/provider失效、精确清理、泛型/多文件身份、独立参考及双入口/native回归。
Spec只选其Goal相关项，不将整张旧矩阵视为全部完成或重跑。

## 复用与条件依赖

借用查询、Missing/Found(null)、Map→List→字段统一复用[M2B](general-borrow-access-results.md)。
算法/物化/N1b复用[collection r3](collection-algorithm-ownership-r3.md)，通用复制复用
[clone候选](explicit-clone.md)；r3未承接Map算法或迭代器协议。
consume、只读Map转换和已有具体正确性缺口由[Spec索引](../specs/README.md)定位，不因本页激活。
Set、开放Iterator/Iterable与视图分别选定，不捆入Map基础能力。

## 非空Map初始化首候选

2026-10-15已选为[v1交付清单](v1-collections-io-delivery.md)的首个实现候选；本节尚未冻结
语义，不修改Guide。当前`mapOf()`/`mutableMapOf()`只接纳空参数，不能把已支持`put`当作
带参工厂已实现，也不以0291的consume转换替代本需求。

### 参数表面与条目身份

首选待审表面为`mapOf(Pair(key1, value1), Pair(key2, value2))`及同形mutableMapOf；
空参数与显式K/V继续保留，非空推断、异质条目诊断及同名用户Pair的处理须冻结后才实施。
不引入`to`/infix、交替key/value参数或新公共Entry类型。仓内prelude虽有Pair声明，不能据此
断言标准Pair名义身份及本工厂泛型能力已具备；完整value-class消费式解构只提供候选基础。须分别核标准nominal身份、泛型
具体实例布局、Copy/Consume模式和两入口事实；普通同名Pair不能按名字冒认标准身份。

现有完整解构可整体消费非Copyable聚合，再一次性交付全部字段；应评审复用已有
`AggregateExplode`路径，而不是逐次移动`pair.first`/`pair.second`或放开一般partial move。
若普通源码无法满足完整移交或编译器边界，停在设计审查；不为工厂顺带扩通用泛型/解构能力。
只读Map装填还需独立构造权限：现有MapPut仅接纳MutableMap，不能为工厂放宽公开只读变异。
应选择受限构造路径或内部可变构造后整体封存，并单独证明身份/清理；这不自动启用公开consume API。

### 求值、重复key与owner候选

- 建议按源码顺序逐条求值，条目表达式与内部实参均遵守既有源码求值顺序且只求值一次；
  仅位置参数Pair(key, value)是key先value，命名逆序实参不得重排。按字段身份拆分已求得结果。
  条目完整求值后再提交，下一条开始前本条交付完成；该提交点是候选，须核现有构造流水线。
- 建议重复key后者覆盖；冻结前逐项比对现行MutableMap.put的Guide、runtime和native证据：
  等价key命中时旧key保留或被替换、incoming key何时消费/drop、旧value替换的准确时点均须一致，
  若存在差异先报告而不自选语义。每个实际owner须恰一次交付或清理，Copyable来源仍可用，MoveOnly来源不得再用。
- 完整条目求值期间正常return/控制退出，需要区分已提交容器、未提交Pair及其初始化前缀，
  核真实清理责任；Abort继续不展开。不能只照搬put成功路径，或借优化改变可观察求值/drop次序。
- 不新增隐式clone、装箱、一般nullable借用或借用逃逸；K仍满足既有Hashable与存储合同，
  V仍满足既有可存储合同。nullable V的Found(null)与Missing分别观察；get仅对Copyable V，
  MoveOnly V仍由既有requireValue/withValue查询，不因初始化而放宽。

### 冻结所需最小验收

1. 空/单/多条与重复key；源码副作用精确顺序、最终size/查询结果，核旧/新key与value各自的最终owner和恰一次清理。
2. Int Copyable、String MoveOnly key/value、nullable V的null/非null、选定Resource与嵌套值；
   不可构造组合列为前置缺口，不能用伪事实或内部ZST冒充普通源码。
3. MoveOnly再用拒绝、活跃loan冲突、非法key/非存储类型、错误条目形状与不一致类型的真实Span。
4. 完整条目移交、正常退出前缀清理与Abort边界分账；单文件、project basic/constant同源码对照，
   SSA owner/loan身份、公开CLI输出与资源普通deinit顺序分别验收，不用allocator计数替代行为。

现有覆盖行为可从[Map native测试](../../crates/lang-codegen/src/native_map_tests.rs)定位，但其中
counted helper不进入云端执行名单；本节只读核对，未新增或运行测试。正式Spec须先独审普通fixture/helper与精确argv。

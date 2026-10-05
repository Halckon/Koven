# SPEC-0276: Unit 普通泛型函数体的具体类型归一化

> **性质**：有界变更合同 · **状态**：approved · **读取时机**：补齐普通顶层泛型 body 的具体类型身份时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | approved |
| Goal ID | `KOV-P2-276` |
| 所属 Phase | Phase 2 发布具体类型身份；Phase 3 保持所有权事实；Phase 4 消费并执行 |
| 语言规范 | [Guide v0.40](../../guide/README.md)、[泛型](../../guide/03-types-generics.md)、[集合](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户持续实施里程碑、满足前置并行、按实际情况调整草稿的站立授权；有界cache及DAG/预算/recipe仲裁独立准备审阅已完成 |
| 前置 Spec | SPEC-0275 |
| 前置 ADR | ADR-0020 |
| 关联 ADR | [ADR-0028](../../adr/accepted/0028-frontend-generic-body-type-normalization.md)（accepted） |
| 阻塞项 | 无；G1–G7仍需fail-first及实际验收 |
| 影响范围 | unit body checker 私有类型归一化、共享实例上限、直接 frontend/SSA/native 验收 |
| 语言语义变更 | 否 |

## 1. Goal 与启动证据

普通顶层泛型函数的 body 首次产生 `Array<T>`、`List<T>`、`MutableList<T>` 时，真实调用
可以获得 frontend 发布的具体类型身份，沿既有 owner/Borrow/size 合同执行到 native。
不依赖签名、其它未使用声明或 backend intern 补种类型；只复用已经提交的类型与调用事实。

基线为 PR53 合并后的 main `2be64066a2011bb07a31bd68f9ac7441ab5a4baf`，独立分支
`feature/spec-0276`。实现/最终归档/main CI、完整原始产物和 source 身份闭环见
[0275交付账本](../../development/evidence/generic-containers-0275-delivery.json)。
main CI37253329605已15/15终态成功；原main的6个未提交文件内容、状态和index已保全。
本页承接[M3A草稿](../../development/sequential-collections-spec-draft.md) C07，未宣称全部M3A完成。

## 2. 实际缺口与复用接口

0275已支持直接容器签名替换；普通 `probe<T>(own x:T)` 中 `val xs=listOf(x)`
仍只发布List<T>，实际probe(1)未产生List<Int>。既有0275测试明确核对canonical不存在、
backend保持arena长度及MissingFact实际construction Span；该历史红边界是本片要补齐的行为。

`BodyChecker::substitute_type`已按UnitSymbolId结构替换并在frontend私有arena intern，
`UnitCallDescriptor`已保存实际有序type_arguments和target；trial已完整回滚signatures/parts。
`materialize_runtime_field_layouts`消费新增concrete nominal identity。复用这些现有接口，
不重新运行AST body检查、overload选择、ownership或布局语义，不引入第三方依赖。

## 3. 支持范围与事实归属

1. 支持普通顶层source函数body及其普通顶层直接调用链，至少三层relay→middle→inner。
   实参有显式/推断、Int/String/具真实deinit的非泛型普通Resource；空与非空、多个body构造。
2. 所有body/trial结束且无signature/body错误后，使用已提交source call descriptor中实参
   全部闭合的普通顶层callable作为种子，包括非泛型deinit/member中的实际helper<Int>调用。
   未调用的probe<T>不自行猜Int；symbolic helper<T>不成为闭合种子。
3. 每个种子按source DeclarationId与完整有序实参建立确定性worklist；替换环境只取该
   callable的UnitSymbolId。无T的普通callee继续传播；visited防同key递归。
4. 根据source-qualified函数item Span关联已提交expression、symbol、TypeRef以及call/
   construction需求；物化其完整类型树，包含局部Nullable<T>。词法lambda归外层函数实例，
   不扩大既有closure ABI。模板facts保持原identity；仅canonical table追加归一化类型。
5. 归一化在已有field layout materialization之前、sealed typed发布之前完成；后端只find，
   不开放可变arena或公开“已完成body实例”capability。

## 4. 有界展开与诊断兼容

共享现有1024 specialized-instance上限，frontend每种子展开是有界类型cache；Phase4仍独立
选择实际entry可达图并在现有位置决定资源拒绝。不新增前端limit诊断或planner入口快速失败。
保持所有已有canonical ID；合法归一化前缀保留，不做全root rollback以免制造提前MissingFact。

当pop specialized且已达到上限时停止该实例的后继展开，对current及尚未planned的pending
直接call实参/receiver/return和body类型做一次有限物化，禁止传播新的实例key。
nongeneric key严格指完整实参为空、实际目标无类型参数的ordinary顶层函数，visited后只
展开一次且不增加specialized counter。counter到限本身不清空pending；仍按pop顺序处理
nongeneric，第一次pop尚未planned的specialized超限key才转有限frontier并结束该种子。
现有recipe collector后续有限symbolic固定点保持template fallback；不无界展开field recipes。
原planner key/计数/recipe优先级与完整kind/Span不变；已有later sibling非法recipe须仍优先于limit。
范围内如发现implicit runtime边使cache不足，先修正本合同并重审，不能吞MissingFact或加大预算求绿。

实例预算不是类型图工作量证明。新cache的闭合性与结构归一化、下游field-layout concreteness
按canonical UnitTypeId缓存已完成结果，区分active与completed；共享子图不按路径反复展开。
具体实参子图与深链使用迭代工作栈，不将1024层展开压入宿主递归栈；保留原结构替换/intern语义，
通过现有helper的最小适配复用，不增加第二套类型规则。测试用unique节点访问计数验证线性
共享DAG读取和深链完成，不用宽泛timeout代替。具体替换的memo作用域必须含该实例substitution，
禁止跨不同UnitSymbol实参环境复用错误结果。

## 5. 必需验收

| ID | 可验证完成标准 | 当前状态 |
|---|---|---|
| G1 | 三容器×Int/String/Resource，空/非空、显式/推断、多个body构造；native完整stdout和正常allocation/free逐指针/次数匹配，无隐式clone/retain | 未执行 |
| G2 | 三层泛型调用；同名T跨文件身份隔离、重复实例去重、fresh反序输入稳定；原模板facts及sealed typed owner保持，backend前后arena不增 | 未执行 |
| G3 | 非泛型Resource.deinit仅闭合调用helper<Int>，entry仅构造Resource；真实implicit清理和helper body-only容器成功 | 未执行 |
| G4 | body局部Nullable<T>归一化；Nullable<Resource>真实native，Nullable<Int>仅核canonical并保持既有inline representation拒绝；无具体调用不产生List<Int> | 未执行 |
| G5 | frontend私有小seam0/1/2核prefix/frontier与specialized计数；codegen沿原planner seam独立核pure limit及later sibling非法recipe完整kind/Span；production1024真实增长源码仅编译拒绝/目标保全，未选增长源码正常entry完成；共享DAG按unique节点访问数和深链验证 | 未执行 |
| G6 | no-match/ambiguous/lambda trial/最终expected-type错误不启动cache；恢复事实与既有诊断保持，不以错误unit arena总长不变替代隔离 | 未执行 |
| G7 | 后继支持不扩大0275 recursive-template/owner-recipe边界；相关frontend、unit plan/recipe/error-order/container/native及CLI/M1A/词频直接消费者通过；独立全审、Architecture、双宿主CI、最终归档CI、merge/main闭环 | 未执行 |

G1–G4必须使用真实body-only构造，源码中不得放入未使用的concrete容器声明补种。
先建立fail-first证据，再修改生产代码；只测find(List<Int>)不替代SSA/object/link/run。
frontend私有cfg(test) seam不从codegen依赖可见，两个0/1/2验收为分层证据；不伪称同一次
跨crate小预算流水线、不新增公开可变owner/test bridge。实际production1024验证跨阶段边界，
增长程序绝不执行native，不注入错误IR或内存故障。
现有历史body-only拒绝测试在本片变为真实成功合同，其其它缺substitution/递归模板保护保留。

## 6. 非目标

generic nominal owner/member/StaticSelf/inherited/delegated派生路线、任意递归模板lowering、
新集合API、runtime长度initializer、投影/Inout扩张、短路owner合流、single/unit全面合并、
M1B-b输入/分词、M4b真实故障校准、性能和公开Release不进入本片。语言允许而未实现的路线
继续登记独立缺口，不将后端拒绝提升为语言限制。

## 7. 实施与提交

1. 工程算法独立审阅、ADR前置与正式合同 → docs/inventory/DAG检查；有实质未决项保持draft。
2. 真实失败frontend/SSA/native矩阵 → root串行Cargo；测试可按独立文件委派，禁止争用target。
3. 最小私有归一化与共享上限 → G1–G6、共享/下游检查、独立完整审阅及修复再审。
4. 同步Architecture/真实账本并单一逻辑实现提交 → 精确head PR CI。
5. 完成验收后归档/inventory/DAG → 最终归档head全部必需CI成功才merge，再核actual main CI。

合同与实现分别提交，消息含SPEC-0276。新模块低于1000物理行，不提高旧尺寸baseline；
既有超限文件仅允许最小注册/hook，并按尺寸规则明确有限增长与审阅依据。
本地只选直接受影响frontend targets/filters，严格clippy/fmt/尺寸/docs及diff；不默认全量frontend。

## 8. 准备记录

2026-10-05：基线main CI及原始artifact已独立复核；正式0276 Rust矩阵未运行。
已否决全root canonical撤销和planner前early-limit候选，因其可能改变MissingFact/recipe优先级。
首次独立准备审阅指出field-layout concreteness只使用路径visiting、共享Function DAG可能
指数重复遍历；已纳入completed memo/迭代深链与确定性节点计数要求。候选源码尚未运行，
不得记作已复现。nongeneric边界与跨crate私有seam的证据层级已明确，修订窄复审无新增实质阻塞；
按用户站立授权批准0276与接受ADR0028，仅授权实施，不据此标记G1–G7通过。

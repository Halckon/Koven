# M2 / M2B 通用借用访问结果：Spec 起草材料

> **性质**：待编号设计任务起草材料 · **状态**：draft / 方案未选定 · **读取时机**：评估借用访问需求与语言代价时 · **唯一真源**：本页维护决策问题和候选验收；语言规则见 Guide

## 1. Goal 与前置

Goal：根据用户新增方向设计可组合的通用借用访问结果，而非仅为 Map<String,Int> 增加特判；
区分现行直接 Borrow 投影与可局部绑定、字段访问、包装函数转发的非 owning 结果，
说明合法/非法程序、实现依赖和代价。用户称 M2B，对应原计划 M2，不另占里程碑编号。
此阶段的完成产物是设计决定，不是功能已实现或新 Guide 已启用。
当前任务属于语言设计准备；若选新增能力，后续跨 Phase 1–4，公共库可能涉及 Phase 5。
共同权限和可调整规则见[计划](post-governance-milestones.md) G0–G3/§10。

前置是 M1A 或其他真实程序提供的具体痛点及最小源码，不能仅凭“借用返回更灵活”实施。
M1A、现行元素 place 的直接 Borrow 以及 M3A 的现行操作支持补齐均不因此阻塞。
只有 M3A/M3B 选定的新增结果形态依赖 M2B；Map<String,String>/Resource、List<MoveOnly>、
字段及 Map→List→字段必须共同复用其核心，不由不同库各自发明借用返回规则。
候选合同、首版范围和来源矩阵见[通用借用访问结果 proposal](../proposals/general-borrow-access-results.md)。

## 2. 现行边界与比较方案

[所有权规范](../guide/10-ownership-borrowing-drop.md)区分 owned copy/move 和调用期 loan；
从 Borrow 参数返回 Copyable 值是 owned copy，不是借用返回。v1 没有 borrow-return 类型。
[Closure 规范](../guide/07-calls-lambdas-closures.md)允许 borrowed closure 在 defining callable
内按规则存活，不应被本设计误改成所有闭包仅有单次调用寿命。

| 方案 | 需要比较的能力 | 必须公开的代价或边界 |
|---|---|---|
| 保持现行访问 | 元素 place、同步 Borrow、索引，以及明确需要时的 clone | API 可组合性限制；不能把所有访问都说成必须 clone |
| 受限借用返回 | 由明确输入来源派生、不能逃过 owner 的返回访问 | 返回表达、来源约束、调用方存活与失效、临时值、泛型及诊断 |
| accessor / yield 投影 | 访问期与恢复点由结构化投影表达 | 新语法/控制流/清理/重入边界；与当前 getter、yield 保留边界的冲突 |

方案名字不是可编译语法。具体规则若被选中，写入 proposal，再准备并明确启用 Guide，
然后形成所需 ADR 和实施 Spec。仅写 ADR、登记 draft 或实现原型都不改变 current 规范。
现行写法作为能力和成本对照；若某候选不可行，说明原因及后继，不以维持 Int-only 查询
冒称完成用户要求的通用方向。方案选择和语法启用仍须独立审查。

候选分三层：所有权来源/Shared/Exclusive/reborrow/失效核心；函数与静态接口的
receiver/指定参数来源合同；集合及用户库 API/语法适配。首版建议单一明确来源、局部绑定、
只读重借用、受控跨函数转发及结构化独占修改，不采用 Rust lifetime 语法或引入 v2 动态派发。

## 3. 决策案例矩阵

每行最终绑定最小源码、输入/来源、期望接受或拒绝、拒绝阶段和理由；下表只定义场景。
“待决定”在方案启用前必须消除，不能当作跳过的测试算通过。

| ID | 场景 | 当前基线 / 要判定的问题 |
|---|---|---|
| B01 | 将顺序容器元素直接交给 Borrow callee | 现行正例对照；不产生独立返回借用 |
| B02 | 从 Borrow 参数返回 Copyable 字段 | owned copy 对照；不能误判成新引用能力 |
| B03 | 从 Borrow 参数返回 MoveOnly 字段为普通 T | 当前非法移出；新模型不得偷换成 owned return |
| B04 | 从具名 owner 的 Map/List/字段取得结果并局部绑定 | 来源链、结果种类、结构化有效期与只读重借用 |
| B05 | 从临时 owner 取得新结果并用于一次完整调用 | 首版候选拒绝新结果；现行直接 Borrow temporary 保留正控 |
| B06 | 把临时 owner 的访问保存到后继语句 | 首版拒绝；不暗中引入延寿或 retain |
| B07 | 在两个参数中按条件选择来源 | 首版拒绝多来源；不能任意猜一个 |
| B08 | 返回指向 callee 局部 owner 的访问 | 所有候选必须防止 owner 消失后的读取 |
| B09 | 取得访问后修改/搬迁容器 | exclusive conflict 与失效必须可判定 |
| B10 | 分支、循环和提前 return 运送访问 | 单来源/兼容 selector join、控制退出、child→parent cleanup 与诊断恢复 |
| B11 | 缺失 key、nullable V 与临时 key/index | Missing/Found(null) 分开；结果依赖 receiver 存储链，定位 operand 不自动延寿 |
| B12 | 将访问存入普通字段/容器或受控转发 | 长期存储拒绝；转发须有声明级单来源合同，不可返回本地 owner |
| B13 | borrowed/move closure 捕获该访问 | 首版不支持新结果逃逸 closure；不改变现行 closure 合同 |
| B14 | 跨函数、泛型、多文件与静态用户接口 | declaration/source/analysis identity、实例替换与同一来源合同 |
| B15 | shared 与 exclusive 的嵌套访问 | 结构化独占修改、只读 child、parent 权限暂停与恢复；不任意流转 exclusive 结果 |

每个方案必须对同一组程序作答；不可为不同方案换成更容易通过的输入。
“无需显式生命周期标注”是设计约束；若无法在可接受复杂度内满足，应明确拒绝方案，
不以未证明的推断隐藏必要限制。可空访问、字段存储和开放迭代器不自动随受限返回获准。

## 4. 决策和后续实现验收

| 阶段 | 产物 | 验收 |
|---|---|---|
| 需求核对 | 实际程序、现有替代写法、资源/复制代价 | 有可复现需求；区分便利性与正确性阻塞 |
| 方案比较 | B01–B15 的完整预期、语义冲突、成本估计 | 独立评审指出最大剩余问题并处理；无未解释行为 |
| 决策 | 保留现状或最小新增范围及非目标 | 用户明确选择/启用需要的新语义；保存取舍 |
| 仅在新增获批后 | 源码规则、typed 来源、ownership loan、SSA/verifier/ABI | 正反例、Span、source identity、native 生命周期和错误原子性 |

候选研究只读入口为[type checking](../../crates/lang-frontend/src/type_checking/mod.rs)、
[ownership](../../crates/lang-frontend/src/ownership_checking/mod.rs)及
[SSA verifier](../../crates/lang-codegen/src/ssa/verify_ownership.rs)。不预定新公共类型或全局 context。
实验若有必要，须另定范围与资源预算；本次起草不运行实现原型。
当前 single place 只能 fields 后接 indexes，unit 仅 terminal element，均不能再接 field；
新核心须设计有序可组合 selector/reborrow 来源链。既有 LoanFact 是调用期产物，
不能只延长 CallReturn loan 就冒称具备跨函数借用结果、缺失分支或通用 ABI。

## 5. 非目标与调整

完整 NLL、普通对象/容器长期存借用、多来源、逃逸 closure、隐式 pinning、协程、
跨线程借用和开放 getter/setter 不作为首版包；其它视图、树、parser、IO 仅列后继。
可按实际证据增补案例或拆研究/实施 Spec；现有 element place 已够用的调用场景不新增规则。
局部绑定与受控返回的通用方向仍须作答；不能为了满足里程碑名称而强行新增语法，
也不能把直接 Borrow 正例当作该方向已完成。调整不得把负例无理由改为正例。

设计选择、B01–B15 的源码与执行、性能调查、正式编号均未完成。
文档检查与评审只记录在[计划 §11](post-governance-milestones.md#11-文档起草记录)。

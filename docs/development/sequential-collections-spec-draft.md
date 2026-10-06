# M3A 顺序集合实用化：Spec 起草材料

> **性质**：待编号 Spec 起草材料 · **状态**：draft / 操作集待选择 · **读取时机**：评审程序需要的顺序集合 API 时 · **唯一真源**：本页维护候选范围与验收；现行合同见 Guide12

## 1. Goal、前置与当前能力

Goal：交付一个由真实程序驱动的最小顺序集合操作集，调用者能够构建、访问和按所选操作
改变集合，同时保持 owner 唯一性、借用有效性及精确清理。
Phase 2/3 发布操作与所有权事实，Phase 4 消费并执行，公共标准库逻辑属于 Phase 5。
共同启动与调整规则见[计划](post-governance-milestones.md) G0–G3/§10。

[集合规范](../guide/12-collections-destructuring.md)已有 Array/List/MutableList 的角色、
size、构造、元素 place 与封闭迭代合同；实现事实见[容器存储](../architecture/unit-container-storage.md)。
必须先核对现有能力。MutableList 可增长的类型角色不等于每个增长 API 都已有封闭表面或实现。
M3A 可为[M1B](text-processing-spec-draft.md)先交付所需片段，不等待整个 M1B 完成。
现行 `use(items[i])` 的直接 Borrow 不等于可局部绑定、包装转发或继续字段访问的借用结果。
用户提出的 List<MoveOnly> 与字段/Map 嵌套访问由[M2B（原 M2）](borrow-access-spec-draft.md)
设计共同核心；仅新增结果形态依赖其 Guide/事实合同，当前构造与直接 Borrow 补齐不等待它。

## 2. 最小操作集选择

| 需求组 | 候选范围 | 决策条件 |
|---|---|---|
| 已有访问 | 构造、size、索引的 Copy/Borrow/Inout、元素替换 | 先补程序实际遇到的支持差异；不为统一命名改现行 API |
| 通用借用结果 | List<String>/Resource 元素局部绑定、只读 reborrow、字段访问与包装转发 | 复用 M2B 来源/权限/期限，不伪装普通 owned 读取或专属 getRef |
| 有界增长 | 向 MutableList 追加一个 owned 元素 | 名字、receiver/参数模式、返回值、大小上限与失败规则需封闭 |
| 有界删除 | 取出或删除一个元素、clear | 仅在程序需要时选择；区分返回 owner 与直接析构 |
| 容量控制 | 预留容量等显式操作 | 不作为首版默认需求；capacity 不是当前公共属性 |

正式 Spec 明确“本片选哪些操作、哪些已实现、哪些留待后继”。一个 API 的语义不从另一语言
的 Vec/push/get/pop 默认继承，不新增 Vec 或将 List 当隐式借用视图。
新增公共 API 先由 proposal/Guide 封闭；现行已有行为的支持补齐不需要虚构语义变更。
[Rust Vec 官方参考](https://doc.rust-lang.org/std/vec/struct.Vec.html)只用于区分 borrowed access、
owned removal 与 relocation 的语义层次，不继承 Rust lifetime/NLL 或其 API/语法。

## 3. 必须决定的所有权与 runtime 边界

- 操作的 receiver 是什么能力，元素参数何时移动，正常提交后谁拥有原值和新值。
- append/insert 对求值顺序、活跃元素 loan、迭代 provider 与 relocation 的影响。
- 容器 header、已初始化元素与容量区域分开；清理只访问已初始化的逻辑元素。
- 容量/字节数受 target DataLayout、对齐及现行 size 上限约束；增长失败不得环绕或发布损坏状态。
- 按现行 Abort 规则处理分配/大小失败，不凭空引入异常展开或可恢复分配失败协议。
- String、class/resource、嵌套容器和零大小布局按选定支持集合验证；内部合成 ZST 测试
  不等于源语言已经能够构造对应类型。
- 元素迁移不能暗中 clone/retain，操作本身是否调用用户代码与 drop 必须列清。
- 标准库公共源码与 compiler/runtime primitive 的职责先封闭；不因底层能力不足而把所有库逻辑塞入后端。
- 新结果沿 source owner→index→field 的有序投影链保存 parent loan；临时 index 只用于定位，
  不能替代 container/storage owner 作为来源。Shared 存活时不允许冲突 replacement/relocation，
  结构化 Exclusive 的只读 child 结束后恢复父权限；不由 List API 另设失效规则。

## 4. 验收矩阵

| ID | 场景 | 要求 |
|---|---|---|
| C01 | 空、单元素、边界容量、反复增长 | 内容、size、顺序及已有元素 owner 保持；独立参考序列对照 |
| C02 | Copyable/MoveOnly 元素 | 每 API 有合法交付及非法 move/borrow 正反例，类型事实与诊断 Span 可追溯 |
| C03 | 正常提交与实参提前退出 | 左到右各一次；正常更新与提交前 return/Abort 分开验证，旧集合状态按合同保持 |
| C04 | 活跃借用、迭代与增长 | 直接 Borrow 与新局部结果分开；别名/CFG/parent 恢复/正常退出清理；拒绝失效与 malformed facts |
| C05 | 所选删除/clear | 如未选则明确排除；如选则验证旧元素归属、逆序/规定顺序及无重复 drop |
| C06 | 大小溢出、分配失败、零大小 | 有界可注入失败和计数 oracle；真实 native 对应分配、存储释放及 logical drop |
| C07 | 泛型与多文件 | 具体类型替换、同名非 intrinsic、source identity 与两个入口的支持边界 |
| C08 | 公共使用与回归 | 最小调用项目与已交付 M1 程序真实 build/run；不等待整个 M1B，检查标准源码资产与所选 direct consumers，记录精确 head CI |

新增结果切片须交叉验证 List<String>/Resource、名义字段及 Map→List→字段；
用不同来源、未知 index、重复读取、包装函数/跨模块泛型、越域读取作正反对照。
可空查询协议先在 M2B 选择，普通 T? 不自动成为借用结果；不顺带启用 getOrNull。

修改前对既有选择记录结果，新行为先红测。测试身份和预期不得由新实现自身生成。
候选接入点包括 frontend `type_containers`、`ownership_containers`、`ownership_iteration`，
以及 codegen 容器/verifier/native 测试；它们是否由当前 CI 选择需实际核对，零命中不算通过。

## 5. 接口复用与实施顺序

先读[单文件容器 lowering](../../crates/lang-codegen/src/ssa/lower_frontend/container.rs)、
[unit 容器 lowering](../../crates/lang-codegen/src/ssa/unit_lower/container.rs)及现有 SSA/container ABI。
复用稳定 operation、布局和分配边界；新增 capability 必须由 frontend 发布，后端不猜来源权限。

1. 从 M1B 或其他程序冻结最小需求，列已支持/缺口/未定义三类操作。
2. 满足语义/ABI 前置，选一个可独立验收的操作集，分配正式 Spec。
3. 类型→ownership→SSA/verifier→native→公共 `.ko` 入口，按 C01–C08 选择实际适用项。
4. 运行用户程序与直接消费者，独立评审资源和失效访问，记录未覆盖表示后交付。

## 6. 非目标与当前记录

Map 实现、getOrNull、可逃逸迭代器、通用 clone、隐式容器转换以及一次支持所有布局
不自动加入。借用结果由 M2B 独立设计和启用，M3A 只承接已选择的顺序访问适配。
普通对象/容器长期存借用、多来源、逃逸 closure、跨线程/async 及开放迭代/借用视图延后。
实施时可删除已完成的工作、缩小首片或因真实依赖新增一片，但需更新矩阵和非目标。

操作集、签名/错误合同、正式编号和 C01–C08 均待确定或未运行。
本轮只有文档起草，验证统一见[计划 §11](post-governance-milestones.md#11-文档起草记录)。


## 7. C07 既有泛型容器签名首片

[SPEC-0275](../archive/specs/0275-unit-generic-container-signatures.md)从PR52合并后的最新main
承接直接Array<T>/List<T>/MutableList<T>签名的native具体替换，现行Guide已封闭语义。
新增API及其余C01–C08仍按原前置；body-only发布与递归模板没有自动启用。具体范围和验收只记Spec。

后继[SPEC-0276](../archive/specs/0276-unit-generic-body-type-normalization.md)承接普通顶层函数
body-only具体需求发布及其既有native消费者适配，保持原recursive-template边界。
该片已按双宿主实现CI验收归档，最终head及actual main交付门禁已闭环，见[交付账本](evidence/generic-body-0276-delivery.json)；两片不代表整个M3A操作集已完成。


## 8. 运行时长度构造承接

[SPEC-0279](../archive/specs/0279-runtime-length-container-native.md)承接现行Array/List构造
的两源码入口、具体callback/helper与普通native路径，按PR57双宿主及用户限定范围验收。
本片未执行的定向注入/计数、后续交付状态见[0279账本](evidence/runtime-constructor-0279/delivery.json)。
本材料的增长/删除、新API及其它C01–C08仍未批准或验收，不建立第二张0279完成表。

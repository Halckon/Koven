# M3B Map 合同与实现：Spec 起草材料

> **性质**：待编号 Spec 起草材料 · **状态**：draft / Map 语义未启用 · **读取时机**：评审键值集合的最小完整合同与实施依赖时 · **唯一真源**：本页维护候选决策和验收；不定义现行 Map 规则

## 1. Goal 与阶段

Goal：选定并交付一个所有权合同完整的键值集合切片，用于改写已经验收的词频程序，
保持其输出协议并证明键/值更新、查询、清理与扩容安全。
先进行语义设计；选择后涉及 Phase 2–5，若改变语法还需 Phase 1。
共同启动/调整规则见[计划](post-governance-milestones.md)，用户本轮授权仅为起草。

前置为[M1B](text-processing-spec-draft.md)的固定需求和输出、Map 语义明确启用及必要 ABI。
依赖[M2B（原 M2）](borrow-access-spec-draft.md)与否由查询接口决定，不预设所有 Map 都需要借用返回。
用户新增方向要求非 Copyable 查询的候选覆盖 Map<String,String> 和 Map<String,Resource>，
并与 List<MoveOnly>、字段及嵌套访问共用[通用来源/结果合同](../proposals/general-borrow-access-results.md)；
Int-only owned 查询可作对照，不能替代此方向的设计验收。
如果只选择某个更小独立应用，也需先固定可观察行为，不能同时移动应用和集合的预期。

## 2. 候选文本需先重基

[现行集合规范](../guide/12-collections-destructuring.md)不授权 Map；
[Map proposal](../proposals/map-ownership.md)只是历史候选。
该页只读保留历史，不直接采用其签名、Hashable 决定或版本声明。候选仍有调用处 `borrow`
marker 的示例，并把非 Copyable 查询连接到未定义的 getRef/借用返回；
重基时必须逐项核对现行 v0.41，不能直接搬入实现或把它当作已批准签名。

| 决策 ID | 必须封闭的内容 | 防止的隐含扩张 |
|---|---|---|
| K1 类型与键能力 | Map/MutableMap 角色，哪些 K 可用，Hash/Equal 的一致性、浮点边界与用户自定义能力 | 不从可存储、Copyable 或指针地址直接推断 Hashable |
| K2 查询 | 临时/具名 key 的查询期 loan；Missing/Found(null)/Found(value)；owned copy 与 Shared 结果 | 普通 V? 不代表可空借用；结果依赖 receiver/storage owner 链，不自动依赖定位 key |
| K3 写入/覆盖 | 新旧 key/value 的归属、返回值、求值次序和提交点 | 不把数组固定槽位替换直接套在插入上 |
| K4 删除 | key 的权限、缺失行为、返回 owner 或 drop、容器状态 | 不遗留第二份 owner 或未初始化条目 |
| K5 扩容/遍历 | relocation、loan/provider 失效、顺序与稳定性 | 不顺带开放用户 Iterator 或允许移动活跃借用的存储 |
| K6 runtime 与失败 | 表示、碰撞、溢出、分配失败、精确清理及 ABI | 算法策略与语言可观察合同分开，不新增任意异常展开 |

Map entry selector 须有稳定逻辑身份和保守 alias 判定；不能用 key symbol、hash 或槽地址
冒充条目身份。查询各 operand 一次，相等 key 访问不得绕过同一来源冲突；
临时 key 在查询后可清理，结果依赖继续保护 Map 及所选嵌套存储。
若 API 真正返回 key 内部投影，须声明 key 参数来源，不能沿用 receiver-result 合同。
Map→List→字段的权限与 child/parent 恢复由 M2B 核心统一发布，不新增 Map 专属规则。

先对 K1–K6 作方案比较，再形成 proposal/Guide 决定。设计阶段可选择只交付所需操作，
但所有可构造状态与失败路径必须闭合；明确不支持的操作不能用占位成功结果伪装。
若选用哈希随机化或宿主实现，必须说明可观察输出如何满足仓库确定性要求。

## 3. 验收矩阵

| ID | 需要验证的性质 | 对照方式 |
|---|---|---|
| K-A | 内容与更新 | 空表、重复 key、碰撞、插入/覆盖/删除序列与独立参考关联表比较 |
| K-B | 键等价 | 对批准的 K 验证等价关系和相等键哈希一致；边界按实际规则测试 |
| K-C | key/value 所有权 | String 等 MoveOnly key 借用查询后仍可用；插入后移动状态、旧值归属/释放精确 |
| K-D | 缺失与错误 | 所选查询/删除协议完整，诊断码/Span或运行结果有独立预期 |
| K-E | 活跃借用与扩容 | 局部结果/只读 reborrow/受控包装转发；别名、CFG、嵌套存储保护、冲突修改与过期/缺 facts 拒绝 |
| K-F | 资源与原子性 | 多次扩容、失败注入、正常及可恢复退出按所选合同精确处理 key/value/storage；Abort 不展开、Rc 强环按 M4 分类，所有路径仍不得双重释放或 UAF |
| K-G | 泛型和声明身份 | 具体实例、同名用户类型不能获得 intrinsic 权限；source/analysis 身份不混用 |
| K-H | 应用替换 | 同一 M1B 输入与输出，两宿主真实 project build/run；不借更换排序或错误协议掩盖差异 |

参考模型使用独立简单关联序列即可；不以相同哈希实现互相验证。
固定 seed 的操作序列及最小失败要保留。资源测试与内容测试分别断言，避免“内容正确但泄漏”。
K-H 的顺序由应用合同确定，不能无依据依赖表内部的遍历顺序。
K-F 复用[M4 的资源语义分类](memory-safety-validation-spec-draft.md#4-资源语义与失败归因)，
不能要求 Abort 执行沿栈析构，也不能把允许的 Rc 强环当作全部路径的免检理由。
K-C/K-D/K-E 同时覆盖 Int、String、Resource 与 List<Resource>/字段投影组合；
明确 receiver owner、entry/index/field selector、parent loan 和正常退出的 child→parent→owner 清理。
借用结果不承担 value deinit；缺失分支不伪造 element loan，nullable slot 与缺失须有独立 oracle。

## 4. 实施候选顺序

1. 比较历史 proposal、现行 Guide 与真实应用需求，完成 K1–K6；列 M2B/其他前置。
2. 明确启用必要语义与 ABI，冻结接口、正反例及首片支持类型，注册正式 Spec。
3. 实现 frontend 身份/类型/ownership，再接 SSA/runtime 与公共标准库；复用既有分配、
   String 比较与资源清理边界，不能仅凭源码名识别 Map。
4. 完成 K-A–K-G，再改写固定应用并执行 K-H；独立评审最大的别名/清理风险。

不预先指定新 crate、哈希库或数据结构；先核对代码/依赖/标准库，再按
[依赖治理](dependencies.md)评估确需增加的组件。

## 5. 非目标、可调整项与记录

Set、并发 Map、可变 key、Weak、通用 iterator、getOrPut/merge 和高阶集合算法
均需独立范围，不作为首版附带能力。公开 API 名称、支持键集合与首片写操作可按决策调整。
借用结果的来源核心由 M2B 承接；Map 不单独启用新返回语义或开放索引。
若某已选择的 owned 查询不需要结果借用，移除该片 M2B 硬依赖；String/Resource 借用查询
则等待对应 Guide/实现合同，不以隐式 clone 或普通 V? 绕过。长期存借用、多来源与逃逸捕获延后。

K1–K6 语义、正式编号、算法/ABI、K-A–K-H 全部待定或未运行。
文档验证统一见[计划 §11](post-governance-milestones.md#11-文档起草记录)。

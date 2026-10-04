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

## 2. 最小操作集选择

| 需求组 | 候选范围 | 决策条件 |
|---|---|---|
| 已有访问 | 构造、size、索引的 Copy/Borrow/Inout、元素替换 | 先补程序实际遇到的支持差异；不为统一命名改现行 API |
| 有界增长 | 向 MutableList 追加一个 owned 元素 | 名字、receiver/参数模式、返回值、大小上限与失败规则需封闭 |
| 有界删除 | 取出或删除一个元素、clear | 仅在程序需要时选择；区分返回 owner 与直接析构 |
| 容量控制 | 预留容量等显式操作 | 不作为首版默认需求；capacity 不是当前公共属性 |

正式 Spec 明确“本片选哪些操作、哪些已实现、哪些留待后继”。一个 API 的语义不从另一语言
的 Vec/push/get/pop 默认继承，不新增 Vec 或将 List 当隐式借用视图。
新增公共 API 先由 proposal/Guide 封闭；现行已有行为的支持补齐不需要虚构语义变更。

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

## 4. 验收矩阵

| ID | 场景 | 要求 |
|---|---|---|
| C01 | 空、单元素、边界容量、反复增长 | 内容、size、顺序及已有元素 owner 保持；独立参考序列对照 |
| C02 | Copyable/MoveOnly 元素 | 每 API 有合法交付及非法 move/borrow 正反例，类型事实与诊断 Span 可追溯 |
| C03 | 正常提交与实参提前退出 | 左到右各一次；正常更新与提交前 return/Abort 分开验证，旧集合状态按合同保持 |
| C04 | 活跃借用、迭代与增长 | 禁止失效访问；前端拒绝与后端 malformed facts 拒绝分别验证 |
| C05 | 所选删除/clear | 如未选则明确排除；如选则验证旧元素归属、逆序/规定顺序及无重复 drop |
| C06 | 大小溢出、分配失败、零大小 | 有界可注入失败和计数 oracle；真实 native 对应分配、存储释放及 logical drop |
| C07 | 泛型与多文件 | 具体类型替换、同名非 intrinsic、source identity 与两个入口的支持边界 |
| C08 | 公共使用与回归 | 最小调用项目与已交付 M1 程序真实 build/run；不等待整个 M1B，检查标准源码资产与所选 direct consumers，记录精确 head CI |

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

Map、借用返回、getOrNull、可逃逸迭代器、通用 clone、隐式容器转换以及一次支持所有布局
不自动加入。开放迭代和借用视图需要各自语义前置。
实施时可删除已完成的工作、缩小首片或因真实依赖新增一片，但需更新矩阵和非目标。

操作集、签名/错误合同、正式编号和 C01–C08 均待确定或未运行。
本轮只有文档起草，验证统一见[计划 §11](post-governance-milestones.md#11-文档起草记录)。

# ADR-0039：动态 capture 的实例来源、槽义务与释放交接

> **性质**：架构决策提案 · **状态**：proposed · **读取时机**：设计跨分支或循环的 closure capture 运输及 Single 实例见证退休时 · **唯一真源**：本提案；不选择 native closure ABI

## 状态

proposed

## 接受依据

不适用。用户要求起草剩余 ADR，不等于接受。所有推荐和待决项仍为候选，不解除现行
L0164/deferred，不启用新的语言规则、closure ABI、隐式分配或运行时引用计数。

## 背景

main `3fe1920a` 的 [ownership 快照](../../architecture/ownership.md) 与
[SPEC-0341](../../specs/active/0341-shared-lowering-core.md)已有静态 capture、条件/循环
Single 回放和有界 Unit Shared/Owned 观察。其独立两轮/实例释放 oracle 不能被两个无条件
CallReturn drop 或“类型检查成功”代替。Unit 的完整动态 capture 运输仍未闭合。

同一 lambda 在不同轮可以有多个存活实例；同一个静态 source owner 也可分别通过不同
capture 槽被引用。静态来源节点、当前 phi 槽和实际旧环境实例不是同一件事。旧环境被新
holder 替换后，若仍被子环境借用，唯一析构义务仍须存在，不能随 named binding 消失。

[ADR-0025](0025-recursive-closure-environment-handles.md)已经提议 native 递归环境句柄，
[ADR-0036](0036-exclusive-closure-capture-storage.md)已经提议独占 capture 持久根槽。
两者仍 proposed，本记录不重开对应 ABI 决策；它补足它们所依赖的 Phase 3 实例来源合同。

## 决策提议

### 1. 静态来源图与动态实例关系分层

推荐保留有限的 lambda/layout/source 图用于验证，同时发布每次实际形成、获取、转交与
释放所需的实例关系。实例身份受同轮 analysis、source-qualified callable 和形成激活约束。
跨循环或递归路径保持有限描述，不按迭代次数展开类型，不把静态 token 直接当运行句柄。

capture 形成时冻结有序槽映射：真实来源 owner/place、Shared/Owned/Exclusive 模式、
Copy/Move/Borrow effect、原始 capture 位置，以及对应实际父/子实例关系。typed capture
事实决定这些信息；后续 Move、alias、replace 和 phi 搬运已经形成的关系，不重搜 AST
或按“最新同 lambda 环境”恢复来源。Copyable capture 不取得 owner 析构义务。

### 2. Owned/Shared 义务分离，保留实际槽的消费状态

Owned Move 将子 owner 的唯一义务转入当前环境的实际槽，不能复制该义务。内部已有合法、
已验证的槽交接才记录已移空并避免重复清理；这不授权 thunk 移出 owned capture，0034
仍规定 thunk 通过环境 Shared loan 只读。未知/opaque 子来源不冒充已知空环境，
未能证明来源与析构合同的路径继续 deferred。

Shared capture 记录准确 source 实例/place 与该槽的独立 loan，不移走或复制源 owner。
多个环境借同一来源须分别追踪；结束某个槽不能一并结束同源兄弟。Borrow/Inout 参数
仍是非 owning place，不能由于实例协议获得 payload drop。

source 不再具名但仍被合法 capture 持有时，保存唯一 retained-source 析构义务，注明
实际 source 与已生效的借用者集合。每次结束以当前实例/槽定位来源，最后借用者判断
必须关联该次结束动作，不能在所有 loan 结束后重复得到多个“最后一个”。
最后 loan 结束后才能按 Guide 的真实安全边界释放；不改变 Resource 的词法规则。

### 3. 控制边、替换与调用交接

按 [0038](0038-path-qualified-owner-transport.md) 的路径化 incoming 运输当前根实例及
其选择关系。先读取全部旧来源再并行提交；未选槽、已移空槽和新一轮缺席的目标显式清空。
来源图可递归，Owned 边必须指向形成前已存在的实际实例；不能用正在覆写的 phi 地址
作为旧环境身份。Shared 关系不被当作 Owned 析构递归边。

完整 RHS 形成并获得所有新 capture 后，才按实际动作退休旧 holder 并提交新 holder。
新旧环境同时借同一 source 时不得出现暂时零借用导致提前释放的窗口。RHS Return/Abort
按现有事实处理，不凭正常替换计划执行提交或 unwind。

调用事实明确交付已经求值且仍存活的当前 callee 环境，thunk 入口绑定同一实例。
Guide 规定先求值 callee、再求值实参；须保存已求值实例及其有效存活证明，不能在实参
求值后按当前 binding 重搜或更换调用对象。当前尚无独立持有证明的 mutable/复杂 callee
继续 deferred，不用本提案改变求值顺序。借用调用不取得环境所有权；返回时恢复对应
权限和实例关系，不重新形成 capture。

### 4. 与释放序列的接合与唯一执行责任

环境清理使用已记录的实际槽与模式，保留 owned 子值、环境与被借 source 各自的唯一
义务，不预设跨角色通用次序。0034 §6 的现行 refine/SSA 合同保持：closure drop 先于其
shared capture loan end；被借 source 的 drop 则必须晚于所有保护它的 loan end。不能把
“closure 自身 drop”与“source drop”混成同一动作，具体点仍由已验证事实确定。

当前 Unit 观察文档另记“逆序 End 组必须连续且紧邻同 point 的环境 Drop”；本草案不
把观察中的 `SharedCaptureEnd`、环境 owner 退休与 SSA `Drop`/`BorrowEnd` 直接视作同义。
实施前须逐项核真实 action、cleanup 点与
refine/SSA 映射，记录同一动作还是不同层事件；若确实矛盾，停止该接线并另行决定，
不能静默改 accepted 0034。跨槽依赖及 retained 级联只有完成这份映射后才能进入
[0040](0040-complete-loan-lifecycle-evidence.md) 的路径内清理序列。

Phase 3 提供事实与独立实例回放，refine 负责机械落实，SSA verifier 独立复核。
若以后接受 0025 的 drop thunk，只能选一个位置执行某项子值释放；旧静态 captured drop
和 thunk 不得对同一实际槽双重析构。释放责任切换须在实施 Spec 中逐项对账。

### 5. 待决项与分批边界

| 需要明确接受的选择 | 本提案建议 | 不随本 ADR 自动发生 |
|---|---|---|
| Unit 的实例来源终态 | 有限来源图＋形成时固定的实例/槽关系 | 不把现有 Single 候选事实直接封成 Unit 成功 |
| retained source 证明 | 按实际来源与每次 loan 结束记录唯一义务 | 不选择 runtime counter、堆句柄或 GC |
| 第一批复杂度 | 先跨 join 存活或来源变化的非递归 Shared/Owned 环境，再循环/嵌套及递归来源 | 不取消旧 oracle，不把任意 opaque 路径放行 |
| native 递归 layout | 复用并继续审议 0025 | 不接受 0025，不改 0009 |
| Exclusive 环境 | 复用并继续审议 0036，单独接合权限 | 不把 Shared 权限升级成 Exclusive |

0025/0036 的接受不是本提案 Phase 3 独立回放的前置；但相应 native 能力在 ABI 及其
实施合同接受、完成前不能宣称可用。跨 layout owned callable join、字段/容器存储和
线程入口仍按 0034 §4 保留，不借本 ADR 顺带选择通用类型擦除方案。

## 替代方案

- 静态 lambda/owner 节点即实例：会混淆同 lambda 的两个存活环境及跨轮旧值，拒绝。
- 以当前 named/phi 槽查旧 capture：覆写会改变旧环境所见来源，拒绝。
- 用引用计数统一处理：额外 runtime 政策且不替代来源/借用/析构时点证明，本 ADR 不采用。
- 继续保留 Single 为永久唯一动态实例 checker：违背 D10 的唯一 Unit 终态；迁移期保留其
  原源与独立 oracle，直到等价能力逐项验收，不等于保留双生产管线。

## 后果与验收

收益是使捕获值、借用来源与释放义务在跨轮/跨调用后仍可追溯；代价是实例关系验证、
retained 义务与原清理动作必须统一接合，不能仅增加形成事件。

- 双 Unit 入口保留原 Single 源码及逐实例 oracle，明确列出仍缺的差异，而非只核诊断数量。
- 覆盖零轮、一轮、至少两轮不同选择、0/1/2/7 轮与 continue/break/exhaustion/return；
  每轮可变选择使用真实会变化的合法输入，不把不变条件的静态模板当运行轨迹。
- 覆盖同源兄弟 Shared、不同来源、多 holder、nested wrapper、同 lambda 不同实例、
  Owned/Shared 子环境、RHS 提前退出、旧环境仍可调用和最后借用者恰一次。
- 检查未选来源及时释放、被选来源活到最后使用、已获合法内部交接的槽不重复 drop、参数
  place 不取得 owner 义务；Resource 与纯内存生命周期分别验收。
- 错误/deferred 后撤销全部实例候选；同源码重分析与跨 source 不可替换。破损模型、
  提前/重复释放等注入只在 CI，普通源码及合法回放按现行测试政策选择。
- Phase 3 回放、SSA、LLVM/native 和精确资源计数分别记账；native 另等 ABI 前置，
  最终精确组合头双宿主 CI 和独立审查，不以任何有限矩阵替代循环不变量证明。

## 关联

- 相关 Spec：[SPEC-0341](../../specs/active/0341-shared-lowering-core.md)；具体增量须在接受后立实施合同。
- 依赖：[0038](0038-path-qualified-owner-transport.md)、[0040](0040-complete-loan-lifecycle-evidence.md)。
- 相关既有决定：[0034](../accepted/0034-refine-closures-and-callable-views.md)；ABI 候选复用 0025/0036。
- 取代的 ADR：无；不改写 0009/0016/0029/0034。被以下 ADR 取代：无。

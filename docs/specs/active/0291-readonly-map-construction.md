# SPEC-0291: 非空只读 Map 构造与插入序共享存储

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：实现或验收 Map 插入序存储与消费式只读移交时 · **唯一真源**：本片实施范围和内部布局约束；语言语义以 Guide v0.44 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P2-0291` |
| 基线 | 真实 main `37e80526e54d88bc7146792d7b6e3d3fdd99a659`，tree `f4796f4aee6007794fb430d403de581e76ef4b1e`，已正常合并 PR116 |
| 分支 | `feature/spec-0291-ordered-map-consume` |
| 依赖 | [0288 Map native](../../archive/specs/0288-map-native-execution.md)、[0330 while 流事实](../../archive/specs/0330-while-conservative-flow.md) |
| 语言合同 | [Guide12 插入序](../../guide/12-collections-destructuring.md#整数-range-与-map-的独立迭代合同)、[消费式移交](../../guide/12-collections-destructuring.md#mutablemap-的消费式只读移交)，均已启用，不新增 Guide 语义 |
| 影响范围 | frontend typed/ownership、typed SSA/verifier、LLVM Map 布局与操作、普通及 CI-only 验收 |

## 1. 重基与范围

旧 draft 的 v0.43/待启用、临时 receiver 待决定、四字段布局保持不变及 Map 插入序未纳入
等条款已被现行 v0.44 合同取代。旧正文与未核历史见
[固定 main 中的原稿](https://github.com/Halckon/Koven/blob/37e80526e54d88bc7146792d7b6e3d3fdd99a659/docs/specs/drafts/v0.44/0291-readonly-map-construction.md)，
不将旧未运行记录转为验收。本次承接用户“0330 合并后继续 F”的实施顺序，保留0291编号。

本合同先统一 Map/MutableMap 插入序存储，再闭合 `MutableMap<K,V>.consume(): Map<K,V>`。
转换只接受 owned 本地根 val/var；Borrow/Inout、字段、元素、临时 receiver 拒绝，活跃
借用拒绝；成功后源不可再用，结果承担唯一清理义务。同 K/V、不 clone、不重新装填、
不析构条目，存储整体移交。只读 Map 不获变异权限。

Map 两分量 shared Borrow for provider 按已启用 Guide 后续独立切片实施；本片不以
将来 provider 的预计行为代替当前插入链验收。没有复制式 toMap、反向转换、非空 mapOf
语法、用户 Hashable 或一般 nullable ABI 扩展。不同 entry 的整体析构顺序不作承诺。

## 2. 单一内部存储合同

本节只定义当前实施所需内部约束，不承诺公开二进制 ABI，不把布局字段加入 Guide。
Map 与 MutableMap 的同一 K/V 实例必须使用相同 LLVM header/slot 字段布局、hash/equality 与
最终 drop glue。类型权限仍由真实 typed/SSA 类型区分，不能以布局相同替代身份核验。

- 保留唯一开地址 bucket allocation 与 EMPTY/OCCUPIED/DELETED 状态；不另建顺序数组、
  第二份条目 owner 或逃逸 iterator。header 在 buffer/size/capacity/tombstones 后附加
  first/last slot index；slot 在 state/key/value 后附加 prev/next index。
- index 使用目标 size 类型；NONE 为该类型全1值，与全部有效 bucket index 区分。
  空链 first=last=NONE；唯一项前后均 NONE。容量、stride、乘法及目标布局继续由现有
  DataLayout/分配器检查，溢出或分配失败沿原 Abort 合同，不截断为小索引。
- 每个 OCCUPIED slot 在链中恰出现一次，size 等于链长度；相邻 prev/next 互相对应，
  首尾端点完整。EMPTY/DELETED 不在链中，其旧 key/value 不作为活 owner 读取或析构。
- 新 key 插入（包括复用 tombstone）接到尾部；覆盖已有 key 仅按既有 put 合同提交
  key/value 与清理，不改变逻辑位置。覆盖前若触发容量维护，维护也必须保序。
- remove 按既有 owned 结果/MapResult 身份移出值并清理 key，同时摘链；分别覆盖删除
  首、尾、中间、唯一项。再次 put 相同 key 当作新插入置末，不复活旧链位置。
- rehash 按旧插入链遍历，将原 key/value owner 搬到新 bucket，再按相同逻辑顺序重建
  全部 prev/next 和 header 端点。不能原样复制物理索引，不能按 bucket 编号重新排序；
  每个旧 occupied owner 恰移交一次，释放旧 allocation 时不析构已搬移条目。
- 整体 drop 可沿既有 occupied bucket 扫描；每个仍拥有的 key/value 恰一次，buffer
  恰释放一次。链顺序不规定跨 entry drop 顺序。consume 传递完整 header/同 allocation，
  不执行 rehash、clone、条目 Drop 或 buffer free；无活 loan/provider/capture 依赖。
  若源/结果采用不同 named LLVM struct，只允许逐字段重包同一 header，不访问或复制条目。

## 3. 阶段事实与交付

Phase2 双入口发布真实 consume 描述符：当前 receiver/root、源 MutableMap 实例、结果
Map 实例及精确 K/V；保留 trial/错误恢复与 source-unit 身份。Phase3 消费同一 owned
本地根，复用既有 move/loan 冲突及清理事实，不能在类型检查阶段伪造 ownership。
Phase4 用显式 owner 交付操作和 verifier 核输入、输出、K/V 与唯一消费；不能仅改一个
类型标签而丢弃旧 owner alias。Phase5 以共布局移交，Phase6 用真实普通源码验值与清理。
各阶段先固定真实普通红证，再小实现与绿证；前端通过不等同后端完成。

### consume 资格诊断的实现选择

在真实类型/名称前置成功后，consume 仍只接受 owned 本地 `Item::Variable` 的 val/var
根；同时核真实 Variable 身份与词法本地资格，不能把 ForBinding、全局或 own 参数当作
本地声明。额外 receiver 形状不合资格且未先产生既有访问错误时，Phase3 使用 L0130
表示本片新增操作资格拒绝；这不是对旧实现已有诊断的追认。Borrow/Inout、稳定字段/
元素、已移动根及活 loan 保留既有 Value receiver 的诊断优先级，以实际普通对照锁定。

拒绝路径仍单次检查 receiver 子树并保留真实退出/诊断，不能先成功消费外层 receiver
再回滚整个状态，也不能直接返回而吞掉子表达式错误。single 的访问校验若暂改该根
nullable-view，只恢复这一个受影响字段。unit 错误后全局清空交付事实只证明 fail-closed，
不能把空表当作内部检查先于提交的证据，内部时序仍须逐行审与后继 CI 合同守卫。

声明 storage 为 Nullable、当前已收窄为 MutableMap 的本地根，不因此新增资格拒绝。
保留真实 read 型、声明 storage、root/non-null 身份；if/when 和普通 own 参数函数近邻
分别取证，后端必须用真实 extraction/owner 交付闭合，不能把 storage 改写成 read 型。
当前结构化未支持可分阶段登记，但最终0291交付不得据此暗缩 Guide 合同。

## 4. 小片顺序与验收

1. 冻结本合同与布局消费者清单；双入口普通 consume 正例先采真实 Phase2 停点，
   近邻已有查询/put/remove 保留。旧错误与未运行状态原样记录。
2. 一致迁移 header/slot 构造、全部提取/写入、put/remove、rehash、drop；用真实普通
   产物的只读结构查询与旧 Map ordinary native 回归验收，不新增公开检查接口。
3. 闭合 consume typed→ownership→SSA→LLVM→single/project native；最后共享清单统一。
   Map 借用迭代另片，不能以本片完成宣称 F 全部完成。

必须逐项登记：
- 空表/唯一项/多项，A/B→覆盖A仍A/B→remove A再put成为B/A；首尾/中间删除、碰撞、
  tombstone 复用、扩容及覆盖触发 rehash 后链与 bucket 的可达条目完全一致。
- consume 空/非空，owned val/var、同 K/V；源后用、活借用及非法 receiver 的精确诊断。
- String/MoveOnly/Resource 的转换不 clone/Drop；最终 key/value/buffer 各恰一次。
  Copyable-only get、requireValue/withValue、MapResult 表示与原 Unsupported 边界不扩。
- 受影响的旧 Map ordinary/SSA/native 按实际 helper 反查，不按测试名猜执行性质；
  计数 shim、故障、Abort、私有变异与校准全部只交 CI，本地只编译。
- 结构查询只读真实产物；若动态链观察需要 LLVM/C 探针，则明确归 CI-only。
  实际 for provider 未交付前，不称已有公开遍历的 native 顺序验收。
- fmt/check/clippy/build、文档/尺寸检查、唯一 exact 输入锁定、完整双平台 CI；
  真实失败与 skipped/ignored/未运行单列，不用局部绿代替全库兼容。

## 5. 初始实证

- 合同与测试基线从真实 main `37e80526` 建立，生产代码尚未修改；0330 源树保留。
  `969d74e2` 重基合同，`39b0447b` / `5bedbc91` 分别加入布局和 consume 普通测试。
- 首次固定 `5bedbc91` 的两 exact 均实际命中1。consume 四源×双入口全部采到
  `Deferred(MemberAccess)`、空诊断，精确结果 Map<K,V> 判据失败，为真实类型行为红。
  布局 fixture 的 `println(Int)` 触发两处 L0084，未输出 header；此项仅为前置源码错误。
- `dd905eed` 仅将 size 观察改为比较后输出 String；同两 exact 重冻重跑，各实际命中1，
  1453份输入前后稳定。consume 保持上述八条红观察；布局四组真实 SSA/内存 LLVM
  全部完成，每组 Map/MutableMap header 均为 `[ptr,i64,i64,i64]`，最终六字段判据
  以 4!=6 失败。这只锁定 header 缺口，不证明 slot 链或运行时插入顺序。
- 两轮有限 `cargo check --locked --offline -p lang-frontend -p lang-codegen --tests`
  成功；真实测试构建与唯一命中门已核，无 native、插桩或故障测试执行。
  初始证据收据 SHA-256 为 `776033b3f2eab43f42a0c47dca16ab264cdd3c42d24887043667c47f26e15839`，
  修正重跑为 `189add5afdce16fe019c5315bc90f9fae284642cbd7276f8b8197ffde42f1e85`；
  原始日志和输入清单仓外保留，不将预期红或有限编译称为功能完成。
- 共享布局全部消费者和 consume 分阶段资格仍在独立设计审查；完整工程、旧 Map
  兼容、native 与双平台 CI 均待后续实际实现和验收。

## 6. 共享布局首片有限验收

- `867a2e0b` 统一六字段 header、五字段 slot、完整组合布局预检与全部 slot/header
  消费者；append/unlink 仅维护链，rehash 按原链搬迁并重建索引。MapResult、SSA
  所有权门和 frontend 不变。静审核对容量/stride 上界、NONE、真实 CFG 末块和清理交付。
- `6cef2cfe` 仅强化 header 查询，锁真实 Map/MutableMap 各一、相同 K/V 身份及
  Int32/StringOwner 实例。该头固定1454输入，普通24 exact 各实际命中1并通过：
  5项 SSA/内存 LLVM（含四组新 header 观察）及19项旧成功 native。覆盖原查询、
  覆盖/删除/增长、unit basic/constant、泛型回调与调用/循环退出；不按测试名猜执行性质。
  收据 SHA-256：`36f0897432e094fe0d8c9e436b634af2d42b0293e22cee9d1c923c0081edd640`。
- 同头 fmt、workspace all-targets check、frontend/codegen/CLI all-targets clippy
  `-D warnings`、CLI build 四项通过，输入前后稳定；收据 SHA-256：
  `2cf449a72e8a26faa1f8b83b055ed7d22f14a80599529de4428839c6d460dd90`。
  首次限定 rustfmt 因 PATH 缺工具退出127、首次提交因无身份退出128，均未运行测试；
  使用既有工具链和单次既定提交身份后完成，未修改全局配置。
- 旧 Map helper 静态反查253个唯一 exact，分143普通 SSA/LLVM、76成功原产物 native，
  及19插桩、6有意 Abort、8私有合同反例、1专用环境测试；后34项仅交 CI。
  本轮只执行上述24项，不能把全部静态清单或完整兼容 CI 记为通过。
- 当前 header 与旧值语义回归绿不代替动态链顺序证明。链完整性、真实碰撞/墓碑、
  覆盖触发增长与重新插入顺序的 CI-only 观察仍待实现/执行；consume、完整双平台 CI
  及最终合并均未完成。本片仅增量分支备份，保留此前真实红证。

- 后续在同生产、仅账本不同的 `a9bbd4e3` 冻结1454输入，完整已审219普通 Map exact
  全部实际命中1并通过（143 SSA/内存 LLVM、76成功原产物 native），34 CI-only 排除。
  219收据 SHA-256：`fa643d1667a1d4a364a43e3d6e636c1d5f8c00bf088c0b20d3adc4a192639c54`。
  这是已分类 Map 普通集合的兼容实证，仍不替代完整 CI 或新动态链探针验收。

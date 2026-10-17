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

## 7. consume 前端首红与动态链 CI 候选

- `e94b938f` 固定上述资格诊断选择；`e471d384` 增加三个普通 exact：用户自定义
  consume 四入口控制、九源双入口交付观察、十三源双入口拒绝观察。所有真实阶段
  先采集再断言，公共 facts 只读；nullable 的声明 storage、收窄 read 与 root 分别记录。
- 首轮三项各实际命中1，结果为0/101/101；1479输入稳定。四个同名用户方法/工厂
  控制断言通过；18条正例观察中14个 consume 在 complete 的 L0165 停止，普通
  own 函数 if 双入口与 when single 三条通过；when unit 独立报 L0084（211..217），
  仍将 receiver 读作 Nullable，ownership 未到达。26条负例均停 L0165，不能记作
  已验证资格拒绝或移动事实。所有矩阵 parse/names 无错误。
- unit when 的 else 非空事实传播为既有入口差异，保留同源近邻及最终修复责任；
  不将此 L0084 混为 consume ownership 红，也不通过排除 nullable 本地根缩小 Guide。
- `be6e1404` 加入七个 CI-only 动态链 exact：17份普通源码、240个独立期望快照，
  原产物与仅 checkpoint 入口增加只读调用的产物分别验证。实际执行尚待 CI；
  本地只编译，不能据候选或静审宣称插入链已动态验证。Clang 构建同步等待，无内部
  总 deadline；自身 fixture run 有限等待，超时或回收失败明确作为基础设施失败。
- 首次 fmt 仅 LLVM 测试模块注册排序失败，`76ba6ef9` 作2增2删机械修正；同头
  fmt 与有限 check 通过。同三个普通 exact 再跑仍为0/101/101且输入稳定，分类同上。
  两轮收据 SHA-256 分别为
  `0c98b00afe5269ba864fd1ceae34bedae784f1de7ec566c6b4be1ec0042bc67a`、
  `c78cd85c424e5cc36ceeb64b3710340063af42c465f148f6e68bfc306eb6c884`。
- 本批仅保全测试与诊断合同，consume 生产尚未实现；七项新探针和原34项 CI-only
  均未本地执行。完整工程与双平台 CI、consume 唯一交付及精确清理仍为后继验收门。

## 8. consume 前端资格与旧回归有限验收

- `870c01c5` 接入真实 consume descriptor、trial/final 身份校验与 single/unit
  ownership 本地根资格。声明 storage、收窄 read、root 与结果同 K/V 分别保留；
  无合法交付时不产生成功 receiver move，不改通用移动、drop 或 nullable when 算法。
- 字段邻控六源双入口先完整采集：旧 Value 的 bare Field 在 single 无错、unit L0132，
  this/own-this Field 双端均 L0132；首版 consume 六条均 L0130，五条优先级不符。
  `94979f48` 仅收窄两 helper 的旧拒绝路由，single bare Field 仍按新资格 L0130；
  不放开一般字段移动。首次测试注册 E0583 为零测试编译失败，`8b6082ff` 补真实模块路径。
  字段首红收据 SHA-256：
  `577be728746594e8018ab4c3f3f97828caf639f66c02d5d982641bf9b2135f6e`。
- 固定 `94979f48` / `85ad55a2`、1482 输入，phase5 五 exact 各实际命中1，
  exit 为0/0/101/0/0。68条观察中66条完整判据通过；唯一红 exact 内保留两条 unit
  when 前置失败：consume 为 complete L0165（168..182），普通 own 近邻为原 L0084
  （211..217），均未到 ownership。不能将此项记绿或据此排除 nullable 声明本地根。
  收据 SHA-256：`f23cfe60e34d9288a6ea56fc55dce004be85d4fece1dc87b5ba33f2aa99a4cad`。
- 同头43个旧 frontend exact 全部实际命中1并通过，涵盖 Map 类型/所有权/require/with、
  最后使用、single/unit 共享类型与 unit 所有权；均普通源码/公共分析，无 native 或 facts 变异。
  收据 SHA-256：`542002bb0b3d9ce3af6d15947943ecde583be86a6dc5dfa49a687ed1c6043421`。
- 同头 fmt、workspace all-targets check、frontend/codegen/CLI all-targets clippy
  `-D warnings`、CLI build 四项通过，输入前后稳定；收据 SHA-256：
  `805cc0d6d67b0860c884639302537c929dcbd0c4e3afb1b9bd16e262b3a987de`。
- 同头完整219个已审旧 Map 普通 exact 再验全部实际命中1并通过，143 SSA/内存 LLVM
  与76未修改成功 native，1482输入前后稳定；原34项 CI-only 与新七项探针均未执行。
  收据 SHA-256：`9bfa72f6d8983a1795b627f3ce0214ebe5ad9dfa5ce62154e2c3635b66dc2b34`。
- 本批仅闭合上述 frontend 类型/资格及旧兼容范围。MapConsume 后端操作、六字段
  同 allocation 交付、nullable if/when 精确 extraction/清理、动态链 CI 与最终双平台
  验收仍待；不将普通回归全绿或 CI 候选编译通过记为完整 consume 或0291完成。

## 9. 非 nullable 本地根 MapConsume 后端有限验收

- `a31d0902` 固定六源双入口首红：Int/Int、String/String 的 owned val/var、
  透明 Group 与直接丢弃结果，12条均完成真实 ownership 后在 consume 表达式报
  lower MissingFact；无前置分析失败。收据 SHA-256：
  `83fc3d26ea98397c7642a32ed0e8e5c21a5c74a29c809074bd51ef61a5f6a014`。
- `8a7ac134` 增加显式 MapConsume：真实 descriptor、本地根、声明/read 类型、
  同 K/V 与当前 Value 身份先闭合，再单次求值 receiver、交付旧 owner 并产生新 Map。
  single 使用完整 checked ownership，unit 另核真实 Move receiver fact；活 carrier
  沿严格门拒绝。SSA operation/ownership verifier、render 与 LLVM 分发同步接线。
  LLVM 仅将同一 source header 的六字段重包到结果命名 struct，不复制条目或新增
  allocation/free/hash/reinsert。Nullable storage 与 read 不同的提取仍未接入。
- 首次 fmt 仅 verifier 的 let-else 排版失败，`2ce64d31` 修正；四个旧超限文件仅各增
  一行分发，policy 只追加审阅说明，不提高额度或 baseline。原12入口随后全部 SSA/LLVM
  通过（收据 `4624f36200566b631c26f75cdaf213bc46ebe4ce4e80c002ea6d0af2325615db`）。
- `de888995` 加入完整六消费来源 fixture，single main 与真实跨文件 p/q unit entry
  分别经过 complete、ownership、SSA verify 与内存 LLVM。`6847ea00` / `27fbba9c`
  增加纯查询身份判据：真实源构造/put、唯一消费、源无后用/Drop、结果实际所有入边
  与正常路径唯一 Drop，以及 LLVM 六字段来源/同 buffer 和纯重包窗口。原六源码未改。
  stage4 首版未执行；审阅中关于 check_paths 八参数的判断已撤回（实际七参数），
  后续仅以只读身份上下文收敛两个 helper 参数，未放宽判据或 lint。
- 固定 `27fbba9c` / `02bd2fd1`、1491输入的四项 stage4-v2 各实际命中1，exit
  为0/0/101/101：前两项原12身份及完整 fixture 双 LLVM 全通过；后两项保留独立
  unit when 观察红。八条基础观察仅两条 unit 缺非空剩余域事实；六条失效/trial
  观察中失败 overload 在 single/unit 都触 complete Internal MissingRequiredFact
  （157..159），分别缺原 Error 诊断/表达式类型，不能计作 flow 行为红或改弱 oracle。
  收据 SHA-256：`455845d5caa82ccc639ed346088dcbaae3cfa693c6cd9fdf486d5c887f17ab29`。
- 同头普通 native 两 exact 各实际命中1并通过，既有未修改产物 helper 严格断言19行
  stdout、空 stderr 与正常退出；没有独立子进程 stdout 文件。此为值语义实证，
  不升级为动态链或释放计数证据；构建/运行 helper 无内部总 deadline。
  收据 SHA-256：`5a31cf9d7320163947d64471234225ab15bec338bce8454895030567b2a68568`。
- 同头原 fmt/check/clippy/build 四工程通过，1491输入前后稳定；收据 SHA-256：
  `634492d57413750e59018167e756faa86b24b72768e982b7c051c476e98a09d1`。
- 同头已审旧 Map 普通219项全部实际命中1并通过（143 SSA/内存 LLVM、76未修改
  成功 native），1491输入前后稳定；收据 SHA-256：
  `9dc5a44fd74a57c3fa07cc22b9d45075f67aac011b808903b74ffae26a88e054`。
- 本片仅覆盖上述非 nullable 本地根交付。严格 carrier 门下的其他合法组合、nullable
  if/when 的真实 owner extraction 与精确清理、unit when 剩余域及独立 complete
  错误、借用迭代 provider 后继、动态链 CI 与最终双平台验收仍待；原34项 CI-only
  与新七项探针均未本地执行。
  不将本片普通值语义和静态同 buffer 身份证明升级为0291完成。

## 10. lambda 创建作用域与失败 contextual trial 的独立收口

- `66acf472` 以四源双入口观察短路 inout、循环潜在写、lambda 创建不泄漏/不清空
  proof。八观察全部到达真实分析：六条控制通过；single 未调用的 lambda 内部 when
  非空事实泄漏到外层，raw 无诊断且 complete 接受。unit 同例则为内层未收窄，
  内层105..106与外层126..127两处 L0084；两端根因分别保留。
  首红收据 SHA-256：`a96fef03466222ee57db35c55e88a177433b64781a15ab0f89d844c3043a3130`。
- `71515105` 仅用15行统一 wrapper 保存/恢复 single lambda 创建点的 facts/versions，
  原 inner 的所有成功/错误路径均返回后恢复，不撤销 body typed facts 或诊断，不改变
  capture/return 规则。expression.rs 为1202行，尺寸 policy 只追加职责审阅，不提高额度。
  原scope与九条成功/失败重载、无 expected、capture 邻控各实际命中1：九项通过；
  scope 中 single 全判据转绿，仅 unit 原内层 when 缺口保持红。
  收据 SHA-256：`8c0b65ecd47615f351e5d0dd0d005f17a20f5e991ebe46cc733de2aad555fb5b`。
- `714920ff` 收紧 W7 complete 为完整原 L0123 相等，新增原失败 trial、成功 lambda、
  同 call 普通错误 argument、独立错误声明四源双入口。新八观察成功 lambda 双端
  通过，另六条被原 Internal 挡住；旧六观察中四条失效控制通过，W7 双端仍 Internal。
  此时额外 L0080 最终保全尚未通过，不把已确认源码/原错误身份当成 complete 已绿。
  收据 SHA-256：`e37e5e55f88963197a20f14520f51aa24760bd273c1220f2c13171c5977953c0`。
- `0769cc71` 仅修共享 final-admission 的 Call→未提交 lambda 消费边界：真实 Name
  源函数多候选、原 typed 精确 L0123 callee span、本 Call Error 且无 selection、
  Group/Lambda 未提交类型资格共同成立时，才不将已回滚的 lambda body 当必需值 root。
  其余实参、callee、type arguments 与独立声明继续遍历；不捕捉 Internal、不补 facts、
  不放宽 L0124。局部 Symbols/Group 分支没有本批专项行为证据，不据静审外推全部覆盖。
- 同头 `0769cc71` / `83d534dd`、1493输入，13个普通 exact 各实际命中1并通过：
  新八与旧六观察完整通过，原 L0123 全量相等，普通 argument/独立声明的精确 L0080
  保留，成功 lambda 与11项旧准入/重载邻控保持。收据 SHA-256：
  `5fcd9a54c046b961b209648665b72161954d0f797dc6fb966a461e2e22da58a4`。
- 同头定向 fmt、frontend all-targets check 与 clippy `-D warnings` 三项通过，
  输入前后稳定；本批未重新运行全 workspace/codegen 工程或 native。
  收据 SHA-256：`0a6eb7d5ead0996f90efa7540ee98281a2be82b30f3c6ec768023fb234ad16b8`。
- 同头将先前scope10扣除已在13内的两重载控制，剩余八项重新验证：各实际命中1，
  scope 为101、其余七项为0；scope 八观察中七条完整通过，唯一 unit 内层 when
  缺口仍为原两处 L0084。1493输入稳定；与13合为21唯一普通 exact，20绿、1既有
  unit 行为红，不能称全21绿。收据 SHA-256：
  `e82fd3d759be3917673e123acd9da3111a9d35b6d905d3768a6de898e0c05ed0`。
- 本批不补 unit when 剩余域或 nullable owner extraction，不新增成功 lambda/call facts，
  不据普通源码正负控制声称私有损坏事实门已有 CI 反例实证；这些守卫与动态链、
  provider、完整0291验收继续分别推进。

## 11. unit when 剩余域与创建点 flow 隔离的有限收口

- `c6b7a03d` 复用既有 single 的 Boolean? null-first 与 null-last/comma 两份源码，
  新增双入口四观察，原七源和全部判据保留。首红四条均到达真实分析：single 两条
  通过；unit 仍读取 Nullable(Boolean)，缺真实 NonNullUse，分别报70..71/82..83
  与62..63的 L0084。没有 Internal，不能把 positive complete 的失败另算保全通过。
  收据 SHA-256：`96c8aef3292ca3d21e50bf7cb8a67f5132fa8aea4df854b1b3768ba4fae9d2d2`。
- `a4bb128f` 仅为 unit checker 的 facts/versions 建立成对状态：when 区分条件 miss
  的剩余域、选中 body 与可达出口；subject 版本见证仍有效时才授予非空事实。逗号
  匹配路径取交集，写入即使此前无 fact 也推进失效版本，未知域保持保守。
  lambda 创建、独立 callable/deinit 与 trial 同步隔离/恢复，保留 body typed facts
  和诊断；enum else 不额外增加单 case 精度。没有新增公开 when plan 或后端提取。
- 固定 `a4bb128f` / `de952ff5`、1493输入，17个已审普通 exact 各实际命中1并通过。
  原七源14观察、scope八观察与 finite四观察完整通过；旧 unit Boolean/enum 有限域、
  重复/else/分支诊断、nullable/while/短路失效及 W7 邻控保持原判据。
  收据 SHA-256：`248e4a366f6de36b0b56a8eb3d542d527e3407b13f8f5058d5e726bbffbe5875`。
- 定向 fmt 与 frontend --tests check 预检通过；磁盘余量不足后暂停新编译，正式
  frontend 工程三项及原 consume 五项虽已准备清单，当前均未执行，不据17项通过
  声称完整工程或 consume ownership 已重验。
- 本批17项是直接风险邻控，deinit/initializer 隔离与版本饱和尚无独立动态专项，
  null-middle+else 仍无新增 unit 实证。nullable Map storage、真实 owner extraction、
  unit 后端 when、动态链 CI 与借用 provider 继续分别验收，不据此声称0291完成。
- 空间条件恢复后，在 docs-only 后继 `0bc04e4a` / `2fb07a01` 重冻1493输入，
  原 frontend 工程三项实际全通过；收据 SHA-256：
  `a94b36c0390c58f2f0b67ee5a9c64a8464d326caab586419f1fc0d9d027ad34a`。
  原 consume 五 exact 各实际命中1并通过，输入前后稳定；收据 SHA-256：
  `227a59f8de8d8f76b4fb40577c54267e97cf04209ff0eb5c6bc99d0ad95942dc`。
  68观察通过由冻结 helper 的完整聚合判据及各 exact 成功证明，绿日志没有逐条观察
  输出。原 unit nullable-local-when 与 ordinary-own-when 两例现经过真实 raw、complete、
  ownership 全部判据；这只闭合前端前置，不证明 unit 后端提取或 nullable native。

## 12. nullable Map 表示、只读 view 与消费提取的分层首红

- `04e3aaa3` 仅新增四个普通测试模块与父注册，八份源码分别使用独立 single/unit
  分析与 Program。三个 exact 各先收齐本层6/4/6条，再由串行 runner 收齐16；
  A1/A2 的纯 nullable 声明不共享 A3 真实 nested MapRemove 的类型预注册。
  首次 fmt 仅因父模块排序失败且未进入编译，修正后 fmt 与有限 check 通过；没有生产改动。
- 固定 `04e3aaa3` / `25df7cf9`、1497输入，三项各实际命中1并为101，16观察完整。
  所有 input oracle、真实 raw/complete/ownership 均通过；A3 的普通 nullable 声明与
  真实 put/remove 三来源同源路径双入口通过 SSA verify/内存 LLVM。其余14条均停在
  lower：独立声明仍缺普通 nullable storage，后续 view/consume 尚未进入对应证明或
  提取行为，不能据此断言这些后门已有正确性实证。没有前置/harness失败。
  收据 SHA-256：`56f98a84f8b1a6e67fd8683e93ddf6034d2b8d54c804f0a1923c288233af23b7`。
- A3 只覆盖一种声明/producer 顺序，不能声称 canonical 注册顺序独立；反序与多源
  顺序对照仍须后继验收。Map-only nullable storage、ordinary-if producer、唯一 owner
  extraction 分片推进，保留既定 Guide 资格；本轮没有 native 或 CI 探针执行。

## 13. Resource-bearing Map 的旧能力与预注册对照

- `34b15d14` 新增四份真实源码、双入口八观察：独立 remove-only、ordinary nullable
  声明在 remove 前/后、独立 null-only。各自独立分析，真实 Resource deinit、精确
  Map K/V、remove→`!!` 的 Consume plan 和入口来源由公开事实查询，未修改产物。
  固定1501输入，collector 实际命中1并通过，八条 input/FE/ownership 全成立；
  collector 成功只证明采集完整，不是八种行为均支持。
  收据 SHA-256：`4e5cc8aa4161663b5ae43052950cd4b5ce9f076cfab2c756ca9076332573864f`。
- single 的 remove-only 及两份混合声明源均真实通过 SSA/LLVM；unit 前两类仍在
  `outer.remove(1)`（remove-only、声明置后）或 `null`（声明置前）报
  UnsupportedNode。独立 null-only 两端均拒绝。
  没有 verifier/LLVM 失败。这证实不能以非 Resource 总门退化 single 既有能力；
  unit 保留完整 deinit planner 原资格，不能用简化 boolean 代替。
- `3955c885` / `47af8b31` 将八项真实入口与成功或 lower kind/span/path/text 固定为
  独立旧能力 exact，实际命中1并通过，八条 matched=true，1501输入前后稳定。
  收据 SHA-256：`e0f58b26af75d7ff4fc7c3e8f009c6f41431288b18b7132aa0d44acef0c4c4e6`。
  single 独立 null-only 是 A 层拟推进项，后续改预期须显式审查；本轮仍无生产改动。
- 源码声明先后不等于真实 mapper 注册反序，两端预扫描可能始终先登记 remove。
  真正注册反序、普通 nullable Map 表示闭包、view/consume 提取、native 与动态 CI
  继续分别验收。本轮仅有限 codegen --tests check 与两个普通 exact，不作整体完成结论。

## 14. Map-only nullable 表示与类型需求规划

- `bcdcd44d` 引入独立 Map-only nullable 注册表：普通 nullable 与真实 MapRemove
  共用 canonical storage，物理表示为 presence 加原六字段 Map header；操作来源资格
  与旧 MapResult 注册表分开。Null/Wrap/Branch/Take、封装检查、条件 drop、LLVM
  与两 mapper 同步；没有泛化其他 nullable payload。single 保留完整 Resource/KV
  资格，unit 仍先经过既有完整 planner。有限 codegen --tests check 通过。
- `1c9a4de1` 独立将 Resource R3/single 的拟推进期望改为 VerifiedLlvm，其余七项
  旧成功或精确拒绝均保持。首轮 fmt 只在测试 println 换行失败；`5ece3e02` 修正，
  旧 v1 清单未执行，v2 fmt/size 通过。
- 固定 `5ece3e02` / `bbdb7c6d`、1501输入，A/B/C 与 Resource 四项均实际命中1，
  均为101，所有观察及输入条件完整。A1/unit 新增真实 null declaration SSA/LLVM
  成功，A3 双入口继续成功；A1/single 与 A2 双入口仍在普通声明需求规划处拒绝。
  Resource 原三成功与四 unit 精确拒绝保留，仅 R3/single 拟推进期望仍失败。
  B/unit 已到 InvalidSsa，须独立定位；C 仍有声明或 consume 提取拒绝，未计完成。
  收据 SHA-256：`334d0d0a133ed3985600c1eca3382f1bd4d96fd363816d30ecc6d6007be304bd`。
- `0a18dc62` 先在未补需求的头采 return 后 `MutableMap<Int, Int?>?` 死尾，
  两端真实 SSA/LLVM 成功；`081e0487` 冻结原来源、entry 与双成功断言。
  收据 SHA-256：`c8417d411458b5491ea26ff35502fc07373ff2b4ae9330e35b45c90c07e52f30`。
- `9db446e4` 仅在 planned body 首条直接 val/var、直接 null 或真实零参 Map 构造上
  补精确声明 storage 需求；原 nominal walker 不变。固定1502输入，五项各实际1，
  exit 为0/101/101/0/0。A 的六入口全通过 SSA/LLVM，Resource 八项精确表全部匹配，
  R3/single 推进成功且四项 unit 旧拒绝保持；死尾双入口继续成功。B 四入口均为
  InvalidSsa/spanNone；C2/single 仍在后续 seed 初始化声明149..195拒绝，
  其余五条 C 在 consume 处 UnsupportedNode，分别保留为后继缺口。
  收据 SHA-256：`72e49d99386809bf96aedeeb6cb9000a80538a087fef7c9d22601a82a67b20c3`。
- 首条声明仅是根因验证的中间范围，不能成为“首句才支持”的最终语言合同。
  后继由本片继续收口为基于真实可达/受验初始化事实的可组合需求，普通前置语句
  不应任意改变支持；不能将完整 typed 表扫描当可达性证明。
- 本轮仅有限 codegen --tests check、fmt 与尺寸检查；未重跑完整工程或旧219回归。
  注册反序 CI 对照当前仅仓外候选静审，尚未安装或行为验收。普通 if 的当前
  view/owner 身份、consume 提取、native、完整旧回归与动态 CI 仍分别验收，当前不作 A 层或0291整体完成结论。

## 15. Nullable Map 只读 view 与组合 local 首证

本阶段保持 Map-only N/R 表示及来源权限；不扩大 `consume`、其他 Map 操作或 Guide 资格。

- 原 B 的四个 `InvalidSsa` 经唯一普通 exact 的只读诊断采证：single 已有真实
  `NullableBranch`/Shared Map view，但 `MapSize` 错取 nullable owner；unit 则为普通
  Boolean 条件分支，尚无 view。首次 trace 预检因 `Program` 无 Debug 报 E0277，未执行；
  改用既有 SSA renderer 后采齐四条 OperationContract，随后独立撤销 trace。
  收据 `c9691b799a120e9b11a0a0a02003274436c0a360d85271b923d00e1c30535818` 保留原始错误与 SSA，不能将诊断打印作为生产修复。
- single `af973d90` 仅在 MapSize 中校验真实 descriptor、root、declared storage、
  narrowed read/KV 与当前 owner/proof.loan，使用当前 view 并沿原 CallReturn cleanup；
  不改其他 Map operand 或 Consume。原 B 两入口 SSA/LLVM 转绿。
- 新 B3 独立源码在 size 读取后将原 nullable owner 交给 `kept`，原 A/B/C 分区不动。
  `493a4a99` 首证 single LLVM 成功、旧 unit InvalidSsa；收据 `bdfcdb77…e565`。
- unit `10286687` 独立限定实际函数 body 顶层 If/Group 与 N Map 的普通只读 size，
  由真实 NullableBranch 按极性产生 view；校验、兄弟分支恢复、condition cleanup、
  proof retirement 沿当前实体处理。旧 While producer 门不扩；未知 carrier 仍拒绝。
  stage6 收据 `8aee8eb2…5948`：六 exact 各实际 1，退出 `0/0/101/0/0/0`，
  1505 输入前后稳定。原 B 四观察及 B3 双入口六条均完成 SSA/LLVM；A 六、Resource
  八精确匹配及 dead-tail 双 LLVM 保持。C 六条原失败文本逐字保留，其中 C2 single
  仍停声明 149..195，其余五条停 consume；不称 nullable consume 已闭合。
- B1/B2 的真实 MapSize loan 在原 CallReturn NamedDrop 处 End→Drop；B3 两臂只运输
  owner，view 在 If 结束，join 后 `kept` 的 AfterStatement 唯一 Drop。此前已按 raw
  独立人工核验。`43a38d34` 增加六路径纯查询，锁真实 FE construction→MapConstruct→
  NullableWrap→当前 owner/view、精确 MutableMap<Int32,Int32> 及每条 End/Drop；
  首次 check 的四个 E0308（builtin 返回 Option）由 `30be5ae3` 两处结构化解包修正，
  原失败日志保留、未执行错误版本。唯一 exact 实际1通过，六条 identity verified 齐全。
- 注册反序 CI2 已安装并编译，真实 mapper 冷启动双顺序、N/R/KV/Resource qualification
  留 CI 执行；本地未运行。`b290edbf` 的原工程四项与旧 Map 219（143 SSA、76 普通
  native）均实际通过，收据分别 `48566a53…d5c8` / `56aaf89d…37bbe`。
  这些受验身份早于后续 B 生产，不冒充最新头回归。
- 组合 local 需求的 9 源/18 入口在 `ece1c860` 普通 collector 首证完整采齐：
  收据 `8d832383…5f72`，实际 1/exit0，1509 输入稳定，18 个 input oracle 均空失败。
  实际 9 LLVM/9 lower，无 verifier/LLVM 拒绝。K1/K2/K5/K6 双入口及 P2 unit 为
  九项既有成功；P1 双、P2 single、P3 双为五项待推进。K3/K4 的两端四条拒绝保留
  原 kind/span；其中 unit MissingFact 是历史缺陷，不立为永久能力合同。
  Collector 通过仅表示前置与采集成立、K1 双成功保留，不表示其余源码行为全绿。
- P2 实际仅 delivered 有 NamedDrop，seed/source 无自身 Drop；K4 由真实 scope/query
  锁 inner 属 Lambda、action 属 main，不能以整体 lower 拒绝代替 planner 隔离证明。
  新 formed-owned-local API 及可组合类型需求尚未实施；首句需求仍是中间桥。

- 后继同头 `30be5ae3` / `6f63c0fd` 的1640输入包含完整CLI闭包；工程四项全0，
  原 A/B/C、Resource、dead-tail、B3、identity 与 formed collector 共八 exact 各实际1，
  退出0/0/101/0/0/0/0/0。C六条停点原样；identity与formed分类再次成立。
  工程收据 SHA-256：`21ce227b1bb459d1a68e1e59a3c2e145e4534ec0c3f84da39aeb226d79b68f4d`。
  八项收据 SHA-256：`4743c9e6838e7cc0ea87f42c48e39c530b15bcd535e6a6dabb5df63825cc5cad`。
- 同头277旧普通回归全部实际各命中1并通过：旧Map219 + 0330的42普通lower/
  12普通native + 既有consume4，三元组无重复；共187普通SSA/内存LLVM、90普通native，
  不含故障、插桩或私有guard。原42边界分类仍由既有oracle锁定，未开放原拒绝合同。
  收据 SHA-256：`ec93bedb52a1c8477530dde1f96afa24e68e89dc2f679aa7349416030c5bed57`。

CI7 动态链、CI2 注册反序、If/N 私有拒绝门、nullable consume 提取及 provider 后继仍待；
本阶段不作 SPEC-0291 完成或合并声明。

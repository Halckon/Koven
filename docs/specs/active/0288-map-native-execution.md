# SPEC-0288: Map 键值容器原生执行基础（SSA 原语、LLVM IR 代码生成与 Native Runtime 哈希表）

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或评审 Map 容器 SSA 原语、LLVM 代码生成与原生哈希表运行时时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P2-0288` |
| 所属 Phase | Phase 1 普通借用语法；Phase 2 类型与来源合同；Phase 3 所有权延续与终止；Phase 4 typed SSA；Phase 5 LLVM；Phase 6 native 集成 |
| 语言规范 | 现行 [Guide v0.42](../../guide/README.md)；[集合、索引与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 2026-10-07 12:24 UTC 用户明确批准普通借用返回、显式 borrow val 与受检查的作用域访问；[启用账本](../../archive/migrations/v0.42-enablement.md) |
| 前置 Spec | SPEC-0287 M2B 通用借用合同冻结与 Map 键值容器类型系统基础已合入 main |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md)、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 关联 ADR | [ADR-0029 普通借用结果延续](../../adr/accepted/0029-ordinary-borrow-result-continuation.md)，局部扩展 ADR-0016 |
| 阻塞项 | §2.12 已实现借用 parent 的直接 alias carrier 与 pointer-like nullable Map 实参提升，聚焦/frontend/门禁与最终过滤 codegen 全库通过；owned root/只读字段的直接 initializer carrier 尚未接通，复杂 CFG/更多返回 ABI 延后；非空 readonly Map 转换 API 未批准；完整 sanitizer 验收依赖 Linux，本轮故障类操作不执行 |
| 影响范围 | `lang-frontend`（Typed 容器描述符交接）、`lang-codegen`（typed SSA 模型与 verifier、单文件/编译单元 Lowering、LLVM 布局与 Native Runtime 哈希表生成、端到端 native 测试） |
| 语言语义变更 | 是，用户已明确启用 v0.42；原通用 Map runtime 与 owned remove 成果保留 |

## 1. Goal

基于 SPEC-0287 建立的 Map 前端类型系统与规范，实施 Map 键值容器的完整后端执行管道：
1. **Typed SSA 模型与 Verifier**:
   - 定义 `MapContainerKind` (`Map`, `MutableMap`)；
   - 在 `SsaTypeKind` 增加 `MapContainer { kind, key, value }`，归属于 `Ownership::MoveOnly`；
   - 增加 typed SSA 操作：`MapConstruct`, `MapSize`, `MapContains`, `MapGet`, `MapPut`, `MapRemove`；
   - 在 verifier 中校验各 Map 操作的类型约束、操作数所有权与 Inout 语义。
2. **前端描述符交接与 Lowering**:
   - 在 `lang-frontend` 的 `TypedFile` 与 `CompilationUnitTypedBodies` 中发布 Map 描述符；
   - 在 `lang-codegen` 的 `lower_frontend` 与 `unit_lower` 中实现 Map 构造、属性读取、方法调用及下标读写脱糖的 lowering。
3. **LLVM IR 生成与 Native Runtime 哈希表**:
   - 内存表示：以堆分配条目缓冲区实现开地址（Open Addressing）线性探测哈希表；
   - 槽位布局：每个槽位包含状态标志（`Empty`, `Occupied`, `Deleted`）、`key` 和 `value`；
   - 哈希与相等判定：内建支持 `Int`, `Boolean`, `Char` 及 `String` 的确定性哈希计算与相等性判定；
   - 动态扩容与再哈希：负载达到 75% 时触发自动扩容与再哈希；
   - 精确零泄漏析构：Map 作用域退出时遍历活跃槽位，对 MoveOnly 的 key（如 String）及 value 分别调用 drop glue，随后释放条目缓冲区。
4. **端到端原生可执行文件测试**:
   - 支持 `mapOf()`、`mutableMapOf()`、`m.size`、`m.contains(k)`、`mm.put(k, v)`、`mm[k] = v`、`mm.remove(k)` 单文件与编译单元原生编译执行。

SPEC-0287 已在前端类型系统和所有权规划中确立了 `Map<K, V>` 与 `MutableMap<K, V>` 的规范与约束。
此前本分支记录以下较新设计主张，但与已合入 SPEC0287 的显式局部借用合同不一致；以下是历史未启用主张，已由 v0.42 的正式批准取代，不作为当前实施依据：
- **参数**：保持 `borrow x: T`（修饰符在名字前，类型不变）；
- **返回值**：使用 `: borrow T from receiver`（函数结果位置，修饰符在类型前），二等类型（仅限函数结果，不可嵌套在泛型实参或集合元素中）；
- **局部变量**：`val s = names.first()` 依靠推断，不写标注，由 LSP inlay hint 显示 `: borrow T`；
- **可空借用结合性**：第一片规定 `borrow V?` 等价于 `(borrow V)?`（可选的借用：条目存在时借用已分配槽位，不存在时为 null）；拒绝“借用可空存储”的形式。

初始实现缺口已由前三轮通用 Map 存储、Copyable 查询和 owned remove 首片补齐；历史验收见下。
当前新增范围是普通只读借用结果、显式绑定、真实 caller/source continuation 与 scoped Map 访问。
不启用条件借用结果、隐式 val 借用、inout 局部/返回、多来源或任意借用存储。

## 2. 2026-10-07 正式批准后的首片合同

普通结果及绑定的语言唯一规则位于 Guide，不以旧 proposal 或 Spec 示例替代。
`get`/下标保持 Copyable 按值身份；新 `requireValue` 对所有可存储 V 使用确定 shared loan，
`withValue` 通过成功 callback 与 Boolean 区分缺失。两者的名字分别表达存在性要求与
作用域访问，避免同名方法按 Copyable/MoveOnly 改变所有权合同。
nullable V 可以存储；callback 的 true 且 payload null 与 false 的 Missing 是不同路径。
旧查询／remove 的非 nullable V 能力按原合同保留。

第一闭环先贯通单文件普通 Borrow 参数来源、显式 borrow val、包装调用及真实 loan 终止；
随后覆盖 unit、Map real slot 与 scoped callback。每层消费上层事实，复杂未支持路径明确拒绝。
受限 last-use 需要独立 loan 证明，owned ASAP 不能代替。所有新结果在最终冻结源码上重新验证。

| 当前验收项 | 对应目标/证据 | 本次批准后实际状态 |
|---|---|---|
| B1: 普通返回语法、唯一 from、显式 borrow val；拒绝 borrow var/隐式绑定/条件结果 | parser/type borrow-result 正反例与诊断 Span | 前一阶段 parser 4 通过；本轮 type 3、全局绑定 parser L0076/Span 和显式局部 continuation 均通过，复杂路径仍封闭 |
| B2: callee origin 与 caller source continuation；嵌套普通包装、移动/变异冲突、恢复权限 | frontend ownership 与 unit 合同；SSA model/verifier/lowering | 前端四测通过；single/unit 来源和终止已验证；单参数现行 storage 类型的表达式返回、字段投影及 wrapper 已接通 SSA/verifier/LLVM/native，其他 ABI 形状未接通，见 §2.6 |
| B3: 通用 K/V 的 requireValue 真槽位借用与缺失 Abort | 正常单文件/unit native、真实 loan/slot 与清理回归 | String、MoveOnly inline、Resource、现行 nullable V 的 single/unit 命中及独立 missing native 已通过；receiver/source/end 和 verifier 合同见 §2.7；非空 readonly Map native 命中未单独验证 |
| B4: withValue 非逃逸、冲突访问、callback 局部退出后权限恢复 | 诊断/Span、真实 callback 调度与 native | single/unit 通用正常 native、精确清理计数和纯 SSA 合同均通过；源码合法的非逃逸/Map 失效负例通过；当前全库与边界见 §2.9 |
| B5: nullable V 存储；Missing 与 Found(null) 独立控制流，普通确定借用 nullable payload | type/ownership/SSA/native 三态矩阵 | requireValue 的 missing Abort、present-null/present-value 槽位借用已有 single/unit native；withValue 实际 null 比较三态 single/unit native 通过，unit 普通 pointer-like null 比较已接通；旧 nullable get/下标由 §2.13 前端明确拒绝，nullable owned remove 仍拒绝 |
| B6: 保留既有通用 Map、Copyable get、owned remove 与资源清理行为 | 原 37 条聚焦及重新冻结的过滤全库 | §2.11/§2.12 保留旧冻结片证据；最新 §2.13 borrow 222/Map 66 通过，过滤 codegen 全库 1163 passed/0 failed（library 1157、integration 2、doctest 4）；旧 nullable 查询与 owned remove 边界分开验证 |
| B7: fmt/check/clippy、尺寸、文档与必要下游检查 | 串行门禁及冻结 SHA | 最新 §2.13 check/clippy/fmt、尺寸（965 Rust / 41 历史超限）、docs 581 通过，frontend 全量 1898、过滤 codegen 全量 1163 passed；§2.11/§2.12 的历史门禁与安全脚本 84 证据保留 |
| B8: 受限 last-use 的独立 loan 证明 | scope/return 与 last-use 分开验收 | §2.10 直线局部的独立未来使用与真实 parent 链已接入 single/unit checker/drop/SSA；§2.11/§2.12 保留 alias 与多别名/子 loan 清理证据；最新 §2.13 稳定 owned-root/字段 carrier、borrow 222、frontend 全量 1898、过滤 codegen 全量 1163 通过，复杂 CFG/NLL 尚未实现 |

严格排除原四项故障生成／注入／编译／校准入口，不运行间接触发的聚合脚本。
Mac skipped reasons、partial 与 requirements_met=false 保持，Linux 动态闭环不运行。

### 2.1 2026-10-07 批准后 parser/type 检查点

已冻结 v0.41 原页及来源 SHA，启用 v0.42，并登记 ADR-0029。普通返回 AST 独立保存
borrow/from marker 与唯一来源；借用不进入可嵌套 TypeRef。共享签名 resolver 拒绝 owned、
缺失和非 receiver 来源；单文件与 unit 的 callable/call descriptor 运输声明来源，
命名实参与参数索引映射已有回归。这些是声明合同，尚不证明实际返回值的 origin。

`cargo test --locked --offline -p lang-frontend --test parser_borrow_result --test type_borrow_result
--test parser_declaration --test parser_block --test parser_lambda --test parser_file`：104 passed，
0 failed；另 `--test parser_implicit_unit`：7 passed。日志为
`/tmp/koven-0288-approved-parser-type-checkpoint.log` 与
`/tmp/koven-0288-approved-implicit-unit-checkpoint.log`。
`cargo check --locked --offline --workspace --all-targets` 的 AST 调用方修复后重跑 exit 0，
日志 `/tmp/koven-0288-approved-workspace-checkpoint-rerun.log`。
`cargo clippy --locked --offline --workspace --all-targets -- -D warnings`：exit 0，
日志 `/tmp/koven-0288-approved-clippy-checkpoint.log`。

`cargo test --locked --offline -p lang-frontend --test ownership_borrow_result`：0 passed、4 failed，
日志 `/tmp/koven-0288-approved-ownership-checkpoint.log`。实际来源、嵌套返回、源权限阻止/恢复、
普通 val 的 L0163 均仍待实施，红测试保留；未接线结果与绑定的 L0164 防护不计作行为验收。
下一步必须以真实 origin、caller/source continuation 和 scope-end facts 替换该防护，
然后接 typed SSA/verifier、LLVM 和正常 native。`requireValue`、`withValue`、nullable V
三态及非逃逸 callback 未实现；旧 non-nullable V Map 成果保留。

文档检查：581 Markdown 通过；安全限定的 `test_check_docs` 与 `test_check_rust_sizes`：
84 passed。尺寸检查失败的 9 个路径均在现有超限文件：single ownership checker、lambda
expression dispatcher、single callable checker、unit bodies/calls/model/signatures、single
type model 与 parser_declaration 测试；必须逐项按尺寸政策审阅增长或进行合理职责拆分。
本检查点未提高 baseline 或例外额度；不把旧尺寸绿灯用于新源码。

本次按父线程要求收束检查点，未运行新源码的完整 frontend/codegen/native 长测试；
上一检查点 1114 个 codegen 普通用例通过仅证明上一份冻结源码。

### 2.2 普通返回来源与尺寸整改的分层检查点

以下为 2026-10-07 14:26 UTC 的前一检查点，后续结果见 §2.3。

本阶段验证实际 Borrow 参数来源、稳定投影、generic/nullable 存储和 typed 参数映射的
包装返回，并发布封闭 `BorrowReturnOriginFact`。单文件与 unit 共同使用来源追踪规则，
分别保存自己的 ID/target。错误或 deferred 分析清空新来源事实。owned 交付报 L0163，
未证明的 caller continuation/绑定保留 L0164；后端普通结果 ABI 仍明确拒绝。

该检查点的 `ownership_borrow_result`：2 passed、2 failed，日志
`/tmp/koven-0288-phase3-ownership-four.log`。实际来源错误与 owned 绑定两项已通过；
源 loan 延续、L0135 移动冲突和 scope-end 权限恢复仍失败。第四项旧 fixture 曾被
`println(Int)` 的 L0084 挡住，未到所有权层；本阶段改用类型合法的 Int 消费函数，
保持原 L0163 断言，未以更改预期掩盖缺口。

来源正反例、相关 type/ownership、Map 和 parser/shared unit 最终回归：400 passed、0 failed，
含新增诊断 Span 用例，日志 `/tmp/koven-0288-phase3-frontend-final.log`。合法单文件/unit
声明在 ABI 未接通时的结构化拒绝回归为 2 passed、0 failed，日志
`/tmp/koven-0288-phase3-backend-boundary-passed.log`；最终状态及命令由本阶段
`/tmp/koven-0288-phase3-checkpoint.results.json` 记录。

上一阶段 9 处尺寸增长已按完整职责提炼：声明遍历、lambda grammar、候选合同、三个
callable/call descriptor、callable 签名采集、函数 AST 场景；新产生的 unit 接线增长通过
loan facts、flow use modes、receiver context 与完整 unit orchestration 模块收敛。
policy/baseline/max_lines 未提高。尺寸检查通过（915 Rust，42 个历史超限），日志
`/tmp/koven-0288-phase3-size-final.log`；后加测试不扩张超限文件。

仍需真实 caller result/source loan identity、父子依赖、作用域结束与 drop 次序、single/unit
同步事实及反例。不得通过将 borrow initializer 改为 Read 或删除 L0164 假装四测闭环。
最终 workspace check/clippy/fmt、文档和尺寸均通过；保留 Map 回归 37 passed、0 failed，
日志 `/tmp/koven-0288-phase3-map-final.log`。完整命令、后端拒绝边界和实际状态由检查点
JSON 记录；完整 codegen 和故障/校准操作不运行。

### 2.3 caller/source continuation 与词法 scope-end 首片

以下为 2026-10-07 15:26 UTC 的检查点；本轮收尾结果见 §2.4。

本阶段仅收束前端剩余两项 ownership 失败。single/unit 发布独立结果绑定、真实来源
lease、原调用 loan identity、父绑定及实际终止边；结果没有 owned payload。scope/return
按子结果、来源/父依赖、owner drop 的顺序清理。原来源 loan 在正常 CallReturn 交接，
无关参数结束；initializer 提前退出只结束实际调用前缀，不结束未建立的结果。

原四项在最新检查中全部通过；新增 continuation 套件覆盖别名/reborrow、If 分支、来源 move/变异/
exclusive 冲突、scope 与 return、named mapping、generic/nullable/Copyable、payload 不析构、
父子终止顺序和 initializer 早退，并保持复杂控制、循环和 capture 的 L0164。
前端结果 ABI 仍未接入 SSA/LLVM：合法声明及无 borrowed-return 函数的 stable borrow val
均由后端 guard 拒绝，single/unit 实际 marker Span 有正常源码回归。

15:10 UTC 的 20 个 frontend targets 为 424 passed、0 failed，后端拒绝边界为 4 passed；
随后增加非局部 borrow val 防护及对应负例。最新 15:20 UTC 检查为 423 passed、1 failed：
该负例在 parser 阶段得到 L0076（invalid declaration modifier），fixture helper 的解析成功
断言失败，尚未进入所预期的 ownership L0163。原四项和其他八项 continuation 用例仍通过。
最新失败后，串行的 backend/check/clippy/fmt 未继续；上述门禁的 15:10 UTC 成功记录仅证明
修改前源码。最新尺寸检查通过：923 Rust、42 个历史超限，policy 未增加额度。
按父线程要求在此收束，保留代码和失败用例，不启动新任务；本段更新未再运行文档检查。

相关 frontend targets、后端拒绝边界和各次门禁的命令、计数、退出码与冻结 SHA 记录在
`/tmp/koven-0288-phase4-checkpoint.results.json`；直接日志为
`/tmp/koven-0288-phase4-frontend-final.log`、`/tmp/koven-0288-phase4-backend-final.log`。
尺寸不增加 policy/baseline/max_lines；unit call contract 与两条 local binding planner 按完整
职责提炼。既有大文件欠账保持显式报告。本阶段不做 LLVM/native/Map 新 API，不运行完整
codegen、Linux sanitizer、故障生成/注入/校准或远端 CI；Mac 合同保持。

### 2.4 前端 continuation 的最终源码收尾

以下为前端检查点；后端最小 ABI 的后续状态见 §2.5。

全局 `borrow val` 负例独立验证 parser L0076，并检查诊断 Span 精确指向 `borrow`。
capture、控制 initializer、嵌套借用调用、temporary 来源和循环绑定各有独立 ownership
测试：共同 helper 要求 single/unit 解析、名称和类型检查成功，再检查 L0164/L0162 及新
binding/end facts 原子清空；parser 提前拒绝不会掩盖这些 ownership 检查。

最终同一 Rust 源码的 20 个 frontend targets 为 429 passed、0 failed、0 ignored，含原四项
4 passed、continuation 14 passed 与 origins 5 passed。后端正常源码拒绝边界为 4 passed、
0 failed、1113 filtered。完整命令、门禁退出码、源码清单与补丁 SHA 见
`/tmp/koven-0288-phase5-checkpoint.results.json`；直接日志统一为
`/tmp/koven-0288-phase5-*-final.log`。本轮未改生产实现或扩大 SSA/LLVM 能力，Spec 继续
in-progress；后续 verifier/ABI、Map scoped API 和 nullable V 三态仍待实现。

### 2.5 普通单参数只读返回的最小 native 闭环

以下为前一阶段检查点；泛型 storage 与字段投影的后续状态见 §2.6。

SSA 函数记录唯一 Borrow 参数来源和 semantic target；`BorrowCall` 返回 shared 子 loan，
`BorrowReturn` 交接该入口来源链。verifier 检查调用参数/结果 delivery、来源活跃性、
alias roots 和父子终止约束，owner 在来源或结果存活时不得 move/drop。single/unit
lowering 消费已发布的实际 origin、source call/argument identity 和 binding-end facts。
读取显式绑定（包括 Group）使用真实返回 loan；scope-end 先结束结果，再结束实际建立
的来源 loan，最后清理或移动 owner。LLVM ABI 是原存储的非 owning pointer，不引入
owner、copy、隐藏 RC 或 callee payload drop。

正常源码 native 覆盖 single/unit 的 String 直接返回、一层 wrapper、owner scope-end 后
移动，以及 Int 共享存储读取产生普通 Copyable 交付。SSA 正例锁定真实返回 pointer 的
读取和结果→来源→owner 次序。拒绝负例查询正常源码产物的 verifier 合同，不改写 IR；
编译负例保留源移动诊断和未接通 CFG/block/multiple-parameter/stable-place 形状的 Span。
更多类型、投影、nullable/generic、receiver/Inout 返回及复杂 caller CFG 未开放；Map
scoped API、callback 与 nullable V 三态不在本轮实现范围。

完整命令、实际计数、门禁退出码、最终源码清单和补丁 SHA 记录在
`/tmp/koven-0288-phase6-checkpoint.results.json`，日志为
`/tmp/koven-0288-phase6-*-final.log`。尺寸整改按完整职责提取 operation 模型、callable verifier、
alias roots、LLVM call ABI、local binding lowering 和 unit native analysis helper；不增加
policy/baseline/max_lines。完整 codegen/frontend、Linux sanitizer、故障/注入/校准及远端 CI
未运行；Mac 既定合同保持，Spec 继续 in-progress。


### 2.6 泛型 storage 与实际字段借用返回

本阶段仍限定一个 Borrow 参数、唯一只读来源、表达式返回和普通 wrapper。去掉
String/Int 特例及来源类型必须等于结果类型的限制；具体实例复用既有 storage planner。
single/unit 按 actual origin 逐层匹配字段 symbol、实际根参数和 concrete projection type，
再建立 SharedFieldLoan/SharedHeapFieldLoan。函数参数保留来源 target，返回保留 payload
的独立 target；unit 调用使用实参 target，不能将返回字段的类型冒充来源根对象类型。
已有 alias、parent-child、active-source 和终止检查继续验证来源链与 owner 生命周期。

最终正常 native 的两个新增 test functions 各执行 direct/wrapper × 三个场景，共十二次
object/link/run：泛型 String、含 String 的 MoveOnly value class、Resource；inline/heap
字段；null 与非 null 的现行 pointer-like nullable storage。结果 scope-end 后仍可 owned
交付来源，Resource 的原 deinit 只执行一次。SSA 合同回归检查 shared delivery、来源
identity、投影 target、无 callee copy/drop/隐藏 retain，并查询错误交付合同。对应 LLVM
文本证明身份返回使用入口 storage pointer，nullable wrapper 不 load/unwrap payload，
null payload 不会变成缺失 loan；全程没有改写或生成故障 IR。

17:08 恢复连接后的只读核对发现中断文件没有落盘，而模块声明已加入；补齐文件后恢复
编译，没有重复覆盖已有文件。此前 unit nullable fixture 在普通 `source == null` 比较处
报 UnsupportedNode：unit scalar/if 尚未接现行 pointer-like nullable 比较，不是 borrow
结果 ABI 失败。本阶段不依赖该比较，native fixture 改为验证 nullable 位置借用、转发与
owned 交付；另保留正常源码拒绝回归，锁定比较表达式的真实 Span。该缺口没有被修复，
nullable native 绿灯不证明 null 比较能力。通用 inline nullable、Resource wrapper、
更多参数/来源、caller CFG/block 返回、Inout 和 Map callback 不因本阶段开放。

同一最终 Rust 源码的 codegen 定向为 28 passed、0 failed、0 ignored：普通 ABI/native
过滤 15、storage 合同 3、下游 SSA 5、四个正常 native 单项 4、原 inline nullable 拒绝
单项 1。20 个 frontend targets 为 429 passed、0 failed、0 ignored；包含实际来源、
continuation、错误来源、owned 交付、Map type/ownership 和 unit 共享合同。
workspace check、clippy -D warnings、fmt、尺寸门禁均 exit 0；938 Rust 文件，41 个历史
超限，policy/baseline/max_lines 未改变。最终文档检查和 diff 检查通过，全部命令、计数、
源码清单及 patch SHA 由 `/tmp/koven-0288-phase7-checkpoint.results.json` 记录，日志为
`/tmp/koven-0288-phase7-*-final.log`。

完整 codegen/frontend、Map native 全套、Linux sanitizer、故障/注入/校准和远端 CI
本阶段未运行。Mac 仍明确跳过 address/leak，skipped_reasons 为
`["address:macos-asan-unsupported", "leak:macos-counter-only"]`，acceptance.status 为 partial，
requirements_met 为 false。Spec 保持 in-progress；后续通用 Map requireValue/withValue、
真实 slot 来源及 nullable V 三态仍需实现和验收。未提交、暂存、push、开 PR 或合并。

### 2.8 withValue 的部分实现检查点

以下为前一冻结源码的历史部分结果；恢复开发后的修复与最新验收见 §2.9。

本阶段按用户批准的同步、非逃逸 Borrow V → Unit scoped callback 接线。single/unit
类型描述符保留实际 receiver/key/action 和通用 K/V，所有权入口发布 Borrow receiver、
key 和 action 参数事实；unit Shared receiver 另发布实际 LoanFact。共享 capture 对
Mutation 与 ExclusiveLoan 都保持只读权限，不能借 Inout Map receiver 绕过。

`MapWithValue` 原子操作输入 active Shared Map/key/action loans，返回 Boolean。
verifier 检查 callback 的唯一 Shared V 参数与 Unit 返回，并检查三项输入的活跃性。
LLVM 复用 lookup 的 Occupied slot，found 只将实际 V 字段地址交给现有 closure ABI 并
调用一次、返回 true；missing 不调用、返回 false，不 load/copy/move/retain V。
callable signature 对 Borrow 参数的既有非逃逸检查仍承担 callback 内部权限边界。
后端 callback slot 是该同步操作内部借用，没有新增 caller loan 结果或条件借用协议。

single 使用既有 pending-call loans；unit 使用既有 PendingCallFrame 与稳定 pending
operand slots，避免参数控制流后沿用旧 LoanId。正常结束按 action/key/source 次序结束
实际建立的 loans，再清理参数临时值和 CallReturn；Nothing 参数停止求值前缀。
这些清理接线不能替代通用矩阵与提前退出路径的完整证明。

格式化后的 map_ 为 **42 passed、2 failed、0 ignored**：前一阶段 41 条全部通过，
新增单文件 String 最小 native 通过，两个通用 native matrix test functions 失败。
String 最小场景证明 found 调用、callback 局部 return、missing 不调用、Boolean 结果及
随后 Map 变异。single 通用矩阵在 nullable 用例前已执行 String、MoveOnly inline、
Resource、missing 与 shared capture；unit 在 Resource 前执行 String 与 MoveOnly inline。
子场景不是独立通过的 test functions，不能把失败矩阵计作通过。

失败保留真实正常源码：single 的 `observe(item: Token?)` 在普通 null 比较/条件体
Span 报 UnsupportedNode；unit 的 Resource 回调 `observe(item)` 在实际 item 引用
Span 报 UnsupportedNode。后者已核对真实 Shared LoanFact 与 Borrow callback signature；拒绝入口定位到
unit deinit planner 的 selected-resource-lambda 检查：登记集仅消费普通调用路由、runtime
initializer 与 callable return，未登记 withValue action。缺口尚未修复。nullable 三态 fixture 没有改为固定字符串输出；
原 unit 普通 nullable 比较的 UnsupportedNode 回归保持。

前端新增 ownership 目标为 2 passed：callback 后恢复权限；拒绝 callback 中修改 Map、
owned 交付借用值与 move closure 捕获逃逸。源码在进入 ownership 前已分别通过 single/unit
parser/type 检查。后续扩展诊断/Span 和单独 verifier 查询未写入，不声称覆盖。

用户随后要求立即冻结本轮检查点，当前 runtime constructor 命令结束后关闭队列，
尚未开始的 frontend 22-target 回归、runtime source 回归、workspace check、clippy、fmt
check、尺寸检查不运行；格式化已执行。前端 2/2 为格式化前的定向结果，不能冒充最终
冻结源码上的完整前端验证。最终命令与源码 SHA 见下述账本。证据路径为
`/tmp/koven-0288-phase9-checkpoint.results.json` 和 `/tmp/koven-0288-phase9-*-final.log`。
非空 readonly Map 仍无正常构造命中 native 证据：mapOf 仅支持空参数，当前 assignability
不接受 MutableMap 转 Map；已询问批准的构造/转换入口，不自行扩展语义。
完整 codegen/frontend、Linux ASan/LSan、故障生成/注入/校准与间接聚合及远端 CI 未运行。
Mac 合同仍为 skipped_reasons
`["address:macos-asan-unsupported", "leak:macos-counter-only"]`，acceptance.status=partial、
requirements_met=false。Spec 保持 in-progress，不归档；没有 stage/commit/push/PR/merge。


### 2.9 withValue 恢复开发与最终验证

前一检查点关闭队列后，用户授权继续修复两条正常 native 矩阵。unit Resource 回调
缺口通过既有封闭 callback 来源表登记实际 withValue action 解决，没有放宽 deinit
planner 的选择断言。nullable 缺口通过已发布 NullComparisonDescriptor 的 owned
NullableIsNull 与 shared NullableLoanIsNull 接通；后者要求实际活跃 nullable storage
loan，LLVM 只加载 pointer bits 比较，不创建或 unwrap payload owner。借用 smartcast
view、`!!`、通用 inline nullable、新 Resource wrapper 没有因此开放。

unit Missing 的合法 error() callback 曾因 Abort 路径活跃临时值被 closure finish guard
误拒绝；现依既定非展开终止在正常 return 清理检查前完成 Abort。未新增 Nothing
callable ABI；callback 的结果合同仍是 Unit。
恢复聚焦检查时，原普通 owned nullable 比较 fixture 在 unit 触发 MissingOwnedExit：
描述符直接读取存在性，却遗漏原 operand 的 AfterExpression drop。现在在 presence
读取后消费原前端 operand 清理事实；不手工补 owner drop，也不放宽 verifier。
原源码回归从 InvalidSsa 转为通过，并同时要求实际 presence 操作/分支、nullable
owner drop 和 LLVM pointer-null 比较。该 owned ASAP 修复不代表借用受限 last-use。
受影响 unit closure 旧边界测试仍将 Borrow Host 参数归为 String-only 布局拒绝；
通用 storage 接线已合法支持该普通 ABI。完整原 fixture 保留为正向 shared storage
合同，检查 SSA/verifier、LLVM pointer 参数及无 payload copy/drop/retain；原负向
矩阵改测仍封闭的 Borrow Unit，Inout 与其他控制流拒绝断言保留。新增测试一度误要求
callee 显式 BorrowEnd，现按既有入口 call-scoped view 在 Return 结束的合同检查无
显式 end；caller 的来源 loan 仍由正常调用边界管理，生产规则未改变。纯 SSA 测试曾错误要求独立 action/key
loans 的相对终止顺序，改为验证实际合同：所有输入 loan 在同步操作后结束，每个临时
owner 在自身 loan 结束后析构，没有改变生产断言或借用规则。

最新格式化源码 `with_`：6 passed、0 failed/ignored，19.211s；日志
`/tmp/koven-0288-phase9b-with-final.log`。其中 single/unit 正常矩阵各含七种源码场景：
String、MoveOnly inline、Resource、mutable Missing、共享 capture、nullable 三态及
空 readonly Missing。两项正常 allocation identity 计数测试分别核对 capture 场景的
8 个分配及 Resource 场景的 6 个分配，每个 source owner 恰释放一次；没有故障注入。
独立 String 最小 native 覆盖 callback 局部 return 和恢复变异；另一个纯 SSA 测试在
同一七场景上检查 single/unit slot/signature、活跃输入与临时清理，并查询错误合同。

当前 frontend `ownership_map_with`：4 passed、0 failed/ignored，3.623s；日志
`/tmp/koven-0288-phase9c-frontend-with-final.log`。泛型 callback 在局部 return 后允许
key 复用与 Map owned 交付。七组负例均先通过 single/unit parser/type，再验证 Map
put/remove/owned 转移、payload owned 转移/字段存储、move/shared closure 逃逸的
所有权诊断与真实 Span。`return value` 单列为类型边界，精确 L0087 与 value Span；
没有把类型拒绝当成非逃逸的所有权证据。

非空 readonly Map 构造缺口明确保留：现行合法入口只有空参数 `mapOf<K,V>()`，
以及通过 `mutableMapOf<K,V>()`/put 填充 MutableMap 后直接借用访问；后者不是 Map
转换。当前 factory 不接收条目，类型关系不允许 MutableMap → Map，标准库无现行
转换。非空 readonly 命中需要另行批准条目 factory 或 owned 转换 API；本阶段不自行
新增，也不阻塞独立全库验证。旧 get/下标保持 Copyable 按值合同；nullable V 的旧
get/remove 带 Span 拒绝，nullable 三态由本阶段 scoped 访问验证，不采用旧 PR 候选的
条件借用表示。普通结果当前只证明单来源和词法 scope/return，受限 last-use 未实现。

四个故障入口静态核对后从 codegen 全库中精确排除：native_sanitizer_tests 的
asan_instruments_generated_user_runtime_and_drop、counter_failures_preserve_compile_and_run_evidence，
native_generated_owner_tests 的 export_generated_owner_calibration、
export_generated_owner_case_with_missing_deinit_fault。普通 owner exporter 的两个可选
环境路径在测试子进程中移除，确保只走自建正常源码入口，不读取外部 fault.txt。
其他正常 allocation 计数、静态 verifier 负例和原有 ignore 保留。全库并非未过滤全通过。

聚焦回归当前全部通过：codegen 134 个不同 test functions、147 次执行（过滤器重叠），
frontend 22 targets 共 436 passed；均 0 failed/ignored。Map 47、borrow-result ABI 22、
storage 3、下游 SSA 5、四项普通 native 4、原 inline nullable 1、unit/single closure
10/11、nullable 34、runtime source 10；命令与实际日志为
`/tmp/koven-0288-phase9c-*-final.log`。正常 constructor 12 条的前一阶段日志不能代替
当前源码证据；它们将由本轮过滤 codegen 全库再次执行。
workspace check、严格 clippy 与 fmt 已通过；fmt 首次发现新增 helper 的格式差异已修正。
随后尺寸门禁发现 control.rs 1522 超出既定 1520、renderer 1005 超出 1000；完整 nullable
operand cleanup helper 移至 nullable comparison 模块，完整 Map opcode 渲染移至私有
render/map.rs。按职责迁移保留原逻辑/文本，不压行或提高 policy/baseline/max_lines。
迁移后聚焦重跑全部通过。最终 workspace check/clippy -D warnings/fmt/尺寸/docs/diff
全部 exit 0；959 handwritten Rust、41 个历史超限，581 Markdown；安全限定的
check_docs/check_rust_sizes unit tests 84 passed。当前阶段没有修改 size policy；
已有大文件的接线增长在上一冻结的既定额度内，自审不声称独立评审。
第一次冻结清单后完整 frontend 为 1886 passed、2 failed、0 ignored，257.864s；
142 个结果目标（含 lib/integration/doctest），日志
`/tmp/koven-0288-phase9c-frontend-full-before.log`，源码清单/patch 保存为
`/tmp/koven-0288-phase9c-firstfreeze.*`。codegen 当时未启动。两处普通失败随后修正：
严格诊断目录测试显式补入 Guide v0.42 已发布的 L0162–L0164；unit capture 的新增
exclusive 检查收窄到 non-owning shared capture，不把 owned move capture 的字段
访问误报为 shared 冲突。原 owned 字段替换 deferred 测试与所有断言保留；另加
shared 字段替换负例，精确验证 L0135、Inout 标记 `&` 的 primary Span 和 holder 来源
label，保持不发布 owner/field commit。该测试曾误期望 primary 为 holder，按既有
Inout marker 诊断合同修正为 `&`，没有改动生产诊断。四个修复目标为 66 passed、
0 failed/ignored，随后重跑 Map、unit closure、frontend 共享合同及门禁并重新冻结。
修复后 Map 47/47、unit closure 10/10、frontend 24 targets 共 459/459 通过；
workspace check、clippy -D warnings、fmt、尺寸（959/41）、文档（581）、diff 与安全
脚本 84 条均再次通过。第二次冻结后的完整 frontend 为 1889 passed、0 failed/ignored，
245.206s，19:57:09–20:01:14 UTC，含 lib/integration/doctest；原日志另存
`/tmp/koven-0288-phase9c-frontend-full-before-unit-object-fix.log`。
过滤 codegen 曾观察到 367 passed、1 failed；20:22:32 UTC 后执行会话被取消且日志停止，
没有终态或退出码，不能视为全库结果。截断证据保存为
`/tmp/koven-0288-phase9c-codegen-full-interrupted.log`；当时 959 Rust 与冻结清单一致。
单独复现失败 `unit_object_failures_preserve_targets_and_cleanup_sibling_temporary` 为
0 passed、1 failed，exit 101：旧 `borrow Bundle` closure 现已合法 lower，原 expect_err
收到成功 `()`。保留原 Bundle callback 源码为正向 native；输出保护负例改用仍明确
不支持的 Borrow Unit，目标保留、临时清理、输入顺序诊断与 mismatch 断言全部保留。
修复后输出发布两项 2/2、Bundle 正向 native 1/1 通过，后者含 object/link/run 与正常
allocation identity 计数。新增计数初始误将空 capture closure 算为 heap allocation；
依据实际 LLVM 内联 environment ABI 修正为 String/Bundle 两个分配，仍要求逐指针
精确释放一次；原错误日志保留，没有修改生产断言。库测试枚举当前为 1143，四项故障
入口名单不变。修复后 check/clippy/fmt/尺寸/docs/diff 均再次 exit 0，安全脚本 84 条
通过，959 Rust / 41 历史超限、581 Markdown；size policy 未提高。重新冻结后串行
重跑完整 frontend/过滤 codegen（包含 integration/doctest）。持久化队列 PID 15533 于
20:59:32 UTC 正常结束，exit 0；汇报和交接期间未停止该队列或另开 Cargo。

最终同一冻结 Rust 源码的终态证据：

| 检查 | 实际结果 | UTC 起止与墙钟 |
|---|---|---|
| 完整 frontend（含 lib/integration/doctest） | exit 0；1889 passed、0 failed、0 ignored、0 filtered；142 个结果目标 | 20:32:46.998990–20:34:06.483267；79.483s |
| 过滤 codegen 库 | 1138 passed、0 failed、1 既有 LLDB ignored、4 filtered；库 test harness 1518.10s | 同一 Cargo 全库命令，见下行 |
| codegen integration / doctest | 2 / 4 passed，均 0 failed/ignored/filtered；合计全库 1144 passed，exit 0 | 20:34:06.753489–20:59:31.825864；全命令 1525.044s |
| 冻结一致性与排除核对 | 959 个 Rust 文件集合及每个 SHA-256 与冻结清单完全一致；四项故障入口未执行，两个可选 fault 环境路径已移除 | 终态核对；源码无修改 |

frontend 实际命令为 `cargo test --locked --offline -p lang-frontend --no-fail-fast`；
codegen 命令没有 `--lib`，如下（两个命令均由 runner 设置 LLVM prefix 并移除可选 fault 环境路径）：

```sh
cargo test --locked --offline -p lang-codegen --no-fail-fast -- --test-threads=1 \
  --skip native_sanitizer_tests::asan_instruments_generated_user_runtime_and_drop \
  --skip native_sanitizer_tests::counter_failures_preserve_compile_and_run_evidence \
  --skip native_generated_owner_tests::export_generated_owner_calibration \
  --skip native_generated_owner_tests::export_generated_owner_case_with_missing_deinit_fault
```

既有 ignored 为 `llvm::debug_tests::lldb_hits_a_koven_source_breakpoint_and_reports_the_frame`。
聚焦累计 137 个不同 codegen test functions、150 次执行；原 Map 47、frontend 24 targets
459 条及相关门禁证据保留，最终全库直接覆盖冻结源码。原 inline nullable 回归与修复后的
unit 输出保护、Bundle callback 正向 native 均在本次全库通过。
冻结 Rust SHA-256 为 `a5157fe55d2e5cbcdf2c7d0568fdf2dd523ca8c4b28c446394ed0b0f6cfe4875`。
原始日志为 `/tmp/koven-0288-phase9c-frontend-full.log`、
`/tmp/koven-0288-phase9c-codegen-full.log`；完整 argv、退出码、耗时与源码清单在
`/tmp/koven-0288-phase9c-final-checkpoint.results.json`、
`/tmp/koven-0288-phase9c-final-checkpoint.rust.json`，故障入口静态账本为
`/tmp/koven-0288-phase9c-fault-audit.json`。终态后仅补录文档，不再运行 Cargo 或修改 Rust。

剩余两项的后续最小建议仅供决策，未作为新语言/API 批准，也未实施：

- 非空 readonly Map：显式消费 owned MutableMap 并返回同一 K/V 的 Map。复用现有
  put 填充与四字段 LLVM 布局，移交条目缓冲区及 size/capacity/tombstones，不 clone
  key/value、不保留第二个 owner。仍需批准新 API 与 Guide/Spec 范围；随后补 type
  descriptor、receiver Owned 事实、SSA 消费/产生与 verifier、LLVM 四字段移交，以及
  single/unit 的 String 键、MoveOnly/Resource/nullable V 非空命中和逐指针清理回归。
- 受限 last-use：在当前已支持的直线局部路径上独立证明 borrow binding 的全部别名、
  投影及子 loan 未来使用；在实际结束点发布 BorrowBindingEndFact，并让 checker 同步
  解除 source continuation/父依赖。复用 existing single/unit 终止消费者，明确子结果、
  父/source loan、owner cleanup 的顺序；若结束点接线尚缺则最小补入。需要同时验证
  same-scope 权限恢复、晚用别名/投影仍拒绝、子 loan 未结束不得恢复父权限、调用期
  Borrow 不提前结束、Resource 仍按现行清理规则；不把完整 CFG/NLL 当成首片前提。
所有命令使用已安装 LLVM/Clang 21.1.8、locked/offline；本次不 stage/commit/push/PR/merge。
Linux ASan/LSan 与远端 CI 未运行。Mac 合同保持 skipped_reasons
`["address:macos-asan-unsupported", "leak:macos-counter-only"]`，acceptance.status=partial、
requirements_met=false。Spec 保持 in-progress，不因部分绿灯或平台计数证据归档。


### 2.10 受限直线 last-use 独立阶段

本阶段用户批准在 scope/return 安全基线继续最小直线闭环；
readonly owned 转换 API 尚未批准，未实施。§2.9 的全库结果属于其旧冻结 SHA，
不能当作本节新增 Rust 的全库验收。

shared `borrow_last_use` 独立收集后续表达式的实际 symbol 使用，沿发布的 binding parent
链保留存活别名、投影和全部祖先，并分别跟踪共享同一 origin 的独立结果。完整直线证明
只处理本层 borrow val；外层继承绑定以及出现 nested block、return、条件求值、捕获、
cast 或循环的序列保守保留词法/return 边界。没有用 owner ASAP 或单一名字 last-use
决定 loan 终止。single/unit checker 在正常语句完成后撤销选定 continuation 并发布
AfterStatement end；drop planner 消费同一选择，先子后父，再按既有双轨规则处理 owner。

后端原来统一拒绝带 parent 的结果绑定。现仅接通有实际调用 source-loan 的直线子结果：
lower_borrow_argument 复用真实父 loan，BorrowCall 保留该依赖，既有 end 消费者先结束
结果再结束其实际创建的来源 loan。direct stable-place alias 仍缺调用 carrier，single/unit
在 SSA 前返回 UnsupportedNode；该边界有独立负向回归，未扩更多参数、receiver 或 CFG ABI。

失败证据保留：`phase10-last-use-red-final.log` 为 frontend 3 passed / 5 failed，
`phase10-native-last-use-first.log` 为两个 native 矩阵 UnsupportedNode；子调用接线后
`phase10-native-last-use-second.log` 因直接 Token 实参到 Token? Map 未提升而 InvalidSsa。
只读 verifier 诊断确认该失败为 MapPut operand 类型不匹配。当前阶段使用已有显式
nullable owner 交付路径保留 null/full 与后续变异断言，未放宽 verifier；直接 nullable
promotion 缺口记录于 Architecture，临时 test-only 诊断代码已移除。

当前实际结果：frontend 六目标 38 passed / 0 failed，含新 last-use 8 项；
codegen `last_use_` 过滤器 5 passed / 0 failed，其中两个新 native test functions 各执行
5 个正常源码的 object/link/run 与精确 allocation identity 计数。场景涵盖泛型 String
子链、MoveOnly value class/Resource 的只读字段返回、Resource 保留词法析构、
MutableMap<String,Resource> 覆盖和 MutableMap<String,Token?> 的真实 null/full 槽位。
新纯 SSA 查询验证返回 loan 类型、真实父依赖、唯一 ends、子/父/source 终止顺序及
后续 owned 交付/MapPut；前端负例验证未来别名、投影、子 loan 与第二独立结果仍阻止
move/失效变异，普通同步调用的参数 loan 不提前结束。

冻结源码上 `cargo test --locked --offline -p lang-codegen --lib borrow -- --test-threads=1`
为 214 passed / 0 failed / 0 ignored / 933 filtered，耗时 257.821 秒；日志为
`/tmp/koven-0288-phase10-codegen-borrow-focus.log`。workspace check、workspace clippy
（all-targets、-D warnings）、fmt 均 exit 0，文档 581 Markdown 与 diff 检查通过；安全
文档/尺寸脚本测试 84 passed。尺寸为 962 Rust / 41 历史超限；本阶段 checker.rs
1419 行、single/unit drop planner 1656/1667 行，保留旧欠账且未提高 policy。

2026-10-07 22:01 UTC，用户要求收束检查点。新的 frontend 全量已经启动，继续当前命令；
后续 codegen 全量尚未启动，队列关闭，未执行结果不能记为通过。frontend 命令为
`cargo test --locked --offline -p lang-frontend --no-fail-fast`，准确终态以
`/tmp/koven-0288-phase10-frontend-full.log.json` 为准；队列状态保存在
`/tmp/koven-0288-phase10-persistent-full.json` 与 `phase10-live.json`。
Rust 冻结 SHA-256 为 `2d61334232dda25df36f2b3b74a4ab5de695a3ff6ebc9890b19e1aa3c2a6897d`。
交接前该 frontend 命令已于 22:02:23 UTC 正常结束：1897 passed / 0 failed / 0 ignored，
143 个结果目标，282.307 秒，exit 0。队列随后停止，wrapper 的 125 是未启动下一命令的
检查点状态，不是测试失败。没有剩余运行中的本轮 Cargo 命令。
完整源码清单、结果和 diff 为 `/tmp/koven-0288-phase10-checkpoint.rust.json`、
`phase10-checkpoint.results.json`、`phase10-checkpoint.patch`；运行期间不改 Rust。

本阶段日志前缀为 `/tmp/koven-0288-phase10-`，与前阶段证据分开保存。
本阶段不 stage/commit/push/PR/merge，不执行故障生成/注入/校准及其间接入口。
Linux ASan/LSan、远端 CI 未运行；Mac 的两个 skipped_reasons 与 partial/false 验收合同保持。

### 2.11 前一冻结源码的过滤完整 codegen 与只读边界核查

用户在 §2.10 检查点后明确要求继续完整 codegen，含 integration/doctest；本阶段未改 Rust。
完整命令为 `cargo test --locked --offline -p lang-codegen --no-fail-fast -- --test-threads=1`，
随后四个 `--skip` 精确选择以下原有禁止入口：

- `native_sanitizer_tests::asan_instruments_generated_user_runtime_and_drop`
- `native_sanitizer_tests::counter_failures_preserve_compile_and_run_evidence`
- `native_generated_owner_tests::export_generated_owner_calibration`
- `native_generated_owner_tests::export_generated_owner_case_with_missing_deinit_fault`

静态重新扫描 src/tests 的 fault/inject/calibration 调用链、新增 last-use native/SSA 测试、
integration rustc metadata 与 compile-fail doctest，未发现新增间接故障入口。
移除 `KOVEN_GENERATED_OWNER_CASE`、`KOVEN_GENERATED_OWNER_CALIBRATION` 后，普通 exporter
只读取自己创建的正常 MINIMAL 源码，不能进入 fault.txt 分支。正常逐指针计数与静态 verifier
负例继续执行；未运行故障生成/注入/校准或 sanitizer 聚合脚本。

实际终态：2026-10-07 22:07:38.501558—22:34:03.233500 UTC，1584.631 秒，exit 0。
library 1142 passed / 0 failed / 1 ignored / 4 filtered，integration 2 passed，doctest 4 passed，
合计 **1148 passed / 0 failed**。既有 ignored 为 macOS LLDB 测试；四个排除入口均未出现在
执行记录中。四项新增 last-use 回归（两个 native 矩阵、SSA 依赖/终止次序、直接别名拒绝）
均在该全库实际通过，没有沿用旧全库结果代替。本机 IR/计数 Clang 与 LLVM 均为已安装
21.1.8，`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`。

962 Rust 的完整冻结 SHA-256 在启动、结束及交接时重新核对一致：
`2d61334232dda25df36f2b3b74a4ab5de695a3ff6ebc9890b19e1aa3c2a6897d`。
同一 SHA 的 frontend 全量为 1897 passed / 0 failed / 0 ignored；check/clippy/fmt、尺寸、
安全脚本复用 §2.10 的同源码成功证据，终态后只更新事实文档并重新运行 docs/diff。
日志和准确 argv 为 `/tmp/koven-0288-phase10b-codegen-full.log` 及同名 `.log.json`；
静态账本为 `phase10b-fault-audit.json`，结束状态为 `phase10b-persistent-full.json`。

只读分类：以下两项属于现行 Guide 已批准语义的 Phase 4 实现缺口，不是语言非目标。

| 形状 | 前端/现行语义 | 当前 SSA/native 与用户影响 |
|---|---|---|
| 单参数 ordinary call、包装与调用派生子结果 | 真实 origin/source/parent 与直线 ends | 已验证的 storage 类型支持 native；子先于父/source 结束，之后可 owned 交付 |
| `borrow val alias = parent` 或稳定只读 place initializer | Guide10 明确允许，前端保留真实来源与别名未来使用 | source_loan 缺席时 single/unit 在 SSA 前 UnsupportedNode，native 为 UnsupportedSource；检查通过仍不能编译为 object |
| callee 返回 MoveOnly value class/Resource 的只读字段 | actual-return 字段 origin | 支持真实 field storage pointer 与来源 lease，正常 native 已验证 |
| String 键、Resource/现行 Token? V 的 MutableMap | 通用 Map、确定 slot 借用及 nullable 存储合同 | 显式 nullable owner 路径支持 null/full/missing 与 last-use 后变异；Resource 清理计数通过 |
| `m: MutableMap<String,Token?>; m.put("key", Token(7))` | Guide3 的 T→T? 与 Guide12 nullable Map 合同，前端按 expected V 检查 | MapPut 未做 NullableWrap，verifier 拒绝为 InvalidSsa，native 为 InvalidModel 且当前无 Span；显式 `val value: Token? = Token(7)` 后 put 可执行 |
| 复杂 caller CFG/循环/capture/cast 与更多返回 ABI | 保守 scope/return 或结构化拒绝，不宣称完整 NLL | caller binding 的 If/When/Return/Binary、循环及未接 ABI 仍拒绝；终止粒度为正常语句完成点 |
| 非空 readonly Map 转换 | 新 API 尚未批准 | 未实施；只有空 readonly Missing 与已填充 MutableMap 的命中证据 |

直接别名的完整复现就在 `ssa/borrow_storage_contract_tests.rs`，本次 single/unit 回归通过。
nullable promotion 的真实普通矩阵失败保留在 §2.10 日志；缩减复现、支持形式、文件行号和
语义依据为 `/tmp/koven-0288-phase10b-boundaries.json`。缩减的 promotion 源码未单独执行，
不把静态推断冒充新运行证据；没有新增或放宽测试来隐藏该缺口。通用 inline/tagged nullable
ABI 的延期属于 Guide15 的明确实施边界，不能用来解释已支持普通 class Token? 的 Map 提升缺失。

本阶段没有普通测试失败需要修复。Linux ASan/LSan、远端 CI 未运行；Mac 仍为
`["address:macos-asan-unsupported", "leak:macos-counter-only"]`、partial/false。
Spec 保持 in-progress，未 stage/commit/push/PR/merge，当前没有本轮 Cargo 命令运行。

### 2.12 已批准 alias carrier 与 nullable Map operand 提升

用户在前一全库检查点之后明确要求修复这两项既定语义缺口；不启用 readonly 转换 API，
不扩展 direct inline nullable ABI。实现限于 Phase 4，复用既有 SSA operation、verifier 与 LLVM。

single/unit 的 `borrow val alias = parent` 消费真实 BorrowBindingFact parent/origin、
initializer 引用和当前 shared loan 的 semantic target；为 alias 创建独立 SharedReborrow。
canonical origin 只描述来源 lease，不用于猜测 Map slot 或字段 payload 指针。
已有 end facts 按子先于父/source 的顺序终止，alias 不得到 owner，不制造 Copy/Retain 或 payload drop。
多别名未来使用与调用派生子 loan 都受真实 parent closure 保护，之后可消费来源或变异 Map。
owned root/只读字段的直接 initializer 没有 parent，仍保留带 Span 的 UnsupportedNode 边界。

MapPut 依据 typed V 和 actual expression type 做通用 T→T? 适配，先转移原 owner 再生成
NullableWrap；原 ValueId 不留在 binding/temporary 清理中。unit 复用已有 owned adaptation，
single local/Map 复用同一 helper。MapPut exact-type verifier 未放宽，LLVM ABI 未改动。
Resource class 的 nullable Map storage 在 Map V 范围内使用已支持 pointer niche 与 conditional drop；
无 Map 的 Resource? 和其他 resource wrapper 仍保留原有 guard。unit Map construction 消费
具体 descriptor 类型，对不支持的 inline nullable storage 返回带 Span 的 UnsupportedNode，
不让该边界退化成 MissingFact。

| 回归/命令过滤器 | 实际结果与证据 |
|---|---|
| alias 新 SSA 正例 red | 1 failed，UnsupportedNode/Span；`/tmp/koven-0288-phase11-alias-red.log` |
| codegen `alias` | 15 passed；包含原始 direct alias 源码及五组 single/unit 合同；`phase11-alias-ssa-green.log` |
| codegen `native_borrow_direct_alias` | 2 passed；各五组 object/link/run 与正常逐指针计数，26.611s；`phase11-alias-native.log` |
| 原始 `m.put("key", Token(7))` single/unit red | 2 failed，均 InvalidSsa；`phase11-promotion-red.log`；基本适配后同两项通过，`phase11-promotion-basic-green.log` |
| 通用存储矩阵 red | 初次测试编译错误已纠正；第二次 2 passed/4 failed，暴露 single Resource Map guard、unit inline 错误种类与非法 Box<String> 测试输入；原日志保留 `phase11-promotion-storage-red*.log` |
| codegen `nullable_map_promotion_` | 6 passed/0 failed，21.751s；Resource、含 MoveOnly Packet 的 class、Box<Packet>、含 String 的 class，每种 constructor/owned local、覆盖再写 null；single/unit 真实 native 与每个 allocation 精确释放；`phase11-promotion-storage-green.log` |
| direct inline nullable 负例 | String?、Packet?、Int? 的 single/unit 均 UnsupportedNode/Span；包含上行 6 项；原 inline nullable 回归仍保留 |
| codegen `--lib borrow` | 219 passed/0 failed/0 ignored，316.087s；`phase11-borrow-focus.log`，含恢复直接构造后的 last-use native |
| codegen `--lib map` | 63 passed/0 failed/0 ignored，68.083s；`phase11-map-focus.log` |
| 原 Resource wrapper / 原 inline nullable 拒绝 | 各 1 passed；`phase11-resource-wrapper-boundary.log` / `phase11-inline-boundary.log` |
| frontend `cargo test --locked --offline -p lang-frontend --no-fail-fast -- --test-threads=1` | 完整 1897 passed/0 failed/0 ignored（含 doctest），90.132s；`phase11-frontend-full.log` |
| workspace check / clippy `--all-targets -- -D warnings` / fmt | 全部 exit 0，2.423/4.173/1.977s；`phase11-check.log` / `phase11-clippy.log` / `phase11-fmt.log` |
| 尺寸 / 安全 scripts unittest | 964 Rust / 41 历史超限，policy 未提高；84 tests passed，15.469s；`phase11-size.log` / `phase11-safe-script-tests.log` |
| docs / diff | 581 Markdown，exit 0；中途 `phase11-docs-interim.log` / `phase11-diff-interim.log`，最终后重跑 |
| 最终过滤 codegen 全库 | 23:06:19.292858—23:32:11.015754 UTC，1551.726s，exit 0；library 1151 passed/0 failed/1 既有 ignored/4 明确 filtered，integration 2、doctest 4，合计 1157 passed；`phase11-codegen-full.log` / `phase11-full-status.json` |

正常 last-use nullable slot 源码已恢复直接 Token 构造，不再以 annotated nullable local 回避
Map promotion。测试库存保留 String/MoveOnly/Resource 通用载荷；Box<String> 违反现行 Box
类型参数约束，因此 String payload 使用合法 class 字段，直接 String? 仍作为 ABI 拒绝回归。
历史全库结果仅属于 §2.11 的冻结 SHA，不能用于声称本轮新源码全量通过。
本轮 964 Rust 冻结 SHA 为 `3158dca10c65d24cf477d2fbe53bc2b4be3aa7536b4dfa8a60704801f2728bd7`，
聚焦/frontend/门禁每项前后核对一致，最终全库前后及交接时同样一致；新增 alias/promotion、原 inline nullable 和 Resource wrapper 回归均实际在全库通过。工具为已安装 Rust 1.96.0、
LLVM/IR Clang 21.1.8，`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`；未安装或更改系统配置。

最新上级要求收束检查点，不再扩大实现。另有一项静态待核验组合：模块先登记 Map 查询
结果的 nullable identity，再创建 direct inline nullable Map；缓存前置返回可能绕过 storage
边界。仅准备 `/tmp/koven-0288-phase11-nullable-cache-probe.patch`，未应用、未执行，
不能把静态疑点作为已复现缺陷，也不以本次全库通过声称该组合已证明。交接后由上级核验。

Mac skipped_reasons 仍为 `["address:macos-asan-unsupported", "leak:macos-counter-only"]`，
acceptance.status=partial、requirements_met=false；G5 未勾选。未运行故障生成/注入/校准、
Linux ASan/LSan 或远端 CI。Spec 保持 in-progress，未 stage/commit/push/PR/merge/清理 worktree。

### 2.13 稳定 place initializer、旧 nullable 查询与缓存存储边界

2026-10-07 收尾审计后，用户授权本片修稳定 owned-root 与字段 initializer carrier、
nullable V 旧 get/下标的前端明确拒绝，以及只用普通源码核验的 nullable identity 缓存组合。
不扩展其他 return ABI、readonly 转换 API 或 nullable owned remove 的返回表示。
§2.12 的实现边界与全库数字保留为当时冻结片事实，不能替代本片验证。

single/unit 消费 BorrowBindingFact 的实际 origin、parent、initializer 与既有结束事实。
owned 根和只读字段路径逐项对照已发布 projection 与根引用，形成真实 RootPlace/BorrowBegin
及 SharedFieldLoan/SharedHeapFieldLoan；父借用的字段从实际 payload loan 投影，不把
canonical source lease 当成 payload 地址。新 root/投影祖先随 binding 保存，结束时叶到根
清理；已有 parent 不由子 binding 清理。普通 alias 使用 SharedReborrow，未来 alias/child
使用继续保护 parent/source。没有 owner copy/retain、payload drop 或 LLVM ABI 改动。

Copyable nullable V 的旧 get/下标在 single/unit 类型阶段给 L0130 与实际查询 Span，提示
withValue 区分 Missing/Found(null)，不发布成功 MapGetDescriptor。MoveOnly 查询保持
L0136，非 nullable Copyable get 保持按值合同。旧 nullable owned remove 仍在后端带 Span
拒绝；其独立 owned Missing 表示未定，不用借用结果代替。

普通合法源码先登记非 nullable Int Map 的查询结果，再构造 Int? inline Map，
single/unit 均曾错误接受；owned-remove 缓存后的 String?/Packet? 另作边界回归。Map V storage 消费处现在独立检查 nullable storage 必须是
已批准 NullableHandle，不能因同一前端 nullable identity 已缓存为查询 result aggregate
而绕过存储边界。原 Copyable 查询 result 表示及 pointer-like nullable Map 不改变。

| 本片回归/命令过滤器 | 实际结果与证据 |
|---|---|
| codegen `stable_place` red | 5 failed，均 UnsupportedNode/Span；`/tmp/koven-0288-phase12-place-red.log` |
| codegen `stable_place` green | 5 passed；single/unit 各五组 root、嵌套 inline/heap 字段、parent 与 Map slot 投影的正常 object/link/run、精确分配/释放；`phase12-place-final-green.log` |
| native 计数首轮 | 3 passed/2 failed；新增 Map 期望计数误将 header 算作 heap 分配；静态核对 header 是 SSA aggregate，一块 table buffer 加两次显式 String clone，共 3；严格逐指针计数断言保留，首轮 `phase12-place-green.log` 留存 |
| frontend 旧 nullable 查询 red | 1 failed，缺少要求的诊断；`phase12-query-red.log` |
| frontend 九个相关套件 | 合计 60 passed，exit 0；type_map 14 passed，包含 Copyable nullable L0130/Span 与 String/String? L0136；借用未来 root/field/alias 冲突及原 Map require/with 合同通过；`phase12-frontend-focus.log` |
| frontend 首轮诊断 green | 12 passed/2 failed，测试跨 SourceMap 读取 Span；修正测试持有实际 source map，不改生产诊断 Span；`phase12-query-green.log` 留存 |
| codegen `nullable_map_cached_` red/green | red 2 failed，错误接受 Int? inline storage；green 3 passed，含 single/unit Int? 及 String?/MoveOnly Packet? owned-result 缓存组合的 UnsupportedNode/Span；`phase12-cache-red.log` / `phase12-cache-green.log` |
| codegen `nullable_value_owned_remove` | 2 passed，保持 single/unit owned remove 拒绝边界；`phase12-remove-boundary-green.log` |
| codegen `--lib borrow` / `--lib map` | 222 / 66 passed，0 failed，361.524 / 85.253s；`phase12-borrow-focus.log` / `phase12-map-focus.log` |
| workspace check / clippy all-targets `-D warnings` / fmt | 全部 exit 0，44.279 / 38.165 / 2.630s；`phase12-check.log` / `phase12-clippy.log` / `phase12-fmt.log` |
| 尺寸 / docs / diff | 965 Rust / 41 历史超限，policy 未提高；581 Markdown，均 exit 0；`phase12-size.log` / `phase12-docs.log` / `phase12-diff.log` |
| docs 首轮 | Architecture 201 行超过 200 行入口上限，exit 1；移除重复的 Guide 启用历史表述，依据仍留在 Spec，当前 200 行并通过；`phase12-docs-interim.log` / `phase12-docs-interim-green.log` |
| 最终 frontend 全库 | 1898 passed/0 failed/0 ignored（含 doctest），359.924s，exit 0；01:02:52.270160—01:08:52.194474 UTC；`phase12-frontend-full.log` |
| 最终过滤 codegen 全库 | library 1157 passed/0 failed/1 ignored/4 filtered，integration 2 passed、doctest 4 passed，合计 1163 passed/0 failed；1622.539s，exit 0；01:08:52.432687—01:35:54.977304 UTC；`phase12-codegen-full.log` / `.log.json` |
| 全库回归核对 | wrapper 核对本片及原始失败相关 12 条必需回归实际通过，四个禁用入口未执行；唯一 ignored 是既有 LLDB debugserver task-port 权限检查；串行队列终态 exit 0、current=null；`phase12-verification.json` |
| 收束检查点交接 | 01:26:55.191023 UTC 曾记录 library 367 passed/0 failed、尚无终态；只关闭只读监视 cell，保留原 session 31179，恢复后接续至 exit 0，未新开 Cargo；`phase12-checkpoint.results.json` 保存历史检查点及最终命令、冻结核对和未提交状态 |

本片 965 Rust 冻结 SHA 为 `bff7a0e52758ebc91d9c973c125c269559729cdb37e5e554f0054edf5257a05a`。
串行队列每项前后与终态均核对同一 manifest；终态 Rust 未变，文档补终态后只重跑 docs/diff。
恢复后工作树仍为 feature/spec-0288，HEAD f82e4deaa2cc392d97992473a6d612d5df7fb003，
250 个未提交文件（132 tracked modified、118 untracked），暂存区为空。
2026-10-08 00:00 附近 exec-server 连接中断，缓存绿测与门禁未启动；00:50 UTC 后恢复，
先核对实际目录、工作树与日志再续跑，没有重复 patch 或宣称中断时已完成验证。

Mac skipped_reasons 保持 `["address:macos-asan-unsupported", "leak:macos-counter-only"]`，
acceptance.status=partial、requirements_met=false；G5 未闭合。完整测试明确排除既有四个
故障/生成/校准入口，并清除可把普通 exporter 导向外部故障目录的可选环境。
不执行 sanitizer 聚合、故障生成/注入/校准（含故障 IR 生成）、Linux ASan/LSan、远端 CI，
不 stage/commit/push/PR/merge 或清理工作树。Spec 仍为 in-progress。

### 2.14 Map 签名约束的独立本地提交（2026-10-09）

用户授权在保留现有 WIP 字节的前提下整理小型本地提交，不 push/PR/merge。
`50bf67dfa6098b6894a3c20735d26ad9e1889e92` 仅含三个文件：unit signature 的
Hashable/storable 检查、既有 Hashable helper 的模块内可见性及对应回归，
118 insertions / 2 deletions。它仅依赖父 HEAD 已有 Map intrinsic/type 接口，
不依赖未提交的 ordinary borrow、carrier 或 receiver API。

从父提交 `fbc4e0e92a766f010f0f768f30b8cde7fad5e5e5` 导出的隔离候选目录先复现
新回归 1 failed：`Map<List<Int>, String>` 签名未报 L0161。加入两份已校验前置
实现后，六套类型/容器/unit 测试 **282 passed、0 failed、0 ignored、0 filtered**，
workspace/all-targets `cargo check` exit 0，三个改动文件 rustfmt exit 0。
实际索引树 `65a598b82fb75856db2d456865b25d925169d8d1` 的 3263 个文件哈希与
已测试候选逐项一致。没有以当前脏树测试替代候选证明，也没有覆盖当前工作文件。

候选严格 Clippy 和全仓 fmt 均失败；隔离父 HEAD 分别复现完全相同的三处
`checker/map.rs` 参数数量 lint 和四文件格式旧债。归一化格式输出一致，新增三个
文件未新增失败；这些门禁不计作候选通过。首次测试命令误用了不存在的 target，
exit 101 的回执保留，正确命令另记。证据为
`/tmp/koven-n1a-map-signature-commit/{precommit-evidence,terminal}.json`。

提交后索引为空，3463 个原工作文件字节不变，357 项剩余 WIP 保留。
ordinary borrow、Map SSA/runtime 与 N1a 的其余依赖提交尚未整理，不将累计检查点
声明为可独立提交。当前工作树的冻结完整 frontend/codegen 终态另见
SPEC-0289 §17（后继分支记录）；
它不替代本提交的隔离候选证据。Spec 继续 in-progress，Mac partial/false 与 Linux
ASan/LSan 未运行边界不变。

## 3. 技术方案

### 3.1 前端 Typed 描述符发布 (`crates/lang-frontend`)

为保证 frontend 到 codegen 的解耦与精确交接，在 `lang-frontend` 中定义并记录结构化描述符：
- `MapConstructionDescriptor`: 记录构造调用、目标容器类型；
- `MapSizeDescriptor`: 记录 receiver 表达式、容器类型与返回 Int 类型；
- `MapContainsDescriptor`: 记录 receiver、key 表达式；
- `MapGetDescriptor`: 记录 receiver、key 表达式与返回类型；
- `MapPutDescriptor`: 记录 receiver、key、value 表达式；
- `MapRemoveDescriptor`: 记录 receiver、key 表达式与返回类型。

### 3.2 SSA 模型与验证器 (`crates/lang-codegen/src/ssa/`)

1. **类型定义**:
   - `MapContainerKind`: `Map` (只读), `MutableMap` (可变)；
   - `SsaTypeKind::MapContainer { kind: MapContainerKind, key: SsaTypeId, value: SsaTypeId }`；
   - 所有权：`type_ownership` 恒为 `Some(Ownership::MoveOnly)`。
2. **SSA 操作**:
   - `MapConstruct { container: SsaTypeId }` -> `Value(container)`；
   - `MapSize { owner: EntityId }` -> `Value(Int)`；
   - `MapContains { owner: EntityId, key: EntityId }` -> `Value(Boolean)`；
   - `MapGet { owner: EntityId, key: EntityId }` -> `Value(V?)`（对于 Copyable V）；
   - `MapPut { owner: ValueId, key: ValueId, value: ValueId }` -> `Value(container)`（消费旧 owner，返回更新后的 owner）；
   - `MapRemove { owner: ValueId, key: EntityId }` -> `(Value(container), Value(V?))`。
3. **验证器扩展**:
   - 验证 `owner` 必须是匹配的 `MapContainer` 类型；
   - 验证 `key` 与 `value` 类型与容器的类型实参一致；
   - 验证 `MapPut` 与 `MapRemove` 的 receiver 必须是 `MutableMap`。

### 3.3 LLVM 目标代码生成与运行时哈希表 (`crates/lang-codegen/src/llvm/`)

1. **结构布局**:
   - `MapLayout`: Header 为 `{ buckets: ptr, size: size_t, capacity: size_t, tombstones: size_t }`；
   - `SlotLayout`: 每个条目槽位为 `{ state: i32, key: KeyType, value: ValueType }`，其中 `state` 取值：
     - `0`: Empty
     - `1`: Occupied
     - `2`: Deleted (Tombstone)
2. **运行时例程**:
   - `map_construct`: 初始化 `buckets = null, size = 0, capacity = 0`（或分配初始 16 槽位全 0 清理）；
   - `map_size`: 直接提取 Header 的 `size` 字段（转为 i32 / Int）；
   - `map_contains`: 按 key 哈希定位线性探测，若在达到 Empty 前找到 Occupied 且 key 相等则返回 true，否则 false；
   - `map_insert`: 若 capacity 为 0 或 `(size + tombstones + 1) * 4 >= capacity * 3` 则进行扩容（初始 16，后续翻倍）；线性探测查找已存在槽位或首个可用槽位；若键已存在则覆写并析构旧值，否则插入新槽位且 `size += 1`；
   - `map_remove`: 查找匹配槽位，若命中则将 state 置为 Deleted，`size -= 1`，析构旧键并交付原值；
   - `map_drop`: 遍历所有槽位，对 state == Occupied 的槽位分别 drop 其 key（若 MoveOnly）与 value（若 MoveOnly），随后调用 `free(buckets)`。
3. **哈希与键相等逻辑**:
   - 对标量（`Int`, `Boolean`, `Char`）使用位混淆乘数哈希与 `icmp eq`；
   - 对 `String` 使用 FNV-1a 遍历字节计算哈希，比较时先比长度再比字节内容。

## 4. 非目标

- 本 Spec 不支持用户自定义类型的 `Hashable` 实现；
- 本 Spec 不包含 Map 的迭代器（`for (k, v in map)` 留待 M3B 迭代器 Spec）；
- 本 Spec 不实现通用的标量可空结构体（`Int?` 通用语言层 Option），针对 Map.get 的 Copyable V 返回提供端到端查询与非空断言支持。

## 5. 验收标准

- [x] G1: SSA 模型支持 `MapContainerKind`、`SsaTypeKind::MapContainer` 及 6 个原 Map SSA 原语；Map 查询、requireValue、withValue 的新增合同与完整 verifier 回归通过，见 §2.9。
- [x] G2: 单文件与多文件编译单元成功将 `mapOf()`、`mutableMapOf()`、`m.size`、`m.contains(k)`、`mm.put(k, v)`、`mm[k] = v`、`mm.remove(k)` lower 为 typed SSA（保留旧 get/remove 对 nullable V 的结构化拒绝边界），见 §2.9。
- [x] G3: LLVM IR 生成开地址哈希表，正确处理标量（`Int`, `Boolean`, `Char`）与 `String` 键的哈希和比较（Char 通过现行常量物化路径）。
- [x] G4: 哈希表支持动态扩容、哈希冲突线性探测与墓碑复用（正常 native 定向已覆盖；最终过滤全库结果见验收账本）。
- [ ] G5: Map 作用域退出与条目覆盖/删除实现精确析构，通过 ASan/LSan 零泄漏检验。
- [x] G6: 最新冻结片 Rust 尺寸与文档门禁通过（965 Rust / 41 历史超限，581 Markdown）；本片记录见 §2.13，旧片记录与历史欠账保留。

## 6. 验证记录

### 2026-10-07 接手与当前首片（正式批准前历史）

以下保留接手时的判断与检查记录；12:24 UTC 后的正式裁决及当前状态见 §2。

- 工作树 `/private/tmp/koven-spec0288`，分支 `feature/spec-0288`，HEAD
  `f82e4deaa2cc392d97992473a6d612d5df7fb003`；原未提交改动保留，tracked 原始 diff
  备份于 `/tmp/koven-0288-handoff-baseline.patch`。核对进程与打开文件未发现并发写入。
- 本轮没有安装工具或修改系统配置，codegen 命令显式使用本机已安装工具链
  `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`（LLVM/Clang 21.1.8）。
- 原 sanitizer 失败由 IR reader 误选 Apple Clang 21.0 引起；未放宽版本断言。
  原 inline nullable 失败由未提交 lowering 将 T? fallback 到 T 引起；恢复 nominal mapper 的
  受检 NullableHandle 构造后，`rejects_inline_nullable_lowering_without_panicking` 定向 1/1 通过。
- 新限制到达前实际运行了
  `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo test --locked --offline -p lang-codegen native_sanitizer_tests::asan_instruments_generated_user_runtime_and_drop -- --exact --nocapture`：
  1/1 通过，运行耗时 2.66 秒；准确墙钟起止未保留。它执行正常计数程序，调用 `inject()`
  生成故障 IR 并通过 Clang 检查 instrumentation；没有动态运行故障程序或 Linux 检出闭环。
  收到“故障校准/注入只记录”后，该类检查不再执行，也不由全库回归间接触发。
- 正常 Map frontend 定向：`cargo test --locked --offline -p lang-frontend --test type_map --test ownership_map`，
  17/17 通过；覆盖查询 Borrow、put owned、mutable receiver、unit signature bounds、nullable 归一化。
- 正常 native：`cargo test --locked --offline -p lang-codegen native_map_ -- --nocapture`，8/8 通过，
  日志 `/tmp/koven-0288-native-map.log`。新增整表墓碑合法负载先在 20 秒上限内无法终止，
  日志 `/tmp/koven-0288-tombstone-before.log`；有界探查修复后通过。
- 多文件 `unit_map_string_keys_and_move_only_values_execute_with_precise_drops` 定向 1/1 通过，
  正逆 inputs SSA 一致，正常 malloc/free 计数与两次 Resource deinit 匹配。
- 后续全库回归、静态 verifier、fmt、clippy、尺寸及文档门禁结果待本轮最终补录；不提前宣称通过。

### 验收缺口与合同核对

- 第一轮检查点时 G1/G2 仅构造、size/contains、put 和 pointer-like owned remove 子集闭合。
  第二轮已保留并修复五条 Int? 查询 native 测试，以独立 presence/payload identity 保留 V?。
  最新验收结果及未闭合的 M2B/nullable V 边界见第二轮账本。
- G4 的扩容、碰撞、墓碑复用与合法连续增删终止已验证；私有 header 增加墓碑计数，
  `size+tombstones` 阈值、复用计数递减与 rehash 清零由正常计数回归验证。
- G5 只有正常 deinit 与逐指针分配/释放计数证据。Mac 合同保持
  `skipped_reasons = ["address:macos-asan-unsupported", "leak:macos-counter-only"]`、
  `acceptance.status = partial`、`requirements_met = false`；完整 ASan/LSan 检出仅由 Linux 提供。
- 只读核对 PR #64：未合并，正文明确未启用语义，要求显式非 owning binding 和 Missing 三态。
  已合并 PR #67／SPEC0287 的合同是局部 `borrow val`；本 Spec 所称较新裁决是普通 val 推断、
  可选 `borrow V?`，没有独立批准记录。Guide §10 仍排除跨调用借用返回。
  已提出二者的最小具体确认问题，未据候选材料实现返回 loan ABI。
- 保持 in-progress，不归档；本轮用户要求不 push、不开 PR、不合并。

- **Phase 1 (规范与拓扑)**:
  - 待执行。
- **Phase 2 (SSA 与后端实现)**:
  - 通用 Map 存储及 Copyable 查询片段已实施；借用返回与 nullable V 三态待正式启用合同。
- **Phase 3 (测试与门禁)**:
  - 待执行。
- **Phase 4 (归档与 PR 闭环)**:
  - 待执行。

### 2026-10-07 第二轮前的历史检查点（保留 diff，未提交）

该历史检查点按用户当时要求先返回，没有启动新的长测试。第二轮随后继续，当前结果见下表。

| 检查 | 实际结果 / 日志 |
|---|---|
| frontend 全库 `cargo test --locked --offline -p lang-frontend` | 1,850 passed，0 failed/ignored；227.593 秒；`/tmp/koven-0288-frontend.log` |
| 正常 Map native（nullable 后续修改前） | 10 passed；`/tmp/koven-0288-map-final.log` |
| 多文件 Map Borrow 参数、Resource 析构与确定性 | 1 passed；`/tmp/koven-0288-unit-map.log` |
| workspace clippy `--all-targets -- -D warnings`（nullable 后续修改前） | passed；`/tmp/koven-0288-clippy.log` |
| 文档门禁（检查点补录前） | passed，563 Markdown；`/tmp/koven-0288-docs.log` |
| Rust 尺寸（nullable 后续修改前） | passed，886 Rust / 46 历史超限；`/tmp/koven-0288-size.log` |
| 文档与尺寸脚本 unit tests | 84 passed；`/tmp/koven-0288-script-tests.log` |
| Copyable 查询原生红测（后续表示修改前） | 5 failed / UnsupportedNode；`/tmp/koven-0288-map-query-before.log` |
| fmt | 早期通过后新增测试有格式差异；最新 diff 未重跑 |
| 最终 lang-codegen 全库、workspace check | 未运行；故障注入／校准项本轮不得执行 |

Copyable 查询后续改动正在实现：Module 显式登记独立 query-result aggregate identity，
verifier 要求 presence Boolean + Copyable V；MapGet 缺失分支返回标志而非 Abort，
MapResultUnwrap 缺失才 Abort；LLVM 比较准备按存在性和值比较。源码 comparison 包装、
null/!! 交接与 unit result 类型登记尚未实现。**该历史检查点的后续表示修改当时尚未编译验证，以上旧绿灯不代表第二轮完整 diff 全绿。**
第二轮按约定先补接线与聚焦正反例，再重跑受影响门禁及过滤故障类操作的全库回归。

检查点保留全部原有及本轮修改，HEAD 不变，不 commit/push/PR/merge；不归档此 Spec。


### 2026-10-07 第二轮验收账本（本轮检查点）

本轮止于 Copyable、非 nullable V 的 Map 查询常规实现与普通回归，不启用 M2B 的借用返回语法。
复查发现 put 退休 Copyable 源 binding 的遗漏，先用单文件/unit 两项运行回归复现 0/2，
再限定只退休 MoveOnly 输入，补齐 unit 既有下标赋值描述符路由；修复后 2/2 通过。
最终当前源码的过滤全库尚未运行；用户要求本回合收束检查点，不再启动新的长回归。
Module 登记独立 presence/payload identity；SSA verifier 验证注册形状与 Copyable 权限，
LLVM 保留 Missing 与 Found(0/false) 的区别。单文件/unit 下标 receiver/key 及普通短路两侧出口
消费真实清理事实；`!!` 检查 presence，失败时按既定语义 Abort。跨文件 Copyable nullable 结果
经已登记的签名传递，正逆 inputs 得到相同 SSA。Copyable value class payload 同样按具体布局执行。
nullable V 查询保持带 Span 的 UnsupportedNode，未擅自合并三态；普通 inline nullable 的原拒绝边界保持。

| 验收项 → 测试目标/过滤器 | 实际结果 / 日志 |
|---|---|
| Copyable put／下标赋值复用 → `map_put_preserves_copyable` | 先 0 passed / 2 failed，修复后 2 passed；`/tmp/koven-0288-copyable-put-before.log`、`/tmp/koven-0288-copyable-put-after.log` |
| 原报告 inline nullable 边界 → `ssa::lower_frontend_tests::rejects_inline_nullable_lowering_without_panicking` | 最新源码 1 passed；`/tmp/koven-0288-ssa-inline-boundary-round2.log` |
| 当前完整 Map 定向 → `cargo test --locked --offline -p lang-codegen map_ -- --nocapture` | 30 passed / 0 failed / 0 ignored，18.271 秒；`/tmp/koven-0288-map-focused-final.log` |
| 原有五项查询红测 → `native_mutable_map_` | 5 passed；`/tmp/koven-0288-map-compile.log` |
| missing/zero/false、比较、窄化、重复 !!、value class、跨文件 → `map_copyable` | 4 passed；`/tmp/koven-0288-query-cases.log` |
| 查询 identity、Shared key、Hashable/Copyable 限制 → `ssa::verify_operation::map::tests` | 5 passed（Map 定向内）；`/tmp/koven-0288-map-focused.log` |
| nullable V 三态未启用 → 单文件/unit Map 边界 | 单文件已通过；unit 测试 fixture 修正逻辑 package/path 后独立 1 passed；`/tmp/koven-0288-unit-nullable-boundary.log` |
| workspace clippy → `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | 最新修复后 passed，3.298 秒；`/tmp/koven-0288-clippy-final.log` |
| 文档/尺寸脚本 → 仅 `scripts.tests.test_check_docs`、`scripts.tests.test_check_rust_sizes` | 84 passed，15.995 秒；`/tmp/koven-0288-script-tests-round2.log` |
| frontend 全库 → `cargo test --locked --offline -p lang-frontend` | 1,851 passed / 0 failed / 0 ignored，222.247 秒；`/tmp/koven-0288-frontend-round2.log` |
| workspace check → `cargo check --locked --offline --workspace --all-targets` | 最新修复后 passed，1.556 秒；`/tmp/koven-0288-workspace-check-final.log` |
| 最终 codegen 过滤全库 | 已启动后因发现 Copyable put binding 遗漏而停止：214 passed / 0 failed / 1 既有 ignored；exit 143，不能视为全库通过；`/tmp/koven-0288-codegen-interrupted.log` |

静态审计后，最终 codegen 只排除以下四项故障 IR 生成／编译／校准操作：
`native_sanitizer_tests::asan_instruments_generated_user_runtime_and_drop`、
`native_sanitizer_tests::counter_failures_preserve_compile_and_run_evidence`、
`native_generated_owner_tests::export_generated_owner_calibration`、
`native_generated_owner_tests::export_generated_owner_case_with_missing_deinit_fault`。
`KOVEN_GENERATED_OWNER_CASE`、`KOVEN_GENERATED_OWNER_CALIBRATION`、`KOVEN_SANITIZER_ARTIFACTS`
均未设置；普通 clean export 不读取外部 fault.txt。未运行会间接触发注入的聚合脚本或全 scripts test discovery。
四项排除与仓库既有 LLDB ignore 分开报告，不改变任何测试断言、ignore 或 sanitizer 平台合同。
Mac 仍明确跳过 address/leak，`skipped_reasons = ["address:macos-asan-unsupported", "leak:macos-counter-only"]`，
`acceptance.status = partial`，`requirements_met = false`；Linux 检出闭环未运行。

尺寸自审只对有限父模块路由/事实传递增长调整精确例外，Map 与 query-result 算法放独立领域模块；
46 个历史超限文件仍报告为欠账，不通过提高 baseline 或机械切片消除。
HEAD 保持 `f82e4deaa2cc392d97992473a6d612d5df7fb003`；不 commit/push/PR/merge，不归档 Spec。

第二轮检查点当时仍存缺口：完整 codegen 普通回归尚未结束；M2B 返回 origin、caller loan continuation、权限恢复和跨调用 verifier 未实现；nullable V 三态及 String/MoveOnly inline/Resource nullable remove 未闭合。unit 的 MoveOnly 下标赋值清理矩阵未由第二轮 Copyable 用例证明。第三轮进展见下表。

### 2026-10-07 第三轮验收账本

本轮 Phase 4–6 只补已确定的非 nullable V owned remove 与普通下标赋值清理。
先用新增的 2 条 native 与 1 条 unit 回归复现 String/inline 布局、Resource nullable 包装与
unit 键活跃性拒绝，再补最小实现；本轮总共新增 6 条普通 native/unit 与 1 条静态
verifier 契约测试，不生成故障 LLVM。
String/MoveOnly inline 结果使用显式 presence/payload；owned 提取沿用真实 owner 的
NullableBranch/NullableTake proof，条件 drop 不析构 Missing payload。Resource class
保留 pointer-niche，unused nullable 结果在 lexical 出口按逆声明顺序到达 deinit。
普通下标赋值在单文件和 unit 的 liveness/drop planner 消费 K/V，保留 receiver 与 pending 参数。

| 验收项 → 命令/目标 | 实际结果 / 日志 |
|---|---|
| `cargo test --locked --offline -p lang-codegen map_ -- --nocapture` | 37 passed，0 failed，0 ignored；30.573s；`/tmp/koven-0288-map-final-third.log` |
| 原 `ssa::lower_frontend_tests::rejects_inline_nullable_lowering_without_panicking` exact | 1 passed；8.045s（含编译）；`/tmp/koven-0288-inline-third.log` |
| frontend `ownership_map` / `type_map` / `ownership_nullable_when` / `ownership_resource_deinit` / `basic_unit_ownership` / `basic_unit_ownership_compile_contracts` | 84 passed，0 failed，0 ignored；12.239s；`/tmp/koven-0288-frontend-third.log` |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | passed；10.774s；`/tmp/koven-0288-clippy-third.log` |
| workspace check / fmt check | passed；8.271s / 1.892s；`/tmp/koven-0288-check-third.log`、`/tmp/koven-0288-fmt-third.log` |
| `check_rust_sizes.py --base main` / `check_docs.py` / `git diff --check` | passed；894 Rust 文件、46 历史超限文件；563 Markdown；`/tmp/koven-0288-size-third.log`、`/tmp/koven-0288-docs-third.log` |
| 安全 Python 模块 `scripts.tests.test_check_docs scripts.tests.test_check_rust_sizes` | 84 passed；15.971s；`/tmp/koven-0288-script-tests-third.log` |
| 第一次冻结源码的过滤 codegen 全库 | exit 101；1106 passed、2 failed、1 原有 LLDB ignore、4 filtered out；1292.237s；10:22:12–10:43:44 UTC；`/tmp/koven-0288-codegen-full-third.log` |
| 收窄单文件 Resource nullable 包装登记回归 | 1 passed；10.148s（含编译）；`/tmp/koven-0288-wrapper-fix.log` |
| unit 短路套件：carried owner、原 skip 表达式及 OR/AND 两出口 native 计数 | 2 passed；2.173s；`/tmp/koven-0288-short-circuit-fix.log` |
| 修复后 Map 聚焦回归 | 37 passed；31.216s；`/tmp/koven-0288-map-after-full-fixes.log` |
| 修复后严格 clippy / workspace check / fmt check / 尺寸 | passed；`/tmp/koven-0288-clippy-after-full-fixes.log`、`/tmp/koven-0288-check-after-full-fixes.log`、`/tmp/koven-0288-fmt-after-full-fixes.log`、`/tmp/koven-0288-size-after-full-fixes.log` |
| 第二次最终源码过滤全库 | exit 0；库测试 1108 passed、0 failed、1 原有 LLDB ignore、4 filtered out；集成 2 passed、doctest 4 passed，合计 1114 passed；1407.753s；10:56:33–11:20:01 UTC；`/tmp/koven-0288-codegen-full-third-rerun.log` |
| 最终源码一致性与排除核对 | 冻结清单与当前 894 个 Rust 文件的集合及 SHA-256 全部一致；四项排除均未执行；`/tmp/koven-0288-round3-frozen-rust-rerun.json`、`/tmp/koven-0288-round3-rerun-verification.json` |

所有 Cargo 命令使用已安装的 `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，未安装工具链或修改系统配置。
历史 Rust size baseline 原值保留；本轮 7 个既有大文件仅有限接线/封闭 aggregate 绕过增长，
在 policy 逐项记录精确上限、owner、拆分计划及第三轮自审；不声称独立评审。

M2B 借用局部语法仍待用户决定，本轮未启用隐式 val 借用、borrow 返回或 nullable V 三态。
故障生成、注入、静态编译、动态运行和聚合脚本均不执行。过滤全库排除两个
`native_sanitizer_tests`，以及 `export_generated_owner_calibration` 和
`export_generated_owner_case_with_missing_deinit_fault`；普通 export 的可选故障路径所需
`KOVEN_GENERATED_OWNER_CASE`/`KOVEN_GENERATED_OWNER_CALIBRATION` 环境变量均核对为 unset。
Mac 合同仍为 `skipped_reasons = ["address:macos-asan-unsupported", "leak:macos-counter-only"]`，
`acceptance.status = partial`，`requirements_met = false`；Linux 完整 ASan/LSan 闭环未运行。
不 push、开 PR、合并、归档 Spec 或清理其他工作树。

修复后冻结源码的 clippy、workspace check、fmt 和尺寸门禁已通过，以上全库重跑期间未修改 Rust 文件。
最终验收补录后文档门禁 563 Markdown 通过（`/tmp/koven-0288-docs-after-rerun.log`），
`git diff --check` 通过。本轮结束时没有运行中的测试。

第一次全库两个普通失败分别为：`single_resource_deinit_rejects_unsupported_resource_wrappers`（本轮放宽了非 Map 的 Resource? 参数；现已恢复原拒绝边界，只有已登记 Map 结果绕过普通包装检查），以及 `rejects_rhs_only_move_until_frontend_publishes_short_circuit_owner_facts`（第二轮已有真实分支清理事实，旧 known-gap 拒绝断言失效；现保留原表达式，并以 frontend skip 清理事实、OR/AND 的四次正常 clone 分配逐指针释放及 stdout oracle 验证）。两处不改变借用语法或启用 nullable V 三态。第一次冻结哈希留存；修复后的冻结记录另存，不覆盖旧证据。

### 2.7 requireValue 的 generic receiver/slot 借用闭环

本阶段仅接 `requireValue` 的确定只读结果。single/unit 的专用 typed descriptor 发布
实际 receiver/key、K/V 与唯一 receiver 来源；不要求 V Copyable，也不将 nullable V
再次包装。两条 ownership 路径复用普通 binding/source continuation；unit 为此调用的
真实 Shared receiver place 发布 LoanFact，修复此前没有可交接来源 loan 的 L0164。
后端消费实际 call/argument identity 与精确 Call Span，不能用 binding statement Span
冒充 unit 调用 loan 的结束点。key 在查询后结束，receiver 延续至借用结果 scope-end；
put/remove/移动 Map 的冲突负例报 L0135，owned 绑定报 L0163，错误时清空新结果事实。

`MapRequireValue` 输入 active Shared Map source 与只读 key，输出 Shared V loan。
verifier 的类型合同检查独立 V target 和 shared delivery；alias roots、活跃来源与
parent-child/end 顺序复用现有所有权验证。contains/get/requireValue 共享原有有界
lookup 的 Occupied slot 查找；LLVM 在 found 分支取槽位 V 字段地址，不 load/copy/move/
retain V，不制造 payload owner，也不析构来源。Missing 使用既定 Abort；nullable
payload 为 null 时槽位地址仍非空，不能把 present-null 当成 Missing。

两个新增正常 native test functions 各执行四个命中场景和一个独立 missing 源码，共十个
single/unit object/link/run 场景：Map<String,String>、含 String 的 MoveOnly value class、
Resource、现行 pointer-like nullable V 的 present-null/present-value。命中后 scope-end
恢复 Map 变异，Resource 的覆盖和最终清理分别执行原 deinit。readonly Map 参数另有
前端合同、SSA 普通 borrowed wrapper 和 missing native 正例；非空 readonly Map 的
native 命中未单独验证。新测试未添加动态故障或独立 sanitizer/分配计数插桩；旧 Map
逐指针正常释放、清理及资源回归随原 37 条一起重跑。

17:53–17:56 UTC 同一最终 Rust 源码：map_ 为 41 passed（旧 37、新 native 2、新合同 2）；
普通 borrow_result 17、storage 合同 3、下游 SSA 5、四个 native 单项 4、原 inline nullable
拒绝单项 1，均 0 failed/ignored。两项 requireValue native 同时被 map_ 与 borrow_result
过滤匹配，去重后 codegen 为 69 个用例，不能把 71 次测试执行计作不同用例。21 个 frontend
目标为 432 passed、0 failed、0 ignored，其中新增 requireValue ownership 3 个用例。
workspace check、clippy -D warnings、fmt 与尺寸均 exit 0；946 Rust 文件、41 个历史
超限。四个既有大文件仅加入 MapRequireValue 委托/分派，仍在原审阅上限内；没有提高
policy/baseline/max_lines。最终文档及 diff 检查与完整命令、源码清单、patch SHA 记录在
`/tmp/koven-0288-phase8-checkpoint.results.json`，日志为 `/tmp/koven-0288-phase8-*-final.log`。

withValue/non-escaping callback、条件借用、Inout/多来源返回、通用 inline nullable 和新
Resource wrapper 不在本阶段开放。unit 普通 pointer-like nullable 比较仍在实际比较
Span 报 UnsupportedNode，并有保持该边界的正常源码回归；旧 get/remove nullable V
拒绝回归仍通过。完整 codegen/frontend、Linux ASan/LSan、故障生成/注入/校准及间接
聚合、远端 CI 未运行。Mac 合同仍为 skipped_reasons
`["address:macos-asan-unsupported", "leak:macos-counter-only"]`、acceptance.status=partial、
requirements_met=false。Spec 保持 in-progress；未提交、暂存、push、开 PR、合并或清理工作树。

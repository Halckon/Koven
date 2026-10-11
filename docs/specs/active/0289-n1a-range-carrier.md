# SPEC-0289: N1a 单来源范围描述符端到端交付

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：准备或验收 N1a 范围 carrier 首片时 · **唯一真源**：本 Spec；语言语义以 [Guide](../../guide/README.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0289` |
| 所属 Phase | Phase 1 声明；Phase 2 carrier 类型；Phase 3 来源与终止；Phase 4 SSA；Phase 5/6 std 与 native |
| 语言规范 | 当前 v0.43；仅启用已批准 N1a 最小合同，完整旧版已冻结 |
| 批准依据 | 2026-10-08 12:15 UTC 用户明确同意 r3 小修后的 N1a 与立即消费首片本地实施 |
| 前置 Spec | [SPEC-0287](../../archive/specs/0287-m2b-and-map-type-system.md) 的已发布来源合同 |
| 前置 ADR | [ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md)、[ADR-0029](../../adr/accepted/0029-ordinary-borrow-result-continuation.md) |
| 关联 ADR | [ADR-0030](../../adr/accepted/0030-range-carrier-source-facts.md)；List/View 终态见 §12–13，§15 授权与类型选择已落地，receiver 来源/后端待接通 |
| 阻塞项 | receiver 实际 origin/continuation/end 与 SSA/verifier；§15 授权和类型选择首片已落地，§14 CFG 冻结全量终态保持 |
| 影响范围 | lang-frontend、lang-codegen、lang-std；必要 CLI 标准来源接线 |
| 语言语义变更 | 是；已批准最小合同已启用，未实现能力保留精确拒绝 |

此前声明检查点只交付 Phase 1 Parser/AST。当前已推进 Phase 2 类型、来源与构造合同，
以及稳定根绑定、producer 返回/转发和真实 std caller 的 Phase 3 continuation/end。
List 的 take/size/native、具名 for 与同步临时来源已接通；终态见 §12。View 来源
构造/转发终态见 §13，调用前缀提前 return 的冻结全量终态见 §14；§15 交付可信扩展
授权与类型选择，实际 receiver 来源和后端及更广 CFG 仍保留能力门。现行合同见 Guide/ADR-0030；计数批准记录于 §6。

里程碑主线是 M3A 的单来源 take 首片，依赖 M2B 来源与 continuation/end 合同，
并回归 SPEC-0288 的 B2/B7/B8；当前不扩至 consume、Clone、N1b 或更多算法。

## 1. Goal

单文件与编译单元中的连续范围视图有真实内联描述符和根来源，能借用访问/迭代、链式
切范围及合法唯一来源返回；全部活跃依赖结束后恢复根权限，并在 native 实际运行。

## 2. 范围与非目标

首个闭环是单来源 take；随后扩至 drop/dropLast、最小访问/迭代与合法 wrapper。
描述符构造和算法必须分层：编译器提供可复用构造/访问原语，算法 body 在 `.ko`；
不按三个算法名字各增一套 intrinsic。扩展声明只加入首片真正需要的前置。
String、不可 clone 的 MoveOnly、Resource 与 Copyable 共享 carrier 合同。
N1b/借用 filter、用户自定义非逃逸类型、多来源、条件借用、inout 局部/返回、惰性链、
nullable owned remove、Option、T?? 展平和通用 Clone 不在本 Goal。

## 3. 阶段交接与依赖

typed facts 区分构造新描述符与借用已有描述符；发布实际 root origin、caller loan
continuation、绑定/参数/返回与结束事实。SSA/verifier 证明描述符来源和权限恢复，
LLVM 返回内联描述符而非 callee 局部对象地址。root-flat 只去掉父 metadata 依赖；
根保护覆盖全部父、子、兄弟与元素 loan。现有 0288 WIP 是集成输入，保持原验收账本。

临时来源不得经保存或返回越过其有效范围；合法同表达式使用与既有 for-source 合同
保留，正常 break/return 的清理必须有直接事实，不用类型通过替代。

## 4. 实施顺序

1. 声明前置与 carrier 类型红测试；冻结首片 Guide/ADR 和稳定诊断/Span。
2. single/unit 的新描述符与来源事实；合法/非法返回、活跃父子/兄弟借用直接验证。
3. typed SSA、verifier、LLVM 与 native take 小闭环；无前端算法名重推。
4. 在同一来源合同上扩至剩余范围操作、访问/迭代和正常清理；同步 Architecture。

## 5. 唯一验收账本

本节表格保留声明检查点的实际结果；后续阶段逐项证据在 §7–14，当前阶段结果以 §14 为准，前片全量终态见 §13。

| 验收项 / 目标 | 实际结果 | 未运行或未完成原因 |
|---|---|---|
| 声明前置 `parser_n1a_frontier` | 初始 1 passed、2 failed；最终 8 passed、0 failed；另有 inout 扩展红转绿 | carrier/extension 仅完成声明与来源 AST；不代表语义启用 |
| single/unit 不支持语义拒绝 `type_declaration_frontier` | 3 passed、0 failed；明确 L0164/Span，unit validate 失败，single 停在 TypeChecking | 保护后端能力边界；不是 carrier 类型实现 |
| single/unit carrier 类型与来源签名 | 初始类型 1 passed、3 failed；最终类型/来源 7 passed，普通借用签名 3 passed | closed mode 保留真实 from/来源索引；新 descriptor 实际来源尚未证明 |
| 已有 carrier metadata 的 origin/continuation/parent/end | 2 passed、0 failed；Int/String/MoveOnly；实际 origin/来源 loan/parent/child-before-parent end | incoming View 参数的既有存储借用；不是 List 根构造或新描述符交付 |
| 构造、稳定根绑定与可信来源 | §8 七套件41 passed；SourceId 授权、真实操作数、新 descriptor/root-flat/end | 不代表 producer 或 runtime；后续证据见 §9 |
| producer 返回/转发与真实 std caller | §9 直接行为红转绿；host/公开 project 证据 | 当前 List 顶层 take，View/extension 与 runtime 未完成 |
| SSA descriptor/origin/end 与权限恢复 verifier | 未实现、未运行 | 上游事实已发布，typed SSA 内联 descriptor/ABI 尚未接通 |
| String/MoveOnly/Resource 正常 native、精确清理 | 未实现、未运行 | 后端闭环未接通 |
| 分支/父子/兄弟/元素借用、scope/return/for break 清理 | 未实现、未运行 | 相应阶段未接通 |
| 共享 Parser/名称/既有借用回归 | 最终 75 integration suites：415 passed、0 failed、0 ignored；parser 内部 28 passed、178 filtered | 定向共享契约覆盖；不是 frontend 全量 |
| 普通借用 SSA 消费者 `ssa::borrow_result` | 9 passed、0 failed、1153 filtered | 正常 SSA/verifier 与结构化拒绝；未运行 native |
| fmt/check/clippy、尺寸、docs/diff | 全仓 fmt/workspace check、frontend 严格 clippy 通过；尺寸门禁通过，969 Rust / 41 历史超限 | 最终 docs/diff 收据在本片检查点；无新增长例外，class.rs 1088 行未增长 |
| 已批准计数边界与最终过滤全库 | §6 合同已冻结；本片未实现算法、未运行计数/native 或 codegen 全库 | 当前只交付声明前置；后续实现与完整 Spec 验收继续推进 |

## 6. 已批准计数边界与交付

2026-10-08 13:08 UTC 用户在 `Sentinel_585a20f1438881919a2a73a139eb2662`
明确批准：“同意负数走 Abort，超过 size 则截到边界”。此决定只冻结首片计数合同，
不启用更多 API、扩展语义或清理实现。

take/drop/dropLast 在 `n < 0` 时 Abort；其余情况先取 `k = min(n, size)`，
再计算范围。相对源范围，take 保留 `[0, k)`，drop 保留 `[k, size)`，dropLast 保留
`[0, size - k)`；最大 Int 也先 clip 后算，不先执行可能溢出的边界加减。

| 输入 | take | drop | dropLast |
|---|---|---|---|
| `n = 0` | 空 | 原范围 | 原范围 |
| `n = size` | 原范围 | 空 | 空 |
| `n > size` | 原范围 | 空 | 空 |
| `size = 0`、`n >= 0` | 空 | 空 | 空 |
| `n < 0`，包括空源 | Abort | Abort | Abort |

borrow/consume 的保留数量一致，但 owner 移交与正常清理各按其来源/消费合同验证；
Abort 不展开。索引访问仍按现行越界规则处理，不能用此计数合同改成 clip。
通用复制能力另审，不以 String 特例收尾。
现行 v0.43/ADR-0030 已启用最小 N1a 合同；显式 SourceId 授权与稳定根构造见 §8，
可信 std 宿主与 producer 交付见 §9；扩展选择与 runtime 未完成，继续精确拒绝。
此账本保留此前声明阶段的实际证据。
本次只本地编辑和测试，不暂存/提交/push/PR/merge，不关闭或归档 0288。
故障生成/注入/校准及间接脚本严格排除；Mac 指定 skips/partial/requirements_met=false 保持。

声明红测试在 2026-10-08 12:35 UTC 执行：
`cargo test --locked --offline -p lang-frontend --test parser_n1a_frontier`；
日志 `/tmp/koven-n1a-declaration-red.log` 与同名 `.json` 收据。`T??` 拒绝回归通过；
失败来自现行 parser 的真实语法诊断，不是编译错误、ignore 或检测规避。

本片最终源码的聚焦命令与收据：

- `cargo test --locked --offline -p lang-frontend --no-fail-fast` 加 75 个显式 `--test`；
  完整目标/数量见 `/tmp/koven-n1a-parser-final-regression.log` 与同名 `.json`。
- `cargo test --locked --offline -p lang-frontend --lib parser::`；
  最终收据 `/tmp/koven-n1a-parser-final-private.log` 与同名 `.json`。
- `cargo test --locked --offline -p lang-codegen --lib ssa::borrow_result`；
  `/tmp/koven-n1a-borrow-consumers.log` 与同名 `.json`。
- `cargo check --locked --offline --workspace --all-targets`、
  `cargo clippy --locked --offline -p lang-frontend --all-targets -- -D warnings`、
  `cargo fmt --all -- --check`；对应 `koven-n1a-workspace-check`、`koven-n1a-clippy`、
  `koven-n1a-fmt` 的 `/tmp/*.log` 与 `.json` 收据。
- `python3 scripts/check_rust_sizes.py --base origin/main`、`python3 scripts/check_docs.py`、
  `git diff --check`；最终结果与源码/未动文件哈希见 `/tmp/koven-n1a-parser-checkpoint.json`。

首次 frontend all-targets 编译曾因旧测试的完整 Function pattern 缺新字段而失败；已显式适配
并保留普通函数 receiver 为 None 的断言，最终 workspace check 通过，未隐藏该次失败。

## 7. 前端类型与已有 metadata 检查点

本轮 compiler-bound `IntrinsicTypeConstructor::View` 在 single/unit 复用相同身份规则；
源码同名 nominal 不取得 carrier 权限。显式和推断 owning 局部、own/inout 参数、字段、
nullable、显式/推断 generic argument 受非逃逸限制。封闭 `CallableResultSource` 区分
Owned/Borrow/Carrier，真实 from/source Span 与参数映射到达已选择调用，
普通 borrow getter 只投影既有存储借用。Carrier 签名必须来自相同元素类型的 Borrow
List/View；声明事实不替代实际返回来源证明。

初始推断红测试实际确认 unit 普通 val 漏过限制；修复过程中曾有 body table 接线
编译错误及一个使用错误泛型语法的 fixture，均单独留收据，不计为行为通过。
新 carrier 非法来源红测试为 0 passed、1 failed，来自缺 L0162 的实际诊断。

该历史检查点未完成：可复用范围构造原语、可信标准库来源授权、扩展 canonical 选择、新 root-flat
descriptor 存储与继承根事实、临时表达式/for、元素访问、SSA/verifier/native take。
新的 carrier 声明继续 L0164；backend 对 View 类型仍 UnsupportedNode，不能按 owned
List 或普通借用返回编译。本轮没有 count/native 执行、codegen 全库或 Linux 动态检查。

下一最小纵向路径固定为 List/View 的同一个 take：可信 std 来源接线与 `.ko` body →
可复用范围构造及 root/continuation/end → SSA 内联 descriptor/verifier 权限恢复 →
String/MoveOnly/Resource 正常 native 与 count 边界；不新增后续算法。

最终源码验证（2026-10-08 本地检查点；无测试进程仍在运行）：

- `cargo test --locked --offline -p lang-frontend --no-fail-fast` 加 15 个显式 `--test`：
  **129 passed、0 failed、0 ignored**；具体目标见
  `/tmp/koven-n1a-semantic-final-focused.log.json`，含 carrier/type/ownership、普通借用、
  Map、sequential storage、Copyable 与 ownership primitives。
- `cargo test --locked --offline -p lang-frontend --lib type_checking::tests::`：
  **3 passed、0 failed、203 filtered**。
- `cargo test --locked --offline -p lang-codegen --lib ssa::borrow_result`：
  **9 passed、0 failed、1153 filtered**；普通借用消费者，没有 native 或故障 IR 注入。
- workspace/all-targets check、frontend/all-targets 严格 clippy、全仓 fmt check 均通过；
  精确 argv/起止时间/exit 位于 `/tmp/koven-n1a-semantic-final-{workspace,clippy,fmt}.log.json`。
- 文档检查器测试 **37 passed**；docs **603 Markdown**；尺寸门禁 **977 Rust / 41
  历史超限**通过；没有新增尺寸例外。初次尺寸失败来自 container 超出既有 1180 行
  上限，已把完整元素存储性检查归入同领域子模块并重验。
- `git diff --check` 通过；阶段增量 diff、SHA、原 WIP 未动文件保全核对及未运行项
  见 `/tmp/koven-n1a-semantic-checkpoint.patch` 与同名 `.json`。

进程枚举被沙箱拒绝，未绕过；阶段基线哈希没有发现授权文件之外的变更。
HEAD/branch 保持原状，未暂存、提交、push、PR、merge。本检查点不宣称 M3A/take
或整个 SPEC-0289 已验收；typed/ownership 新 descriptor 的缺口保持在上文列表。

## 8. 构造与稳定根绑定检查点

已实现 SourceMap-owned SourceId 授权；重复授权幂等，foreign-map 和其他来源不继承。
授权不启用 extension，也不绕过来源参数检查。CLI 对可信标准源码的接线尚未实现。
compiler-bound rangeView 发布真实三操作数、List/View 来源与元素 identity；源码同名
函数不获得事实。PrimitiveCall 不伪造 from，类型错误撤销可执行构造事实。

新 descriptor 的稳定绑定续接实际 source loan，并区分 NewRangeDescriptor 与既有
metadata 借用；View 派生 root-flat，metadata alias 仍保留 parent。三组 single/unit
所有权回归覆盖 owning List 根、活跃兄弟阻止移动、根继承与最后使用后的结束/恢复。
没有将 carrier 放进普通 borrow-return ABI。

构造行为红测试为 1 passed、2 failed；稳定根红测试为 0 passed、3 failed。
首次来源测试因 fixture 缺 package 失败，修正后才取得行为证据；收据均保留。
2026-10-08 15:19 UTC 提炼特殊调用分派后，最终七个 integration suites 为
**41 passed、0 failed、0 ignored**，完整 argv 见
`/tmp/koven-n1a-producer-close-focused.log.json`。

workspace/all-targets check、frontend/all-targets 严格 clippy、全仓 fmt check 通过；
收据为 `/tmp/koven-n1a-producer-close-{workspace-fresh,clippy,fmt}.log.json`。
旧 target 两次 check 读取旧 metadata 失败；使用独立 target 后源码编译通过，未删除
缓存或隐藏失败。初次尺寸门禁 callable.rs 超三行，已按特殊调用职责提炼至1042行；
没有修改尺寸政策或删除文档凑行。

producer 实际返回与转发、可信 std 加载、临时/for 与元素 loan、SSA/verifier/native
仍未实现；计数 body/runtime 没有运行，不宣称 take 或整个 Spec 完成。

## 9. producer 与真实标准库宿主检查点

single/unit 从实际 expression body 或唯一 terminal return 建立构造依赖，按已选择
callee identity 和真实参数 symbol 求有限证明闭包；签名、自身或互相递归不能建立
构造证明。body 检查 actual root 等于声明来源后才发布 RangeReturnOriginFact，普通
borrow-return API 保持既有存储含义。返回与 caller 转发保留真实 source loan，稳定
caller 新 descriptor 绑定发布 root/continuation/end；诊断/deferred 原子清空这些事实。

标准库通用 `take(source: List<T>, count: Int)` 的负数 Abort 与 clip 在
`lang-std/koven/algorithms/ranges.ko` 中实现。CLI project 加载编译期绑定的仓库资产，
只授权宿主创建的 SourceId；用户同名路径/package 不取得能力。公开 import/alias
采用现有 unit 名称选择，没有 take 名称特判或用户源码自行授权。

producer 行为红测试 **0 passed、4 failed**，真实失败为未证明交付/错误 metadata
移动；首轮及共享九套件回归绿色，后者 **47 passed、0 failed、0 ignored**。
公开 project 加载红测试 **0 passed、1 failed**，来自 L0148/L0080 未解析 import；
修复后 String/MoveOnly 两种调用到达 UnsupportedSource/UnsupportedNode 且无输出。
host 两项测试直接检查真实 from、caller loan、root/end 和伪标准来源拒绝。
初次 host 接线编译错误与 Long fixture 错误均保留日志，修正后才计行为通过。

当前只交付 List 的顶层 take 前端路径；View receiver/extension、临时/for/元素 loan、
SSA 内联 descriptor/verifier 与正常 native 计数/清理尚未验收。前端 `.ko` body
通过不是 Abort/clip runtime 证据；不扩展其他算法、consume 或 Clone。

本子检查点最终证据（全部使用 `--locked --offline`，Cargo 串行，LLVM21.1.8）：

- frontend 十二个显式 integration suites：**63 passed、0 failed、0 ignored**，含
  producer 六项（String/MoveOnly、顺序无关/跨来源转发、错误 root、递归拒绝、错误
  body 原子撤销）及普通 Borrow、Map、类型/构造/来源回归；完整 argv 与收据为
  `/tmp/koven-n1a-producer-final-focused.log.json`。
- 真实 std host **2 passed**、公开 take project **1 passed**（内含 String/MoveOnly），
  project_cli **13 passed**、native_cli **10 passed**；对应
  `host-loader-green3`、`host-public-green`、`producer-project-regression`、
  `producer-native-cli` 的 `/tmp/koven-n1a-*.log.json`。
- 普通 Borrow SSA 消费者 **9 passed、0 failed、1153 filtered**，
  `/tmp/koven-n1a-producer-final-borrow-consumers.log.json`；未改 codegen 源码。
- lang-std 源码资产 **1 passed**；workspace/all-targets check，frontend/CLI/std
  all-targets 严格 clippy、全仓 fmt check、CLI build 通过。
  首次最终 clippy 仅因新测试显式解引用失败，修正后 `final-clippy2` 通过，未放宽 lint。
- 正常健康 take 程序直接尝试 native build：**exit 1**，
  UnsupportedSource / UnsupportedNode；没有程序输出文件，因此没有执行 runtime。
  源码位于 `/tmp/koven-n1a-native-red`，实际命令与失败收据为
  `/tmp/koven-n1a-producer-native-red.log.json`。这条失败锁定后端缺口，不计通过。

本片没有运行 frontend/codegen 全量、N1a count/cleanup runtime、Linux 动态检查或
故障生成/注入/校准；Mac 指定 skips/partial/requirements_met=false 不变。
尺寸/docs/diff 的最终结果及阶段增量 diff/WIP 保全哈希见
`/tmp/koven-n1a-producer-checkpoint.json`；HEAD/branch 不变，未暂存或提交。

## 10. 健康公共 CLI native 纵向首片

稳定 List 来源沿真实 `take` Koven body、NewRangeDescriptor、borrow val、View size
走通正常 project CLI。SSA RangeView 保留源 List identity；构造/调用各交付 metadata
value 与 protecting loan。producer 的 carrier_return/RangeReturn 与普通 Borrow ABI
分离，实际来源按入口参数及 CFG alias 验证；caller 续接该 loan。LLVM 使用内联
`{root header pointer, begin, end}`，不复制或分配元素；构造检查真实边界。
RangeEnd 结束 metadata 后清理 protecting/祖先 loan；metadata alias 和普通 View
borrow-return 使用 caller 的 metadata storage，结束顺序由 verifier 验证。

最初公共 CLI size 红例 **0 passed、1 failed**，真实错误为 UnsupportedNode；接入后
曾因 Borrow Int 边界未读出、View Borrow 参数未列入类型合同、入口 source 经 CFG
传输的 loan identity 未接续而失败。保留实际红日志；未放宽断言或跳过该用例。
首次公共 CLI 绿例 **1 passed**，实际运行 String/MoveOnly；最终五项 CLI **5 passed、
0 failed、0 ignored**，含 Resource 转移及析构一次、metadata alias/普通返回、count
0/空来源/等长/超长/Int 最大值与负数 Abort。证据为
`/tmp/koven-n1a-native-final-cli.log.json`。早期保留 project 本体也已 build/run **exit 0**、
stdout `done`，对应 `/tmp/koven-n1a-native-red-{build,run}-green.log.json`。

single/unit SSA/LLVM 回归和纯 verifier 负例 **7 passed、0 failed、0 ignored**（包括
三个已有范围相关回归，新增四项），
`/tmp/koven-n1a-native-final-focused.log.json`。metadata alias 与擦除合同曾为 **5 passed、
2 failed**，修正后全部通过；负例只验证 SSA，不编译缺陷 IR。三个初次尺寸失败已按
职责提炼 View member、range expression dispatch 和完整 runtime requirement collection；
不修改尺寸政策。

本首条路径使用普通公共 CLI，没有手工 SSA 替代用户源码。仍未验收 View extension、
View 来源的新 descriptor、临时 owner、元素 loan/读取和 for，不能称整个 SPEC-0289
或 M3A 完成。Linux ASan/LSan 未运行，Mac 指定 skips/partial/requirements_met=false
保持不变；故障生成/注入/校准未运行。本阶段最终门禁与增量 diff/WIP 保全收据见
`/tmp/koven-n1a-native-checkpoint.json`，未暂存、提交或发布。

本检查点最终门禁：frontend 全量（含 unit、ownership/type integration 与 doctest）
**1934 passed、0 failed、0 ignored**，151 个套件结果；原 inline nullable 拒绝回归
**1 passed**，普通 Borrow SSA **9 passed**。workspace/all-targets check、workspace
all-targets clippy `-D warnings`、fmt check、尺寸（1000 Rust、40 历史超限）、docs
（603 Markdown）与 diff check 均通过。收据为
`/tmp/koven-n1a-native-final-{frontend,nullable,borrow,workspace,clippy,fmt-check,size,docs}.log.json`。
按用户要求先交回审查，当前源码的过滤 codegen 全库没有启动；它仍是接续验收项。
本片未重跑两个 sanitizer 与两个故障 export 项，不将这些排除称为通过。没有仍在
运行的已启动测试，没有接续后台队列。原分支/HEAD 与既有 WIP 保留。

## 11. 同一 take 路径的只读元素与具名 for

本片沿真实 std take、稳定 List 与具名 View，增加独立 View provider 和只读
RangeElementPlace；没有增加算法、extension、索引、consume/N1b 或临时 owner 延寿。
新 descriptor 与 protecting/祖先 loan 都作为明确 CFG slots 运输，verifier 对每个入边
保持真实配对；不能仅凭相同 List 类型替换根。element/字段/调用子 loan 结束前，metadata
仍受保护。file loop owner 摘要误将 borrow val 注册 owned 的缺陷在前端修正，后端仍拒绝
普通 Drop/Consume 擦除 metadata。

公共 CLI 首个读取红例 **0 passed、1 failed**，L0159 拒绝 View provider；同函数红例
另揭示当前 for last-use 保守到词法 scope。实现保留该边界，使用显式词法结束证明恢复
根权限，不新增跨循环 last-use 推断。后续 single 红例因错误 metadata 条件 Drop 被
UnsupportedNode 拒绝，修正来源分类后 single/unit 正常路径通过。负 verifier 用例不
交给 LLVM/native；正常源码 IR/native 用例继续经过真实 frontend 流水线。

当前通过证据：

- 公共 CLI **8 passed、0 failed、0 ignored**，19 个实际 build/run 程序：String/
  MoveOnly/Resource 读取、具名新 descriptor、0/empty/max count、empty/break/continue/
  return、body-local Resource 逆序清理及来源元素只析构一次。
  `/tmp/koven-n1a-element-final-cli.log.json`。
- codegen range 聚焦 **11 passed、0 failed、0 ignored**，其中八项 N1a 和三项既有回归；
  single/unit 的 for/break/continue/CFG、正常 LLVM 与纯 verifier 拒绝均有直接证据。
  `/tmp/koven-n1a-element-final-range4.log.json`。
- 类型、iteration、multifile ownership/type、普通借用 last-use 聚焦通过；另有六项
  range ownership 通过，证明 View provider、根/MoveOnly 元素移动拒绝、没有 metadata
  owned Drop，以及未证明临时 range provider 的 L0164。
  `/tmp/koven-n1a-element-frontend-focused.log.json`、`/tmp/koven-n1a-element-temp-guard.log.json`。
- frontend 全量 **1938 passed、0 failed、0 ignored**，151 个套件结果，包含 12 个 doctest；
  `/tmp/koven-n1a-element-final-frontend.log.json`，253.232 秒。
- workspace/all-targets check、workspace/all-targets clippy `-D warnings`、fmt、尺寸与 docs
  通过。尺寸报告 1006 Rust、38 历史超限；docs 检查 603 Markdown。
  首次尺寸检查发现 container provider 测试与 unit branch 文件超额，按完整职责提炼，
  没有修改尺寸策略或压行。

Rust 1006 个文件已冻结为 `/tmp/koven-n1a-element-rust-freeze.json`；frontend 全量与最终
CLI 已结束。过滤 codegen 全库正在运行（含 integration/doctest），命令为
`cargo test -p lang-codegen --no-fail-fast --target-dir /tmp/koven-n1a-producer-target --`，
加下列四个显式 `--skip`，日志 `/tmp/koven-n1a-element-final-codegen.log`；终态追加到本节：

- `native_sanitizer_tests::asan_instruments_generated_user_runtime_and_drop`
- `native_sanitizer_tests::counter_failures_preserve_compile_and_run_evidence`
- `native_generated_owner_tests::export_generated_owner_calibration`
- `native_generated_owner_tests::export_generated_owner_case_with_missing_deinit_fault`

四个既定故障入口及间接
调用严格排除，Linux ASan/LSan 未运行；Mac 指定 skips、partial、requirements_met=false
保持。临时/extension/从 View 构造和更广控制流能力仍有缺口，不关闭整个 SPEC-0289。
没有暂存、提交、push、PR 或 merge。

2026-10-08 本轮按用户要求先交回检查点审查：过滤 codegen 全库保留运行、不终止；
执行 session `80939`，日志及最终 `.log.json` 为
`/tmp/koven-n1a-element-final-codegen.log`（终态前不计为全库通过）。没有排队的后续
Cargo 检查，父线程须等当前命令结束再使用同一 target。当前增量 diff、保全及冻结哈希
见 `/tmp/koven-n1a-element-checkpoint.{patch,json}`。


## 12. 同一 take 路径的临时来源检查点

本片只接通真实 List<T> 来源的同表达式立即 Borrow 和 for-source hidden owner。
RangeUseFact 保留构造表达式、实际 source call/argument loan、根和消费点；unit 聚合
与稳定排序遗漏在本片修复。实际 source loan 在 producer 返回后转交外层 call/provider，
后续参数不能趁 descriptor 存活移动根，消费结束后恢复权限。descriptor 与 root/protecting
loan 沿既有 CFG slots 配对运输；退出先结束 element/provider，再结束 descriptor/祖先
loan，最后由前端清理事实析构实际临时 List。continue 保留来源，break/return/正常
退出结束来源；body-local Resource 先逆序清理。连续描述符按 live metadata 匹配清理
槽位，不能依赖复用的旧索引。保存或错误返回临时范围仍 L0162。

初始公共 CLI 健康红例 **0 passed、2 failed**，均 L0164；接通后首个 native 绿为
**2 passed、0 failed**，真实 std take、source factory once、Resource 在消费后逆序一次。
`/tmp/koven-n1a-temporary-cli-red.log.json` 和
`/tmp/koven-n1a-temporary-cli-green-candidate.log.json`。

本片已取得的聚焦证据：

- range ownership **9 passed**；来源身份、命名根保护/恢复、禁止临时保存/错误返回。
  `/tmp/koven-n1a-temporary-front-regressions.log.json`。
- temporary SSA/native **5 passed**；single/unit 实际 std take 的连续构造、正常/break/
  continue/return 与纯 verifier 拒绝。String/MoveOnly/Resource 共 42 个健康 native
  身份计数程序，每次实际释放元素分配 0/1 与 List 分配 2，顺序 `[1,0,2]`；不生成
  手工故障 LLVM，不执行故障校准。
  `/tmp/koven-n1a-temporary-ssa-native3.log.json`。
- 新 schema 负例分别在 single 与 unit 删除 continuation/source loan 或改错根，仅
  运行前端产物验证；它们不进入 LLVM/native。完整终态收据待追加。
- 调用所有权检查及 String operand 清理按完整职责提取；尺寸策略保持原样。
  当前尺寸检查 1010 Rust、38 历史超限通过；最终 fmt/check/clippy/docs 待追加。

§11 的冻结源码过滤 codegen 重跑已于 2026-10-08 18:20 UTC 结束：library
**1165 passed、0 failed、1 ignored、4 filtered**，integration **2 passed**、doctest
**4 passed**，退出码 0；证据 `/tmp/koven-n1a-element-codegen-recovery/status.json`。
该全量只覆盖先前 element 冻结源码，不能用来宣称本节新增临时来源已全量通过。
当前源码的最终过滤全量、frontend 全量及 CLI 完整范围回归待本片检查点追加。

View receiver/extension、从 View 构造、更广 CFG（例如调用前缀分支转移）继续待后片；
不添加算法、consume/Clone/N1b，不关闭整个 SPEC-0289。Mac 的 address/leak 指定
skips、partial、requirements_met=false 保持；Linux ASan/LSan 未运行。没有暂存、提交、
push、PR、merge；故障生成/注入/校准及间接脚本均未运行。


2026-10-08 19:13 UTC 最小绿检查点补记：

- frontend 全量 **1943 passed、0 failed、0 ignored**，151 个套件结果（含 12 doctest），
  255.212 秒，退出码 0；`/tmp/koven-n1a-temporary-final-frontend.log.json`。
- 公共 CLI 完整范围 **15 passed、0 failed、0 ignored**，38 个真实 build/run 程序；
  新增 empty、连续两次临时调用/for、显式消费恢复、body-local Resource 与跳转清理。
  `/tmp/koven-n1a-temporary-final-cli.log.json`。
- frontend 临时事实过滤 **4 passed、0 failed**（两项 schema 负例及两项外部 ownership
  回归），150 个套件结果；`/tmp/koven-n1a-temporary-facts-negative.log.json`。
- workspace/all-targets check、严格 workspace/all-targets clippy、fmt、尺寸、docs 和
  diff check 均退出 0；收据 `/tmp/koven-n1a-temporary-final-{check,clippy,fmt,sizes,docs,diff}.log.json`。
  尺寸 1010 Rust、38 历史超限；docs 603 Markdown；未改策略或添加增长例外。

增量 `/tmp/koven-n1a-temporary-checkpoint.patch` 与 `.json` 保存相对本片起点的
37 个既有文件变更、4 个新文件、零删除；3409 个文件原样保留。标准库 take、0288、
尺寸策略、原两条失败所在文件及故障入口保持原样。1010 Rust 冻结为
`/tmp/koven-n1a-temporary-rust-freeze.json`；分支/HEAD 未变，暂存区为空。
当前临时来源路径已绿，整个 SPEC-0289 仍 in-progress。

最终过滤 codegen 全量已启动且只运行这一项 Cargo：
`cargo test --locked --offline -p lang-codegen --no-fail-fast --target-dir /tmp/koven-n1a-producer-target --`
加 §11 同四个 `--skip`；独立日志及终态状态为
`/tmp/koven-n1a-temporary-codegen-final/{codegen.log,status.json}`，worker PID 24997、
Cargo PID 25007。启动前源码冻结匹配且任务 target lock 无持有者。终态前不计为全量通过；
不执行故障注入/校准、不发布。最终结果在本节继续追加。


2026-10-08 19:16 UTC 用户要求立即交回可核验检查点；本片停止扩展，保留有效全量测试。
任务范围内 lsof 确认 worker/Cargo/test binary 持有独立日志；当前冻结仍一致，暂无失败行。
已知 PID signal-0 探测被环境 EPERM 拒绝，未重试或提权；日志/状态与精确文件持有者提供
当前运行证据，见 `/tmp/koven-n1a-temporary-codegen-final/handoff-running.json`。
没有排队的后续 Cargo 检查；父线程须等本次最终测试终态后再使用同一 target。
最新本片 diff 与保全账本为 `/tmp/koven-n1a-temporary-handoff.{patch,json}`，原绿检查点
与冻结文件保留。下一步只接收终态、核对冻结及补记验收；不提前扩展后片。


2026-10-08 临时来源最终全量终态补记（19:37:25 UTC）：

| 测试组 | passed | failed | ignored | filtered | 时间 |
|---|---:|---:|---:|---:|---:|
| lang-codegen library | 1170 | 0 | 1 | 4 | 1437.39 s |
| const_native_view_compile_contracts integration | 2 | 0 | 0 | 0 | 2.05 s |
| lang-codegen doctest | 4 | 0 | 0 | 0 | 0.22 s |
| 合计 | 1176 | 0 | 1 | 4 | worker 1456.018 s |

原执行 exit code 0。临时来源五项全部实际运行并通过，包括 String/MoveOnly/Resource
42 个正常 single/unit native 身份计数程序。四个 --skip 与 §11 固定名单完全一致，
四个禁用入口没有执行；唯一 ignored 为 LLDB debugserver task-port 权限用例。
1010 Rust 冻结与全部交接文件哈希均一致，日志与 target lock 已无持有者。
19:28 附近工具读取断线恢复后继续接收原 worker/Cargo，未启动重复测试。

终态 `/tmp/koven-n1a-temporary-codegen-final/{status.json,codegen.log.json}`、独立核验
`terminal-verification.json`；日志 SHA256
`e359b97b460a000daae170a0747b681ec36e842d90e7b4c4b9f41b11416a1237`。
本补记只确认临时 List 来源首片；SPEC-0289 仍 in-progress。Mac skips 精确保持
`["address:macos-asan-unsupported", "leak:macos-counter-only"]`，acceptance.status 为
partial、requirements_met 为 false；Linux ASan/LSan 未运行。故障生成/注入/校准、
提交/push/PR/merge 均未执行。


## 13. View 来源 root-flat 构造与转发检查点

本片依据用户继续同一个 take 的 View 来源构造/转发最小范围授权，不扩至更多算法。
实际 std ranges.ko 加入 View<T> 的顶层 take 重载；沿用现有普通函数候选选择与固定
资产 SourceId 授权，没有接通 receiver extension 或修改 accepted ADR 的宿主决定。
可信扩展仍 L0164；后片 CFG、consume/Clone/N1b 仍未启用。

初始 `take(parentView,1)` 红例 L0140（仅 List 候选）；后续 single/unit signature、
RangeConstruct/RangeCall、root provenance/capability 与 LLVM metadata 构造已接通。
子 descriptor 继承 List header 根和绝对坐标，父 metadata 可在子使用前结束；借用
已有 metadata 保留实际依赖。非零父 begin 与可信 producer 返回转发使用受授权测试
来源，不增加公开标准库算法。嵌套临时表达式初始揭露外层 facts 误以父 View 临时值
为 root；专门前端红例确认后，Shared 实参继承真实 List 根，清理只登记一次，父临时
metadata 在子构造返回后结束，最终消费者/provider 负责原 List 清理。

阶段已通过证据：

- `/tmp/koven-n1a-view-native2.log.json`：View SSA/native **8 passed、0 failed**；
  30 个正常 single/unit native 身份计数程序，覆盖 String/MoveOnly/Resource 的父→子→
  孙、已有 metadata alias、嵌套临时立即 Borrow/for、0/clip/Int 最大值、非零父起点与
  合法可信返回转发；每个 List/元素分配的释放身份与顺序必须匹配，metadata 不分配。
  两项纯 verifier 负例拒绝活跃子范围时释放集合，以及签名仍正确的 sibling 返回根；
  无缺陷 Program 被交给 LLVM/native。
- `/tmp/koven-n1a-view-cli-first.log.json`：公共 CLI 新增 **2 passed、0 failed**，
  11 个真实 build/run，覆盖三类元素、范围 clip/empty/Int 最大值和负数 Abort。
- `/tmp/koven-n1a-view-front-phase2.log.json`：10 个相关前端套件 **380 passed、0 failed、
  0 ignored**；range 类型/授权、origin/continuation、普通 Borrow、iteration、multi-file
  类型与 basic unit ownership。初始门禁命令误填不存在的 type_compilation_unit target，
  在执行测试前退出 101；已改为真实 multifile_type_checking/basic_unit_ownership 并完整通过。
- 嵌套 root facts 专项 `/tmp/koven-n1a-view-root-facts.log.json` **10 passed**；上述最终
  前端门禁也覆盖新增的 derived child 活跃时禁止 root move 拒绝例。

后续 range 完整回归、CLI 完整范围、fmt/check/clippy/尺寸/docs/diff 与冻结全量收据
待本节追加；现阶段不宣称整片或整个 SPEC-0289 完成。先前 List 全量只确认 §12 源码。
Mac 指定 address/leak skips、partial 与 requirements_met=false 保持；Linux ASan/LSan
未运行。未暂存、提交、push、PR、merge；故障生成/注入/校准及间接脚本均未执行。

自动审批拒绝过拟议的 List 路径收窄调整，理由是类型相等判断/早退可能放宽 SSA root
验证。该调整未应用，已撤回；保留当前严格 root/capability 验证及已通过拒绝回归，
没有重试或绕过拒绝。临时定位用的 test-only verifier 打印已移除，orchestrate.rs
与本片起点哈希完全一致。


2026-10-08 View 阶段最终聚焦补记：

- View 最终 `/tmp/koven-n1a-view-native-final.log.json` **8 passed、0 failed**，50 个
  健康 native 身份计数程序；在先前 30 个基础上补父/子/元素同时活跃，以及嵌套临时
  范围的 break/continue/return；没有手工故障 LLVM 或故障校准。
- range 全套 `/tmp/koven-n1a-view-ssa-regressions.log.json` **21 passed、0 failed**；
  包含先前 List/temporary 回归、新 descriptor/borrow metadata 区分及纯 verifier 拒绝。
  其后只扩充两个测试函数的正常 native 情况，最终 View 8 项已重新运行并全绿。
- CLI 完整 `/tmp/koven-n1a-view-cli-final.log.json` **17 passed、0 failed、0 ignored**，
  49 个实际 build/run 程序；先前 List 来源与新增 View 来源均运行。
- 宿主 SourceId/canonical 选择 `/tmp/koven-n1a-view-host-green.log.json` **2 passed**。
  旧单 producer 断言先红（1 passed/1 failed），最终逐一核对两种构造事实、来源 Span、
  不同 canonical 声明 identity、同一 owning root、真实来源 loan 与 parent-before-child
  metadata 终止；用户伪造 std 路径仍无授权。不是宽松忽略额外返回事实。
- 尺寸 `/tmp/koven-n1a-view-sizes.log.json` 退出 0，1012 Rust、38 历史超限；策略与
  文档未改，没有新增增长例外。docs 检查 603 Markdown、diff check 均退出 0。

workspace/all-targets check/clippy、fmt 和冻结全量终态继续追加；SPEC-0289 仍 in-progress。


2026-10-08 用户要求立即停在当前安全边界并交回最小 checkpoint：

- workspace/all-targets check 退出 0；`/tmp/koven-n1a-view-check.log.json`。
- 严格 workspace/all-targets clippy 最终退出 0；
  `/tmp/koven-n1a-view-clippy-final.log.json`。初次发现 collapsible-if 和测试 filter-map
  两个样式诊断，按等价条件合并/过滤修正；没有降低 root/capability 条件或加 allow。
- 最终 fmt 退出 0；`/tmp/koven-n1a-view-fmt-final.log.json`。尺寸/docs/diff 前述通过，
  本交接补记的 docs/diff 重新检查收据单独保留。
- 源码冻结 `/tmp/koven-n1a-view-source-freeze.json`：1012 Rust、96 Koven 源、8 个
  Cargo/工具链配置文件，共 1116 个文件；包括实际 std take 资产，而非只冻结 Rust。
  冻结 SHA256 `2f822aabcc46f8855f13c9627f071ad7b1c8854ba5beb191b7badd61ec0abcdf`。
- 当前所有本轮测试均已终态；没有本轮正在运行或排队的 Cargo。View 冻结源码最终
  过滤全量尚未启动，留待父线程统一安排；不把 §12 的 List 全量作为本片全量证据。
  不为等待全量延迟 checkpoint，也不在交接后自动启动新检查。

本片增量及保全账本 `/tmp/koven-n1a-view-checkpoint.{patch,json}`，相对
`/tmp/koven-n1a-view-base`；没有暂存或提交，分支 feature/spec-0288、HEAD f82e4de。
现有 WIP 保留；SPEC-0288、Guide、accepted ADR、AGENTS 与尺寸策略没有本片改动。
当前无未修复的已执行失败；未运行项是 View 冻结源码最终过滤全量、完整 frontend
重跑与 Linux ASan/LSan。其余 receiver extension/更广 CFG 是后片范围，仍明确拒绝。
SPEC-0289 继续 in-progress；Mac 继续 partial、requirements_met=false。

2026-10-08 View 冻结全量终态补记（21:16:10 UTC）：

| 测试组 | passed | failed | ignored | filtered | 时间 |
|---|---:|---:|---:|---:|---:|
| lang-frontend 全量（含 12 doctest） | 1944 | 0 | 0 | 0 | 260.350 s |
| lang-codegen library | 1178 | 0 | 1 | 4 | 1501.75 s |
| const_native_view_compile_contracts integration | 2 | 0 | 0 | 0 | 2.66 s |
| lang-codegen doctest | 4 | 0 | 0 | 0 | 0.31 s |
| lang-codegen 合计 | 1184 | 0 | 1 | 4 | worker 1521.701 s |

frontend 与 codegen 顺序执行，均 exit 0；完整命令、原日志与回执在
`/tmp/koven-n1a-view-full-verification/{frontend.log.json,codegen.log.json,status.json}`。
独立 `terminal-verification.json` 核对 8 个 View 回归均实际通过，覆盖 50 个健康 native
程序；原 inline nullable 拒绝回归、前端嵌套 View 真实 root 事实也实际通过。
1012 Rust、96 Koven、8 配置共 1116 个冻结哈希与文件集合全部一致，3452 个交接文件
哈希无变化。四个 --skip 与 §11 固定名单完全一致，禁用入口没有执行；唯一 ignored
为 LLDB debugserver task-port 权限用例。日志与 target lock 终态无持有者。
codegen 日志 SHA256 `3605787ea46866c7139ca2136b8527625066d1dd1806ab6250e6073688525cfa`；
frontend 日志 SHA256 `54609838dec2fca56aac795fa187a3ada5174f8e97a379d450ec196f273683f3`。

此终态只确认 View 检查点源码；SPEC-0289 仍 in-progress。Mac skips 精确保持
`["address:macos-asan-unsupported", "leak:macos-counter-only"]`，acceptance.status 为
partial、requirements_met 为 false；Linux ASan/LSan 未运行。没有故障生成/注入/校准、
提交/push/PR/merge，也未重试或绕过被拒绝的 List 根验证放宽方案。

## 14. 同一 take 的调用前缀 CFG 清理首片

用户后续授权限定外层调用的后续实参提前 return：普通合法程序已求值的 range metadata、
实际借用和临时 owner 必须按依赖结束，未执行的外层调用不能接管 owner。先记录直接
回归的真实失败点，再修必要路径；不放宽 verifier、不引入完整 NLL 或更广来源 CFG。
验收覆盖 named/temporary List 与 View 来源、String/MoveOnly/Resource，正常调用与
早退分支的 single/unit/public CLI 编译运行、原分配身份、恰好一次析构和权限恢复。
receiver extension 只读核对 ADR-0030 与本 Spec，可信宿主接线不在本片实施范围。
本片起点 `/tmp/koven-n1a-cfg-base/manifest.json`，保留当前分支所有既有 WIP；聚焦
证据与最小绿检查点在本节追加，阶段验证前不启动新的冻结全量。


本片普通合法回归与修复证据：

- 起点 `/tmp/koven-n1a-cfg-legal-red.log.json` **0 passed、4 failed**。其中 named List、
  temporary List/View 三项为单文件 InvalidSsa，严格 verifier 报 owner/loan 冲突、
  缺失 metadata owned exit 与活跃 loan exit；named View 无显式作用域时先报 L0135。
  后者超出当前直线 last-use 证明，保留稳定拒绝，用现有词法结束点验收合法 named View。
- single 控制转移先结束当前已建立的调用实参 loan，再结束该调用已构造的短期 range，
  最后消费 owner drop facts。未求值范围不伪造 RangeEnd；正常路径合同不改。
- unit 原能力门先明确 UnsupportedNode；接通 frontend 已发布 range-use 对应的精确
  调用前缀。new descriptor 绑定的预检仅接受相同根来源证明覆盖的调用内 If/Return，
  其他表达式、普通 Borrow 或更广 CFG 门保持。没有更改任何 SSA verifier 规则。
- scoped named View 又真实暴露 unit NonDominatingUse：then/else source-loan 状态未
  分别保存，使用了另一分支的 loan。现在分支入口与其他状态一起保存真实 source loans，
  控制转移用重绑定后的前缀槽生成精确 metadata loan/RangeEnd/ancestor end。
- 所有临时诊断输出已撤回；single/unit orchestrate 与 drops.rs 均恢复起点哈希。
  中间测试/API 名称的编译错误均未执行用例，日志保留；不计入通过。

当前阶段通过：

- `/tmp/koven-n1a-cfg-front-facts.log.json`：ownership_range_construction **12 passed**，
  真实来源及已求值 outer argument 的正常/转移 end，另证明更广绑定 last-use 仍 L0135。
- `/tmp/koven-n1a-cfg-native2.log.json`：**6 passed、0 failed**。四个 SSA 回归各验证
  single/unit 早退分支不调用消费者、RangeEnd 先于 root Drop；两项 native 矩阵共
  **48 个健康程序**（四来源 × 三类元素 × 两条流水线 × 两种调用形态），每个程序实际
  运行正常与早退分支。原分配身份、释放顺序、恰好一次析构、named root 权限恢复
  有独立计数/输出断言；owned 前缀早退由调用者释放，后续工厂和未执行消费者不运行。
- `/tmp/koven-n1a-cfg-cli.log.json`：public range CLI **19 passed、0 failed**；新增
  两项 **24 个实际 build/run**，连同既有 49 个共 73 个程序，使用真实标准资产。
- `/tmp/koven-n1a-cfg-clippy.log.json`：严格 workspace/all-targets clippy exit 0。
  fmt 已执行；最终 fmt check、共享回归、尺寸/docs/diff 收据在检查点继续追加。

receiver 只读核对 `/tmp/koven-n1a-cfg-receiver-readonly.json`：ADR-0030 §决定要求独立可信
source、bound receiver 与 canonical callable identity，具体宿主接线待确认；现行 Guide
和本 Spec 不授权从顶层 SourceId 证明推定扩展能力。选择门仍 L0164，single/unit carrier
SSA 来源仍限 Parameter；本片未改 Guide、ADR、扩展实现、尺寸策略或 SPEC-0288。
SPEC-0289 继续 in-progress；本片冻结全量尚未启动，不复用 §13 全量为新 CFG 全量证明。


本片最终阶段门禁补记：

- range 全套 `/tmp/koven-n1a-cfg-regression-range.log.json` **27 passed、0 failed**，
  包含本片 6 项和全部既有 List/temporary/View、严格 verifier 拒绝回归。
- 普通借用 `/tmp/koven-n1a-cfg-regression-borrow.log.json` **9 passed**；更广 caller CFG
  与 block borrow-return 仍稳定 UnsupportedNode。unit when/short-circuit 分别 **6/2 passed**，
  对应 `/tmp/koven-n1a-cfg-regression-{when,short-circuit}.log.json`。
- 前端 10 个类型/授权/所有权/unit 契约套件 `/tmp/koven-n1a-cfg-front-contracts.log.json`
  **382 passed、0 failed、0 ignored**；新用例与既有来源、普通 Borrow、iteration 均完整执行。
- fmt check `/tmp/koven-n1a-cfg-fmt-check.log.json` exit 0；严格 workspace/all-targets
  clippy 前述 exit 0。尺寸 `/tmp/koven-n1a-cfg-sizes.log.json` exit 0，1013 Rust、
  38 历史超限，未新增增长例外、未修改尺寸策略。最终 docs/diff 回执在检查点保存。

本片最小绿检查点 `/tmp/koven-n1a-cfg-checkpoint.{patch,json}`，相对
`/tmp/koven-n1a-cfg-base/manifest.json`，保留原 WIP。新源码冻结
`/tmp/koven-n1a-cfg-source-freeze.json` 包含 Rust/Koven/构建配置；全部聚焦检查已终态，
没有正在运行或排队的 Cargo。本片 frontend/codegen 最终冻结全量尚未启动，留待检查点
交接后统一安排；§13 全量不作为本片全量。无提交/push/PR/merge、故障生成/注入/校准
或校验器放宽；Mac 精确 skips/partial/requirements_met=false 保持，Linux ASan/LSan 未运行。
SPEC-0289 仍 in-progress，receiver 宿主决定与更广 CFG 不在本片实现范围。


2026-10-08 CFG 首片冻结全量终态补记（22:25:45 UTC）：

| 测试组 | passed | failed | ignored | filtered | 时间 |
|---|---:|---:|---:|---:|---:|
| lang-frontend 全量（含 12 doctest） | 1946 | 0 | 0 | 0 | 78.205 s |
| lang-codegen library | 1184 | 0 | 1 | 4 | 1574.05 s |
| const_native_view_compile_contracts integration | 2 | 0 | 0 | 0 | 1.98 s |
| lang-codegen doctest | 4 | 0 | 0 | 0 | 0.28 s |
| lang-codegen 合计 | 1190 | 0 | 1 | 4 | runner 1582.057 s |

frontend 于 21:58:40 UTC 结束，codegen 于 21:59:23 UTC 启动，严格顺序且均 exit 0；
实际 argv、日志和收据在 `/tmp/koven-n1a-cfg-full-verification/{frontend,codegen}.log.json`。
独立 `terminal-verification.json` 核对新增六项调用前缀回归全部实际通过，包括 48 个
健康 native 程序；原 inline nullable 拒绝回归及 Map String/MoveOnly 的 single native、
unit 精确析构回归也实际通过。四个 --skip 与 §11 固定名单完全一致，禁用入口和间接
故障生成/注入/校准没有执行；唯一 ignored 为 LLDB debugserver task-port 权限用例。

1013 Rust、96 Koven、8 配置共 1117 个冻结哈希及文件集合全部一致；全量结束时
3453 个检查点文件均保持原哈希，任务日志与 target lock 无持有者。frontend 日志 SHA256
`1141664d17ce39d26652184cd36ee0c035e488809d2a78d0f356d70ad95f5c2c`；codegen 日志
SHA256 `a42d68ce5ffad8f6ac71bf012ea3117c5e051eecab3ab15f349a736514ade077`。
本次获准补记只改本 Spec 文档，1117 项源码/配置冻结继续保持；上文“尚未启动”是此前
最小绿检查点时的状态，由本终态补记更新，不作为仍缺全量的判断。

本终态确认同一 take 的调用前缀 CFG 首片，不宣称 receiver 已接通或整个 Spec 完成。
可信扩展宿主证明与接线仍待后片，其他更广 CFG、算法及 consume/Clone/N1b 不扩展。
SPEC-0289 保持 in-progress；Mac skips 精确保持
`["address:macos-asan-unsupported", "leak:macos-counter-only"]`，acceptance.status 为
partial、requirements_met 为 false；Linux ASan/LSan 未运行。没有暂存、提交、push、PR
或 merge，没有重试或绕过先前被拒绝的 verifier 放宽或故障操作。


## 15. receiver 第一阶段：独立授权与 canonical 类型选择（2026-10-08）

用户批准 ADR-0030 所记录的内部宿主接线，本阶段仅到签名/类型选择；不新增语言语义。
真实 std 资产加载边界显式登记独立 SourceMap-owned extension authority，producer
权限不自动升级。single/unit 从本次分析的真实顶层声明绑定 canonical callable、
compiler-bound List/View 接收者、相同元素类型与 Borrow/from this 的真实 Span。

名称阶段只保存可选 member 词法查询提示，不给普通 member 增加 unresolved 诊断。
unit 提示复用现有 package/exact/alias/wildcard 查询；局部值遮蔽和不可见来源不能成为
候选。receiver 参与现有 T 推导，显式 T 必须一致；List/View 同名接收者选择不同
canonical 声明，同 receiver 的 alpha 等价签名仍拒绝，receiver mode 不进入 shape。
扩展不能以省略 receiver 的顶层函数语法调用。

真实红例首先复现 receiver TypeRef 未进入函数泛型作用域；后续普通源码红例又复现
扩展被直接按顶层函数调用。最小修复分别建立同一函数类型参数作用域与显式 receiver
候选限制。第一次测试 API/AST 字段名称错误属于编译失败；测试夹具遗漏 package 导致
index 拒绝，以及两项夹具误用诊断文案/期待新增诊断，均已纠正，不计通过。
尺寸初检发现 resolver 增长，按完整函数作用域职责移入独立模块，未修改尺寸策略或例外。

`type_range_extension` 新增 13 个普通源码/宿主权限回归：可信重命名算法绑定、String/
MoveOnly 与 List/View 的 T 推导、std 名称/路径伪造、producer 不升级、foreign SourceMap/
SourceId 隔离、错误 receiver mode/类型/来源/结果、显式 T 冲突、跨文件 import/alias/
wildcard/同 package、局部遮蔽、跨声明权限隔离、无 receiver 直接调用、重复 shape 与可信授权后的实际 pipeline 门禁。
`standard_sources` 既有 host 回归额外断言只有真实加载的 SourceId 同时获得两种独立权限。

本片保留 extension 声明 L0164，文案明确尚未实现 ownership/lowering；unit validate
失败，single 停在 TypeChecking。RangeExtensionBinding 和已选择类型签名不是实际 root
origin、caller loan continuation、权限恢复或可执行证明。未改 ownership/codegen/SSA
verifier，未新增 .ko 扩展 body；现有标准 take 加载不因 L0164 失效。

聚焦及共享契约、CLI/std 与工程门禁的最终收据在本节检查点终态继续补记；§14 全量
只证明此前 CFG 冻结源码，不作为本片 receiver 改动后的全量。receiver ownership/body
来源与 caller continuation/end、封闭 SSA source slot、verifier 及正常 native 仍是后片。
SPEC-0289 继续 in-progress，不归档；没有 fault/IR/loan/drop fact 修改注入、校准或 verifier
放宽。Mac 的精确 address/leak skips、partial/requirements_met=false 保持；Linux ASan/LSan
未运行。本片不暂存/提交/push/PR/merge，交付首阶段 checkpoint 后暂停。


第一阶段检查点终态：

| 检查 | 实际结果 | 收据 |
|---|---|---|
| 前端名称/类型/unit 14 套件，含新增 receiver 13 项 | 243 passed、0 failed、0 ignored、0 filtered | `/tmp/koven-n1a-receiver-stage1-front-final.log.json` |
| ownership carrier/construction/producer、普通 borrow result、Map ownership/type 6 套件 | 43 passed、0 failed、0 ignored、0 filtered | `/tmp/koven-n1a-receiver-stage1-ownership.log.json` |
| CLI 实际 std 加载与权限隔离 | 2 passed、0 failed、0 ignored、48 filtered | `/tmp/koven-n1a-receiver-stage1-host.log.json` |
| 公共 range CLI 集成 | 19 passed、0 failed、0 ignored、0 filtered，73 个正常源码 build/run 场景 | `/tmp/koven-n1a-receiver-stage1-cli.log.json` |
| CLI build | exit 0 | `/tmp/koven-n1a-receiver-stage1-build.log.json` |
| workspace/all-targets clippy -D warnings | exit 0 | `/tmp/koven-n1a-receiver-stage1-clippy-green.log.json` |
| fmt check / Rust 尺寸 / docs / diff | exit 0；1020 Rust、38 旧超限；603 Markdown | `/tmp/koven-n1a-receiver-stage1-{fmt,sizes,docs,diff}.log.json` |

clippy 首检在新增 generic 逻辑中报两处 collapsible_if；按原条件等价合并后严格检查
通过，并重新执行 243 项前端回归。失败首检保存在 `/tmp/koven-n1a-receiver-stage1-clippy.log.json`。
clippy 修复没有改变分支条件、诊断或来源校验，先前夹具纠正已单列。本片所有 Cargo
检查顺序运行，使用已安装 Rust 1.96 /
LLVM 21.1.8，不安装工具链或更改系统配置。

可审查增量 `/tmp/koven-n1a-receiver-stage1-checkpoint.patch` 和完整哈希/命令收据
`/tmp/koven-n1a-receiver-stage1-checkpoint.json` 以本片起点
`/tmp/koven-n1a-receiver-stage1-base/manifest.json` 为基准，保留此前 WIP。当前源码/构建配置
冻结 `/tmp/koven-n1a-receiver-stage1-source-freeze.json` 为 1020 Rust、96 Koven、8 配置共
1124 项；本片未改 ownership/codegen/标准 .ko 资产/尺寸策略。第一阶段之后不启动全量
frontend/codegen，也不把 §14 全量当作 receiver 改动后的验证。本阶段授权范围已落实并
按要求暂停，实际 receiver root/continuation/end 与后端仍 L0164，SPEC-0289 继续 in-progress。


## 16. receiver 第二阶段：实际来源与 caller 续接（2026-10-09）

用户在确认第一阶段检查点后授权本片 Phase 3，范围仅为前端 receiver origin、
`from this` producer 证明、caller loan continuation 与权限恢复。Guide v0.43 和
ADR-0030 决定不变，不新增扩展种类、算法或后端能力。

single 名称产物为真实扩展 receiver 分配合成 Borrow 参数，以本声明 receiver
TypeRef Span 锚定；`this` 引用指向该参数。unit 保存 source-qualified UnitSymbolId，
不把 compiler-bound List/View 当作 nominal receiver，也不以 temporary 冒充实际根。
类型与 callable body 复用该参数身份及 Borrow mode；完整 unit callable body/context
职责移入独立模块，保留现有显式 return 检查，未修改尺寸策略或增加例外。

producer proof 只从实际构造、已选择 canonical 转发及返回表达式取得；receiver 与
声明来源不同则 L0162。caller 用真实 receiver expression 续接 Shared source loan；
View 派生继承原根，root-flat 新 descriptor 不依赖父 metadata，借用既有 metadata
仍保留父依赖。临时链逐层续接到真实立即消费者，在其返回后清理原根一次。
语义错误/Deferred 的既有撤销条件不变；成功 recovery facts 之后单独发布后端 L0164，
single pipeline 停在 OwnershipChecking，unit owned validate 仍失败。

正常源码 `ownership_range_receiver` 9 项覆盖单文件/unit、String/MoveOnly/Resource、
真实 source loan 与 child-before-parent end、根权限在全部依赖结束后的恢复、临时链
消费者与唯一 drop、sibling/local/temporary 错误来源、临时绑定逃逸、活跃父/子/兄弟/
metadata/元素依赖下的 move，以及跨文件 import/alias 与反转输入顺序。provider 的
`source` 对具名 View 指 metadata binding；根 owner 由独立 BorrowBinding/source-loan
facts 证明，两种身份均单独断言，不更改 provider 合同或降低拒绝条件。

红例先复现 receiver body 类型缺口、缺失 return origin、unit source loan 和临时链
消费者续接，逐项修复后运行同一选择。两次测试代码编译错误及元素正例误认 metadata
来源的断言已纠正，失败回执保留；不计为通过。断线前 fmt 初次终态未知，恢复后核对
原回执 exit 0 及无 target/log 持有者。恢复首轮 receiver 9 passed，类型 12 passed/1
failed：不可见/遮蔽扩展的 type-only Deferred 路径仍可 validate，原断言依赖已迁移的
声明 L0164。现额外检查所有签名夹具不能发布 owned/backend capability、无错误来源
facts，候选排除断言不变；22 项复跑全绿，再运行完整受影响套件。

| 验收项 | 实际结果 | 收据 |
|---|---|---|
| receiver/type 与名称、类型、unit 15 套件 | 252 passed、0 failed、0 ignored、0 filtered | `/tmp/koven-n1a-receiver-stage2-front-final.log.json` |
| ordinary borrow、ownership、iteration、receiver、Map 16 套件 | 480 passed、0 failed、0 ignored、0 filtered | `/tmp/koven-n1a-receiver-stage2-contracts.log.json` |
| single/basic/unit/const/name snapshot 及外部 Rust 能力编译契约 11 套件 | 80 passed、0 failed、0 ignored、0 filtered | `/tmp/koven-n1a-receiver-stage2-handoff.log.json` |
| 诊断目录 | 9 passed、0 failed、0 ignored、0 filtered | `/tmp/koven-n1a-receiver-stage2-diagnostic.log.json` |
| 真实 std 加载/权限隔离 | 2 passed、0 failed、0 ignored、48 filtered | `/tmp/koven-n1a-receiver-stage2-host.log.json` |
| 公共 range CLI 既有顶层 take 路径 | 19 passed、0 failed、0 ignored、0 filtered；69.812 s | `/tmp/koven-n1a-receiver-stage2-cli.log.json` |
| workspace/all-targets clippy -D warnings | exit 0 | `/tmp/koven-n1a-receiver-stage2-clippy.log.json` |
| Rust 尺寸 | exit 0；1023 Rust、38 旧超限 | `/tmp/koven-n1a-receiver-stage2-sizes.log.json` |

最终 fmt check、docs（603 Markdown）与 diff 全部 exit 0，收据分别为
`/tmp/koven-n1a-receiver-stage2-{fmt-check,docs,diff}.log.json`。当前源码冻结为
1023 Rust、96 Koven、8 配置共 1127 项。检查点以 `/tmp/koven-n1a-receiver-stage2-base/manifest.json` 为基准，
保留第一阶段及此前全部 WIP。此片未修改 codegen、SSA/verifier、CLI、标准 .ko 或尺寸
策略，未运行 frontend/codegen 全量；§14 全量只证明旧 CFG 冻结，不作为 receiver 验证。

用户在恢复期间新增本地定时提交授权，取代此前不提交约束。第一个隔离单元
`fbc4e0e92a766f010f0f768f30b8cde7fad5e5e5` 仅提交诊断目录及对应测试两文件，9 项回归
通过，提交后 show/暂存区/剩余 WIP 已核验。其余 parser/result-source、borrow-results、
Map/carrier 及 backend guard 的未提交 API 依赖仍交织；不能仅将本片 29 个 frontend
文件提交到当前 HEAD 并宣称它是可独立编译的 receiver 阶段。按已保存阶段检查点继续
整理依赖提交，不按行数切片、不把未验证代码混入、不破坏性 stash 或 reset。

本片 receiver SSA/native 继续 L0164，未构造或删除 loan/drop/IR facts 注入故障，
未运行故障生成或校准，未放宽 verifier；只执行正常源码和能力编译正反例。Mac 精确
skips 为 `["address:macos-asan-unsupported", "leak:macos-counter-only"]`，acceptance.status
保持 partial、requirements_met=false；Linux ASan/LSan 未运行。SPEC-0289 保持
in-progress，不归档；没有 push/PR/merge，最小绿检查点交付后暂停本片开发。


最终增量检查点为 `/tmp/koven-n1a-receiver-stage2-checkpoint.{patch,json}`，终态核验与
源码冻结分别为 `/tmp/koven-n1a-receiver-stage2-terminal.json`、
`/tmp/koven-n1a-receiver-stage2-source-freeze.json`。以第一阶段结束的 3460 文件基线逐项
比较，保留其余 WIP；当前 branch 为 feature/spec-0288，HEAD 为上述诊断目录提交，
暂存区为空。所有 Cargo 顺序运行且已取得终态，使用已安装 Rust 1.96/LLVM 21.1.8；
没有安装工具链、修改系统配置或留下排队 Cargo。此处“绿”只表示本片已列验收，
不表示完整 SPEC-0289、receiver 后端或未执行的全库验证通过。

## 17. 小提交整理与同源码冻结全量（2026-10-09）

用户在 §16 初次交付后授权整理下一段可独立验证的前置提交，并补跑本片完整
frontend/codegen；receiver 后端未获本轮实施授权。只先提交 Map unit signature
约束 `50bf67dfa6098b6894a3c20735d26ad9e1889e92`，三文件且不依赖未提交的
borrow/carrier/receiver API。隔离候选 282 tests 与 workspace/all-targets check 通过，
3263 文件的索引树与候选逐项匹配；父 HEAD 既有 Clippy/fmt 失败单独保留，不计通过。
详见 [SPEC-0288 §2.14](../../archive/specs/0288-map-native-execution.md#214-map-签名约束的独立本地提交2026-10-09)。

源码未追加实现：3463 个工作文件在提交前后及全量结束时逐项确认字节不变；
1023 Rust、96 Koven、8 配置共 1127 项冻结 SHA-256 在两项完整测试前后均一致。
实际 branch 为 feature/spec-0288，HEAD 为上述 Map 约束提交，完整测试对象是冻结的
当前 WIP，不将它视为只含已提交 HEAD 的独立验收。

| 顺序命令 | 终态 | 实际收据 |
|---|---|---|
| `cargo test --locked --offline -p lang-frontend --no-fail-fast --target-dir /tmp/koven-n1a-producer-target` | 1968 passed、0 failed、0 ignored、0 filtered；含 12 doctests；400.475 s | `/tmp/koven-n1a-receiver-stage2-full-verification/frontend.log.json` |
| `cargo test --locked --offline -p lang-codegen --no-fail-fast --target-dir /tmp/koven-n1a-producer-target -- <下列四项 skip>` | library 1184 passed、0 failed、1 ignored、4 filtered；integration 2、doctest 4；合计 1190 passed、0 failed；1518.291 s | `/tmp/koven-n1a-receiver-stage2-full-verification/codegen.log.json` |

codegen 精确过滤以下四个既定故障入口，未静默新增过滤：

- `native_sanitizer_tests::asan_instruments_generated_user_runtime_and_drop`
- `native_sanitizer_tests::counter_failures_preserve_compile_and_run_evidence`
- `native_generated_owner_tests::export_generated_owner_calibration`
- `native_generated_owner_tests::export_generated_owner_case_with_missing_deinit_fault`

runner 移除 `KOVEN_GENERATED_OWNER_CASE`/`KOVEN_GENERATED_OWNER_CALIBRATION`，只读
调用链核对确认 calibration exporter 仅由已过滤测试进入；未启动 Python 聚合或
校准 driver。既有正常 source/SSA/verifier 回归随全库执行。唯一 ignored 是既有
macOS `llvm::debug_tests::lldb_hits_a_koven_source_breakpoint_and_reports_the_frame`。
原始 inline nullable 拒绝测试在本轮实际通过，仍断言 UnsupportedNode。
通用 Map single/unit withValue/requireValue、String 键、MoveOnly 值和精确释放均实际
通过；不使用旧 CFG 全库替代本片结果。

使用已安装 Rust 1.96、LLVM/IR Clang 21.1.8，未安装或修改系统配置。一次短暂
exec-server 连接中断后恢复原 session，未重启 Cargo；1127 项冻结哈希再次一致。
§16 同源码 workspace/all-targets Clippy、fmt、尺寸等通过证据仍有效，本轮未重复
Cargo 门禁；全量后仅同步文档事实并重新执行 docs/diff。

冻结、准确汇总及最终检查点存于
`/tmp/koven-n1a-receiver-stage2-full-verification/`。其余 ordinary borrow、Map
SSA/runtime、N1a 与 receiver 的依赖提交仍为 WIP；不 staged 大型基础提交。
receiver SSA/native 继续 L0164，Linux ASan/LSan 与远端 CI 未运行。Mac 精确
skipped_reasons 仍为 `["address:macos-asan-unsupported", "leak:macos-counter-only"]`，
acceptance.status=partial、requirements_met=false。Spec 保持 in-progress，不归档；
未 push/PR/merge 或清理其他工作树。


## 18. 新推送合同接收（2026-10-10）

接收 `origin/feature/spec-0289` 的 `970e90a116bbaac9893e4d842845d2e6f9306988` 文档，
与独立 `origin/feature/spec-0291` 的草案分开整合到 `fix/spec-0290`。用户要求审查新推送
分支后继续里程碑；此前缺少该分支资料导致的 N1a 启用疑问由既有批准与 Guide v0.43
合同补齐，不回退现有实现。§1–17 保留原检查点与临时路径的历史记录，本次未读取或
重建其原始日志，不将这些历史数字视为当前分支复跑结果。

当前只接入文档与结构门禁；receiver SSA/native 后继验证仍须独立记账，Spec 保持
in-progress。旧 consume 草案0290因与修复编号冲突改为0292，保持 draft；0291
readonly Map consume 保留 v0.44 候选，均未启用或实施。v0.42保全与本次验证见
[接收记录](../../development/n1a-contract-integration.md)。

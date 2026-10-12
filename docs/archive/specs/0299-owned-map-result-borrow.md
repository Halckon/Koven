# SPEC-0299: owned Map 删除结果的非消费借用

> **性质**：变更合同 · **状态**：done · **读取时机**：准备窄化owned Map结果的只读Borrow时 · **唯一真源**：本页的冻结首片有界交付；语义以Guide为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0299` |
| 所属 Phase | Phase 3事实消费、Phase 4 nullable place/loan与native |
| 语言规范 | [Guide v0.43](../../guide/README.md)、[Map](../../guide/12-collections-destructuring.md)、[所有权](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 2026-10-12按已确认0300→0299/0298顺序，从真实main `08c6b8dc` 在独立 `fix/spec-0299` 分支本地实施；发布与CI另行确认 |
| 前置 Spec | [SPEC-0295](../../archive/specs/0295-map-argv-word-frequency.md)、[SPEC-0296](../../archive/specs/0296-owned-null-comparison-cleanup.md) |
| 前置 ADR | [ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md)、[ADR-0017](../../adr/accepted/0017-nullable-handle-ssa-abi.md) |
| 阻塞项 | 首片本地已验收；嵌套prefix等后继及发布/CI尚未完成 |
| 影响范围 | 两入口窄化Map结果的Borrow实参、SSA/verifier直接消费者 |
| 语言语义变更 | 否；不启用新nullable表示或借用语法 |

## 1. Goal 与重起草来源

对`remove`已交付的具名owned nullable结果，在已验证非空分支同步交付只读Borrow，
支持其MoveOnly payload而不复制、clone、retain或消费原owner；调用结束后该owned结果仍按
原事实存活、可再次合法使用并恰一次清理。

2026-10-12原未发布草案丢失，本页依据已核边界重新起草，不是原稿恢复。原审计见
[历史摘要](../../archive/migrations/pr72-74-followup-audit.md)；本页不重用其旧静态数字作为新验收。

## 2. 起点与范围

0295的两入口桥接核验typed non-null proof和注册Map结果后，只为Copyable payload复制
inner再借用；其Copyable门不是所有Borrow的语言限制。现行`get`/下标的MoveOnly按值查询
仍必须拒绝。owned `remove`返回、`!!`消费和本片非消费Borrow是不同合同。

首片为具名whole-root删除结果、非nullable V及同步Borrow参数。核对声明/窄化类型、symbol、
真实结果owner、非空proof与支配关系；保留已有表示，不将presence/payload aggregate
直接解释为inner RootPlace。loan依赖被删除值的新owner，不误绑Map entry或key。

String、现有可存储MoveOnly值及Resource分别核范围；原已支持分流保留，不能为统一表面
制造新复制。相同值的连续Borrow调用后仍可合法读取/消费，调用结束只结束借用，不drop payload。
缺失分支不能建立可读place。沿用frontend事实；事实/ABI不足时停下，不以去掉Copyable判断代替实现。

## 3. 非目标

不扩MoveOnly get、不改变withValue/requireValue、条件借用或nullable V的owned三态表示；
不做通用nullable ABI、普通Resource?工厂、字段/元素/temporary结果全覆盖、Inout或逃逸Borrow。
不顺带重写0296判空最后使用清理或0300循环owner运输，不改变普通Borrow/Copy语义。

`break`/`continue`及其他实参提前退出仍是独立后继候选，非本片验收；
后继必须先证明精确loop目标、未求值后缀、前缀owner/loan及provider逐边清理，不由普通调用成功推定。

## 4. 冻结首片与最小验收

首片只选直接 `if (result != null)` / `if (result == null) ... else ...` 与透明括号条件，
不扩大短路条件、任意when、循环、borrow val或其他实参提前退出。single现有直接条件proof分流
与unit普通条件分别先跑同源红测；不能由两处Copyable门推定两入口全部失败。
类型先分String、含String的MoveOnly value class、Resource class三种既有表示，
Copyable Int/Boolean/value class保留原桥。消费式 `!!` 只作借用完成后的独立对照。

## 5. 最小验收与交付

- [x] single/unit分别建立具名remove结果窄化Borrow的普通源码红测，区分现有成功路径与真正缺口。
- [x] 冻结首片的String、MoveOnly、Resource及Copyable对照的SSA/verifier、未改写LLVM和正常native验证；
  新增MoveOnly payload的非消费Borrow路径不得出现Copy/Clone/retain或消费式NullableTake；
  Copyable对照保留既有合法复制桥，不要求重写为同一实现。
- [x] 同一非空结果连续两次Borrow后再合法消费或scope清理；独立可观察资源事件无提前/重复析构。
- [x] 缺失、错误窄化、借用期间冲突/逃逸及MoveOnly get保持拒绝；原Copyable桥和0296判空回归保持。
- [x] 独立分支按红测→最小实现→直接回归→Architecture/本地首片验收推进；正式发布和CI另行闭环。

## 6. 唯一验收账本

基线 `08c6b8dcbb8e69f20a7dd82bbd0426b0df97c4c9`（tree `3b081d41227420bd61ca7751436d24147923d4aa`）。
0300已由PR79合并；其actual-main CI [38259510177](https://github.com/Halckon/Koven/actions/runs/38259510177)
启动时pending，不作为0299验收。以下是本地首片真实结果，正式发布及CI未在本片认领。

| 验收项 | 测试目标/过滤器 | 实际结果 |
|---|---|---|
| plain/group single/unit普通源码基线 | 初版 `owned_map_borrow_`，生产未改 | 12例：single plain三种表示3通过；其余9失败。String/inline为source-located UnsupportedNode，Resource为InvalidSsa；均先通过frontend |
| 独立Resource native基线 | `owned_map_borrow_resource_plain_native_baseline_probe`，detached真实main仅加入测试 | 0/1；两借用输出空行、!!后value，见§7；修后同测通过 |
| 首片正反与结构合同 | `cargo test --locked -p lang-codegen --lib owned_map_borrow_ -- --nocapture` | 23测试通过：12核心同源SSA、范围/缺失矩阵及根追溯/drop/pending断言、16项前端负例、3项unit边界源码、7个普通native测试组 |
| 两入口真实读取/清理 | 同上；unit native普通与constant、SourceUnitInput源文件正反排列 | String/Resource真实字段，两借用后!!、三种表示missing与定义Abort、最后使用/Resource词法drop通过；inline核心与辅助读严格分列，见§7 |
| 非Map nullable adapter旧路径 | `native_tests::nullable_class_box_and_rc_sources_link_run_and_release` exact | 1通过；包含Rc.share/value。新测试另含非Map class字段读后!!与single Resource loop静态proof槽重入 |
| 0296/临时判空回归 | `owned_null_cleanup` / `temporary_null_cleanup` | 14 / 9通过 |
| 0297/0300回归 | `map_key_return` / `map_loop` | 42 / 19通过 |
| Copyable旧桥与词频应用 | `map_word_frequency` | 4通过；含single/unit通用get/remove窄化Borrow |
| 文档/格式/静态检查 | `cargo fmt --all -- --check`；`cargo clippy --locked -p lang-codegen --all-targets -- -D warnings`；docs/size/diff | 全通过；docs 652页，size以真实main为基线并保留历史债务 |
| 非作者审查 | 多轮定位result-source/裸receiver gate与SharedOwner分流；逐层核HeapOwner slot/loop结束点、完整production diff、测试oracle及范围 | 已修审查发现；最终只读复审无新增阻塞。审查者未运行测试，动态结果由实际日志独立登记 |

前端负例区分阶段：MoveOnly get为type L0136；无proof、错误edge、作用域已结束及另一symbol为
L0084；已消费owner为ownership L0131，先借后move为L0135，callee消费Borrow为L0133，
两入口均有真实Span。unit合法源码但本片越界的Borrow-return、requireValue key、container index
分别为source-located UnsupportedNode；不把它们写成语言禁止。

single `control.rs` 1277→1296行是既有超限文件的19行有界接线（透明Group查询及原condition清理）；
独立审阅保留原状态机，不为本片机械切片。unit CFG重绑按完整职责迁至`cfg/rebind.rs`，旧suffix=0
合同不变，只有新proof入口接受经精确验证的单后缀。

### 尚未完成，不由首片通过关闭

- [ ] 特殊receiver的key/index/element等嵌套同步调用：先封闭所有裸局部prefix运输，再独立真实红绿验收。
- [ ] Borrow/Carrier结果及borrow val、一般nullable来源/temporary/字段/loop等原草案广义候选按案例重新冻结。
- [ ] inline unit显式receiver字段与single相应成员调用callee旧边界；本片辅助读取不表示这些旧缺口已修复。
- [ ] 正式发布、双宿主CI和合并闭环。本地首片完成不等于SPEC0299广义目标全部完成或可归档。

不运行fault injection、calibration、sanitizer、reducer、aggregate drivers或畸形IR/runtime注入。
普通stdout不替代完整泄漏或内存安全证明；正式发布、双宿主CI及合并不在本地完成声明内。

## 7. 实施中已确认的独立边界

2026-10-12 普通源码同源红测：single plain 三种表示的 SSA/verifier/LLVM 已成功，
single grouped 和 unit plain/group 合计九例失败；因此没有将全部十二例标为 lowering 红测。
Resource single plain 的 native 另在未改生产代码的真实基线 `08c6b8dc` 重现错误：
两次 `observe(removed)` 读取 String 字段只输出空行，而后续 `removed!!` 的同字段读取输出 `value`。
这是真实 native 红测，不能由先前 SSA 验证成功覆盖。

最小已批准修复限定现有 `NullableHandle -> Shared view` LLVM adapter：
其交付必须与既有 HeapOwner Borrow 参数相同，为**存放 handle 的 storage 地址**，不是裸 heap handle。
仅对 inner 为 HeapOwner（class/Box，包括Resource）的路径使用函数 entry stack slot，每次分支执行写入当前 handle；SharedOwner/Rc 的既有 retain/payload view 分流保持原样；不改 ABI 定义、Guide、verifier 或 runtime，
不 clone/retain，也不扩大一般 nullable place 支持。需保留非 Map NullableHandle 与 aggregate 分流直接回归。

unit 新 proof 首片进一步限于非嵌套直接表达式语句的同步 `Owned`-result 调用（`Unit` 是普通 owned 结果模式）。
同一调用多个 Borrow operand 及已登记 prefix 仍须通过。Borrow/Carrier 结果与特殊 receiver 的
key/index/element 嵌套调用不在首片内；它们的未登记 Rust 局部 receiver 运输仍是0299后继未完成项，
不能以此位置 gate 宣称原草案广义目标已全部完成。此 gate 只约束新增 unit proof，不改变旧 single 或 Copyable 路径。

inline 核心同源 callee 仅输出标签，验收仅证明真实 Shared call proof 与后续 `!!` 后字段值；
不把标签宣称为借入 payload 读取。独立辅助读取分别核 single 显式 receiver 的 Int 字段、unit 的 `this` 方法；
两入口另用源码显式 `value.text.clone()` 读取 String 字段，区分源码显式 clone 与桥隐式复制。
unit 直接 `value.n` / `value.text` 与 single `value.number()` 的既有 callee lowering 缺口作为后继保留，未认领修复。

附加非Map class字段回归使用两次Borrow后的显式`!!`，该普通源码确实通过。
删去后续消费的另一非Map class候选在本轮仍止于lowering的`InvalidSsa`；没有将其归因于此次adapter、
没有修改一般nullable最后使用路径，也没有把这个候选计入通过验收或宣称它已修复。

## 8. PR80 实现验收与冻结首片有界归档

除顶部生命周期状态外，§1–7保留本地实施检查点原文，包括当时未发布、待验证和广义后继未完成
的记录；本节追加远端事实，不将历史观察改写成重跑。用户于2026-10-12批准PR80实施验收后的
有界归档；`done`仅关闭§4/§7已冻结的首片，§6列出的嵌套调用、Borrow/Carrier返回、借用存储、
一般nullable/loop及callee边界继续开放，未被改写为完成或语言禁止。

| 项目 | 精确输入与实际结果 |
|---|---|
| PR / 实现head | [PR80](https://github.com/Halckon/Koven/pull/80)，`5a53bb15067ad0d9ecafd061111b436ced270501` |
| tree / base | tree `ed561068dbe67bc4e8efecde88b687e048853aca`；base `08c6b8dcbb8e69f20a7dd82bbd0426b0df97c4c9` |
| CI输入 | [run38269053352](https://github.com/Halckon/Koven/actions/runs/38269053352)，attempt1，pull_request；synthetic `bcc3f5fb7f14ebe19574f2b0e69864ba6ae524fa`的父为base/实现head，tree与实现head相同 |
| 实现CI终态与时间 | completed/success；created/start为2026-10-12 15:16:42 UTC，最后job completed为15:30:41 UTC，run updated为15:30:42 UTC；run API没有completed_at字段，不把updated冒作完成时间 |
| 实际job范围 | 15个job成功；唯一Tree-sitter因`editors=false`合法skip；`reuse=false`，双宿主代码及preview实际执行 |
| Linux新增选择 | [Targeted Tests](https://github.com/Halckon/Koven/actions/runs/38269053352/job/114879572339)：23个完整新增测试名逐项在bounded composition步骤内实际ok；12核心SSA、2扩展SSA、2普通源码负例组及7native组 |
| macOS新增选择 | [Targeted Tests](https://github.com/Halckon/Koven/actions/runs/38269053352/job/114879572462)：同23个完整名逐项实际ok，无0299 ignored或跨host汇总替代 |
| 完整lang-codegen lib测试目标 | Linux 1355通过/0失败/0ignored/0filtered；macOS 1354通过/0失败/0filtered，1个既有LLDB权限ignored，不属于0299新增23项 |
| 其他门禁 | 变更检测、直接依赖、尺寸、Docs/Fmt、双宿主Check/严格Clippy均成功；[Code CI Evidence](https://github.com/Halckon/Koven/actions/runs/38269053352/job/114884093006)与[CI Passed](https://github.com/Halckon/Koven/actions/runs/38269053352/job/114884093035)分别成功 |
| preview链 | 双宿主producer针对上述实现head；Linux候选artifact `11756288354`及macOS候选`11756068699`分别与[Linux consumer](https://github.com/Halckon/Koven/actions/runs/38269053352/job/114883631100)/[macOS consumer](https://github.com/Halckon/Koven/actions/runs/38269053352/job/114883764331)实际下载ID一致，安装/命令验证成功 |

Code CI Evidence v1绑定上述PR80、base、head及synthetic，代码输入摘要为
`a98634d021e8547c87b330cb4390abf1a1f4e0086da6aa9e428777ac2671c480`。
独立只读核对将11个artifact的ID/API digest与attempt1生产日志逐一匹配；原始收据和日志留在仓外，
未另行下载ZIP内容作独立检查。macOS的Linux-only sanitizer步骤依平台条件合法跳过，
不构成macOS动态检测通过。远端通用门禁成功也不是0299专属完整泄漏或内存安全证明；
Resource字段/deinit事件、SSA owner/proof/drop断言及通用检测保持各自证据边界。

本归档只迁移合同、更新索引/库存/DAG及真实验收摘要；不改Guide、生产代码、测试、verifier或runtime。
最终归档head CI、正常merge和actual-main CI仍须分别验证，本节没有提前认领其通过。

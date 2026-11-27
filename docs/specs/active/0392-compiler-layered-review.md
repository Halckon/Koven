# SPEC-0392: 编译器分层复审与前端合同修复

> **性质**：变更合同与复审账本 · **状态**：in-progress · **读取时机**：执行 R0–R5 复审或核验结构复审退出条件时 · **唯一真源**：本 Spec 的范围、发现与实际验收；语言语义仍归 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-REVIEW-0392` |
| 所属 Phase | Phase 0–6 |
| 语言规范 | 现行 [Guide v0.47](../../guide/README.md) |
| 批准依据 | 2026-11-06 用户明确要求实施分阶段 review 计划；先结构复审，再恢复语言增量 |
| 前置 Spec | 无 |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 双宿主完整动态验收、后续结构迁移收口与 v1 发布候选冻结尚未成立 |
| 影响范围 | frontend、codegen、CLI/LSP/标准库的阶段合同与验收；按确认问题最小修复 |
| 语言语义变更 | 否 |

## 1. Goal

在明确源码身份上建立可追溯、独立复核的分层审查基线，修复已确认合同偏差，并以阶段门槛
决定是否恢复增量或进入全链路验收。测试存在、历史 CI 和静态阅读均不等于本轮通过。

## 2. 基线与证据口径

- 原始输入：`ea476adcccdd3d7fad768752173c2d76b1861aa1`，工作树初始干净；
  分支 `feature/spec-0392-compiler-review-baseline` 从本地 `origin/main` 同 SHA 建立，未 fetch，
  因而不声称这是远端最新 main。本地 `main` 指向另一旧提交，不将其作为本轮输入。
- Guide v0.47；Rust/Cargo 1.96.0；本机 arm64 macOS，LLVM 21.1.8。
- 本轮独立审查按 R1/R2/R3 分配全新上下文，只读规范、目标实现和测试；修复后另行复核。
  主代理串行执行 Cargo；独立审查的静态结论和实际运行分别记账。
- 临时 target、探针和原始日志在 `/private/tmp/koven-review-0392-*`，不入库；临时目录不保证
  长期保留，最终必须以本轮 PR/CI 固定身份补充持久来源后才能验收归档。

原始提交的直接调用语法清点（`git grep -n -E '<name>[[:space:]]*\(' <SHA> -- crates`，
排除 `fn <name>` 定义）如下；计数包含测试和字符串，不是编译后调用图，也不覆盖别名/宏。

| 旧入口 | frontend 文件 / 匹配行 | codegen 文件 / 匹配行 | 消费者处置 |
|---|---|---|---|
| `check_types` | 84 / 149 | 17 / 19 | 旧测试与对照仍真实依赖；本轮 lambda 回归另新增一个直接消费者 |
| `check_ownership` | 46 / 57 | 16 / 18 | 待按诊断、事实、资源 oracle 分批对等迁移，不能无条件删除 |
| `lower_scalar_file_with_entry` | 无 | 4 / 7 | 入口 crate-private，现有引用为测试/对照；原迁移仍归 SPEC-0341 |

CLI/LSP `src` 与 frontend `analysis` 未检出旧 `check_types`/`check_ownership` 直接调用；
公共单文件/project 实际走 unit，不把其同名“单文件”误认为旧检查器验收。R0 尚未完成
所有别名、测试字符串及保留 oracle 的逐消费者判定，因此不以以上数字作为删除许可。

## 3. 阶段与退出门槛

| 阶段 | 合同、producer → consumer 与验证入口 | 退出条件 |
|---|---|---|
| R0 基线 | SourceMap → lex/parse → unit names/types/ownership → owned view → SSA → LLVM/native；CLI/LSP 显式提供 source/environment；见 [流水线](../../architecture/pipeline-and-workspace.md) | 固定 SHA/工具链、消费者及证据清单；每项区分当前实跑、历史、未实现与未知 |
| R1 语法 | Source/Span、Lexer、四 Parser 入口/索引 AST；按 [前端架构](../../architecture/source-and-syntax.md)定位 suite | 合法 AST 与非法诊断匹配规范；恢复前进、poison 根因唯一、试探无副作用、资源计数受限 |
| R2 名称/类型 | unit index/names → types → final admission；legacy TypedFile 仍有真实消费者；见 [名称与类型](../../architecture/names-and-types.md) | trial 回滚、流事实失效、身份与 single/project 等价；旧消费者均有可解释处置 |
| R3 所有权/交接 | typed descriptors → move/loan/capture/drop → basic/const owned view；见 [所有权](../../architecture/ownership.md) | 非法消费拒绝；循环/分支及所有退出边一致；交接拒绝异源、异分析与能力混用 |
| R4 中后端 | refine subset/build → CFG/block 参数 → SSA verifier → LLVM/layout/runtime → object/link/debug | 默认路由正确，退役路径不能误入；非法 SSA 负例、双宿主 build/run 与资源动态证据齐全 |
| R5 全链路/发布 | 同一冻结候选的源码 → 进程退出，另核 CLI/LSP、标准库、conformance 与 sanitizer | 规定矩阵通过；所有失败、skip、timeout、未实现与平台边界有解释 |

阻断项是非法程序被接受、合法程序误编译、资源/内存安全问题、未解释 ICE 或准入绕过。
阻断项修复并独立复核前，相关阶段不结项；一般可维护性债可登记后继任务，不强制清零。

## 4. 已确认发现

F1–F5 的最小探针在原始基线上实际运行；F6 为修复工作树上的新增失败回归，F7 在 Unit
修复后的同一工作树对照确认。各项均有实际运行，不将不同输入身份统一冒称原基线。

| ID | 合同偏差 / 最小输入 | 原始结果 | 处置 |
|---|---|---|---|
| F1 | Guide 07：创建 lambda 不执行其 body；`fun missing(): Int { ({ -> return }) }` | single/unit 均无缺 return 诊断 | 已修复并独立复核；`lambda_completion` 验证 L0088、创建/内部 return 的不同 completion |
| F2 | Guide 06：分支赋值必须 kill smart cast；`if (x != null) { if (flag) { x = null; ({ -> return }) }; return x }` | single/unit 均错误接受 nullable 返回 Int | F1 同根因；修复后两入口 L0084；expected/推导路径及非法局部返回 L0087 负控均验证 |
| F3 | Guide 08/10：MoveOnly this 不能在循环中重复消费；own 方法 `while(flag) { consume() }` / `this.consume()` | unit ownership 均无诊断，消费后 break 正控同样无诊断 | 已修复并独立复核；`ownership_receiver_backedge` 覆盖正常/continue、while condition、for 单次 source、嵌套循环、退出及 Copyable |
| F4 | Guide 01：Lexer poison 唯一根因；`super<I>.async` | async 同 Span 重复 L0002/L0011；普通 `x.async` 仅 L0002 | 已修复；三阶段 poison 消费并返 Error；后继实参身份与原词法 Span 有直接断言 |
| F5 | Guide 06：同行 block elements 必须有分号；`{ val x = 1 val y = 2 }` | parse_block 无诊断，带分号正控同样无诊断 | 本机修复并独立复核；三 owner 共用 L0169，非法正例及下游 fixtures 已迁移；双宿主 CI 已验，见 §10、§15、§17–18 |
| F6 | 恢复不得吞外层分隔符；`f(a.async., sentinel)` / `{ a.async.; sentinel }` | 新失败回归分别证实吞逗号、分号并产生额外 L0034/L0013 | 已修复：error-postfix 保留 caller stops 和分号；逐实参/逐 statement 核对后继 identity |
| F7 | 旧单文件 receiver 与 unit 的所有权合同未等价；同 F3 输入 | 修复后仓外探针确认旧 `check_ownership` 仍 diagnostics/deferred 均空，unit 已报 L0131 | 本机以 L0170 明确拒绝旧入口不能建模的 own receiver，撤销成功事实，旧 lowering 负控通过；不声称实现旧 receiver 语义或完成消费者迁移，见 §10 |
| F8 | Guide 12：顺序容器显式签名的元素必须可存储 | Unit 迁移漏 L0125×2，旧 CLI 接受单独 `List<Nothing>`。独审另确认 capability、名义参数和静态 bound 内的接口旁路 | 后置签名实参校验与 runtime type-ref 检查，保留重载形状，见 §12 |
| F9 | Guide 12：具体单态化后的顺序容器必须仍满足存储合同 | 独审实跑 `fun <T> fail(): List<T> = error("never")` 和 `fail<Nothing>()`，CLI exit 0 | 调用签名与有界顶层函数体的复现已修，其他替换入口见 F11 和 §14 |
| F10 | Rc 签名载荷与 body 构造资格存在差异 | 独审实跑 `fun reject(a: List<Rc<Copyable>>): Unit {}`，CLI exit 0 | 显式签名与调用实例化的复现已修，泛型名义存储入口见 F11 和 §14 |
| F11 | 其他 Unit 存储替换入口缺少准入 | 成员仅在函数体使用 `List<T>`、具现 `Factory<T>(val payload: List<T>?)` 字段和 `Shape<Rc<T>>` 接口上界的具体实例仍被接受 | 已修所列复现并独立复核；双宿主候选验收见 §16–18 |

发现修复采用失败测试 → 最小实现 → 直接消费者/相邻合同 → 独立复核，不在审清前建立新抽象。
不通过修改 Guide 或放宽 oracle 消除失败；历史账本原义保留。

F5 的已有不合规正例包括 `parser_block` 的同行 expression/val/var 与无换行注释案例，
以及 `parser_control_flow` 的同行多个 loop 案例。第二切片先建立三个 owner 的统一正反例，
再按实际失败修正真实 fixture 和共享语料，最终扩大到全部 frontend 目标；独立复核未发现
oracle 放宽。此为已确认 F5 的修复，不代表全部 grammar 与阶段合同均已审完。

## 5. 重构与执行顺序

1. R0 固定身份与审查合同；R1–R3 独立静态审查，动态确认发现。
2. 先修正控制流、重复消费和恢复诊断；恢复/共享接口变化升级相应测试范围。
3. 消费者先分成用户入口、共享算法、旧检查器专用和测试 oracle，再决定迁移；
   不因同名、文件过大或处于 legacy 目录而直接删除。
4. 既有 SPEC-0341 与 E3 迁移范围仍由各原合同负责；本 Spec 不宣称一次审查结清它们。
5. 高风险模块按事实模型/验证/状态转移/诊断职责评估；只在证明重复与行为等价后共享。
6. R0–R3 阻断项关闭后恢复增量；结构迁移收口后在同一冻结 SHA 做第一轮纵向复审。
   v1 功能冻结再做第二轮 R5；新优化等级启用前另验语义与资源一致性。
7. 本地用真实源码和正常构建验证行为。受控故障注入、生成资源校准、检查器变异和
   故障缩减由现有 CI 作业执行，其产物按对应候选身份记录。

## 6. 非目标与兼容边界

不更换手写 Lexer＋递归下降＋Pratt，不追求全语言 LL(1)，不新增通用 HIR/优化框架或依赖，
不扩展 Guide、不补齐全部 v1 功能。DFA/最左推导属于算法或理论，不是额外发布能力。
修复只改变违反现行合同的接受结果/事实/诊断；公开接口与跨 crate 类型变化须单列验收。

## 7. 当前阶段结果与未覆盖

R1–R4 第一轮从 [#443](https://github.com/Halckon/Koven/pull/443) 的
`625f6804` 推进到 [#448](https://github.com/Halckon/Koven/pull/448) 的
`15955ffb`。§327、§347–348、§411–412 保留各次验收身份。
#448 最终 head `757aca35` 的双宿主 Targeted Tests 和聚合门禁通过。
从 `9d9bc3d8` 起的增量对账见 §413。恢复候选在 `d405ed2b`
完成扩测，随后对齐 `36012f86`。新增复审、对齐和冻结候选证据见 §504–514。
Map 提前退出的补审见 §437、§439–442、§444–445，
接口上界、默认体和 Display 的补审见 §482–488，新基线接线见 §487，
字符串拼接与词法借用的组合见 §489–498，顺序容器迭代、普通可空合流、局部布局接线及 When 结果的可达性见 §499–503。
R5 公开语料证据见 §443、§446 及后续各冻结候选账本。退出条件见 §3。
[#448 最终 CI](https://github.com/Halckon/Koven/actions/runs/44457419954)

| 阶段 | 当前证据 | 未闭合边界 |
|---|---|---|
| R0 | 已定位公共流水线和准入。§151、§224–225 清点生产依赖及保留对照，§236、§238–241 迁移跳转、名义类型、nullable when 和常量测试，§347 刷新直接调用清单，§348 验证剩余两个 Single lowering 测试消费者，§349、§371 分别分类 frontend src 与 codegen src 直接调用 | §372、§378、§383 已分类旧入口的直接集成测试消费者。§395 已核对仓内显式别名与裸函数引用。§413 确认私有 Single codegen 引擎已退役，仍有测试消费旧前端产物。§419 补 workspace 自定义构建目标和字面 Rust include 清点；§513 复核 macOS 默认生产目标的宏展开。其他配置、间接身份和仓外调用仍待核验 |
| R1 | UTF-8、模式栈、Pratt、strict trial、lambda DFA 有独立抽查。§208、§242–244 补齐已复现的调用、下标及 else 恢复，覆盖红绿、扫描计数及公开入口 | §316 修复 §255 已复现的复合条件。§375 复验嵌套 if 的 else 归属，§396 对账截断跨行 when 的完整文件恢复。§413 在新基线复跑分隔符、控制流和恢复，并接收 #477 局部证明的 CI 证据。其余缺分隔输入仍按矩阵复审 |
| R2 | F1/F2/F8–F11 的已列复现有修复与验证。§214–223、§227 核对条件写回、循环退出及后继读取。§405 验证 Unit 类型检查迭代遍历短路左链 | §397–401 的已知 selector 与实达布尔边修复随 #448 合入。§414–415 修复 §402、§405 的直接左链重复扫描及条件事实遍历递归，§416 增加操作次数门禁；§511 修复取反跳转并双宿主复验门禁。剩余集合与右子树成本继续跟踪。旧测试消费者处置与 R0 同步 |
| R3 | basic/const、临时父 owner、两阶段借用已独审。§209、§213 核对部分构造退出，§245 复跑交接身份及失败原子性，§403–405 验证析构规划、所有权流和活跃性的短路左链 | §406–411 已修复并验收该批的短路链中止，涵盖 128 层括号左链和 192 层右链。§421–422 新增短路条件后的 continue、break、return 组合及对应的实际 loan-end 和析构证据，其他旧事件见证仍待核对。本机和 CI 的资源矩阵分别按各自输入记账 |
| R4 | #429 的写回、SSA 负例、双宿主 conformance 与 Linux 资源证据见 §228–235、§245。§246 补验 refine 与 Compatibility 共存。§305–319 验证可空 String 读取、while 独立借用、Map 移出 ABI、分支借用、返回借用来源及自有载荷交付。§328–332 核验直接字段、调用独占性及控制流写回。§333 补齐 Compatibility 中普通可空标量局部的类型需求。§360–369 验证接收者字段置换、终止型实参、容器元素交换、堆字段与直接 Inout 槽置换。§385 复验最新测试快照的 Linux codegen 全套，§389 核验双宿主公开入口的可达性，§393 修复循环内写回后重新借用，§406–412 记录 #448 的长短路链修复及原生验收，§418 验证跨文件两条 lowering 路由与资源写回，§511 补齐接收者字段交换的闭包逃逸校验 | 生产 Unit 仍有兼容路由；§514 已清点冻结语料的实际分流及资源对照，剩余家族迁移及接线另按原合同验收。§334 核验 Map 可复制结果与普通可空局部共存。§335–336 核验泛型实例的 nullable when 写回及直接 T? 局部布局。§337 复核 RangeCall 辅助独占实参，§339 处理活跃 RangeView 跨缺省 else 的语句式 if，§340–345 补齐普通词法块和受限 if 控制体的直接 metadata 别名。第一轮冻结 SHA 纵向复审尚未完成。目标机仍为 `OptimizationLevel::None` |
| R5 | §240、§245 已核验合并树及同 SHA 双宿主 conformance。§311–312、§314、§318 在源码候选复验 137/154 探针、综合程序 0/6，§348 对账 #444 后候选，§365 复验后续冻结源码，§370、§373 补 Linux 资源缩减与 ASan/LSan，§376 复验当前双宿主完整公开语料，§386 通过 Linux 生成资源全门禁，§388 的 macOS LSP 标准复跑通过，§394 复验集成候选的双宿主公开语料和 Linux sanitizer，§417 补 macOS 公开语料与 LSP 接线证据，§420 核验同一清单的 macOS 与模拟 Linux 公共入口一致性，§424 在新基线重建证据并复跑普通 Map ASan/LSan，§425 核验生成资源原始用例，§510 记录对齐候选的双宿主结果，§512 对修复后冻结源码复验公开 conformance、CLI/LSP 和普通资源 | 第一轮等待阶段出口及冻结候选的全链路证据。v1 尚未冻结，第二轮发布验收待启动 |

## 8. 验收账本

| 输入 / 检查 | 实际结果 | 范围与限制 |
|---|---|---|
| 原始基线；`python3 scripts/check_docs.py` | 通过，846 Markdown | 仅结构，不证明语义 |
| 原始基线；`cargo test --locked --offline -p lang-frontend` 指定下列10 suite | 127 passed，0 failed/ignored/filtered | source_span 10、indexed_ast 7、lexer 20、lexer_boundary_matrix 4、lexer_stress_matrix 7、parser_expression 57、parser_contextual_type_ref 9、parser_return_control 11、parser_entry_prefix_truncation_matrix 1、parser_entry_trivia_invariance_matrix 1 |
| 原始基线；仓外 Rust 最小探针，经同次 frontend rlib | F1–F5 均复现 | raw 临时保存，不冒充正式回归测试或全链路证据 |
| 修复工作树；三新增 suite 初次行为回归 | lambda 0/3、receiver 1/4、super 0/2；均确实失败 | 测试初稿语法编译错误已先纠正，不把编译失败当行为红测 |
| 修复工作树；22个直接合同 suite，`--no-fail-fast` | 640 passed，0 failed/ignored/filtered | 含 index14、basic/const view7/10、branch9、final57、lambda4、names15/13、unit types154、unit ownership154、signature2、receiver4、deinit30、two-phase18、parser block29/call31/control7/witness1/poison1/expression57/super2、callable21 |
| 共享 postfix 扩展回归 | 吞逗号：1/2失败；吞分号：1/3失败；修复后7个 parser suite 129 passed，0 failed/ignored/filtered | 最后一次 source 变更仅恢复分支与测试；覆盖 super3/expression57/call31/block29/control7/witness1/poison1 |
| `cargo test --locked --offline -p lang-frontend --lib parser::` | 30 passed，0 failed/ignored，314 filtered | 精确选择 Parser 资源/trial/DFA；非整个 frontend lib 通过 |
| `cargo build --locked --offline -p lang-cli` | 通过 | 本机 LLVM21.1.8；后续公共入口测试会重编最后恢复修复 |
| `cargo test --locked --offline -p lang-cli --test check_cli` | 6 passed，0 failed/ignored/filtered | 新增4种缺陷各 single/project，退出码/JSON唯一code；公共single仍使用unit，不替代legacy回归 |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | 通过，5个 member | 初轮新测试的 useless_format 与 cloned_ref_to_slice_refs 已修正，不放宽 lint |
| `cargo test --locked --offline -p lang-codegen --lib ssa::verify` | 103 passed，0 failed/ignored，2553 filtered | 包含 dominance、block 参数、重复消费/drop/loan 与预算 fail-closed 等；不是全 codegen lib |
| codegen `--lib adapter_rejects_invalid_ssa_before_constructing_llvm` | 1 passed，0 failed/ignored，2655 filtered | 非法 SSA 在 LLVM 构造前被拒绝 |
| codegen `--lib unit_object_failures_preserve_targets_and_cleanup_sibling_temporary` | 1 passed，0 failed/ignored，2655 filtered | 原 entry/source/analysis/commit 失败保护旧目标、清理 sibling；不代替全 native 故障矩阵 |
| 最终 `lambda_completion` / `ownership_receiver_backedge` / `parser_super_recovery` | 11 passed，0 failed/ignored/filtered | 含独立审查追加的局部 return 负控、source 相对外层循环、conditional continue 与分号 owner |
| 最终 Parser 内部资源过滤与 CLI `check_cli` | 分别30 passed/314 filtered、6 passed/0 filtered；均0 failed/ignored | 最后共享恢复修复后重跑资源检查；CLI补核src唯一文件及源码字节保持 |
| `cargo check --locked --offline --workspace --all-targets`、`cargo fmt --all -- --check` | 均通过 | 无新增跨crate API、依赖或语言语义 |
| `python3 scripts/check_docs.py`、`git diff --check` | 均通过；847 Markdown | 只证明结构与diff；初次新增段落触发两页200行限制后，移除重复说明并保留单一账本链接 |
| `python3 scripts/check_rust_sizes.py --base ea476adcccdd3d7fad768752173c2d76b1861aa1` | 通过；2112手写文件，39个历史超1000行 | 未提高baseline或exception；旧大文件债仍存在 |
| `python3 scripts/conformance_gate.py --cli <本轮kovenc> --evidence <仓外目录> --compiler-commit ea476adcccdd3d7fad768752173c2d76b1861aa1+uncommitted-spec-0392 --jobs 1 --timeout 30` | gate通过；154 probe能力138/154，六程序0/6；两模式差异/内部错误/回归/错误输出均0 | macOS arm64单宿主；121 run、3 abort、14 diagnostic；16 probe+6程序仍known_unimplemented，不能称160项全通过 |

Conformance 编译器 SHA256：`0f4ce1b6457633c752697813e4cc3ab60c06828381f7db7dafbe36ddc1bb78a5`。
74个基础合法程序两模式58/74，未达到P2的70项门槛；六程序分别仍为
config_parse L0080、expr_eval L0165、linked_stack L0085、raii_state_machine UnsupportedNode、
varint_codec UnsupportedNode、word_count L0165。此为原有大验收程序，不与已通过的简化词频probe混淆。
路线图只更新本轮单宿主测量，不将旧双宿主数据重写为当前CI，也不改变oracle或未实现分类。

本地命令统一 `CARGO_TARGET_DIR=/private/tmp/koven-review-0392-target`；后端使用本机
`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`。复用只限本轮相同源码/工具链的实际产物。

## 9. 交付约束

- [ ] 已确认发现均有意图测试、最小修复与独立复核；未闭合项不隐藏。
- [ ] 阶段/消费者/测试清单及实际结果可追溯；Architecture 与路线图不把历史实测当当前。
- [x] 本轮切片适用 fmt/check/clippy、尺寸、文档和直接消费者检查通过；不扩成整阶段验收。
- [ ] 双宿主及全链路按前置条件执行；PR/CI 持久证据齐全后再归档。

2026-11-07 用户要求先提交已复核切片、再继续实施；本地提交已获授权，当前不自动发布 PR
或合并。Spec 保持 in-progress，未来执行结果在本账本追加，不能以已建计划替代验收。

## 10. 第二切片：分句与旧入口能力门（2026-11-07）

前片已提交 `488497a9`，未推送。第二切片仍在同一隔离分支完成，不覆盖前片结果：

- F5 新增 `L0169 expected statement separator`，在下一 element 的起点空 Span 报告；普通、
  control、lambda 三个 owner 共用检查。完整表达式后拆分属于错误恢复，缺分隔符仍拒绝，
  不把普通 expression-start 当合法 initializer stop；缺 operand/poison 保留原根因。
- Lambda 顶层换行与普通 block 一致，`x\n(y)` / `x\n+ y` 分句；group/call/index 内仍续行。
  每次只查相邻 element 的不重叠 trivia gap，不重扫 block；不构造虚假 AST 或分隔 token。
- 先有公共入口失败测试，再迁移违反现行 Guide 的旧“正例”；专用负例保留错误意图，
  Span/golden/资源计数按真实源码调整，不在通用 harness 隐式重写用户输入。
- F7 选择为旧 Single 增加专用能力诊断 `L0170`，不复制 Unit receiver/drop 引擎，也不将
  能力不足塞入只承载内部错误的 `OwnershipCheckingError`。已有类型/所有权错误优先保留，
  旧后端必须拒绝此 recovery 产品；合法 own receiver 的完整事实继续由 Unit 提供。

| 第二切片检查 | 实际结果 | 限制 |
|---|---|---|
| `parser_statement_separators` 原始回归 | 2 passed、3 failed | 确认缺分隔符、无换行注释、lambda 歧义换行 |
| 最小实现后同 suite | 5 passed，0 failed/ignored/filtered | 三 owner + 文件入口，精确诊断Span/后继identity；扩大检查结果见下 |
| `legacy_receiver_admission` 原始行为回归 → 最小实现 | 2 passed/1 failed → 3 passed | 保留 L0133、Copyable/Borrow；Unit 正例核实际 This drop；后续又补裸字段/shared capture/未知copyability与成功事实撤销 |
| 71个 Parser/diagnostic 目标初次清点 | 34个目标失败 | 旧非法正例、lambda换行旧期待及共享语料冻结数量；未删除逐变异检查 |
| 所有受影响 frontend 目标第一次清点 | 182目标，2424 passed/27 failed，0 ignored/filtered | 分句为公共 producer，明确扩大到全部消费者；失败集中7目标，已逐项修正真实分隔与资源输入 |
| 上述失败目标与新回归重跑 | 878 passed，0 failed/ignored/filtered | 原资源32/34界限未放宽；helper复用current，修复新增重复peek成本；最终完整复跑结果见下 |
| CI bounded composition 接线与纯静态选择测试 | 新增5个review suite，2个静态测试通过；`bash -n`通过 | 保留原23条Cargo命令、唯一普通target数130→135；CI-only shell失败stub未在本地运行 |

共享 entry 语料新增2个分号：scalar 721→723、边界733→735、内部删除23791→23893；
完整 grammar 语料新增2个分号：scalar1337→1339、边界1359→1361、内部删除43926→44101。
25个矩阵的冻结数量由真实枚举重算并经独立复核，分号不属于owner/lexical-mode或13字母
替换表；逐输入 AST、Span、owner、词法诊断及确定性断言保留，不将预期错误改成成功。

| 第二切片最终检查（输入 `488497a9+uncommitted-spec-0392-f5-f7`） | 实际结果 | 限制 |
|---|---|---|
| `cargo test --locked --offline -p lang-frontend --no-fail-fast` | 182目标，2451 passed，0 failed/ignored/filtered | 含全部 lib 与 integration；不能替代未覆盖合同的独立审查 |
| CLI `--test check_cli` | 6 passed，0 failed/ignored/filtered | L0169 与合法 own receiver 各 single/project；公共入口仍走 Unit |
| codegen `--lib ssa::refine` | 111 passed，0 failed/ignored，2546 filtered | 含本机执行；非整个 codegen suite |
| codegen `--lib unsupported_own_receiver_cannot_enter_legacy_lowering` | 1 passed，0 failed/ignored，2656 filtered | names/types clean、L0170 唯一，未被 entry 调用的 own 方法同样使旧 lowering 拒绝 |
| codegen Copyable readonly boundary 过滤组 | 52 passed，0 failed/ignored，2605 filtered | 保留原边界；不代替所有旧消费者迁移 |
| `cargo check --locked --offline --workspace --all-targets`、`cargo fmt --all -- --check`、workspace all-targets Clippy `-D warnings` | 均通过 | 未放宽 lint |
| `python3 -m unittest scripts.tests.test_check_ci_results -q` | 42 passed | 纯 CI 结果/选择 fixture，不是实际远端 CI |
| `python3 scripts/check_rust_sizes.py --base 488497a9` | 通过；2115手写文件、39个历史超1000行 | 未提高限额；checker 单行能力门调用增长已独立审阅 |
| `python3 scripts/check_docs.py`、`git diff --check` | 均通过；847 Markdown | 路线图由 v1.json 重新生成；不证明语义 |
| 同 §8 conformance 命令，compiler-commit 改为本片输入身份 | gate通过；能力138/154，程序0/6；回归/两模式差异/错误输出/ICE均0 | 121 run、3 abort、14 diagnostic；16 probe+6程序仍未实现；基础合法58/74仍未达70门槛 |

第二切片 conformance 二进制 SHA256：
`5c05ba085f0e98a04fa8ce00618d501ed745b9f4a126705ce28ed4a4ecbf0552`。
未修改 conformance oracle、分类或输入；原始 stderr 未出现新 L0169/L0170，未由新拒绝掩盖
既有未实现。独立上下文分别复核分句实现/预算、语料枚举算术与旧入口/后端/CI接线，均无
剩余阻断发现。Linux、sanitizer、完整 codegen suite、CI-only shell故障stub与远端 CI 未运行；
R0逐消费者对账和R1–R5其余合同继续保持未闭合，不因这两项修复恢复整阶段完成标记。

## 11. 第三切片：真实消费者分流与常量映射测试迁移（2026-11-07）

第二片已按用户要求提交 `03807a9e`；本片在此基线上继续，未推送。只迁移不依赖旧接口
专有语义的消费者，不借此删除旧实现或宣称 SPEC-0341 完成。

重复 §2 的直接调用形状清点，frontend `check_types` 为86文件/151行、`check_ownership`
为47文件/58行；codegen 在常量 helper 迁移后分别16/19、15/18，旧 entry 为4/8。
CLI/LSP/标准库未检出这些直接调用；整个 crates 未检出这三名的显式 `as` alias。
此为词法清点，仍含可能的测试源码字符串，不覆盖宏展开、间接 re-export 或函数指针数据流。

codegen 的16个直接调用文件已按用途分流（下列路径相对 `crates/lang-codegen/src/ssa/`）：

| 用途 / 文件 | 处置与迁移约束 |
|---|---|
| 接口拒绝与身份：`lowering_entry_contract_tests.rs`、`single_readonly_method_boundaries.rs` | 保留显式 legacy-only witness；不能把换用 Unit 后的成功冒称旧接口拒绝已覆盖 |
| 源码与阶段差异：`borrow_result_contract_tests.rs`、`integer_range_value_tests.rs`、`lower_frontend_owned_source_continue_tests.rs`、`lower_frontend_tests.rs`、`mixed_borrow_boundary_tests.rs`、`rc_nullable_while_bounded_tests.rs`、`rc_nullable_while_observation_tests.rs`、`while_flow_tests/if_cleanup_observation.rs` | 后继按真实 AST/诊断/ownership/SSA oracle 迁移；部分比较已走 Unit，保留的旧形状不可整文件替换 |
| 私有 callable planner：`lower_frontend/instances/callable_plan_tests.rs`、`canonical_callable_tests.rs`（同目录） | 直接消费旧私有类型和规划身份；需连同被测职责退役，不能仅替换检查函数 |
| 旧 closure lowering：`lower_frontend/source_closure/tests.rs` | 保留，待对应 source/cleanup identity oracle 在 Unit 取得对等证据 |
| CI-only 观察：`lower_frontend/cleanup_guard_harness.rs`、`lower_frontend/orchestrate/nullable_map_guard_support.rs`、`registration_tests.rs`（同目录） | 保留原边界与故障/恢复职责，不纳入普通本机运行 |

以上源文件从实际 `cfg(test)` 模块链进入；旧 lowerer 与导出由 SSA 根的 `cfg(test)` 封闭。
另外四个名字引用文件是入口定义、test-only re-export，以及 dependency/pr120 诊断字符串，
不算新增生产消费者。该分流不代替逐个下游 helper 的完整语义审计；R0尚不结项。

本片实际迁移 `constant_materialization.rs` 的四个映射测试：沿真实 SourceUnitInput/index/
validated names → const-enabled types → const-enabled ownership 获取计划，保留同轮兼容检查、
恰好一次 runtime use 与全部原始值/字节/SSA类型断言，不合成事实、不增加通用 harness。
独立审查发现 validate 只拒绝 Error，不能替代旧的“全部诊断为空”；已恢复三阶段空诊断
断言并复审通过。仅改变测试事实来源，不改变生产映射逻辑或常量语言能力。

复用现有 `lowering_dependency_tests` 的词法扫描器，在其既有 Unit/plan/support/carry/
materialization/expression/statement/IR边界禁用 `check_types`、`check_ownership` identifier。
先运行新增回归，真实失败于 alias 导入；再补最小规则。覆盖正确 API 模块的 alias、raw
identifier 调用与精确 `cfg(test)` witness；不扩大扫描器能力，不声称覆盖所有 Rust 依赖。

| 本片检查（输入 `03807a9e` + 第三片工作树） | 实际结果 | 限制 |
|---|---|---|
| codegen `--lib ssa::constant_materialization::tests`，迁移前/首次迁移后 | 各4 passed、0 failed/ignored，2653 filtered | 原期望未改；独立审查后恢复空诊断断言的最终结果另记 |
| codegen `--lib dependency_guard_rejects_legacy_checkers_in_production_cores` 红测 | 0 passed、1 failed，2657 filtered | 确认既有规则漏过旧检查器 alias |
| codegen `--lib ssa::lowering_dependency_tests` 首次修复 | 14 passed、0 failed/ignored，2644 filtered | 后续只修正fixture模块路径，最终复跑另记 |
| 最终常量映射与依赖门禁复跑 | 分别4 passed/2654 filtered、14 passed/2644 filtered；均0 failed/ignored | 已包含恢复三阶段空诊断断言与正确模块fixture的最后源码 |
| `cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings` | 通过 | 本片仅改codegen测试，不重复无关frontend全套或conformance |
| `cargo fmt --all -- --check`、`python3 scripts/check_docs.py`、`git diff --check` | 均通过；847 Markdown | 仅结构/格式检查；非远端CI |
| `python3 scripts/check_rust_sizes.py --base 03807a9e` | 通过；2115手写文件、39历史超限 | 依赖门禁976行，未提高任何限额 |

新增门禁由既有codegen完整CI套件自动执行；本片未更改生产代码，未重跑native、sanitizer、
Linux或远端CI。已迁移helper的生产映射函数逐字不变；剩余旧消费者按上述分流继续处理，
本片不改变R0–R5退出条件。迁移的所属合同仍为[SPEC-0341](0341-shared-lowering-core.md)，
本页只保留此次复审触发的最小切片与实际验收。


## 12. 第四切片：顺序容器与混合借用消费者迁移

本片基于已提交的 `c260be13`。迁移前，`type_containers` 32 项和
`ssa::mixed_borrow_boundary_tests` 7 项均通过。顺序容器的八项原 Single 测试用同一原始
源码、AST 和自定义环境分别走 MemoryDocument 与 Rooted。保留构造顺序、参数模式、
Copyability、element place、诊断序列、调用返回及 size 的类型、来源和 Span 断言。
首跑 31/32，原四条诊断缺两条 L0125。仅含 `List<Nothing>` 的原 CLI 探针 exit 0。

混合借用迁移七份非法源码的前端检查，保留原诊断码与主 Span，分别检查原始
MemoryDocument 和原跨文件 Rooted 路由。两路 names/types 均无诊断，raw ownership
匹配同轮 typed，失败无法取得 validated ownership。四项明确比较 legacy 和 Unit 的
后端 UnsupportedNode 测试仍执行原两个入口。

独审复现了直接能力谓词、名义实参及静态上界内接口元素的漏检。首版在建立签名时将
非法顺序容器替换为 Error，引入重复重载 L0097 和二义 L0124。终版在实参校验中报
L0125 并保留类型形状。真实顺序容器的元素在静态 bound 内仍按运行时类型检查，
接口错误归原 L0094 并保留声明 label。现有 Box、Map 与范围载体路径保留原职责。
本片针对显式签名。泛型调用替换后的具体实例和 Rc 载荷另记 F9/F10。

| 直接调用形状 | 基线文件 / 行 | 本片文件 / 行 |
|---|---:|---:|
| frontend `check_types` | 86 / 151 | 85 / 149 |
| frontend `check_ownership` | 47 / 58 | 47 / 58 |
| codegen `check_types` | 16 / 19 | 15 / 18 |
| codegen `check_ownership` | 15 / 18 | 14 / 17 |
| codegen `lower_scalar_file_with_entry` | 4 / 8 | 4 / 8 |

借用返回旧 helper 在六个其他文件中的调用路径已核对。保留整文件编译的用例包含
`map_require_readonly_source_can_return_its_slot_through_ordinary_wrapper`。其 `view` 未被
`entry` 调用，直接换成按 entry 裁剪的 Unit helper 会丢失该函数。迁移该见证需先保持真实
borrow-return slot、owner 和 verifier 观察。frontend 的 `type_iteration` 和
`type_range_carrier` 仍保留名字解析失败后的 Single raw 恢复见证。

迁移所属合同见 [SPEC-0341](0341-shared-lowering-core.md)。


| 第四切片验证 | 实际结果 |
|---|---|
| 原 `type_containers` 迁移后、修复前 | 31 passed、1 failed。独立新增用例也先失败，Unit 未报 L0125 |
| 中间版全 frontend 与本机 conformance | Cargo 183 个测试结果组共 2464 passed，含文档测试。Conformance 138/154，验收程序 0/6，错误输出、ICE 和模式差异均 0。该二进制早于签名后置校验修订 |
| 最终 frontend 七个 integration suite | `final_type_admission` 57、`multifile_type_capability_graph` 4、`multifile_type_checking` 154、`type_containers` 35、`type_copyability` 16、`type_map` 17、`type_range_carrier` 7，共 290 passed，0 failed/ignored/filtered |
| 最终 codegen `--lib ssa::mixed_borrow_boundary_tests` 与保留的 `map_require_readonly_source_can_return_its_slot_through_ordinary_wrapper` | 7 和 1 passed，0 failed/ignored，各过滤 2651 和 2657 |
| 最终 CLI `--test check_cli` | 6 passed，0 failed/ignored/filtered。单文件和 project 都精确拒绝 `List<Nothing>` 与 `List<Copyable>`，各一条 L0125 |
| 最终 CLI 的 F9/F10 最小探针 | 仍各 exit 0，保持开放 |
| fmt、Rust 尺寸检查 | 通过。2115 个手写 Rust 文件，39 个历史超限，限额保持原值 |
| workspace all-targets Clippy `-D warnings`、文档与 diff 检查 | 通过，文档门禁覆盖 847 个 Markdown |

独立复核在两轮修订后确认本片代码无剩余阻断，检查了原用例源码与预期、签名恢复、
接口诊断归属和旧 lowering 见证。最终版本完成定向回归。全前端与 conformance 的实跑
身份是中间版。最终全量、Linux、sanitizer 与远端 CI 待验。


## 13. 主线同步

第四切片提交为 `81bcd6cf`。以本地 `origin/main` 记录的 `a56e1b17` 合入 41 个主线提交。
主线已有顺序容器直接 Any/Nothing 检查和独立签名测试。合流保留两边的测试证据与更完整的
后置签名检查，移除重复的早期诊断。沿用主线的双入口 helper，保留主线刻意留下的两个
Single 见证。选择器保留所有目标，共 138 个唯一 frontend integration。

本地合流树的 `type_containers`、`multifile_type_signatures` 和 `type_checking` 共 134 项通过。
文档门禁和三个成功路径选择器测试通过。远端 CI 待首次推送触发。


## 14. 泛型调用和 Rc 载荷准入

以合流提交 `73167017` 为基线。三项泛型调用回归与两项 Rc 签名回归先红。第五切片沿用
L0125 校验真实容器与 Rc。显式签名保留原类型形状，静态 bound 中的 Rc payload 由现有
运行时接口检查发 L0094。源码同名 Rc、有效泛型模板及具现的正常 payload 保留原行为。

调用检查复用替换 memo，仅检查实际变动的存储位置。已知 Any 待定形态没有值表示，
与其他恢复型待定事实区分。候选失败后撤回自 bound 检查起追加的类型，成员 owner 的
提前替换也有独立检查点。失败候选未发布调用或独占的运行时布局，合法重载继续可用。
顶层函数体的有界具现检查涵盖经其他泛型调用抵达的仅函数体类型，诊断定位具体调用根。
存储及 range 检查按规范化节点访问。独审先复现共享 DAG 的指数放大，修订后默认
1024 实例预算双入口探针约 1.36–1.55 秒，独立 48 层预算回归进入 frontend lib。

签名、成员 owner 提前替换和普通顶层函数体构成当前修复范围。成员仅 body、泛型名义字段及
构造、实例化接口上界仍有已复现的 F11。成员 owner 替换成功后，成员自己的类型参数才触发失败
时，前一阶段已追加的类型仍可能保留，此边界来自静态审查。R2 继续开放。

远端 GitHub `main` 经只读对比与合流基点 `a56e1b17` 相同。功能分支首次推送可触发
`feature/**` 的 CI。建议第五切片通过本地门禁并提交后推送，借双宿主 CI 补充 conformance、
Linux sanitizer 及合流版本的证据。


| 第五切片验收 | 结果 |
|---|---|
| 合流基线与逐轮回归 | 5 项新用例基线失败。两组独占布局回归分别在签名回滚前和 bound 回滚前失败，最终通过 |
| frontend lib 与 8 个定向 integration suite | 660 passed，零失败、忽略及过滤。含共享 DAG、两入口、最终准入、范围及 callable 相邻合同 |
| CLI `check_cli` | 6 passed。单文件和 project 对原 F9/F10 复现各精确报一条 L0125 |
| workspace all-targets Clippy `-D warnings`、fmt、文档、尺寸、diff | 通过。2119 个手写 Rust 文件，历史超限仍为 39，`calls.rs` 953 行 |
| 独立复核 | 验证诊断边界、布局回滚、可行重载和类型 ID 生命周期。已有的成员准备阶段边界见上 |

最终第五切片未跑全量 integration、conformance、Linux sanitizer 或远端 CI。

## 15. 首轮远端 CI 与 fixture 语法修正

第五切片提交 `a3b0a6da53687dfd4f46ca68c295e946c698589a` 已推送。
[首次 push run](https://github.com/Halckon/Koven/actions/runs/42338909476)只执行静态门禁，
Rust 双宿主 jobs 因事件条件跳过，不能用其 success 声称动态通过。为实际触发双宿主门禁建立
[草稿 PR #336](https://github.com/Halckon/Koven/pull/336)，保持未合并。

[首轮 PR run](https://github.com/Halckon/Koven/actions/runs/42339261426)的双宿主 Check/Clippy
通过；Linux/macOS composition 均在 codegen 的同一组 14 个 tests 失败。日志中的实际根因是旧 fixture 同行
缺少语句分隔符，触发 F5 修复引入的 L0169；后续缺少 conformance/sanitizer artifact 是前序
中断的结果，不能当成独立运行通过或根因。此轮测试输入为 PR 合流树
`79f6aa10c612348ca295463ff2ccdca3df65e543`，head 为上述第五切片。

按 Guide 06 为十个 codegen 测试文件/fixture 补显式分号，保留原函数、操作顺序、SSA 断言、
运行输出和逐资源计数。`original_generic_functions.ko` 仍保留历史场景，但字节已作本轮语法
修正；不再用历史“原始”命名声称它与冻结版本字节相同。生产 Parser 的分隔规则保持。

修正提交 `a8bbe913`。本地这 14 项精确定向回归全部通过（2644 filtered，零 ignored），
包含 native 输出、Range prefix 与逐资源计数。扩大 codegen lib 全套曾因耗时主动 SIGINT，
该轮没有完整结果，不计作通过。远端 macOS 首轮另有一个已有 ignored LLDB test，原因是
缺少 debugserver task-port 权限；Linux 该 test 未忽略。完整矩阵以后续 CI 为准。

## 16. F11：成员 body 与名义存储义务

以第五切片为基线，两组新增用例实际失败：成员只在 body 使用存储类型，以及具现名义字段/
接口 bound。修复复用现有迭代替换和真实 call descriptor，不复制类型检查器：

- body template 从顶层扩展到实例/companion member，以 `UnitCallTarget` 和精确函数名称
  Span 定位；实例参数按 owner 后 method 排列，保留原闭合种子及 1024 有界 frontier。
- 名义签名在冻结前对字段、enum payload、直接接口和参数接口 bound 求有限参数义务不动点。
  义务只单调加入所属泛型参数集合，不展开 `Grow<List<T>>`；Rotate 的参数置换可传播多轮。
- 签名 TypeRef、body TypeRef、构造推断和调用 bound 替换检查具体义务；沿用 L0125，保留
  既有接口值 L0094 来源。构造失败仍检查操作数的独立错误，给恢复型 expected error 避免
  因失去 nullable expectation 额外报告 null 类型错误，不发布失败构造。
- 未调用泛型模板、函数值、源码同名 Rc、phantom 参数和合法递归 heap 类型保持可用。

独立上下文静态核验身份、参数映射、固定点终止、替换入口和失败回滚；复用本轮 frontend rlib
在仓外实跑 9 个输入，涵盖 owner/method 混合、跨成员传播、参数置换、嵌套函数存储和两个
独立非法 TypeRef。未发现可确认的新增 P0–P2；该复核没有运行 Cargo，不替代 CI。
永久回归补入双入口的混合参数反例；CLI 的 single/project 两入口增加成员、构造和 Rotate。

| 本片本地验收 | 实际结果 |
|---|---|
| frontend lib 与第五切片相同的 8 个 integration suite | 681 passed，零失败、忽略及过滤 |
| 最终成员 body 定向回归 | 15 passed，148 filtered，零 ignored；含追加的 owner/method 混合参数 |
| CLI `check_cli` | 6 passed，零失败、忽略及过滤；single/project 均精确报预期诊断且无产物 |
| workspace all-targets Clippy `-D warnings` | 通过 |
| 文档门禁、尺寸、diff、fmt | 通过；2120 手写 Rust 文件，39 个历史超限，未新增超限文件 |

F11 仅关闭上列已复现入口；R2 的完整流事实、剩余旧入口消费者及全部组合仍开放。
第五切片账本已注明的成员 owner 准备成功、方法参数后续失败时的早期类型保留边界，
不因本轮通过而视作已清除；需要独立回滚合同审计。R0–R5 尚未整体退出。

## 17. F11 候选的扩大验收

候选 head `c34ee481b48c11d8d2256b13e4084f01a3ecb8e2`，对应
[PR #336 第二轮 CI](https://github.com/Halckon/Koven/actions/runs/42341138357)。

- 本地 `check_stage_integration.sh` 退出 0：1764 passed，零失败/忽略，161 项按脚本精确筛选；
  已执行 147 个测试目标调用，含 Parser 压力、阶段交接与选定名称解析合同。
- 本地 `conformance_gate.py --baseline conformance/baseline-aef3f4f8.tsv` 用该候选的 `kovenc`
  退出 0：154 探针中 138 达到能力预期（121 run、14 diagnostic、3 abort），两入口无差异，
  无回归、错误输出和内部错误。16 探针与 6 个验收程序仍属已登记的 `known_unimplemented`；
  门禁通过不等于 6 个验收程序运行通过，P2 能力指标仍未达成。
- 第二轮双宿主 Check/Clippy 通过；完整 codegen lib Linux 2658 passed、macOS 2657 passed
  加 1 个已登记的 LLDB ignored。随后两个 Range project CLI 用例在两台宿主均因生成源码
  `}println` 缺少语句分隔符而失败（各 44 passed、2 failed），尚未抵达后续动态门禁。
- Range project CLI 两处借用块 close 从 `}` 改为 `};`，仅修正 fixture；保持普通/提前退出
  路径、原调用顺序及输出中的析构顺序断言。
  本地 `cargo test -p lang-cli --test range_project_cli` 46 passed，零失败、忽略及过滤。
- 本地词频字节与边界合同 40 条公开命令通过；使用 `c34ee481` 的同一 `kovenc`，本轮 fixture
  修改不改变编译器二进制。远端后续 conformance、sanitizer、生成资源与 Preview 仍需实跑。

## 18. Range fixture 修正候选的验证

候选 head `3abf84c2ba6563e960e49ebf0459dec3de2f4ec1`，对应
[PR #336 第三轮 CI](https://github.com/Halckon/Koven/actions/runs/42342382980)。

- 本地 `lang-lsp` / `lang-std`、`ownership_iteration` / `guide_litmus` 共 272 passed，
  零失败/忽略/过滤。教程 32 个案例已执行，1 个 planned；练习 4 个变体及 API 示例通过。
- macOS 本地生成资源门禁的 16 个运行案例均通过；同轮 checker mutation 因原仓库 Git 元数据
  只读而无法建立 worktree。用同一 SHA 的仓外副本单独执行 checker mutation，变异反例被杀死，
  恢复字节一致且恢复后正控通过。未在本地执行 Linux sanitizer。
- R2 后续边界：仓外探针 `Phantom<Nothing>` 直接形参无诊断，而显式 `List<Phantom<Nothing>>?`
  及经 `Outer<T>` 字段实例化的同一元素类型均报 L0125。第五切片已按名义实参递归判断；
  F11 将既有保守判定延伸到隐藏的存储位置。未参与布局的实参与普通 class handle 的资格
  仍需按 Guide 12 独立复审，本轮未放宽或另定语义。
- 第三轮核心 CI 的合流输入为 `5b47b2b624277afef489796aab703f284950efd5`（base `a56e1b17`）；
  Linux `Targeted Tests` 全部通过。实跑 2658 个 codegen lib 测试、46 个 Range project CLI 测试，
  词频 40 条命令，conformance 与本地同一分布，Linux native sanitizer、16 个生成资源案例和
  checker mutation 均通过。
- 同一合流输入的 macOS `Targeted Tests` 全部通过：codegen lib 2657 passed、1 个 debugserver
  task-port 所需 LLDB test ignored；46 个 Range project CLI 测试、词频 40 条命令、conformance、
  16 个生成资源案例及 checker mutation 均通过。Linux 专属 sanitizer 步骤在 macOS 按条件跳过。
- Preview 对候选 head `3abf84c2` 在 Ubuntu 24.04 和 macOS 15 各自完成 producer 构建与
  independent consumer 安装/命令验收，四个 jobs 全部通过。Editor job 因改动范围跳过。
- 同一运行的 Code CI Evidence 和 `CI Passed` 汇总均成功；这是本候选的物理实跑证据。
  Spec 仍处于 in-progress，R2/R3 余项及 R4/R5 阶段退出并未由本轮 CI 代替。

## 19. 第二次主干合流

验收账本推送期间，远端 `main` 从 `a56e1b17` 前进到 `11130e16`，包含 SPEC-0393/0394
后端旧路径退役。`523135a3` 的 branch push 文档门禁通过，GitHub 因真实合流冲突未创建
对应 PR 运行。冲突限 active 索引、inventory 和四个依赖图生成物；保留双方三份新增 Spec
的登记并由现有生成器重建图。主干改动的 52 个 crate 文件逐一与 `11130e16` 比较一致，
没有额外生产代码冲突修订。§18 的实跑证据保留原 head/base 身份，新合流需重新验收。

新候选 head `89d02823d23c4221ccb1301e326cb6c9d27a3f1d`，对应
[PR #336 新基线 CI](https://github.com/Halckon/Koven/actions/runs/42344544786)，实际核心合流
输入 `8c024fd4a281355fb33fb25dd12aa358690e40be`。本地合流后原失败 codegen 14 项、
Range CLI 2 项与覆盖双入口的 F11 `check_cli` 1 项均通过，零失败/忽略；其他项按精确过滤。
Linux 全部动态门禁通过：codegen lib 2660 passed、Range project CLI 46 passed、词频 40 条
命令、conformance 138/154（同上列 known-unimplemented 分布）、native sanitizer 与生成资源。
macOS 全部动态门禁通过：codegen lib 2659 passed、1 个既有 LLDB ignored，Range project
CLI 46 passed，词频、conformance 与生成资源通过。双宿主 conformance 无回归、入口差异
或内部错误；已知未实现能力与前一轮相同。两宿主 Check/Clippy 均通过。
Ubuntu 24.04 / macOS 15 的 Preview producer 与 independent consumer 四项均通过，
`Code CI Evidence` 和 `CI Passed` 成功，运行最终结论为 success。Editor job 按改动范围跳过；
macOS 未执行 Linux 专属 sanitizer。R2/R3 的独立复审余项及 R4/R5 退出仍开放。

## 20. PR #336 独审 P2 修复

独审基线为 `2e3b41f5`。普通类的容器存储资格曾递归检查所有泛型实参，导致
`List<Phantom<Nothing>>` 误报 L0125。签名与函数体现在共用类布局相关参数选择。
沿字段与 owning 载荷传播参数依赖，包括 Map 条目和 Box 载荷。参数集合变化只重新排入
受影响声明。回归区分 phantom 实参、真实字段依赖与直接 `List<Nothing>`，并覆盖
反向声明顺序、签名与泛型替换。

泛型 body 只为需要类型替换的 callable 建模板。每份源码的函数节点扫描一次，表达式、
类型引用、符号与调用记录各遍历一次，并按所属函数范围分发。全局已提交调用仍提供
闭合实例种子。回归在旧实现观察到 130 个模板，修复后仅保留 1 个泛型模板，并验证
普通方法经泛型成员转发到 companion 方法仍会报告实际非法实例。

独立复核曾发现初版实现遗漏类字段里的 Map 和 Box 载荷，并指出布局全表迭代会在反向
依赖链重复扫描。补上载荷回归并切换到反向依赖工作表后，复核未发现上述问题仍存在。

本地前端库及 `multifile_type_checking`、`multifile_type_signatures`、`type_containers`、
`type_map` 共 583 项通过。CLI `check_cli` 6 项通过，覆盖单文件和项目入口。LSP 56 项
通过。三组测试均无失败或忽略。前端、CLI、LSP 的 all-targets Clippy（`-D warnings`）、
格式、文档结构、Rust 尺寸及 diff 检查通过。本批修复的双宿主 CI 待验证。

## 21. 最新主干合流与双宿主验收

PR #336 合入 `9ab52dd2` 后，保留 SPEC-0341 双方验收条目，并将两处语句分隔符修正
移到主干迁移后的 nullable when 测试。合流提交 `01b467da` 本地通过 `type_checking`
83 项及 `multifile_type_checking` 165 项，格式、文档及尺寸检查通过。

[PR CI 42356276937](https://github.com/Halckon/Koven/actions/runs/42356276937) 对
`01b467daee01572a9feb4baea76a82baefdb3c16` 与 `9ab52dd2` 的合流输入
`89a5611e033d8b525fd4d7f4d6a6b5a134f4cf59` 实跑成功。两宿主的 Check、Clippy、
动态测试、Preview 构建与独立安装验收及 `CI Passed` 均通过。Linux sanitizer 通过。
两宿主 conformance 均为 138/154，零回归、入口差异或内部错误。macOS 有 1 个既有
LLDB 用例因 debugserver 权限被忽略。Editor job 按改动范围跳过。本次结果对应
PR #336 交付切片，SPEC-0392 的后续分层复审继续保留在 active。

## 22. R2 成员候选的早期替换回滚

基于已合并 PR #336 的 `main bff9468a`。原实现发现成员时具现接收者的类型实参，
随后方法实例化才建立检查点。仓内红测确认 `Factory<Long>().choose<Nothing>(1)`
虽然正确选中 Long 重载，失败候选独占的 `Holder<Long>` 仍留在规范类型表。

候选发现保留模板和 owner 实参。继承成员去重只保留结构化签名形状，临时具现结果
随即撤回。实参映射完成后建立统一的类型表检查点，涵盖 owner 替换、方法推断、
接口上界和存储校验。失败撤回本候选追加的类型，已发布的接收者、实参和其他成功
候选的类型仍可用。回归在 MemoryDocument 与 Rooted 两入口覆盖存储拒绝、接口
上界不满足和实参映射淘汰，均确认合法重载发布调用且无 `Holder<Long>` 残留。

独立只读复核核对了接收者、形状去重、临时 ID 和回滚边界。另发现已成功实例化
但在后续普通实参适用性筛选中被淘汰的候选仍可能留下类型。该边界保留在 R2，
本切片的回滚范围截止于实例化失败。后续适用性筛选的修复见 §23。

本地前端库及 `multifile_type_checking`、`multifile_type_member_graph`、
`multifile_type_signatures`、`type_checking`、`type_callable`、
`type_borrow_result`、`final_type_admission` 共 703 项通过。`check_cli`
6 项通过。前端 all-targets Clippy（`-D warnings`）、格式、文档、尺寸和差异检查通过。
本切片双宿主 CI 待验证。

## 23. R2 重载适用性与 lambda 试算的类型隔离

在同一 `main bff9468a` 基线及 §22 工作树上，仓内红测确认
`Factory<Long>().choose(input)` 的 `input: Long` 正确淘汰 String 重载，
但被淘汰的实例独占的 `Holder<Long>` 仍留在规范类型表中。

多候选调用的实例化探测与普通实参适用性检查分别使用短期类型表检查点。
跨检查点只保留原模板与实参映射。唯一胜出后重放替换。涉及上下文 lambda 的
多个候选在既有试算事务内各自完成实例化与 body 检查。单候选路径沿用原期望类型
与诊断。已检查的普通实参保留原事实，重放不重新检查源码实参。空替换跳过类型图遍历。

回归使用 MemoryDocument 与 Rooted 两入口，按两种声明顺序验证普通实参筛选、
lambda 试算、L0123 无匹配和 L0124 二义性。成功路径只发布胜出的
`Winner<Long>`、调用及完整字段布局。失败路径不发布这两个候选的具体类型或调用。

独立复核核对了原实参检查顺序、结构化探测结果、类型 ID 生命周期及胜出试算的恢复。
成员查找仍可能在这些检查点之前具现继承接口的实参。该路径的类型发布及其消费者
继续保留在 R2 对账范围中。

本地前端库及九个相关 integration 目标共 756 项通过。`check_cli` 6 项通过。
`project_cli` 首轮 38 项通过，1 项越界转换子进程超出 10 秒门限，收集的
SIGABRT、stdout 和错误文本符合预期。该项独立复跑通过，其他 38 项未重复运行。
codegen 4 项定向通过，包括重载目标、泛型实例、lambda 交接与 native 链接运行。
前端 all-targets Clippy（`-D warnings`）、格式、文档、尺寸及 diff 检查通过。
本切片双宿主 CI 待验证。

## 24. R2 继承成员的 owner 实参隔离

沿 §23 的只读发现，在同一 `main bff9468a` 基线及 §22–23 工作树上复现了
`Factory<Long>` 经 `Intermediate<O>` 继承 `Pattern<Holder<O>>` 的情况。
本类 Long 重载胜出时，仓内红测发现规范表仍包含被淘汰的继承候选的 `Holder<Long>`。

成员发现从冻结的接口闭包读取声明及模板身份。普通调用以实际接收者为替换根，
`super` 调用以显式限定接口为替换根。只有形状去重探测或实际实例化才具现继承
实参，先前已发布的类型 ID 保留。去重只提交结构化形状，候选淘汰时沿用 §22–23
检查点。继承关系核验只保留布尔结果，显式限定类型仍属于正常源码事实。

双入口回归同时验证本类和继承默认成员胜出。前者不生成 `Holder<Long>`，
后者发布实际 owner 实参及完整字段布局。两个路径的调用事实均可通过类型产物验证。

独立复审与下游红测确认，接口默认方法经具体 `StaticSelf` 调用 abstract requirement
时，Phase 4 需要从前端规范表读取 effective implementation 的具体 owner 实参。
委托还可能更换接收者，并在默认方法体内调用外层委托接口未暴露的成员。
初次延后继承具现时，4 项既有 SSA/native 用例失败。后续前端红测分别确认了
无关抽象方法重新发布落选类型、委托丢失 owner，以及委托对象独有接口缺少实参。

修复按已提交的接口调用登记具体接收者和目标。泛型 body 仅在实例存储检查通过后登记
替换后的接收者。发布器消费已验证的 override、委托字段和 next-hop/implementation
合同，在字段布局冻结前发布实际接收者及相关类型。编译单元内已出现的
`StaticSelf` 目标用于保守匹配各接收者的合同。精确到单个默认方法体的可达性
仍待后续审查。重复的具体接收者和目标在同一根内去重，递归变化的委托实参保留
受实例限额约束的有限边界。确切转发链单独按声明及目标判环，避免将已知重复的下一跳
扩张到泛型预算。独立复核验证了候选回滚身份、委托终点、正常多跳及判环条件。
Phase 4 继续验证并消费事实。

最终工作树本地前端库与九个相关 integration 目标共 761 项通过，零失败、忽略或过滤。
codegen lib 按 interface、super、delegation 与 inherited owner recipe 过滤后 59 项通过，
2601 项未纳入，零失败或忽略。SSA 规划同时断言直接、单跳与两跳路径的实现实参和实际
`StaticSelf`。CLI `check_cli` 6 项通过。另用本机 `kovenc run` 分别链接运行直接调用、
泛型转发、单跳、两跳和委托对象独有接口的 5 个独立输入，均退出 0、stdout 逐字节符合预期、
stderr 为空。

前端 all-targets Clippy（`-D warnings`）、格式、文档、尺寸及 diff 检查通过。
尺寸护栏记录 2142 个手写 Rust 文件、38 个历史超限文件。

本轮双宿主 CI、全量 conformance 与 sanitizer 待验。R2 的流事实组合、精确上下文可达性和
剩余旧入口消费者仍开放。唯一候选成功具现后若实参检查失败，诊断候选中的临时类型保留
边界的后续复审见 §27。

## 25. R2 循环回边和 Elvis 出口的流事实

以 `bff9468a` 及 §22–24 工作树为基线，先在已构建 CLI 上实跑源码探针。
`for` 和 `loop` 在读取后写回 nullable root，下一轮仍沿用进入循环前的非空事实。
可能零次迭代的 `for` 会把 body 建立的事实带到出口。Elvis 的可跳过 fallback
也会把独有的非空事实带到出口，或者在写入后提前 return 时丢掉 bypass 的非空事实。
既有 while 守卫在每轮重新建立非空事实，`&&` 中的显式 `inout` 清除旧事实。
两组新增定向测试在修改前失败。

循环检查复用现有潜在写入收集，按实际稳定 symbol 失效。while 在失效后检查每轮守卫；
for 在 source 和 binding 检查后才进入重复区域，涵盖 Range、顺序 provider 及 Map。
循环结束恢复清除过的入口事实。Elvis 只合并正常到达出口的流事实及来源版本，
`Nothing?` 左侧必走 fallback，右侧提前退出时保留 bypass。
当前修复覆盖 Unit 两入口与仍有实际消费者的 legacy checker。前端验证范围与 Guide
已有 Nullable native 阶段边界独立。

独立复审将组合反例实跑到两个原始类型入口。可短路的 `||` 右侧含 return 时，旧终止
标志会使 Elvis 恢复已经被左侧 inout 清除的事实，两入口均误接收。逻辑运算先保留可能
短路的正常出口，再以布尔字面量、分组和取反识别必走右侧的情况，保留必然 return 的行为。
legacy 的 if 曾把只出现在提前退出分支的写入版本传给正常路径。旧局部声明也曾忽略
initializer 终止，以上两个反例在 legacy 各报一条 L0084，Unit 对照通过。两入口复用
同一版本合并规则，旧局部声明透传 initializer 终止标志。

扩大验收命中 SPEC-0326 保留的旧入口 L0088 缺口见证。沿用原源码及两个入口的
`Nothing` 断言，更新准入预期为无诊断且完整产物可用。历史记录保留。

回归区分普通非空证明与旧 `when` 来源见证。提前终止的 fallback 保留原见证，正常
写入和循环头失效后的重新收窄发布新非空证明，原见证不复用。

最终前端库及八个 integration 目标 856 项通过，CLI `check_cli` 6 项通过，均零失败、
忽略或过滤。codegen lib 的 while、循环和 nullable when 定向组 54 项通过，
2606 项未纳入。短路与 branch join 定向组 75 项通过，2585 项未纳入，两组零失败或忽略。
前端 all-targets Clippy（`-D warnings`）、格式、文档、Rust 尺寸与 diff 检查通过。
另实跑 11 个 CLI check 探针，7 个拒绝用例各报一条 L0084，
4 个接受用例退出 0。本机普通短路与 Range for/loop 两个探针均链接运行并输出预期。

R4 新见证为 `fun decide(): Int { ((return 3) == 0) }`。当前 `kovenc check` 退出 0，
由入口调用该函数时 `kovenc run` 退出 2，Phase 4 返回 `UnsupportedNode`。将同一比较
放到可选短路 RHS 也能复现。将比较改为 `left() == (return 3)`，其中 `left()` 打印后返回
`Int`，check 仍退出 0，run 退出 2 并返回 `MissingFact`。scalar lowering 在求值前按左侧
builtin 类型检查比较支持，右侧求值则要求产出 value。后续需同时核验左侧先求值、已退出
operand 终止比较，以及 return 边的清理交接。修复与验收见 §26。

本轮未执行远端双宿主 CI、全量 conformance 或 sanitizer。

## 26. R4 比较操作数提前退出

沿 §25 的两个反例重新运行 CLI，确认类型准入通过，左、右操作数分别在 Phase 4
报 `UnsupportedNode` 和 `MissingFact`。新增本机回归同时覆盖默认细化和测试固定的逐家族
lowering。以两路返回值、左侧可见副作用和被跳过的右侧为 oracle。

细化准入递归核验带括号的 `Nothing`，在已证明左侧退出时跳过右侧的表示选择。逐家族
lowering 区分操作数已退出和缺少值。普通相等比较、短路与条件沿既有 return lowering
消费清理事实，退出边不构造比较结果。

资源反例中，SSA verifier 实际报告 `MissingOwnedExit`。前端 String 比较规划原来要求两个
操作数都是 String，右侧为 `Nothing` 时漏掉已求值的左侧临时值。两套 ownership planner
复用既有 pending owner 清理，让实际 `ControlTransfer` 持有该释放义务。

细化构建器只进入已有前驱的分支。对前端保守发布的死边事实按精确控制点或执行点所在
子树核验，活边仍必须消费清理事实。互斥 if 臂从同一入口临时值账本开始。短路的跳过边
保留自己尚未结束的调用借用和临时值。比较在 `when` 候选式中退出时不创建匹配边。
细化准入读取类型产物的正常完成事实和短路结构，只检查可能执行的分支和候选式。
所有权阶段省略死分支的效果事实时，仍可沿原细化路线执行。若前端已证明短路必退，
两条 lowering 路线都按顺序执行已到达的操作数并传播退出。

最终本机 codegen 定向组 139 项通过，2526 项未纳入，零失败或忽略。新增 5 项回归
覆盖两条 lowering 路线、左右退出、必走与可跳过的短路、when 候选、循环头退出，
以及各 return 边的析构和活动借用。计数夹具验证已执行分配全部释放且死边不分配。
§25 的三个原始源码探针均重新通过 CLI check 与本机链接运行。右侧退出的探针逐字节
匹配左侧一次打印和返回结果。

前端库和四个 integration 目标共 574 项通过，CLI `check_cli` 6 项通过，零失败、
忽略或过滤。前端与后端 all-targets Clippy（`-D warnings`）、格式、文档、尺寸和 diff
检查通过。独立复审核验了准入与构建器的边对应、活边清理和两条 lowering 路线。
本轮未执行远端双宿主 CI、全量 conformance 或 sanitizer。

## 27. R2 唯一候选的调用事实提交

本地分支快进到 `be42f064e348997260710a51792723aff154b5a6`，包含 SPEC-0341
loan-end 门禁。与该基线重叠的 unit drop planner 改动按三方合并保留。

失败回归先确认两个入口都能产生恢复产物，且失败调用留下仅其返回值需要的
`Holder<Long>`。后续红测分别捕获字符串、普通变量、非可空实参处的 `null` 及错误 lambda 参数个数留下
`List<Long>`，回调体 L0084 后仍发布外层调用，以及多候选预查 L0089 后多报 L0123。

候选具现的规范类型后缀在实参检查前脱离类型表。按上下文需要重新物化子图，或在只读
相容性检查后撤回探针后缀。错误 lambda 形状不物化参数类型。检查失败时保留独立实参的
调用事实及有效 lambda 绑定的类型，撤回仅候选实例需要的类型。提交候选前重映射全部
候选类型根。成功回归让内层泛型调用先占用原候选的数字 ID，验证两层描述符、lambda
参数和最终准入。跨文件 lambda 的既有回归保留原诊断码、主位置与参数标签，并要求
回调返回值错误的调用不提交。多候选预查已有实参错误时停止额外重载诊断。

前端库和 10 个 integration 目标共 817 项通过，覆盖最终类型准入和
`unit_direct_call_loan_ends`。CLI `check_cli` 6 项通过。两组均零失败、忽略或过滤。
本机 codegen 通过比较操作数退出和继承 owner recipe 定向组 13 项，零失败或忽略，
2652 项未纳入。该组包含 3 项实际链接并运行的继承调用。
前端 all-targets Clippy（`-D warnings`）、格式、文档、尺寸与 diff 检查通过。
独立复审先发现普通实参期望泄漏及预检诊断级联，修复后复核了类型 ID 隔离、
相容性探针和 `null` 的三态定型。
未执行远端双宿主 CI、全量 conformance 或 sanitizer。

R2 仍待核验需要向内传播期望类型的恢复路径。错误 lambda 形状为检查显式 return 仍可能
物化返回类型。若实际 lambda 体未读取返回上下文，该类型可能没有消费方。条件、Elvis 与
构造器也需要分别判断上下文是否真实被使用。基线已有唯一候选 lambda 体内部 Deferred
未向外传播的边界，后续复审见 §28。

## 28. R2 实参内部 Deferred 与最终准入

沿 §27 在 `be42f064` 工作树上复现。回调体的局部绑定引用未绑定的外部函数，回调仍保有
确定的函数类型，外层原先提交 `Holder<Long>` 和调用描述符。泛型推导预检中的字符串插值
同样可在内部未决时取得 String 类型。另以 `ignore<Unbound>()` 实跑确认未绑定的显式
类型实参进入 `Holder<Deferred>` 和调用事实。最终准入反例还曾在未绑定外部 callee 上
返回缺少 expression type 的内部错误。

检查器记录新增未决 expression、symbol 和 TypeRef 的身份。调用在解析显式类型实参前
记录起点，按本次实参检查的新增记录判断，验证时读取当前事实。重载试算的日志与事实
一起回滚。已发布描述符的顺序容器 callee 占位由生产点豁免，避免把完整构造当成未决。
未绑定外部重载保留 callee 的 Deferred 事实，最终准入可报告原源码位置。

独立复审发现过早读取预检日志会跳过唯一适用候选的 lambda 定型。试算现只读取进入试算
后新增的未决事实，保留原有适用性和无匹配诊断。试算选出唯一候选且预检仍有未决时，在
原发布门禁下重做该候选，保留 lambda 自身事实。最终准入对没有已选描述符且 raw 结果为
`Deferred(Call)` 的调用识别未提交 lambda，继续检查其余实参和 callee receiver。
已有独立源码诊断时沿用该根因，否则报告外层调用未决。

双 Unit 入口回归检查外层无描述符、独立调用保留、最终准入返回源码诊断。覆盖自由函数与
成员的未提交 lambda，并以正常容器构造作正例。既有 `outer<Any>()` 的 `L0125` 需由
泛型函数体的存储检查报告，因此已知 `AnyValueRepresentation` 的 TypeRef 保留该实例入口，
不视为真实未绑定类型；最终准入仍检查原 TypeRef。

最终前端库和 11 个 integration 目标共 864 项通过，CLI `check_cli` 6 项通过，均零失败、
忽略或过滤。前端 all-targets Clippy（`-D warnings`）、格式、文档、尺寸及 diff 检查通过。
本机 `kovenc check` 和 `kovenc run` 实跑空 `MutableList<Int>()` 作为普通调用实参，
退出 0，stdout 精确为 `empty\n`，stderr 为空。另一个带运行时整数插值的探针只通过
check，run 返回既有 Phase 4 `UnsupportedNode`，与已有插值拒绝测试一致。
未执行远端双宿主 CI、全量 conformance 或 sanitizer。

显式类型实参的默认上界见证已纳入门禁。未决类型参与推导或显式上界时的诊断归属，
后续复审见 §29。§27 所列期望类型物化边界仍待逐项审查。

## 29. R2 泛型推导和上界的未决输入

沿 `be42f064` 工作树复现 `identity(unboundExternal())` 的误报 L0140。对照在相同未决
输入之外加入独立的已知错参、冲突替换或不满足上界的类型，仍要求原类型诊断。

推导跟踪未决节点实际影响的模板位置。确实缺少的替换依旧报推导错误，依赖未决输入的
替换使用候选私有占位，让同一候选的已知实参、上界和存储限制继续检查。显式类型实参
及接收者、参数和结果中的未决结构也参与发布门禁。重载用适用、待定、拒绝三种结果，
已知错配可淘汰待定候选。只在未决部分仍可能影响选择时保持待定。已知 callable 形状仍
检查 lambda 的参数个数和 move 要求，缺少完整上下文的 lambda 留给恢复边界。

未决类型的部分比较保留已知构造器身份、参数模式和同一结构中的已知类型冲突。接口
上界复用现有的实现接口遍历。嵌套存储判断允许未决节点继续恢复，同时保留已知
`Any`、`Nothing` 及其他表示限制的诊断。

扩大测试实际复现了单候选 Storage 早拒后缺少普通实参事实的内部错误。该路径现在检查
独立实参。待定成员重载试算全部拒绝时保留 L0123 和未提交 lambda 的恢复锚，最终准入
仍检查接收者和普通实参。双 Unit 入口回归同时核验已知拒绝的原码、未决调用不发布、
独立事实和最终准入的源码诊断。前端库与 11 个 integration 目标共 865 项通过，
CLI `check_cli` 6 项通过，均零失败、忽略或过滤。
前端 all-targets Clippy（`-D warnings`）、格式、文档、尺寸和 diff 检查通过。
本机实跑受 `Copyable` 约束的泛型调用，check 和 run 均退出 0，stdout 精确为
`selected\n`，stderr 为空。本轮执行期间本地 `origin/main` 前进到 `c62e3abd`，
上述结果对应 `be42f064` 工作树。未执行远端双宿主 CI、全量 conformance 或 sanitizer。

待复审的边界包括重复出现的同一未知参数所需的一致性约束，以及 Range 扩展从 receiver
推导的旧入口。两个独立占位可能共享 Deferred reason，部分比较暂不据此推断同一类型。
独立静态复核还发现，已知非函数的参数可因 lambda 体也未决而漏报形状错配。多个显式重载
均因 Storage 被淘汰时也可能漏查独立实参。上述恢复和 Range receiver 的后续复审见 §30。
此前 §27 的 lambda 返回
与条件/构造上下文物化、旧入口消费者迁移及 R2 的整体退出验收仍开放。

## 30. R2 早拒调用恢复与 Range receiver 推导

工作树快进到 `c62e3abd`，包含 SPEC-0341 的 nullable when 剩余单 case 修复。
两侧只有名称与类型架构文档重叠，其不同段落按三方合并保留。

红测复现已知非函数形状漏报 L0084，以及多个候选在实例化阶段被淘汰后最终准入缺少
普通实参事实。已知非函数参数直接诊断 lambda 实参；已报错的参数类型不新增形状错误。
零候选路径在发出 L0123 前检查尚未定型的普通实参。已定型的实参不重检，无可用上下文的
lambda 不发布体事实，并以未决调用锚点交给最终准入。唯一候选的已证实实例化失败
也恢复独立实参；已有错误毒化路径保留原有诊断，避免重复报告 L0125。

Range 扩展 receiver 共用追踪未决来源的推导。显式类型未解析且 receiver 形状仍可能匹配时，
调用保持待定。显式已知的类型仍可与部分未决的 receiver 比较已知外层构造，保留 L0140。
已知错误的 count 参数保留 L0084。双 Unit 入口覆盖显式未绑定、两种已知 receiver 冲突、
部分未决 receiver 的可能匹配，以及实际选择正确实例的正例。

前端库和 16 个 integration 目标共 930 项通过，CLI `check_cli` 6 项通过，
LSP 56 项通过，均零失败、忽略或过滤。独立复核确认已知类型头冲突的诊断、签名错误
无额外 L0084，以及未提交 lambda 体和失败调用的事实隔离。项目入口本机实跑
`source.take<String>(1)`，退出 0，stdout 精确为 `selected\n`，stderr 为空。
同一项目改用 `take<Int>(1)` 时 check 退出 1，仅发 L0140。
前端 all-targets Clippy（`-D warnings`）、格式、文档、Rust 尺寸和 diff 检查通过。
未执行远端双宿主 CI、全量 conformance 或 sanitizer。

同一未知参数在多个位置的一致性，以及普通实参预检已发错时的交接，后续复审见 §31。
R2 的整体退出条件及 §27 其他事项仍开放。

## 31. R2 待定参数的跨位置约束与试算回滚

基于 `c62e3abd` 的复审工作树。源码红测确认 `Pair<T,T>` 的两处待定替换未共享
约束，`Pair<Int,Boolean>` 因此漏报 L0084。同一符号在不同实参中的冲突也可复现。
另外复现了普通实参已报 L0089 后，最终准入遍历未提交 lambda 而返回内部错误。

待定候选按原类型参数符号记录可满足的约束，保留方法自身及成员 owner 在替换前的形参。
在不变结构中比较同一参数的各处类型，已知事实可合并，真正未决的子类型仍保持待定。
不同参数独立，避免把相同 Deferred reason 当成同一参数。实参根只枚举一次当前相容规则
允许的提升，内部结构严格比较。可空正反例覆盖只允许一次提升的边界。

重载 probe 和最终检查使用同一约束。冲突可淘汰候选并保留其他候选的 lambda 上下文，
也可形成 L0123。约束合并使用局部规范类型后缀，检查结束后撤回。回归直接检查实参类型
及独立构造事实保留，并确认只供比较的可空类型未入产物。

普通实参预检报错时保留该诊断及已检查的实参事实。尚未定型的 lambda 以未决调用锚点
交给最终准入。成员重载试算全部拒绝或存在歧义时，同样保留 L0123 或 L0124 及未提交
lambda 的恢复边界。双 Unit 入口回归核对源码诊断位置。
绑定完整的外部重载也以原始 L0123/L0124 和真实外部函数身份证明回滚，修复同类内部错误。

最终前端库和 16 个 integration 目标共 942 项通过，CLI `check_cli` 6 项、LSP 56 项通过，
均零失败、忽略或过滤。前端 all-targets Clippy（`-D warnings`）、格式、文档、尺寸及 diff
检查通过。本机实跑受 `Copyable` 约束且两次使用同一类型参数的 `choose<Int>(1, 2)`，
check 和 run 均退出 0，stdout 精确为 `selected\n`，stderr 为空。未执行远端双宿主 CI、
全量 conformance 或 sanitizer。

本批证据追踪声明中同一个类型参数。不同参数若由同一个未绑定外部类型显式填入，跨参数
的外部身份关联仍待复审。另以 `take<Int<String>>(1)` 直接调用两个 Unit 入口验证，
raw 各有一条 L0082，最终准入均因 `1` 缺少类型事实返回内部错误。CLI check 已在 raw
诊断处退出 1。该 `Poisoned` 入口与 §30 的重复存储诊断一并在 §32 复审。
R2 整体退出条件及 §27 其他事项仍开放。

## 32. R2 显式类型实参早拒后的诊断与事实隔离

基于 `c62e3abd` 的复审工作树。双 Unit 入口红测复现 `take<Int<String>>(1)`
缺少字面量事实，以及 `take<Marker<Nothing>>(Marker<Nothing>(), outside())`
缺少独立实参事实。唯一候选与重载在显式 TypeRef 已报错后继续检查普通实参，
无上下文的 lambda 仍不提交。回归核对原 L0082、独立 L0165、类型实参数量
错误 L0091、值实参映射错误 L0120 与各自的源码位置。

显式 TypeRef 的存储拒绝按规范名义实例登记，仅供同一调用的实参恢复使用。
相同实例的构造仍产生局部错误，调用、试算、顺序及 Map 迭代沿用原错误门禁。
检查期间的诊断索引和恢复证据随试算一起提交或回滚。封装类型产物时仅移除已关联的
重复诊断。最终准入核验同一源码中的原 L0125，并继续遍历独立操作数。不同实例和
作用域外的错误保留独立诊断。

独立复核发现，仅在诊断发布时去重仍需要生产者保留局部检查结果。
嵌套 Range 和 Map 循环的回归检查私有所有权恢复表，失败 header 不产生描述符，
相邻合法循环仍可查询。双 Unit 入口检查字符串插值、lambda 试算、名义构造、
Rc、顺序容器和 String.clone 的失败边界。名义和 Box/Rc 的实参映射早拒以及
顺序容器、Map 的类型实参数量早拒均检查独立 TypeRef。名义实例化会检查首个错误
TypeRef 之后的兄弟节点。传入值实参的 Map factory 保留 L0121 和实参恢复，
同时停止发布构造描述符。

前端库和 16 个 integration 目标共 950 项通过，CLI `check_cli` 6 项、LSP 56 项通过，
均零失败、忽略或过滤。单文件和项目 CLI 入口实跑四个拒绝样例，均退出 1、stdout 为空且未写
构建产物。JSON 诊断依次核对单条 L0082、L0125 与 L0120、单条 L0125、L0125 与
L0121。独立复核核对了诊断索引、试算回滚、原诊断校验和新增描述符门禁。
前端 all-targets Clippy（`-D warnings`）、格式、文档、Rust 尺寸和 diff 检查通过。

R2 的跨参数外部身份关联、§27 的其余复审项及旧入口消费者处置仍开放。
本批未执行远端双宿主 CI、全量 conformance 或 sanitizer。

## 33. R2 外部类型身份与 Map verifier 消费者迁移

基于 `c62e3abd` 的复审工作树。双 Unit 入口红测复现
`take<Unbound, Unbound>(Pair<Int, Boolean>(1, true))` 漏报 L0084。
显式裸 TypeRef 按已解析但尚无类型绑定的 ExternalSymbolId 关联声明形参，
保留不同外部身份的独立约束。此关联只存在于候选比较，原有两条 L0165 和
独立构造事实保留。两个用途相容时仍待定，不发布调用描述符。

独立复核发现，使用字面量的默认宽度会误拒存在后续上下文解的调用。
红测复现 `Carrier<Long>(1L)` 与无后缀 `1` 的组合。比较复用实际字面量的符号族
和范围判断，保留可选宽度，固定后缀和已定型值沿用现有相容性。条件、Elvis
和未显式给定元素类型的列表构造，其暂定类型不作为待定形参冲突的唯一证据。
带可空外层或嵌在类型实参中的外部身份仍待追踪，不能以同一 Deferred reason 合并。
§27 的普通实参上下文问题仍开放。静态复核定位了待定时异宽度条件分支的 L0089、
无 expected 的空列表构造诊断。这些诊断发生在约束取证之前，须单独核验生产者的恢复。

Map `requireValue` 与 `withValue` 的两组 verifier 用例改走 MemoryDocument
和 Rooted Unit，沿用原源码、槽位身份、借用结束顺序、verifier 负例和 LLVM
断言。只观察未被 entry 选中的只读 wrapper 的旧 no-entry 用例继续保留，
因为它覆盖的是旧 lowerer 的无入口发布行为。
Map verifier 模块迁移前后均通过 9 项，零失败、零忽略，各过滤 2656 项。
前端库及六个相关 integration 目标共 713 项通过，CLI `check_cli` 6 项、LSP 56 项通过，
均零失败、忽略或过滤。独立复核确认 `List<U>` 的暂定元素类型也会在取证前排除。
最终工作树复跑 Map verifier 9 项与本机 native 7 项，全部通过，零失败、零忽略，
过滤 2649 项。native 包括真实 object/link/run、缺键 Abort 与按资源计数释放。
显式绑定 Long、Byte、ULong 和 List<Long> 的组合源码通过本机 `kovenc check`
与 `kovenc run`，stdout 精确为 `typed\n`，无 stderr。
前端和 codegen all-targets Clippy（`-D warnings`）、格式、文档、尺寸和 diff 检查通过。
本批未执行双宿主 CI、全量 conformance 或 sanitizer，旧入口消费者仍需继续处置。

## 34. R2 待定实参的条件分支与空列表

基于 `c62e3abd` 的复审工作树。红测复现显式外部 TypeRef 未决时，
`if (true) 1L else 2` 过早报告 L0089。空列表在缺少可确定元素期望时也不应
直接落入普通无上下文的 L0126。恢复限于调用提供的待定上下文。

条件分支相交实际可行的整数宽度，同时区分无数值期望时的默认结果和必须可空的结果。
嵌套条件继续携带这些约束。无共同类型的分支保留 L0089，固定重载按已知约束淘汰，
不会将可空期望当成整数重新定型。空顺序容器保留容器种类，元素仍未决。
和已定型同种容器合并时，候选比较读取已知元素要求，表达式类型与描述符门禁保持待定。
比较证据随重载试算一起回滚，不进入封装产物。

最终准入先核对显式 TypeRef，再检查实参。存在原 TypeRef 根因时，依赖该期望的
待定诊断不重复发布，独立子表达式继续遍历。双 Unit 入口核对源码诊断位置，
无失败调用或空列表构造描述符。独立复核发现的嵌套分支与固定重载边界已补回归并复核。
多分支 `when` 的上下文聚合见 §35。嵌套 TypeRef 的外部身份仍开放。

最终前端库和八个 integration 目标共 751 项通过，CLI `check_cli` 6 项通过，
均零失败、忽略或过滤。具体 Long 与 List<Long> 正例在单文件和项目入口
均通过 `kovenc check`。本机 `kovenc run` 退出 0，stdout 精确为 `context\n`。
前端 all-targets Clippy（`-D warnings`）、格式、文档、Rust 尺寸及 diff 检查通过。

LSP 的 55 个库内用例通过。真实 stdio 用例在若干 Cargo 运行中初始化超时；
独立协议探针和原测试二进制均复跑成功。最终 `cargo test -p lang-lsp --all-targets -- --nocapture`
共 56 项通过，stdio 用例耗时 9.17 秒，接近单次响应的 10 秒预算。超时原因未定位。

本地 `origin/main` 在本轮验证期间前进到 `2fd6bfb9`（PR #345）。
与工作树重叠的所有权 planner 文件在仓外三方合并检查无冲突，以上结果仍对应
`c62e3abd` 工作树。未执行远端双宿主 CI、全量 conformance 或 sanitizer。

## 35. R2 多分支 when 与主干同步

分支从 `c62e3abd` 快进到本地 `origin/main` 的 `2fd6bfb9`（PR #345）。
仓外保留原 diff 和未跟踪文件快照，唯一重叠的 ownership planner 文件经三方合并。
同步后逐字比较其余已跟踪 diff，并核对未跟踪路径保留。本节验收基于快进后的工作树。

双 Unit 入口红测复现 `when` 待定实参在分支存在合法共同宽度时过早报告 L0112。
`when` 与 `if` 共用待定分支比较，逐支累积整数宽度、默认宽度、可空与容器要求。
后续分支和已累计的约束求交，避免按首支字面量重新放宽。同序列容器的已知元素要求
参与固定重载筛选，空容器的实际类型和构造描述符仍待定。

独立复核复现了 `null` 换序漏报，以及普通待定分支覆盖已知整数或列表要求。
可空结果必须保有共同默认宽度，普通待定分支保留自己的未决原因，但不清除其他分支
已证实的约束。双入口回归核对不同排列、第三支冲突、只含 else 的正例和固定重载 L0123。
比较证据随试算回滚，外层调用和未完成构造仍不发布，独立已定型构造保留。

新基线上的前端库与九个 integration 目标共 813 项通过，CLI `check_cli` 6 项通过，
均零失败、忽略或过滤。前端包含本轮主干合入的借用辅助结束与根 swap owner transport
回归。具体 Long 和 List<Long> 的多分支正例分别通过单文件和项目 `kovenc check`，
单文件本机 `kovenc run` 退出 0，stdout 精确为 `when\n`。
LSP 全套复跑 56 项通过。首轮真实 stdio 初始化再次超时，同一测试二进制与独立协议
探针均通过，Cargo 复跑的 stdio 用例耗时 7.99 秒。超时原因仍待定位。
前端 all-targets Clippy（`-D warnings`）、格式、文档、尺寸及 diff 检查通过。
本批未运行双宿主 CI、全量 conformance 或 sanitizer。嵌套 TypeRef 的外部身份见 §36；
旧入口消费者与其余阶段退出条件仍开放。

## 36. R2 嵌套显式 TypeRef 的外部身份

基于 `2fd6bfb9` 的复审工作树。双入口红测复现
`take<Carrier<Unbound>, Unbound>(Carrier<Int>(1), true)` 漏报 L0084。
原来的直接形参别名改为在候选比较事务中还原显式 TypeRef 结构。相同的
ExternalSymbolId 在裸类型、可空类型、名义与已通过存储检查的容器实参、函数签名中共享证据。
已解析的不同外部身份独立，原有 TypeRef 事实及 L0165 保留，事务结束撤回临时模式。
双入口回归直接检查最终类型表没有临时外部身份，独立构造仍可查询。

独立复核发现，显式模式中的刚性调用者类型参数不能当作候选推断槽，递归调用中
同一 UnitSymbolId 也要保持这个作用域区别。函数值回归覆盖已知模式结构、相容与不相容
的调用者参数。可直接判定的 lambda 头部冲突使用现有 L0084 和 arrow 范围，
未决函数类型下的 lambda 体仍保持未提交。相关路径在修复后独立复核通过。

直接 `List<Unbound>` 的探针在本节基线的 body TypeRef 存储门禁报告 L0125，
因此本节使用已通过原门禁的 `List<Phantom<Unbound>>`。直接门禁修复见 §37。

前端库与 13 个 integration 目标共 850 项通过，CLI `check_cli` 6 项通过，
均零失败、忽略或过滤。具体函数类型和嵌套容器正例通过单文件和项目
`kovenc check`，本机 `kovenc run` 退出 0，stdout 精确为 `external\n`。
测试模块按外部身份合同拆分后，`multifile_type_checking` 单独复跑 212 项全通过。
前端 all-targets Clippy（`-D warnings`）、格式、文档、Rust 尺寸和 diff 检查通过。
LSP 首轮 stdio 初始化再次超时，同一测试二进制和独立协议探针通过；
Cargo 全套复跑 56 项通过，其中 stdio 耗时 9.09 秒。超时原因仍未定位。
本批未运行双宿主 CI、全量 conformance 或 sanitizer。旧入口消费者和阶段退出条件仍开放。

## 37. R2 未决外部类型的存储准入与构造恢复

基于 `2fd6bfb9` 的复审工作树。双入口红测复现直接 `List<Unbound>` 的 L0125，
以及局部标注触发的额外 L0126。仓外签名探针复现裸 `Unbound` 和 `List<Unbound>`
缺少原诊断而导致最终准入 `MissingRequiredFact`。body TypeRef 与签名都保留未绑定外部
类型的未决事实。容器元素仍检查已知表示约束，Map 与 Box 只对尚未知类型头延后判定。
直接 `List<Unbound>` 已可参与 §36 的跨实参身份约束。

顺序容器与 Map 工厂保留部分类型和已知容器种类，未决时不发布构造描述符。
具体构造可通过相容的部分类型标注，已知不相容的容器头仍报告 L0084。
未决容器经插值、外层构造或 `String.clone` 传播时，保留已知结果类型并延迟发布描述符；
`.size` 保留 Int。用于最终准入的待定记录与同轮表达式类型身份相核，
和重载试算一起回滚。最终准入核对标注与初始化器，保留独立诊断并传播初始化器内部错误。
故障注入回归移除初始化器类型事实，确认不完整标注不能吞掉该错误。

独立复核发现构造器使用待定签名时缺少调用端的原诊断，以及接收者内部的待定存储可经
分支合并产生具体容器类型并提前发布 size 描述符。两条路径均先用红测复现。
普通类和枚举变体构造器保留已知结果形状并阻断描述符发布，已知实参不匹配仍报 L0084。
枚举变体的静态选择器在限定名已确认时可以恢复；运行时接收者仍须消费自身表达式事实。
顺序容器和 Map 的 size 使用检查接收者期间产生的待定事实。

Map 已知非 Hashable 键使用 Float 作回归。现有 Map Hashable 实现对整数和可空键的
覆盖范围与 Guide 有差异，本批仅处理未绑定类型头和已有键准入路径。
表达式待定存储记录的跨绑定传播、签名组合能力检查中 Unknown 遮蔽已知 No，
均保留为后续 R2 复审项。

本机 arm64 macOS 扩大验收：`lang-frontend` 库测试及 14 个集成目标共 873 项，
CLI `check_cli` 6 项，LSP 全目标 56 项，均通过且无 ignored。外部环境回归分别使用
memory document 与 project source，核对 raw 和最终诊断、未发布的描述符及独立构造。
仓外程序以单文件和 project 运行 `kovenc check` 均退出 0；本机 `kovenc run` 验证
`List<Int>` 长度 2 和 `Map<Int, String>` 长度 0，标准输出为 `storage\n`。
独立复核覆盖待定事实身份与回滚、错误优先级及构造和 size 的发布顺序。
前端全目标 Clippy（`-D warnings`）、格式、文档、Rust 文件尺寸及 diff 门禁通过。
本批未运行远端双宿主 CI、全量 conformance 或 sanitizer，R2 仍在进行。

## 38. R2 已知能力否决与未决字段

基于 `2fd6bfb9` 的复审工作树。红测确认已知 `String` 字段的 Copyable 否决被另一个
未绑定字段遮蔽。Unit 签名、局部类型引用和泛型调用的结构能力合并保留已知否决，
仍保留已诊断错误的优先级。回归覆盖 Copyable 和 Transferable，分别检查明确拒绝、
只有未绑定字段和完全具体的合法输入；核对双 Unit 入口的原始与最终诊断位置。

独立复核定位到非泛型名义类型的未绑定字段。红测确认 `take<Payload>()` 误报 L0115。
泛型候选以能力查询的 Unknown 保持待定，继续检查其他约束并阻止候选发布；回归核对
没有调用描述符以及字段与调用处的 L0165。复核覆盖确定的否决、候选试算回滚和发布门禁。

构造入口仍需单独复审。仓外探针确认 `Needs<Payload<Unbound>>(…)` 在有确定否决时
只报告 L0165，`Needs<Payload>(…)` 在只有待定字段时多报 L0115。
另一个探针显示部分可空标注的 `null` 初始化器会增加
同根 L0165；本批用底类型初始化器隔离能力判定，保留该诊断问题与 §37 的跨绑定传播待核。

最终代码在本机通过前端库及七个相关集成目标，共 742 项，无失败、忽略或过滤。
覆盖 capability graph、签名、成员图、类型检查、最终准入、Copyability、callable 和容器所有权。
前端全目标 Clippy（`-D warnings`）、格式、文档、Rust 文件尺寸与 diff 门禁通过。
未执行双宿主 CI、全量 conformance 或 sanitizer，构造器能力恢复仍是 R2 的开放项。

## 39. R2 构造器能力恢复与裸枚举准入

基于 `2fd6bfb9` 的复审工作树。显式与推断构造的红测确认，部分实参使已知的
Copyable/Transferable 否决被跳过。结构能力查询现在区分确定满足、待定和确定拒绝。
显式部分类型继续使用原实参恢复，保留可确定的结构能力诊断。推断恢复先核对已确定的
类型映射，再用部分实参局部推断补充诊断证据，最多报告首个上界违例。局部映射不进入
构造实例，也不覆盖其他实参或具体 expected 确定的类型。

仅因名义字段能力待定的普通类与裸枚举构造保留已知结果类型，继续检查独立实参，
不发布构造描述符。限定裸枚举的已证实上界失败通过原诊断精确 span 和零载荷变体身份
恢复静态限定符，避免把它当作缺失表达式事实的运行时值。最终准入仍拒绝该构造。
独立复核覆盖部分推断、已确定证据优先、重复否决去重及描述符发布边界。

§38 中间版本的 `lang-frontend --all-targets` 实际通过 2,576 项、182 个目标，
无失败或忽略。该版本早于非泛型字段的 callable 修补，后续 CLI 编译读到 §39 的中间编辑，
因此不计为 CLI 验收。

§39 最终实现通过本机前端库及九个集成目标共 751 项、CLI `check_cli` 6 项、
LSP 全目标 56 项，无失败、忽略或过滤。前端覆盖能力、最终准入、构造、调用、枚举借用及容器所有权。

本地 `origin/main` 更新至 `78a36f25` 后，复审分支快进。入站 11 个文件与未提交修改
无路径重叠，快进前后的跟踪文件 diff 与未跟踪路径指纹一致。
快进后复验前端库及 11 个集成目标共 812 项，包含新合入的借用交接和 `swap`；
CLI `check_cli` 6 项、LSP 全目标 56 项通过，零失败、忽略或过滤。
前端全目标 Clippy（`-D warnings`）、格式、文档、尺寸和 diff 门禁通过。
本批未执行远端双宿主 CI、全量 conformance 或 sanitizer。

下一条边界已用仓外探针确认。`val items = listOf("${listOf<Unbound>()}")`
之后的 `keep(items.size)` 仍发布调用和长度读取描述符，快进后也复现。最终准入仅报告源 `Unbound`
的 L0165 并拒绝输入。需要核对局部绑定到使用处的依赖传播，不能从最终拒绝推定原始事实已隔离。

## 40. R2 不可变局部绑定的待定事实传播

基于 `78a36f25` 的工作树。双入口红测复现 `val` 别名之后的 `alias.size`
和外层调用提前发布。不可变局部声明按已解析符号记录待定状态，读取使用原表达式待定
事实。待定集合随候选试算一起回滚。最终准入沿用已有的失败声明身份，保留源诊断。
同名遮蔽的具体值、具体初始化器及确定覆盖后的独立调用仍保留原描述符。

独立复核发现，未完成的显式标注与回滚后仅保留 Deferred(Call) 的 lambda 调用
也会丢失依赖。绑定同时核对标注类型、初始化器实际结果与嵌套待定事实。
回归保留具体 `listOf<Int>(1)` 的描述符，阻断依赖插值的调用；未提交的 lambda 调用
保留原调用处 L0165，带显式 Int 标注的后继绑定也不发布调用。增量已独立复核。

本批仓外探针确认了剩余的生产者边界。`alias[0]` 仍发布元素 place，
待定 Int 的 `inv()` 仍发布整数操作。可写绑定在覆盖前和仅单分支覆盖后仍会发布
`.size` 和调用描述符。确定覆盖后应保留独立描述符，后续需按可达路径核对。
这些探针最终均由原 L0165 拒绝，尚不能证明原始执行事实已隔离。

本机 `78a36f25` 复审工作树通过前端库及 13 个集成目标共 835 项，覆盖重载、最终准入、
分支与循环流事实及所有权交接。CLI `check_cli` 6 项、LSP 全目标 56 项通过，
均无失败、忽略或过滤。前端全目标 Clippy（`-D warnings`）、格式、文档、Rust 文件尺寸和
diff 门禁通过。未执行远端双宿主 CI、全量 conformance 或 sanitizer。

## 41. R2 索引、Map 写入与整数操作的待定准入

基于 `78a36f25` 的工作树。红测确认顺序容器下标、Map get/put 以及 `replace` 会对待定操作数
提前发布描述符。顺序下标、Map 下标与 get、整数内建保留已知结果类型，延迟发布；
正常的独立元素读取、查询和整数操作保留原描述符。Map 探测先检查顺序写入的接收者时，
将嵌套待定事实保留到后继下标检查，避免缓存命中绕过门禁。

顺序写入与 inout 用同一元素合同核对可写性及 RHS。待定元素传给 `replace` 或 `swap`
仍检查实参及类型，调用和所有权原语都不发布。测试覆盖非法下标、只读写、结果不匹配、
整数实参及类型实参数量错误和独立操作。

Map 下标写入的待定、类型失败和只读拒绝保留私有恢复角色。最终准入核对源限定、实际
Map 类型与三个操作数，不虚构下标查询类型。RHS-only 待定与键待定后产生的独立 RHS 错误
均有回归，`map.put(...)` 使用相同操作数门禁。私有事实随试算回滚，正常 MutableMap 写入
沿用原有类型和 Unit 结果验证。独立复核覆盖缓存、私有写角色、原诊断和所有权原语的发布顺序。

可写局部绑定的分支传播仍开放，确定覆盖和条件覆盖须分别验证。

本机前端库及 19 个集成目标共 938 项、CLI `check_cli` 6 项、LSP 全目标 56 项通过，
无失败、忽略或过滤。仓外正控同时执行 `kovenc check` 与 `kovenc run`，核对 Map
下标写与 put、下标读与 get、数组索引和整数 inv，原生输出精确为 `index-facts\n`。
前端全目标 Clippy（`-D warnings`）、格式、文档、Rust 文件尺寸和 diff 门禁通过。
本批未执行远端双宿主 CI、全量 conformance 或 sanitizer。

## 42. R2 可写局部绑定的覆盖与分支合流

基于 `78a36f25` 的工作树。红测复现覆盖前提前发布 `.size` 与后继调用。
可写局部绑定的待定状态进入原流快照并随候选回滚。确定的直接赋值更新绑定，
分支汇合只合并可达正常出口，任一出口仍待定时阻止后继发布。
回归覆盖未覆盖、单分支覆盖、单分支引入待定、两分支覆盖、终止分支、`when`、Elvis
和短路。数字流版本的 `max` 合并及原短路失效规则保留。

独立复核发现 RHS 内的退出分支和嵌套覆盖会把求值期间的待定事实误算成替换值。
赋值发布仍检查全部求值事实；绑定更新单独检查 `if` 和值用 `when` 的正常出口与尾值。
可达 `when` 条件还核对实际类型，覆盖只剩 Deferred(Call) 的重载回滚。有限域耗尽后的
候选仍参与诊断，其待定事实不进入替换值。后继独立描述符与已拒绝赋值的原始表均有断言，
并经独立复核。初始具体、后来可能变待定的绑定在使用处保留 L0165。

循环回边及控制跳转上的写入、捕获可写绑定和 inout 写回仍待复审。复合条件为真或为假时
必经的写入尚需更精细的边事实。此批不以正常出口覆盖推断这些路径已闭合。
仓外探针分别确认回边、`break`、捕获体和 `replace(&items, …)` 的后继读取仍发布
`.size` 和调用描述符，最终准入均由原 L0165 拒绝。后续须验证对应边上的待定状态。

本机前端库及 19 个集成目标共 940 项、CLI `check_cli` 6 项、LSP 全目标 56 项通过，
无失败、忽略或过滤。前端全目标 Clippy（`-D warnings`）、格式、文档、Rust 文件尺寸及
diff 门禁通过。仓外正控 `kovenc check` 和 `kovenc run` 验证两分支覆盖后读取不同列表长度，
原生输出精确为 `var-flow\n`。本批未执行远端双宿主 CI、全量 conformance 或 sanitizer。

## 43. R2 循环回边、跳转与条件出口

基于 `78a36f25`。红测确认循环回边和跳转状态丢失会使待定读取提前发布。
每个循环记录本层 break 与 continue 的待定集合。普通落空和 continue 合入循环头，
break 只合入退出边。不可达的顺序语句、短路 RHS 及穷尽 `when` 的剩余候选仍参与
诊断检查，跳转记录只使用可达边。无正常出口的内层循环和已知走终止分支的
`if` 也不会形成外层正常回边。

`while` 与 `if` 分开处理真假边的待定状态，组合字面量、布尔 `if` 尾值、否定与
短路运算。类型流收窄仍使用原有数字版本与保守失效规则。待定状态的可达性随候选事务
回滚，`while` 与 `loop` 的公开完成合同继续用于缺少返回值检查和后端降级。独立复核
核对了该边界、嵌套块和不可达跳转。双入口回归覆盖普通回边、continue、break、嵌套、
for 耗尽、条件假边、不可达写入和确定干净的退出。

首次类型检查只发现入口增长的循环位置。恢复轮按该位置启用事务复查，各次调用候选
仍用自己的类型与绑定状态。重放只增加循环入口的待定位，丢弃中间类型产物、诊断及跳转
记录。两级传播回归核对依赖调用被阻断，独立调用只发布一次。若恢复轮发现新位置，才
扩大重放位置集。无增长的正常 Unit 不进入恢复轮，未标记循环不克隆完整试算产物。
多站点错误恢复的最坏开销仍缺少压力测试。

捕获可写绑定和 inout 写回仍待核验。Guide 07 的 mutating closure 应区分形成与实际
调用，Guide 10 的 replace 和 swap 应区分旧值返回、确定覆盖和两处交换。局部初始化器、
`when` 分支正常合流、Elvis 与普通求值操作数的私有终止传播单独保留为开放边界。

本机 Apple Silicon macOS 前端库及 21 个集成目标共 966 项，CLI `check_cli` 6 项、
LSP 全目标 56 项通过，无失败、忽略或过滤。仓外正控通过 `kovenc check` 与
`kovenc run`，循环同时使用 continue 与 break，原生输出精确为 `loop-flow\n`。
缺返回值的 `while (true)` 与包含不可达 break 的 `loop` 均在 CLI check 返回 L0088，
退出码为 1。前端全目标 Clippy（`-D warnings`）、格式、文档、Rust 文件尺寸及 diff
门禁通过。本批未执行远端双宿主 CI、全量 conformance 或 sanitizer。

## 44. R2 所有权原语的局部绑定写回

基于 `78a36f25`。红测确认 `replace` 把待定新值写入具体局部绑定后，后续长度读取
和调用会提前发布。类型检查将原语的求值准入与绑定上的值状态分开。正常出口的
`replace` 返回旧值状态，并按新值状态更新局部绑定。`swap` 将交换前的两份状态写回
对侧。待定操作数和被拒绝的调用不会发布执行描述符，后继独立事实仍可发布。

独立复核找到嵌套原语、局部初始化器、普通赋值、第二操作数中的不可达跳转及
具体 String 掩盖未决插值等传播缺口。
求值使用现有未决实参判据，提前退出使用随候选和循环重放回滚的私有完成状态。
公开静态原语身份继续由既有合同提供。修复后独立复核确认上述边界。双入口回归核对旧值、确定覆盖、双向交换、
循环 break、缺少 `&`、提前退出和插值内部未决。

本批范围是已参与待定流状态的稳定可写局部绑定。已知仍开放的入口包括捕获局部变量、
`inout` 形参内部写回和跨调用的写回摘要。待定状态使用的稳定键仍排除被闭包捕获的
`var`，即使写入发生在闭包外。一般操作数的私有终止传播继续开放。

本机 Apple Silicon macOS 上，最终 `cargo test -p lang-frontend --all-targets` 通过
182 个目标、2,599 项。CLI `check_cli` 6 项及 LSP 全目标 56 项通过。无失败、忽略
或过滤。仓外 `kovenc check` 与 `kovenc run` 验证了 List<String> 的旧值、写回
和交换后两端长度，原生输出精确为 `primitive-writeback\n`。前端全目标 Clippy
（`-D warnings`）、格式、文档、尺寸及 diff 门禁通过。未执行远端双宿主 CI、
全链路 conformance 或 sanitizer。

## 45. R2 inout 形参与捕获局部变量的本体写回

基于 `78a36f25`。红测确认两类根缺少待定状态更新，导致后继长度和调用提前发布。
从已解析的源码身份获取待定写回根，仍按原来的稳定性判据获取 smart cast 的根。
具名函数、成员、缺名恢复入口和上下文 lambda 初始化各自的 inout 形参状态。
捕获局部 var 在外层和闭包本体内的直接写入参与流合并。闭包形成后恢复外层快照，
循环扫描不把闭包体当作当前求值。回归覆盖捕获根、inout 交换、continue 和独立调用。

独立复核复现直接 Deferred 操作数令 replace 与 swap 提前返回、跳过写回。
满足参数合同且正常退出时先转移私有待定状态，再按原优先级返回 Deferred 或 Error。
不符合参数模式或已知操作数存在类型冲突时保留原根状态。显式类型实参未决的情况下，
匹配的具体右值仍可覆盖具体根。独立重审确认上述边界。双入口回归覆盖确定类型外壳、
直接 Deferred、错误的参数模式、已知类型冲突和循环回边。

跨具名调用的 inout 写回和实际调用闭包后的捕获写回仍开放。现有普通调用只失效收窄事实，
后续需核对正常返回、条件写入和回调未被调用时的待定状态。
仓外双入口探针证实，具名 `update(&items)` 与直接调用写捕获的闭包后，
`items.size` 和 `keep(items.size)` 均仍发布。两种程序的最终准入都由源
`Unbound` 的 L0165 拒绝。

最终本机 Apple Silicon macOS 验收通过前端库和 17 个集成目标共 1,322 项，
CLI `check_cli` 6 项和 LSP 全目标 56 项。均无失败、忽略或过滤。仓外正控
`kovenc check` 和 `kovenc run` 验证闭包读取、借用结束后的 inout 调用和调用者
读取，原生输出精确为 `writable-roots\n`。前端全目标 Clippy（`-D warnings`）、
格式、文档、尺寸及 diff 门禁通过。本批未重跑前端全量、远端双宿主 CI、
全链路 conformance 或 sanitizer。

## 46. R2 具名 inout 调用的待定写回

基于 `78a36f25`。双入口红测复现调用者过早发布长度读取和后继调用。按具名目标及声明
顺序形参位置汇总可能带出待定值的正常返回。显式 return、表达式体与 Unit 正常落空
参与汇总，lambda 形成隔离自身出口。出口采集随候选试算和局部循环重放一起回滚。

新增写回在后继 Unit body 轮次消费，独立于函数的源码排列顺序。新循环位置先收敛，
再采集该轮写回；未增长的 Unit 只检查一次。写回只增长，仍保留原有收窄失效规则。
调用者通过实际形参映射更新直接可写根。选中目标的操作数仍待定时可以传播私有写回，
公开调用描述符继续走原门禁。

独立复核复现了实参、成员 receiver 和不可调用 callee 提前退出后的伪正常边，
已知 Nothing 的调用在待定恢复时丢失终止，以及转发深度改变待定传播。
新增回归核对这些边、映射失败的恢复检查、重载隔离和 continue 回边。
待定下标保留已知 List 元素只读性，原 L0122 在写回前否决该调用。最终独立复核确认
该错误和前述三个退出及转发反例均已闭合。

本批使用只增的可能待定摘要。确定覆盖清除调用者原有待定、参数之间的输入依赖、
实际调用闭包后的捕获写回、接收者写回、真正无法选定目标的调用及大规模错误恢复压力
测试仍开放。

最终本机 Apple Silicon macOS 验收通过前端库和 17 个集成目标共 1,334 项，
CLI `check_cli` 6 项和 LSP 全目标 56 项，无失败、忽略或过滤。仓外
`kovenc check` 和 `kovenc run` 验证逆序声明、命名参数及两层 inout 转发的实际写回，
原生输出精确为 `call-writeback\n`。前端全目标 Clippy（`-D warnings`）、格式、文档、
尺寸及 diff 门禁通过。本批未执行前端全量、远端双宿主 CI、全链路 conformance
或 sanitizer。

## 47. R2 直接闭包调用的捕获写回

基于 `78a36f25`。双入口红测确认直接调用可写闭包后，外层长度读取和后继调用会提前
发布。非 move 闭包收集正常出口上新增的待定外层局部根，调用按 lambda 身份消费。
直接字面量、括号与不可变局部别名保留该身份。形成闭包、仅传递闭包、被拒调用及先
退出的实参均不写回。来源、出口采集和待定状态参与候选与循环重放的原有回滚。
循环恢复按入口根上的实际回边状态收敛，捕获写回无需在外层语法中有显式赋值目标。

独立复核复现无正常出口的闭包被当作 Unit 正常完成，以及 L0087 恢复被误作确定
终止。私有闭包摘要区分正常出口与确定终止，硬错误时保守处理终止信息。公开类型
及发布门禁沿用原规则。动态来源、跨具名调用的回调执行和确定清除待定仍开放。

### R3 可变捕获验收缺口

Guide 07 与 Guide 15 已启用非逃逸闭包可变捕获。仓外最小探针验证直接调用和同步
Borrow 回调两种形式。单文件与显式 source-root 的 `kovenc check` 均退出 1，
在捕获局部 var 的赋值处报 L0135，提示捕获绑定不可变。两个所有权检查器仍有旧
不可变捕获门禁。该差异单独列入 R3，Phase 2 的私有恢复事实不授予独占捕获能力。
Unit 的捕获描述符当前把非 move 来源分类为 Shared，dataflow 也创建 Shared loan。
`final_type_admission::simple_assignment::captured_and_local_tail_assignments_keep_ownership_boundary`
仍把捕获根赋值的 L0135 当作预期。R3 修复应同步核对捕获分类、调用和 ASAP 释放边，
并把旧诊断断言改成 Guide 正例与真正冲突的反例。

### 本地验收

最终 Apple Silicon macOS 通过前端库和 17 个集成目标共 1,343 项，CLI
`check_cli` 6 项，LSP 全目标 56 项。无失败、忽略或过滤。前端全目标 Clippy
（`-D warnings`）、格式、文档、尺寸及 diff 门禁通过。仓外只读闭包别名通过
`kovenc check` 和 `kovenc run`，原生输出精确为 `closure-read\n`。同一构建
复现上述四组可变捕获拒绝。未执行前端全量、远端双宿主 CI、全链路 conformance
或 sanitizer。

## 48. R3 独占捕获需求与所有权准入

基于 `78a36f25`。沿 §47 的 L0135 反例，Unit 现在从已定型的全根赋值、解析后的
目标符号和近邻 callable 匹配非 move lambda 的外层局部 var。括号继承根身份，
move 快照、字段、普通形参及跨过紧邻环境的来源不会被归入本批的独占需求。

形成时用已有独占访问规则检查真实 loan 冲突，有冲突保留 L0135。没有冲突时，
具体赋值在所有权门禁以 L0164 报告所需的独占捕获 loan 尚未接通，保留诊断位置
和来源标签。过程中不提交共享或独占 loan，失败产物不发布 captures、loans 或
drops。旧 Single 路径保留 L0135；两个 CLI 输入都使用 Unit 门禁。

独立复核发现 basic 产物可能保留未访问 lambda 的 Shared 捕获。按 source-qualified
lambda 身份同步移除这类闭包和它的全部捕获，保留其他 basic 不可达策略及常量入口
既有过滤。红绿回归核对空发布表与成功的不可达程序验证，独立重审确认闭合。

独占捕获的成功形成、调用期间可写权限、ASAP 释放、escape 及 native 写回仍是
R3 的开放交付。当前 L0164 是实现边界。
[ADR-0009](../../adr/accepted/0009-concrete-closure-internal-abi.md) 的现有环境 ABI
只描述共享捕获地址和 owned 槽位，接通独占捕获时需同时验收其借用身份和释放边。

扩大后端检查发现旧闭包矩阵仍预期拒绝 `while (break)` 嵌套在第二个实参的源码。
真实 native 探针在外层循环退出后只输出 `prefix-break\n`。旧拒绝断言改为验证
实际 Program、LLVM 及 break 边的前缀 loan 结束。邻接的函数值实参内循环仍报
UnsupportedNode，保留原拒绝断言。本项未修改后端生产代码。

Apple Silicon macOS 本地通过前端库和 12 个集成目标共 1,217 项，CLI `check_cli`
6 项，LSP 全目标 56 项，后端 `--lib closure` 筛选 100 项。前端、CLI 和 LSP
没有忽略或过滤，后端筛选过滤 2,565 项。前端全目标 Clippy、格式、文档、尺寸及
diff 门禁通过。单文件及显式 source-root 的 CLI 探针确认 L0164 与真实冲突 L0135
可区分，`check` 与 `build` 诊断一致，失败构建未发布可执行文件。只读闭包及含
不可达独占捕获闭包的程序均实际原生运行。本批未执行全量 workspace、远端双宿主
CI、全链路 conformance 或 sanitizer。

## 49. R3 Copyable 独占捕获借用

沿 §48，在 Unit 所有权层为 Copyable 局部根的已定型赋值建立独占 loan。闭包体仅在
提交该根赋值时取得写权限，仍检查体内活跃 loan。捕获描述符的 `borrow_kind()` 表示借用
权限，`mode()` 表示环境存储分类。独占环境不可传送，函数类型为 MoveOnly。

独立审阅的反例覆盖提前释放、逃逸与控制流。求值完的 callee 和实参保留闭包来源，
内层调用结束时按仍持有的环境判定释放。lambda 结果与 Value 实参、构造和字段
交付检查实际闭包来源，捕获来源按形成时的身份带入闭包体。独占来源按完整 lambda
身份建立 Unit 索引。if/when 的分支入口及
循环退出按活跃性释放已无后续用途的独占环境。合法直调、同步回调及普通局部别名
可在最后一次使用后重新访问源变量。

尚需证明循环回边上的借用状态、显式 borrow 绑定、借用返回和 nullable 函数值的环境
延续，以及 MoveOnly 旧值的清理。这些未覆盖入口报 L0164。Unit 的 SSA 布局入口在构造闭包环境前拒绝
独占 capture，默认路径与兼容路径共用此门禁。独占环境地址、写回和析构的 native
验收归入后续 R4。

基线 `78a36f25` 的 Apple Silicon macOS 工作树验收。前端库与 12 个集成目标
共 1,232 项，含闭包 56 项。CLI `check_cli` 6 项，LSP 全目标 56 项，后端
`--lib closure` 100 项。各组零失败、零忽略，后端按闭包筛选过滤 2,565 项。
前端全目标 Clippy（`-D warnings`）、格式、文档、尺寸及 diff 门禁通过。

单文件与显式 source-root 的 CLI 均确认直调及同步回调的 `check` 退出 0，
`build` 报 UnsupportedNode 并退出 2，未发布可执行文件。共享 loan 冲突两条
命令均报 L0135 并退出 1。只读闭包与不可达独占闭包两个程序均构建并原生运行，
标准输出分别为 `closure-read\n` 与 `unreachable-capture\n`。独立复核跟进
了反例修复及共享释放、重绑来源对照。本批未执行全量 workspace、远端双宿主 CI、
全链路 conformance 或 sanitizer。

## 50. R3 MoveOnly 捕获根替换

沿 §49，Unit 在捕获时对已初始化的具体 MoveOnly 局部根建立独占借用。根存储类型来自
符号的已定型声明。函数和可含闭包的存储，以及未经证明的擦除或泛型存储仍报 L0164。
闭包体可读取旧值及提交完整替换。以 owned 方式交付旧值仍报 L0133。

析构规划以 `BeforeReplacement/ReplacedCapture` 表示提交时槽内当前旧值，包含 lambda、
assignment 和 source-qualified 根符号。完整 RHS 正常完成后才发布该事实。嵌套替换
按实际提交先后执行，每次调用对同一个静态事实使用当时的槽值。return 和 Nothing RHS
不产生旧值替换事实。闭包体的 named owner 集合不持有捕获根，调用者负责最后的根析构。

独立复审复现内联回调的后续实参分支以及内层 for break 提前释放借用源。调用前缀按形成
深度保护具名环境及其直接共享和独占借用根，保留仍待调用的外层前缀。正常返回和真正离开
调用时先清理临时环境，再析构根。return 跨多层 provider 时，逐层使用各自的深度清理，
使外层局部根先于对应 provider 析构。已求值的具名 callee 也约束后续实参内的移动。
独占根与环境借用源按完整 lambda 身份建立索引，planner 按身份查询。当前仍以 L0164
限制会丢失闭包身份的可变槽、控制流结果及嵌套环境。上一批的借用返回、显式 borrow、
nullable 和循环回边边界继续适用。

SSA 布局和既有析构消费者显式拒绝尚无本机实现的独占捕获。后续 R4 必须连接真实槽地址、
提交时读取旧值、析构与写回，并验收资源顺序。

基线 `78a36f25` 的 Apple Silicon macOS 工作树验收。前端库及 12 个集成目标
共 1,242 项，包含闭包专项 66 项，另外运行构造专项 8 项。CLI `check_cli` 6 项，
LSP 全目标 56 项，后端闭包与资源析构专项分别 100 和 45 项。各组零失败、零忽略。
两个后端选择器分别过滤 2,565 和 2,620 项。

前端全目标 Clippy（`-D warnings`）、格式、文档、尺寸和 diff 门禁通过。单文件与显式
source-root 均验证 MoveOnly 独占回调的 `check` 退出 0，`build` 报
UnsupportedNode 并退出 2，无可执行文件。两入口的普通 String 根替换均构建并运行，
原生标准输出为 `new\n`。未执行全量 workspace、远端双宿主 CI、全链路 conformance
或 sanitizer。

## 51. R4 闭包捕获 loan 的 SSA 验证

独立复审定位了四条 malformed SSA 路径，并在修复前逐条实跑复现 verifier 放行。

- 两个活跃闭包接收同一 Shared loan。第一个环境释放后，第二个仍可调用，源 owner 甚至可先析构。
- 父闭包析构释放 capture loan 时，另一环境仍持有活跃的派生 loan。
- 两条 CFG 边以相反顺序传递两个闭包，合流后若两者的捕获依赖都成为并集，第一个析构可释放两条 loan。
- Copyable `LoanMutate` 仅校验自身活跃，允许在 shared reborrow 尚存活时写入。

形成和实际析构按 loan 查找活跃环境，阻断重复接管和提前结束。同一环境内同时清理的捕获可
一并结束，仍活跃的外部派生 loan 阻止析构。实际析构与 CFG 状态迁移分别验证。正例覆盖同一
根的独立 Shared loan、子环境先析构、父子同时跨边，以及交换闭包时显式重绑相应的 loan
参数。写入复用已有重借用依赖检查。

现有 LLVM RootPlace 把当前 SSA 值初始化到临时地址。前端已允许独占捕获的可观察写回，
且在 if/when 分支进入前可以释放死闭包，当前 drop planner 的 if 事实主要在分支退出发布。
R4 的写回需要统一的根槽和可消费的分支入口清理事实。[ADR-0036](../../adr/proposed/0036-exclusive-closure-capture-storage.md)
提出持久槽、专用独占投影及对实际替换事实的消费。提案仍为 proposed，现有独占环境 native
拒绝路径仍是实施边界。

Apple Silicon macOS 定向验证以 `78a36f25` 工作树构建的 codegen 库二进制执行。
`ssa::verify` 103 项和 `ssa::closure_operation_tests` 51 项均通过，零失败、
零忽略。各选择器分别过滤 2,566 和 2,618 项。格式、文档、尺寸与 diff 门禁通过。

另行执行的脚本全套共 512 项，2 项断言失败、1 项子进程超时、1 项跳过。
镜像安装器测试在默认 macOS `sha256sum` 缺少 GNU `--strict` 时失败，切换到 GNU 工具
后单项通过。磁盘诊断子进程退出和 conformance mock 的部分输出两项复跑仍失败。
两项涉及的脚本和测试文件与当前基线一致。它们单独登记，未用作 verifier 修复的通过证据。

扩大 codegen 库终态为 2,660 通过、8 失败、1 忽略；独立 SSA 选择器为 1,899 通过、1 失败，
过滤 769 项。忽略项为缺少沙箱权限的 LLDB 断点测试。7 项 ordered-map 测试在未插桩产物的 10 秒门限
超时，未进入插桩阶段。原可执行文件逐个重跑均退出 0，stdout 与各自预期逐字节相同，stderr
为空。单独重跑 collision 测试仍在新构建产物的首次运行超时。另用同一 Clang 构建的 C smoke
首次运行约 10.16 秒，仓外新链接的 collision raw 约 11.68 秒后成功。保留完整测试失败结果，
插桩结果仍待实际执行。两组共同失败的 `branch_join_and_replacement_keep_current_proof_identity`
已用同一旧二进制单独复现：nullable receiver 失效后 `current.value` 和后继 `return total`
分别产生 L0165，超出原断言的一条诊断。R2 修复及原用例复跑见 §56，该失败独立于宿主超时。
当前候选的 CLI `check_cli` 与 codegen Clippy 验收见后续章节。本节尚无远端双宿主或完整 conformance 证据。

## 52. R3/R4 交接的分支入口清理

借用检查以 source-qualified 控制表达式、分支下标、具名 holder 和环境来源记录释放需求。
planner 在 if、普通 when 和 String when 的选中边开始执行前核对实际 owner 并发布
`BranchEntry`。隐式空分支也具有稳定下标。当前分支的入口活跃集保护分支体将读取的根。
已由后续条件调用结束的环境无需重复清理。仅部分 alternative 仍持有的闭包沿已有
`WhenAlternativeMatch` 清理，共享入口核对各匹配边都已完成对应义务。

核对失败以既有 ResourceLifetime deferred 封闭 drop 产物。覆盖分支合流后无法确定
具体环境、受保护的 owner 以及来源不匹配。具名共享包装按外层到内层的顺序清理。
活跃 wrapper 可继续持有不再被直接使用的内层环境，借用检查的逻辑退役不会提前变成
runtime 析构事实。

独立复审定位并复现内联回调对共享 MoveOnly 根及内层 callable 的保活缺口。借用检查在
每个 callee、receiver 或实参求值后，以调用及 operand 的 source-qualified identity 记录
实际环境链中的借用根。planner 按前缀求值顺序消费见证，涵盖共享包装和移入内层闭包的
move 包装。后续实参的分支仍保活这些根。String when 的独立清理清单与实际析构使用相同的
pending 保活检查，避免额外要求匹配边析构。

现行 codegen 按 source-qualified 控制点识别 BranchEntry，refine 在未接通入口消费前
明确拒绝。独占环境仍由既有 SSA 布局门禁拒绝，持久槽、独占投影、入口 cleanup 消费和
原生写回仍属于 [ADR-0036](../../adr/proposed/0036-exclusive-closure-capture-storage.md)
的后续工作。

Apple Silicon macOS 以 `78a36f25` 工作树验证。前端库和 13 个定向集成目标共通过
1,268 项，零失败、零忽略，其中闭包 74 项。`cargo check -p lang-codegen` 通过。
CLI `check_cli` 6 项、前端全目标 Clippy（`-D warnings`）、格式、文档、尺寸及 diff 门禁通过。
单文件和项目入口均实测：独占闭包分支用例 `check` 退出 0，`build` 以 `UnsupportedNode`
退出 2 且未生成目标产物；既有只读闭包可构建并运行，stdout 精确为 `closure-read\n`。
独立复核检查了源身份、前缀求值、提前退出和嵌套循环的保活深度。远端双宿主、完整 conformance
与 sanitizer 留待 R4/R5 验收。

## 53. R4 嵌套闭包的 Owned 捕获借用归属

直接构造 typed SSA 复现：将捕获 Shared loan 的内层闭包移入 Owned slot 后，正常析构外层环境
未结束该 loan，后续源 owner 析构误报；先显式结束 loan 并析构源 owner，再调用外层环境则被放行。
修复把 Owned slot 和 CFG 参数重绑放在同一固定点中，捕获 loan 随其实际 owner 运输。

独立复审定位、随后实跑复现两条入边调换 capture loan 与独立 loan 的显式参数。合流并集让 `Drop` 一并
结束两条 loan。逐边校验显式目标 loan 的归属与来源一致后拒绝该 IR。普通隐式运输和一致重绑
继续支持；正控在外层析构后显式结束独立 loan。核验使用真实入边与源 owner 身份。

本节覆盖 `ClosureConstruct` 与 CFG。Nullable、root take/replace/swap 等其他局部 owner
运输和入口 Borrow loan 被直接捕获后重绑的合同由独立复审列入后续核对。源码 `move` 包装借用
闭包通过 `check`，仍在当前 native lowering 报 `UnsupportedNode`。独占捕获的原生 ABI
仍待 [ADR-0036](../../adr/proposed/0036-exclusive-closure-capture-storage.md) 接受。

Apple Silicon macOS，基于 `78a36f25` 的工作树。`lang-codegen --lib` 的闭包 54 项、SSA
verifier 103 项、LLVM 闭包 3 项及既有原生 move 闭包 1 项通过，分别过滤 2,618、2,569、
2,669、2,671 项。`lang-cli --test check_cli` 6 项通过；各目标零失败、零忽略。
codegen 全目标 Clippy（`-D warnings`）、格式、文档、尺寸和 diff 门禁通过。
独立复核检查固定点、隐式捕获参数、不同 root、回边及实际 `Drop` 的归属约束。
完整 codegen、双宿主、conformance 及 sanitizer 仍待 R4/R5 验收。

## 54. R4 局部取出与可空证明的捕获借用

- `RootPlaceTake` 旧 verifier 在正常析构结果后误报 loan 仍活跃，显式提前 `BorrowEnd` 则放行。
  现在结果在固定点继承实际槽内容的捕获。同一物理 `PlaceId` 经 raw `Mutate` 时，以最后写入
  值为准并校验借用仍活跃、未被第三个环境接管。独立槽的写入不混同。新旧捕获集合重叠时拒绝
  可能含糊的归属。
  扩大套件确认已有合法的写入后取出并同步生成路径。该路径继续接受，结果析构负责结束当前捕获，
  被覆盖捕获仍沿原 raw 显式清理。`RootReplace` 和 `RootSwap` 对 concrete closure 延续现有准入边界。
- `NullableTake` 旧 verifier 能消费活跃闭包已接管的 proof，也能在 Copyable 非空分支中
  结束仍有活跃 reborrow 的 proof。两条已实跑复现。现在 proof 消费复用 `BorrowEnd` 的持有者、
  活跃派生及生命周期检查，检查成功后才消费 owner。
- InlineNullable 的类型图补齐实际存储边。未知 `List<Closure>` 经包装后 Return 的旧放行
  已实跑复现，现按原有 owned-escape 规则处理。已知空 List 经相同包装仍可交付。

独立复核另外定位了 `CallableInvokeView` 缺少对当前槽内容捕获的检查。该路径和入口 Borrow
loan 直接捕获后的重绑继续留在 R4；本批不改变源码可用的 closure ABI。

Apple Silicon macOS，以 `78a36f25` 工作树验证。最终候选通过 `lang-codegen --lib` 的闭包
60 项、可空操作 12 项、借用生成 31 项、verifier 103 项和 LLVM 可空 6 项，以及
`lang-cli --test check_cli` 6 项。各组零失败、零忽略。第一次扩大验证暴露重复 `take` 的
`ValueUnavailable` 回归，以及写入后取出路径被过宽拒绝。最终候选分别恢复了诊断与同步生成正控。
全目标 codegen Clippy（`-D warnings`）、格式、文档、尺寸及 diff 门禁通过。最终二进制的
198 项 nullable 扩大选择器全部通过，零失败、零忽略，过滤 2,482 项。完整 codegen、双宿主、conformance 与 sanitizer
仍留在 R4/R5 验收。

## 55. R4 CallableView 当前捕获证明

旧 verifier 实跑放行了 raw 槽替换后结束当前捕获，再经 view 调用的 SSA。
`CallableViewOf`、`SharedReborrow` 与 CFG 参数重绑现在保留实际捕获；调用接收者和同步 Borrow
实参消费同一证明。矩阵覆盖本地、重借用、普通 helper、借用返回 helper、真实 captured-view
thunk、独立物理槽及 CFG。raw `Mutate` 对当前物理槽生效，保持独立 `RootPlace` 和原 SSA 快照。

模块一次建立可信 thunk 字段索引，交集只保留所有布局均为 Shared view 的字段，排除普通调用、
函数地址与其他静态调用入口。Owned 引用捕获和另加 `DirectCall` 均有负例。形成检查约束交给
thunk 的 view，外层 owned 调用检查形成后仍活跃的捕获。RangeCall 的 Borrow 参数和 Map action
消费当前内容证明；BorrowCall 与 RangeCall 的 Value 参数执行既有 owned-escape 检查。
类型分类与 alias roots 复用验证上下文，模块级验证移入 `verify/module.rs` 保持尺寸护栏。

R51 的共同失败也在当前前端 CLI 单文件实跑，退出 1，`current.value` 与 `return total` 各有
一条 L0165。该诊断级联回到 R2 修复；raw SharedReference 值逃逸与入口 Borrow loan 重绑另留 R4。

Apple Silicon macOS 最终候选定向验证共 235 项。闭包 61、借用生成 31、可空操作 12、原生
细化层闭包 3、SSA verifier 107（含 callable view 11）、unit 容器 15、CLI 6，各组零失败、
零忽略。codegen 全目标 Clippy（`-D warnings`）通过。完整 codegen、双宿主、conformance 和
sanitizer 仍待候选验收。

## 56. R2 可空成员的根因诊断与赋值恢复

R51 失败用例的旧 Unit 分析把已失去非空证明的 `current.value` 留为 `Deferred(MemberAccess)`。
赋值因此把 `total: Int` 标为待定，后继 `return total` 追加 L0165。定向实跑复现两条诊断。
已知接收者仍可空的普通成员读取现在以完整访问 Span 记录 L0165 和 Error，赋值在待定写回前退出。
重新判空的路径继续生成字段和赋值描述符。可选链与真正待定的接收者沿原恢复路径，回归覆盖
条件调用未决而两个分支都返回可空类型的 grouped `if`。

显式类型实参的调用在无源码候选时，曾把已知 callee Error 改成 `Deferred(Call)`。真实红例
锁定了 raw 类型。检查独立实参后传播原 Error，并验证没有调用描述符、实参自身 L0083 仍出现。
扩大验证确认既有无效赋值仍撤销 smart cast 的合同，实验性保留证明的改动已撤回。
下游同一源码负例改核 body Error、原 L0165 和完整 Span，并验证 final admission 保留诊断与身份。
缓存后的嵌套字段待定传播另留 R2；原生独占捕获 ABI 仍属 R4。

Apple Silicon macOS，以 `78a36f25` 工作树验证。前端库及六个集成目标 929 项，后端
while-flow 37 项、nullable field 拒绝 1 项和 callable view 11 项，CLI 6 项、LSP 56 项。
各组零失败、零忽略，后端选择器分别过滤 2,648、2,684、2,674 项。前端全目标 Clippy
（`-D warnings`）、格式、文档、尺寸及 diff 门禁通过。单文件与显式 source-root 实测非法
源码各退出 1、只报告一条 L0165，重新判空的正例均构建并运行，stdout 精确为 `retaken-proof\n`。
当前二进制的 nullable 扩大选择器 198 项全部通过，零失败、零忽略，过滤 2,487 项。
完整候选、双宿主和 sanitizer 仍待验收。

## 57. R2 缓存字段的待定事实与成员消费者

预检查会缓存嵌套字段类型。旧实现再次读取缓存时丢失接收者的待定状态，实跑出现过早的
L0165，也发布了依赖待定字段的 `!!` 描述符。普通字段现在保留已知类型和诊断所需的字段
身份，同时记录可供缓存消费者查询的待定事实。聚合投影、非空断言、`Rc.value`、
`Rc.share()` 和本批核对的调用路径按待定事实控制执行描述符的发布。字段、断言和 Rc
保留已知 raw 类型。源码调用、函数值字段调用和合成 component 沿已有 `Deferred(Call)`
恢复，独立实参仍检查。

独立复核覆盖失败候选回滚、字段赋值的上下文类型、实际字段与同名合成 component 的优先级、
仅通过控制表达式传播的待定事实和最终准入。负例核对最终为源码诊断，并精确核对 L0165
数量和外部调用根因 Span。复核另外定位了 `MutableList.add` 的专用调用入口仍可发布依赖
待定接收者的 append 描述符。该入口与同类容器操作继续留在 R2 核对。

Apple Silicon macOS，以 `78a36f25` 工作树验证。前端库及八个集成目标 994 项、后端
while-flow 37 项、unit 容器 15 项、原生细化层闭包 3 项、callable view 11 项、nullable
field 拒绝 1 项、CLI 6 项和 LSP 56 项通过。各组零失败、零忽略，后端使用定向选择器。
收紧最终准入断言后重跑 nullable-flow 17 项，全部通过。前端全目标 Clippy
（`-D warnings`）、格式、文档、尺寸和 diff 门禁通过。

补充原生探针中，`holder.child.value` 在单文件与显式 source-root 均通过检查、构建与运行，
stdout 为 `cached-field-ready\n`。同一 `Leaf` 的直接方法调用在单文件构建并运行；
`holder.child.read(1)` 和零实参变体可通过前端，原生 lowering 返回 `InvalidModel`。
两份先前的本地二进制也复现单文件失败，其中一份与当前二进制均在显式 source-root
复现相同的 `check` 与 `build` 分歧。嵌套字段方法调用的前后端交接留作 R4 问题。
完整候选、双宿主、conformance 与 sanitizer 仍待验收。

## 58. R2 MutableList 修改调用的待定实参

旧 Unit 入口实跑为待定字段上的 `add(null)` 和确定接收者上的 `add(outside())` 发布了
append 描述符。六个修改方法现在都在发布前检查本次检查新增的待定事实和已缓存的操作数。
保留 callee 签名、结果类型、求值终止性和原有实参检查，待定结果继续供后续消费者识别。
回归覆盖已缓存字段、条件接收者、待定索引或元素、合法的 `null` 上下文、独立的 L0084
及最终准入的 L0165 根因位置。

独立复核确认六个 producer 与候选回滚边界，并复现 `Carrier<Unbound>` 可能匹配
`Carrier<Int>` 时，`add` 和 `insertAt` 各追加一条假 L0084。两处现在复用已有部分类型
比较，具体不匹配仍报告。最终准入仍报告未绑定类型的声明引用和直接值使用，本批去除假类型
不匹配和执行描述符。

独立复核另定位到局部解构吞掉 initializer 的待定状态。实跑验证：待定 `removeFirst()`
没有描述符，但其结果解构出的 `index` 仍让 `items.removeAt(index)` 发布描述符。最终准入
只报 `outside` 的 L0165。解构传播留作下一批 R2，Map 专用方法及 R57 原生字段调用仍待复审。

Apple Silicon macOS，以 `78a36f25` 工作树验证。前端库及八个集成目标 997 项、后端
Unit 容器 15 项、细化层容器 10 项、CLI 6 项、LSP 56 项。各组零失败、零忽略，两个后端
选择器分别过滤 2,670、2,675 项。前端全目标 Clippy（`-D warnings`）、格式、文档、尺寸
及 diff 门禁通过。六种操作的组合正例经单文件和显式 source-root 检查、构建、运行，
分别核对结果值、清空后大小和精确 stdout `mutable-list-ready\n`。完整候选、双宿主、
conformance 与 sanitizer 仍待验收。

## 59. R2 局部解构的原子待定传播

旧 Unit 路径实跑发布了待定 `removeFirst()` 结果的局部解构描述符，解构后的
`index` 又触发 `removeAt(index)` 描述符。局部解构现在检查初始化值的新增及已记录
待定事实。已知分量保留其类型，用现有不可变绑定机制传播待定状态，整次解构暂缓发布。
可判定的 L0118 仍指向原 pattern。回归覆盖列表结果、条件结果经普通 `val` 传递、不同
分量的后继消费者、合法 Copy 正控以及原源码诊断。

独立复核发现名义外壳可已知，而未绑定字段可被同胞 MoveOnly 字段掩盖。直接字段和另一
内联 value class 的字段都已实跑复现。最终方案复用 Copyability 的内联字段遍历，
另收集未决事实，不改变能力分类或现有失败证明；规范类型树复用带访问记录的检查。
回归覆盖直接字段、嵌套内联字段及泛型分量，既不发布解构和后继列表描述符，又保留
已知同胞的 `Int` 与 `Unbound` 根因。直接字段探针仍报告 `extra` 和 `index` 两个
后续 L0165；局部解构诊断去重另留 R2。

额外探针定位两项 Guide 差异。相同的 `Pair(41, 1)` 用 `(second, first)` 会按位置
得到交换后的值，非字段名 `(left, right)` 也被接受。Guide 要求按字段名绑定，两项分别是
合法源码错误执行和非法源码被接受，留 R2 阻断项。真实 `=` 后的无括号 `if` 初始化式
报 L0046，括号形式通过 `check`，留 R1 复审。本批范围为待定事实和分量恢复类型。

Apple Silicon macOS，以 `78a36f25` 工作树验证。前端库及十二个集成目标 1,061 项，
包含 Guide Litmus。后端解构 9 项、Unit 容器 15 项、CLI 6 项、LSP 56 项，各组零失败、
零忽略。后端选择器过滤 2,676 和 2,670 项。前端全目标 Clippy（`-D warnings`）、格式、
文档、尺寸和 diff 门禁通过。合法解构正例从单文件和显式 source-root 构建运行，验证
初始化只求值一次与两个分量值。完整候选、双宿主、conformance 与 sanitizer 仍待验收。

## 60. R1 局部解构初始化式的表达式起点与分号 owner

旧 Parser 在真实 `=` 后先匹配 block 的 soft stop，实跑把无括号 `if` 拆为下一 element 并报
L0046。现在 control 和 lambda primary 进入同一个 Pratt 入口，完成表达式后仍使用原边界。
回归检查 block 与 lambda body 中的 `if`、`when`、lambda 三种 initializer AST、真实等号
与源码 Span，并核对后继声明保留。

旧缺项及 malformed initializer 恢复会吞外层分号及后继调用，实跑覆盖直接缺项及含内层
分号的非法首 token。解构初始化式现在显式接收外层分号 stop，分别用于 Pratt、顶层
类型操作数及 declaration recovery。分组、调用、索引和内层类型实参维持各自的 owner。
回归覆盖直接分号、首 token、尾部、缺 operand、`as` / `is` 缺类型、调用和索引内的错误
分号。独立复核发现并复核修正了嵌套 owner 一度继承外层分号的问题，检查了原 hard closer、
词法 poison、`when` 分支与缺 `=` 行为。

追加 R2 对照使用异型分量 `Pair(first: Int, second: String)`。逆序书写合法字段名时当前
`return first` 报 L0084，未知字段名仍被接受，重复字段名已在名称解析报 L0079 并标注
首次声明。下一批字段绑定需同时核对 Unit 与仍有消费者的旧检查器，描述符字段身份须可由
所有权规划、细化层和旧 lowering 直接消费。

Apple Silicon macOS，以 `78a36f25` 工作树验证。最终前端全目标共 182 个测试目标，
2,681 项通过，零失败、零忽略。后端解构选择器 9 项通过并过滤 2,676 项，
CLI 6 项、LSP 56 项均通过，各组零失败、零忽略。前端全目标 Clippy（`-D warnings`）、
格式、文档、尺寸和 diff 门禁通过。单文件与显式 source-root 的原生正例覆盖无括号
`if` 与 `when` 的两支，核对四次初始化调用的顺序与次数、两个分量值和精确 stdout。
完整候选的双宿主、conformance 和 sanitizer 仍待验收。

## 61. R2 局部 value class 解构按字段名匹配

旧 Unit 和旧单文件检查器的同一回归均实跑在逆序异型字段上产生 L0084，并接受未知
绑定名。两路现在复用字段名匹配。未知字段报 L0080，primary 在绑定名且 label 指向
value class 声明。数量不符继续报 L0118，恢复只给已匹配绑定原字段类型。重复名字
由名称解析报 L0079，完整且每字段唯一时才发布执行描述符。R59 的待定事实仍阻止发布。

描述符把分量排列成主构造器字段顺序，Refine 和旧 lowering 继续各求值一次 initializer，
直接使用相同顺序的 SSA 结果与绑定对应。两个清理 planner 按 pattern 的绑定位置登记及
安排同点清理，资源退出仍按词法声明逆序。逆序资源原生回归要求准确输出
`42\ndestructured\nleft\nright\n`，并对两个资源执行分配计数。同步迁移标准库 bootstrap
及依赖位置别名的类型、ownership、CLI 和后端 fixtures。独立复核发现并复核修正了一个
遗漏的构造中途退出 fixture，清理 oracle 保持其原观察范围。

本批实跑确认私有字段的既有 R2 缺口。classifier 外对 `Vault(private val hidden: Int)`
做 `val (hidden) = v` 时 Unit CLI 通过，而 `v.hidden` 报 L0080 并标注字段声明。
跨文件项目也错误接受外部解构，classifier 内的合法解构通过。旧单文件探针则无诊断地
发布外部直接读取和外部解构描述符。Guide 02、08、11 没有给局部解构可见性例外。
两检查器的局部解构及旧检查器的直接字段访问留作后续可见性阻断项。后续还需核对同
Unit 的 internal 字段。普通 class component 和 `for` 借用解构各有独立合同。

Apple Silicon macOS。当前工作树以 `78a36f25` 验证，检查时本地 `origin/main` 为
`333ba5ec`，领先 10 个提交，其中有累计工作树亦修改的文件，主干集成证据仍待核对。
最终前端全目标 182 个测试目标，2,683 项通过，零失败、零忽略。后端解构 9、
owned-results 5、independent-results 2、构造中途
退出 1、Refine 准入 22 项通过，各选择器保留各自过滤范围。CLI 检查 6、具体泛型
原生集成 2 和 bootstrap 4 项通过。两种 CLI 源码入口均验证逆序通过、重复拒绝、未知
拒绝，并保持原生 stdout。LSP 全目标 56 项通过。前端全目标 Clippy（`-D warnings`）、
格式、文档、尺寸和 diff 门禁通过。完整候选的双宿主、conformance 与 sanitizer 仍待验收。

## 62. R2 私有构造器字段与模式准入

两套检查器在局部解构、具名 `for` 借用分量和合成 component 路径复用字段可见性判定。
旧检查器还以真实 classifier 源码范围保护直接投影及可选链，避免以 extension receiver
冒充词法归属。Unit 仍使用已有声明身份。classifier 内合法解构和直接读取照常产生事实，
同 Unit 跨 package 的 `internal` 分量继续通过。外部访问报告 L0080，标注私有字段声明。
具名借用分量报告 L0160，拒绝整个迭代计划，`_` 保留为不建立字段借用的 discard。
同名私有字段已产生 Error 时，调用不再回退到公开的合成分量。

独立复核发现三条准入失败并实跑复现。完整局部 pattern 中公开绑定排在私有之前时、
缺局部右括号时，以及缺内层右括号的借用 pattern，原始源码诊断可被
`MissingRequiredFact` 掩盖。共享最终准入先检查 initializer / source，再按已诊断的模式
恢复整组绑定及后继读取。缺局部右括号时使用真实 `=` 的起点，借用 pattern 复用类型
检查的精确左括号回退。两套公开入口均有回归。最终边界经独立复核，绑定检查移到同
一 walker 的 `bindings.rs` 子模块，保持尺寸护栏。

后续仍有三项实际边界。公开字段的借用 pattern 缺内层右括号时，CLI 仍错误接受，留
R1。Unit 的待定 `List<Vault>` 字段仍可能发布迭代计划，独立探针确认最终只有
`outside` 的 L0165，留 R2。Guide 12 禁止 value class 自动提供 `componentN()`，当前
两套实现及其既有正例仍含公开合成路径，私有访问修复未调整这项规范差异。

Apple Silicon macOS，以 `78a36f25` 工作树验证。本地 `origin/main` 检查时已到
`86430a05`，领先 19 个提交。同文件与跨文件项目的外部私有解构
均以一条 L0080 退出 1。合法原生正例在单文件和项目入口成功检查、构建、运行，验证
私有构造参数名、classifier 内逆序解构以及外部 `_`，stdout 精确为
`private-field-boundary-ready\n`。最终前端全目标 182 个测试目标，2,689 项通过，
零失败、零忽略。后端解构 9 项、discard 3 项、CLI 8 项、LSP 56 项及前端全目标 Clippy
通过。格式、文档、尺寸和 diff 门禁通过。完整候选的主干集成、双宿主、conformance 和
sanitizer 仍待验收。

## 63. R1/R2 `for` 解构闭合与待定迭代源

缺解构内层 `)` 的公开字段循环从原先通过修为在 `in` 起点报告 L0010，label 指向真实的
内层 `(`。恢复保留 `in`、迭代源、循环体和后继语句。逗号后遇到 `}`、`;` 或继承的 `]`
时把 token 留给外层。独立探针发现外层 token 虽未消费却被误计入 For 和两个 Error
节点的范围，新增红测锁定 `}` 和 `;` 起点处的零宽范围。For 止于实际消费的逗号末尾，
中间 trivia 不扩大父节点。独立复跑 Parser 9 项与嵌套、Lexer 错误等 5 个补充探针均通过。

Unit 从直接未解决的条件和 `val` 别名识别待定迭代源，统一约束 List、IntRange、View
和 Map 的计划发布。循环绑定保留已知类型，依赖绑定的容器操作随之待定。所有权诊断
恢复读取源并检查循环体，不使用尚未生成的计划。分析只剩待定事实时，typed 和 owned
两层均拒绝最终准入。独立探针复现中间版本的错误准入失败，复核修复后四种 provider
均能完成恢复，最终保留 `outside` 的 L0165 以及独立的循环体诊断。

旧单文件 `check_types` 在 name-only 外部调用形成条件迭代源时，四种同形探针仍各发布
一个计划，列为后续 R2。现行 CLI 和 LSP 调用 Unit 路径。公开合成 `componentN()` 的
Guide 差异仍开放。`78a36f25` 工作树
落后远端 `main` `86430a05` 19 个提交。在隔离副本解决 `checker.rs` 和
`checker/type_graph.rs` 两处上下文冲突，合并候选类型回放与主干 nullable-bottom 兼容规则。
副本的 frontend lib 358、多文件类型 279、Parser 9、调用选择 22、CLI 8、LSP 56 和 Map
后端 5 项通过，均无失败或忽略。Apple Silicon macOS 单文件和项目入口均验证缺括号仅报
L0010 并退出 1，合法 List 解构和 Range 迭代均能构建、运行，stdout 精确为
`for-header-ready\n`。工作分支保持原基线，主干整合未提交。

工作树以 Rust 1.96.0 和 LLVM 21.1.8 验证。最终前端全目标 182 个测试目标，2,693 项通过；
前端全目标 Clippy（`-D warnings`）通过。顺序迭代后端 76 项通过，包含原生资源矩阵，
该后端批次先于末轮 Parser Span 调整；调整后另以当前代码通过前端全量和主干副本原生探针。
工作树 Map 后端 5 项通过。格式、文档、尺寸和 diff 门禁通过。完整双宿主 CI、conformance
和 sanitizer 未运行。

## 64. R2 退役自动 componentN

Guide 12 的 value class 局部解构按真实字段名匹配，`for` 借用解构按位使用布局字段，两者
均不调用自动方法。红测中旧单文件和 Unit 各为未声明的 `component1()` 发布一个 call 和
一个 projection。R63 的两个公开 CLI `check` 入口均错误退出 0。

撤除两个合成回退，移除 `CallableTarget`、`UnitCallTarget`、`AggregateProjectionKind`、
`UnitAggregateProjectionKind` 的 `StructuralComponent` 变体及所有权、SSA、LSP 专用分支。
这是公开 Rust 产物的枚举变化。Copyable 和 MoveOnly 泛型实例均走既有未解析成员诊断
L0165，未声明调用没有调用目标、字段投影、字段标签或定义跳转。显式普通类和 value class
方法按通常规则发布 Source/Symbol，待定 receiver 不发布调用。同名私有字段仍按真实声明
报告 L0080，非 callable 同名字段仍报告 L0119。普通类局部解构保留既有 deferred 合同。
真实字段的部分移动 fixture 仍报告 L0132，Unit 复核字段和被选中参数的两个标签。

独立复审核对两套 producer、所有实际消费者、诊断范围及字段和方法定义目标。定向测试
前端 290、CLI 8、LSP 57 和 codegen 解构 8 项通过。隔离整合远端 `main` `86430a05` 的
副本通过 312 项前端测试及全 workspace `cargo check`，保留 nullable-bottom 兼容回归。Apple Silicon macOS 单文件和
项目入口均验证未声明方法只报 L0165 并退出 1。显式 value class 和普通类方法、逆序局部
解构、具名 List 的 `for` 借用解构均成功构建与运行，stdout 为 `component-contract-ready\n`。
工作树前端全目标 182 个测试目标，2,690 项通过，零失败或忽略。全 workspace 全目标
Clippy（`-D warnings`）、格式、文档、Rust 尺寸和 diff 门禁通过。完整双宿主 CI、conformance
和 sanitizer 未运行。

新增 R4 复现。函数先执行 `val (second, first) = Pair(20, 22)`，再直接迭代临时
`listOf(Pair(20, 22))` 并以 `for ((value, _) in ...)` 读取分量，`check` 通过，`build`
在 R63 和 R64 的单文件与项目入口均报 lowering `UnsupportedNode`。拆开可运行，改用具名
List 也可运行。
在同一函数加入可独立触发 Refine 的 `MutableList.add` 后，原临时组合成功运行。
Refine 的临时顺序源准入要求 `scope_requires_refine()`，局部解构只在遍历时设置
`migrated`。隔离副本只绕过临时源的这一项限制，两种入口随即构建并精确输出 `local\n`。
已恢复实验源码，工作分支未改 R4 准入。正式修复须核验其他临时 provider、退出清理与
保留的旧路由，再验证同一函数的结果不依赖无关操作。

## 65. R4 局部解构与临时顺序源的准入一致性

两条红测分别复现 Refine 默认路由缺失和资源迭代的原生 lowering 失败，均在局部解构处
报 `UnsupportedNode`。旧路径已退役局部解构，本轮只补 Refine 的组合准入。临时顺序源
仍完成 provider、source、binding、body 和退出动作的全部原校验。整个 callable 预检
结束后才判断临时源的额外路由条件，由既有范围触发器或同一 callable 中通过检查的
局部解构满足。这样先写循环或先写解构结果一致，不依赖遍历中的 `migrated` 标志，
也不会放开其他 provider 或其他路由 gate。

路由回归覆盖 Array、List、MutableList 各两种语句顺序，均通过 Refine 构建和 SSA verifier。
纯临时循环、只在 sibling 出现局部解构、已有 field-replacement 路由分别保持。独立复审
核对绑定范围、thunk 隔离、非可达语句及未通过校验的 provider，指出 break 正控原先无法
区别提前退出和最后一项耗尽。加入第三个动态 String 后，只有正常耗尽路径访问第三项。
原生 LLVM 插桩测试三个调用合计十二次分配，逐指针确认各释放一次，覆盖 continue、耗尽、
break 和 return，并核对 source 单次求值及精确 stdout。

以远端 `main` `86430a05` 的隔离整合副本验证路由 2 项和上述原生分配测试 1 项。工作树和
副本各在 macOS 实跑单文件/项目、无/有 `MutableList.add` 四格，均 check/build/run，
stdout 精确为 `local\n`。独立 `while` 范围限制仍保留。原复现仅增加 `while (false) {}`
时，本机 `check` 通过但单文件 `build` 仍报 `UnsupportedNode`，作为下一条 R4 路由合同。
隔离实验将 `while` 检查也延后时，空循环组合成功运行，但涉及 nullable 外层视图重赋值的
组合仍报 `UnsupportedNode`。实验源码已恢复，正式修复仍需核对流事实与旧路由。
旧单文件 `check_types` 的四种直接待定迭代源均仍发布类型计划。List、Map、IntRange
经 `val` 别名也发布；View 的同形 `val` 另受 L0163 限制。IntRange 的直接和别名探针
还能发布所有权迭代计划，最终准入均保留 `outside` 的 L0165，继续列为 R2。

R0 以同一直接调用匹配法重清点工作树，含未跟踪源码。`check_types` 在 frontend 为
87 文件／145 行，在 codegen 为 15／18；`check_ownership` 分别为 47／58 与 14／17。
CLI/LSP 无直接调用。`lower_scalar_file_with_entry` 为 4 文件／8 行，编译入口由
`cfg(test)` 隔离。R65 保留的回退是正式 Unit 流水线内的 `unit_lower`，与独立 Single
测试入口分开处置。上述计数包含测试和字符串，别名与宏仍需逐消费者核验。

工作树 CLI 定向 8 项、Refine 路由组 24 项、workspace 全目标 Clippy（`-D warnings`）、
格式、文档、尺寸和 diff 门禁通过。后端全库在运行，尚未结案，当前有一项既有 LLDB
用例因 debugserver 权限被忽略；三分量加强测试已在同代码的主干整合副本另验通过。

本机完整 conformance 使用 R65 工作树编译器（SHA-256
`1a971b84a55dbbf6ad49ee7f50bd6d45b2ad624ced959cbe23a91eb8c106fb74`），两种公开入口
各跑 154 probes 和 6 个验收程序。能力通过 137/154、验收程序 0/6（六项均为已登记未实现），
入口差异、错误输出、内部错误均为零；门禁因 `p42_lambda_mutate` 失败。历史预期是 L0135，
实测两入口均通过 `check`、原生报 `BE:UnsupportedNode`；R63 主干整合副本同样复现。
Guide 07 已启用同步非逃逸 `var` 可变捕获，此源码没有冲突访问。需独立对齐该 oracle 与
真正 loan 冲突的负例。以先形成捕获 `sum` 的回调、再读取同一 `sum` 作为后续实参的负控，
两个入口均精确报告一条 L0135，primary 指向后续读取，label 指向冲突 loan 的起点。
独占捕获 native ABI 仍未交付，保留 [ADR-0036](../../adr/proposed/0036-exclusive-closure-capture-storage.md)
的 proposed 状态。未改 oracle，未执行双宿主 CI 或 Linux sanitizer。

## 66. R5 可变捕获 conformance 与历史基线对齐

`p42_lambda_mutate` 源码及历史 TSV 逐字保留。Guide 07 的 D7 允许该同步回调修改局部
`var`，因此当前 oracle 按 S1 记录 native ABI 缺口，只接受 `BE:UnsupportedNode`。
两入口拒绝不会计入能力通过。旧 `FE:L0135`、其他后端缺口及错误 stdout 仍阻断门禁，
正确原生输出会标为 `unexpected_pass`，需要复核后晋级。原 probe 只打印常量，S1 的原生
验收须另核调用后 `sum` 的实际值。ADR-0036 和 S1 状态本轮未更改。

`p42` 从 P2 应拒的 B 组移到 C 组，A 组基数仍为 74，B 组七项仍须全部精确拒绝。
生成审计将 Guide 07 的公开负向语料缺口和 Litmus 10 部分覆盖显式登记。独立复审核对
分类、分组与可执行证据。CLI 回归用同一回调的后续 `1` / `sum` 实参形成正负对照，两入口
分别要求无诊断和唯一 L0135。以 `86430a05` 整合副本执行 `check_cli` 6 项均通过。

旧 oracle 的定向测试先失败。新 oracle 自测、审计 2 项、源码及历史 TSV 保全检查通过。
重新读取 R65 的实测 TSV，154 项没有回归或入口差异，能力仍为 137 项。定向 `p42` 本机
原始证据通过校验，能力为 0/1；这不等于独占捕获的 native 实现已经交付。

以旧规则核验 R65 原账本与全部 320 份入口 stdout/stderr/status，再以相同编译器哈希和
源码哈希对原字节重放当前 oracle。比较器门禁通过，能力仍为 137/154，23 项已知未实现
包含六个验收程序。P2 A 组仍为 58/74，未达 70；B 组为 7/7。

随后以同一 SHA-256 编译器完成 macOS 全量新执行及 `--verify-evidence`，两入口各跑 154
probe 和六个验收程序。门禁通过，能力 137/154、验收程序 0/6。没有入口差异、超时、
错误输出或内部错误。Python 全套 17 项中 15 项通过；shell 矩阵达到 10 秒上限，部分输出
用例的模拟编译器在超时前未写出 stdout。两项均在未改 oracle 的主干整合副本复现，工作树
的沙箱外复跑也未过，未修改超时值。双宿主、Linux sanitizer 与发布验收仍开放。

## 67. R22–R66 切片合流

用户要求在已验收切片收口，合入最新 main 并发起 PR。切片提交 `5ec4ed94` 合入从
GitHub 核实的 `86430a05`，两处冲突分别保留 `FlowVersions` 与 `unit_assignable` 的
接线，以及候选类型的暂存/回放与主干 nullable-bottom 相容规则。独立复审逐项核对了
原始两侧与解决结果。与合流工作树的 `crates/` 逐文件相同的隔离副本通过前端定向
375 项（多文件类型 279、调用选择 22、直接调用 loan 14、借用续接 60），零失败、零忽略。
本批继续保留旧单文件待定迭代与 while 交叉边界，完整候选的双宿主结果另记。

草稿 [PR #351](https://github.com/Halckon/Koven/pull/351) 首轮 Rust 行数门禁发现合流后的
`refine_subset_tests.rs` 为 1008 行。将两条临时顺序源路由回归按职责移到子模块，父文件
974 行，以 `origin/main` 为 base 的本机行数门禁及两条原回归通过。

首轮 Linux Targeted Tests 的单文件与项目 `build` 仍沿用 SPEC-0325 的历史 p42 拒绝
预期。两入口实际到达 S1 的 `UnsupportedNode`。将 R66 的真实独占借用冲突负控提取为
共用 fixture，两个 `build` 入口继续核验 human/JSON 的 L0135、主范围、退出码和产物
原子性。历史 p42 源码与 conformance 分类保持原样。以相同 `crates/` 隔离副本执行
`check_cli` 6 项和两个 `build` 回归，均通过。独立复审核对字节范围及入口断言。

[PR #351 的 `cedb448f` 门禁](https://github.com/Halckon/Koven/actions/runs/42428262895)
已通过 macOS 与 Linux 的 workspace check、Clippy、集成、conformance 和生成资源用例，
Linux native sanitizer 及两宿主预览包独立安装也通过。两宿主 conformance 均为 137/154，
验收程序 0/6，零回归、入口差异及内部错误。编辑器门禁按范围跳过。用户完成调试器授权后，
本机 CLI 三条断点测试和 CI 默认忽略的一条后端 LLDB 测试均通过。该证据只覆盖此 PR 候选。

## 68. R2/R3 旧 Single 待定迭代来源

在 `cedb448f` 上用公开的 `check_types`、`check_ownership` 与 `complete_file_types`
复现。直接来源的 List、Map、IntRange、View，以及经 `val` 的 List、Map、IntRange
共七格提前发布类型计划。IntRange 两格还发布所有权计划。最终准入均保留
`outside` 的 L0165。每格的稳定对照在同一 callable 内。

Single 记录恢复类型背后的待定来源，表达式缓存与 callable trial 一起回滚。待定来源仍保留
恢复类型，局部绑定、局部解构和循环绑定传递待定性。独立复审指出外层循环的恢复绑定可让
内层循环提前发布，补顺序、解构和 Map 三种回归。类型检查只发布稳定 provider；所有权在
有恢复标记且没有 descriptor 时诊断遍历，Map 最终准入仍走原合同。CLI/LSP 的 Unit 路径未改。
独立复审进一步复现 drop planner 在没有计划时仍发布 `EndElement`、`FinishProvider`
和 `EndSource`。待定循环改用无 provider 的可零次遍历，保留真实外层的退出深度。
前向常量的稳定对照也曾被旧标记误抑制，修复后仍按已有常量资格重查。

实际探针确认两条可变边界：待定初始化后写入稳定值，raw 计划仍为 0。循环先使用稳定值、
后写入待定值时，回边未重算，raw 计划仍为 1。两条最终准入均只报告 L0165。分支合流、
稳定覆盖和回边的精确语义仍需对账。R4 的 while 路由边界仍开放。

本机顺序执行前端定向七套件，共 564 项通过。回归覆盖嵌套待定循环的 break 和 continue
不结束真实外层 provider，return 则只清理真实外层。下游 Range SSA 定向三项通过。
本批尚未运行完整原生、conformance 或双宿主 CI。

## 69. R2 Single 可变绑定到达写入和回边

基于 §68 的本地工作树，独立复现稳定覆盖、if 正常出口及 fallthrough/continue 回边的
错误计划。对照按源码顺序核实际发布的 For 身份，最终准入仍逐处报告 `outside` 的 L0165。

非捕获局部 `var` 的稳定赋值清除旧待定性。待定性按 if、when、短路与 Elvis 的可达正常出口
合流，诊断仍使用原有类型和普通 `falls_through`。循环分别记录 break、continue 和正常回边。
只有发现可增长的待定回边才启用既有 trial 快照。每次重算只增加待定的循环入口绑定，
候选诊断、类型、计划及恢复缓存一并回滚。普通稳定路径不做整份快照。

独立复审的附加反例包括已知 break 后的死写、无出口的内层 loop、待定 when 条件、穷尽后
不再求值的 when 候选及不可写绑定。根赋值 RHS 的非正常出口也保留，后续死写不能进入回边。
覆盖包括两层绑定的传递收敛、相邻稳定 provider 和真实 break 不回到循环头。

本轮保留复合布尔真/假边相关性、捕获或 inout 跨调用写回及其他复合表达式的非正常出口传播
为独立审查边界。多个含待定回边的循环仍可能多次复制整份试探状态，未量化恢复路径成本。
两条未关闭的本机探针均只报 L0165。短路条件只在 true 边覆写稳定值时，raw 计划为 0，
该 true 分支预期 1。将提前 break 的表达式放到普通调用实参时，raw 计划仍为 0，
前后的两个稳定循环预期 2。

本机顺序门禁通过：前端 lib 加八个集成套件共 930 项，含 21 格到达写入矩阵。Range SSA
定向 3 项通过。前端全目标 Clippy（`-D warnings`）、格式、文档、尺寸及 diff 门禁通过。
本批未运行全量原生、conformance 或双宿主 CI。

## 70. R2 Single 布尔分支与普通调用实参

从 §69 的两个实测见证扩为六格红测。短路条件分别保留真、假出口的待定绑定，`if`、
`while` 和无主体 `when` 守卫按实际出口进入分支。Boolean 主体的 `when` 在连续
字面量匹配中保留该关联，并复用覆盖事实识别括号字面量。可能执行写入的其他候选保守合流。
`if`、`when` 值与必求值的 Elvis 右侧可以把布尔关联传给外层条件。

普通调用从 callee 到实参按源码求值顺序传播非正常出口。提前退出后的实参仍检查诊断，
其写入和跳转不进入调用后的流状态。命名参数映射不改变求值次序。泛型推导及重载预检
保存延后检查的 lambda 在源码位置上的绑定，失败候选恢复该状态。原返回类型、诊断和
普通 `falls_through` 合同保留。

独立复审复现了 Boolean 主体匹配、`in` 操作数提前退出及延后 lambda 入口三个缺口，
修复后回归覆盖它们及括号字面量。仍需复审非短路运算、专用内建或构造、可空调用实参
路径及跨调用捕获／inout 写回。R4 的 while 路由合同仍开放。
本机探针确认，`take((if (true) { break } else 0) + 1)` 及
`listOf(if (true) { break } else 0)` 的后续死写仍可能进入回边。两例 raw 计划均为 0，
预期前后各保留一个稳定计划，最终准入仍只报告 L0165。

本机最终顺序门禁通过：前端 lib 加八个集成套件共 930 项，含 34 格迭代计划矩阵；
Range SSA 定向 3 项。前端全目标 Clippy、格式、文档、尺寸和 diff 门禁通过。
本批未提交或推送，未运行全量原生、conformance 或双宿主 CI。

## 71. R2 Single 复合操作数和专用调用的正常出口

§70 留下的算术与 `listOf` 见证加入红测，扩为七格，原工作树全部漏发两个稳定计划。
共享操作数检查沿源码求值顺序传递恢复可达性。提前退出后的操作数仍接受诊断检查，
写入与跳转不进入实际流。透明一元表达式将退出存到自身 ID，后续查询不再重复遍历包装链。
普通类型、`falls_through`、null 上下文推导和调用映射保持原义。

专用顺序容器构造、名义构造及已识别专用调用接入同一检查。独立复审复现
`MutableList.insertAt` 与 `MutableMap.put` 的后续实参假回边，另核 RangeView、
`replace`、`swap` 以及 receiver 后的单实参方法。下标读取与写入检查 receiver、
index/key、RHS 的正常出口。Map 的 put 描述符仍以原 Index target ID 发布和消费。
通用赋值 fallback 保留既有路径，避免把已验证的下标求值顺序套到其他存储形式。

回归按 For 身份核对清理前后两个稳定计划。可达的第二操作数写入仍经真实 break
影响循环后继。最终准入逐处保留原 L0165。非法实参恢复、可空调用和跨调用捕获／inout
仍需独立复审。R4 静态核对确认 `sequential_for` 仍在节点处检查 while 组合，
早于 callable 级局部解构路由，准入和真实 lowering 的组合验收继续开放。
复跑 §65 留存的 CLI 二进制，空 while 的原组合 `check` 通过但 `build` 退出 2；
去掉 while 可构建并输出 `local`。加入同 callable 的 `MutableList.add` 后，空 while
与 nullable 外层变量重赋值两例都可构建运行，后者输出 `loop\nchanged\n`。
当前 `strip_reassigned_loop_views` 也受 `scope_requires_refine()` 限制。
后续修复需要分别证明路由准入和重赋值后的 view 失效。此处原生复跑使用旧二进制。

恢复通用赋值 fallback 前，前端全目标 182 个目标共 2,700 项通过。最终源码复跑
前端 lib 加十个集成套件共 798 项（含 50 格迭代计划矩阵）、全目标 Clippy 与
Range SSA 定向三项，均通过。格式、文档、Rust 尺寸和 diff 门禁通过。

## 72. 第 68–71 批 PR 合流

用户要求推送、跟进 CI 并合并。提交 `f7d2a1ad` 合入最新主干 `60aa67d1`，
无冲突。复核主干带入的 Map 和容器 ownership 消费者迁移以及相应记录。
合流工作树运行 `final_type_admission`、`ownership_containers`、`ownership_map_with`
共 112 项，零失败或忽略。双宿主验收以 [PR #351](https://github.com/Halckon/Koven/pull/351)
的精确新头为准。

[最终 CI](https://github.com/Halckon/Koven/actions/runs/42435006300) 的候选为 `d42768b2`，
两宿主 conformance 均为 137/154，验收程序 0/6，零回归、入口差异或内部错误。
workspace、Clippy、定向测试、Linux sanitizer 和两宿主预览包独立安装均通过；
编辑器按改动范围跳过，macOS 后端一个 LLDB 进程测试按权限默认忽略。
PR 已合入主干 `d8c7393a`。用户完成本机授权后，在该主干重跑
`native_cli final_debug_artifacts`（3 条真实进程断点、1 条 frame 身份负控）及
`llvm::debug_tests::lldb_hits_a_koven_source_breakpoint_and_reports_the_frame --exact --ignored`，
共 5 项通过，零失败或忽略。

## 73. R4 顺序迭代、局部解构与 while 的组合

在 `d8c7393a` 上加入原生 CLI 回归。单文件 `build` 退出 2 并报告
`UnsupportedNode`，与 §71 的 `check` 成功对照一致。单独存在 `while` 时，
顺序迭代节点在 callable 完成局部解构检查前被原路由拒绝。外层有可空证明时，
准入对 `while` 重赋值根的证明结束也受旧路由限制，实际 builder 已在 loop header 前执行它。

保留已有接受结果。只在碰到上述限制且本 callable 范围内有 typed 解构候选时重试。
范围匹配用于预筛。重试同时要求在当前 callable 遍历到已验证且前端未证明不可达的局部解构，
并验证顺序 provider/source/binding 和清理事实。只对 `while` 和已确认的顺序 `for`
接入既有证明失效，其他 provider 和专用操作继续使用原路由条件。

独立复审复现常量 while 的不可达解构、无 break 的 loop 后解构及顺序 `for` 重赋值。
解构事实沿用前端 flow 进入/离开边，lambda body 有独立入口，现有试算同时回滚该描述符。
解构 initializer、`for` source 和 eager 二元表达式的提前退出按同一流检查传播。
后端保守生成常量 while 的两条边，旧 lowering 遇到两条负控中的死解构仍报
`UnsupportedNode`。负控锁定原路由及错误范围，常量边裁剪与其他表达式的可达性合同留待后续。
旧 scope 门禁仍会按 span 计入内层 thunk 的 while，未扩大该判断的含义。

修复后前端 lib、`multifile_type_checking`、`multifile_ownership_checking`、
`final_type_admission` 共 854 项通过。Refine 路由 33 项、while SSA 37 项、while CLI 7 项
均通过，零失败或忽略。CLI 回归真实构建并运行单文件与项目产物，覆盖零轮、多轮、
continue、break、保留无关根的证明，以及在 while 和顺序 for 中分别重赋值。
独立复核确认原候选红测及上述前端修正，验证保留 null 上下文类型推导、循环作用域和诊断检查。
workspace 全目标 Clippy、格式、文档、Rust 尺寸与 diff 门禁通过。

[PR #353 的首个候选](https://github.com/Halckon/Koven/actions/runs/42438293604) `a82a0bb7`
两宿主定向、生成资源与 conformance 均通过，Linux sanitizer 通过。
两宿主 conformance 为 137/154，验收程序 0/6，零回归、入口差异或内部错误。
首次 Linux workspace check 因 crates.io 索引下载超时失败，原提交重试后通过。
本机该候选复跑三条 CLI 真实断点、frame 身份负控和后端默认忽略的 LLDB 断点，共 5 项通过。
随后合入更新的主干 `4fd948dd`，无冲突。它带入 SPEC-0341 容器与借用测试迁移，
合流后 `ownership_containers`、`unit_direct_call_loan_ends`、`ownership_map_with` 和
`final_type_admission` 共 172 项通过。最终双宿主门禁以合流提交为准。

[PR #353 最终门禁](https://github.com/Halckon/Koven/actions/runs/42440264349) 在 `b09c2f99`
上通过双宿主定向、生成资源、Linux sanitizer 与两宿主预览包独立安装，conformance 均为
137/154、验收程序 0/6，零回归、入口差异或内部错误。编辑器按范围跳过，macOS CI
忽略的后端 LLDB 断点和四条 CLI 调试检查在同一提交上本机通过。已合入 `04583bb2`。

## 74. R4 局部解构与独立 while 的可空证明

从 §73 原生见证去掉顺序 `for` 后，`check` 仍成功，`build` 返回
`UnsupportedNode`。Refine 重试已可识别 `while` 入口的证明失效，却要等顺序迭代
才确认路由。现由通过预检且实际移除外层可空证明的 `while` 确认重试，仍要求同一
callable 的已验证局部解构，保留既有首次准入。独立复核核对了循环资格与 builder 的
失效位置。数值 `for` 在空 `while` 后仍不能借用此规则。

新增单文件与项目原生见证，核对零轮和重赋值后精确输出。另用真实双文件 Unit 验证普通
和常量能力入口，覆盖三次重赋值、continue、break 与无关根的读取，两个入口均完成
7 次分配和 7 次释放。常量 `while` 的死分支 lowering 仍是独立缺口。
当前分支本机通过 Refine 路由 35 项、while SSA 37 项、原生资源计数 1 项和 while CLI
7 项，零失败或忽略。codegen 与 CLI 全目标 Clippy、格式、文档、Rust 尺寸和 diff 门禁通过。

[PR #355 双宿主门禁](https://github.com/Halckon/Koven/actions/runs/42442529470) 在 `48289d95`
上通过。两宿主 conformance 均为 137/154、验收程序 0/6，零回归、入口差异或内部错误。
Linux sanitizer、两宿主预览包独立安装及本机 5 项 LLDB 相关检查均通过。已合入 `adba71c3`。

## 75. R4 常量 while 的可达边与操作数退出

在 `adba71c3` 上，死循环体或无正常出口循环的死后缀含局部解构时，前端接受，
普通 Unit lowering 仍沿不可能边尝试构建并报 `UnsupportedNode`。改为先求值条件，
再依据已产生的 SSA 布尔常量保留实际后继。假条件直接消费 LoopExit 清理，真条件保留
break/continue，无退出时返回 Diverged。动态条件与专用 nullable while 保留原分派。

独立复审及原生对照确认，原型让一元操作数、名义构造参数和方法接收者的已终止求值
误变为后端错误。普通 Unit 现将终止结果传给外层，未产生结果时不会提交构造或调用，
已求值的构造前缀沿实际控制转移处理。修订同时覆盖共享取值入口的 Rc、临时可空比较和字段投影。

单文件与项目回归对照条件副作用、死循环体、真条件 break、一元操作数、标量构造前缀
及方法接收者。SSA 负控核对死解构仍不触发 Refine，标量字段构造由保留的 Unit 路径完成。
MoveOnly Guard 前缀另走默认路径，逐次核对释放且无完整对象析构。强制保留 Unit 时该
MoveOnly 构造前缀在基线也返回 UnsupportedNode，本批未扩大该能力。
最终工作树本机通过 96 项定向检查，覆盖 CLI while、Refine 路由、while SSA、
短路与 aggregate。codegen/CLI 全目标 Clippy、格式、文档、尺寸和 diff 检查通过。

[PR #356 双宿主门禁](https://github.com/Halckon/Koven/actions/runs/42445897440) 在 `7cfe3e8e`
上通过。conformance 均为 137/154、验收程序 0/6，零回归、入口差异或内部错误。
Linux sanitizer、两宿主预览包独立安装与同提交的 5 项本机 LLDB 相关检查均通过。
已合入 `f3d2857e`。

## 76. R4 内层 lambda 的 while 与外层顺序迭代

具名顺序源在外层函数中可进入 Refine。给同一函数内的 lambda 增加 `while (false)`，
外层路由错误回退，原生输出仍为 `nested\nkept\n`。原查询按范围遍历 AST，未区分
最近 callable。改由同源 AST 的 while 节点和名称解析的父作用域建立归属索引。
查询仅在路由需要时按 SourceUnit 构建一次，函数、泛型实例、thunk 和重试共用。
索引缺失或名称事实与 AST 不匹配时，保留保守门禁。

回归对照无 while、内层 lambda 的 while、当前函数的 while 和 thunk 自身的 while。
独立复核核对了同源索引、未包装 AST 的 callable Span、父作用域顺序和重试复用。
隔离候选实际构建并运行单文件与项目，有无内层 while 均输出 `nested\nkept\n`。
当前分支本机通过 Refine 路由 38 项、closure 4 项、while SSA 37 项和 while CLI 8 项，
零失败或忽略。codegen/CLI 全目标 Clippy、格式、文档、尺寸和 diff 检查通过。

[PR #357 双宿主门禁](https://github.com/Halckon/Koven/actions/runs/42448570108) 在 `6ad9723c`
上通过。两宿主 conformance 均为 137/154，验收程序 0/6，零回归、入口差异或内部错误。
Linux sanitizer 与两宿主预览包独立安装通过，编辑器按范围跳过。已合入 `a884ab8d`。
本机完成授权后在同一 PR 提交重跑，Mach-O 行表、CI 默认忽略的后端真实 LLDB 断点和
四项 CLI 调试检查共 6 项通过，零失败或忽略。

## 77. R0/R3 可空容器与借用测试的 Unit 消费者迁移

以 `a884ab8d` 为基线。将 Map promotion、owned-null cleanup、多参数借用、混合借用
四份测试中的 33 处旧 helper 调用移出 Single。成功路径与后端拒绝路径复用既有
MemoryDocument helper，与同一测试原有的 Rooted 入口对照，保留原源码及 SSA、
清理顺序或 `UnsupportedNode` 断言。§12 四项混合借用后端对照的旧入口分支在本批退役，
当前由 MemoryDocument 与 Rooted 覆盖。

两份非法借用源码通过 Unit 分析检查，分别在 ownership 阶段精确拒绝为 L0162 和 L0135，
主 Span 分别指向 `other` 和 `source`。保留可空工厂的旧规划拒绝与 Rooted 成功对照，
以及 Map require 未被 entry 调用的旧整文件见证。没有生产实现改动。

独立复核核对了源码和断言、最终 Unit 准入、Rooted 跨文件来源及保留的旧路径见证。
本机 `lang-codegen --lib` 依次通过四组迁移测试 23 项、借用结果及 Map require 见证
7 项、lowering 依赖门禁 14 项，零失败或忽略。codegen 全目标 Clippy、格式、文档、
尺寸和 diff 检查通过。本批未在本机重跑全量或动态矩阵。
本批迁移归属 [SPEC-0341](0341-shared-lowering-core.md)。其他旧消费者和 R0–R5 退出条件仍开放。

[PR #358 双宿主门禁](https://github.com/Halckon/Koven/actions/runs/42451131543) 在 `babae5a0`
上通过。两宿主 conformance 均为 137/154，验收程序 0/6，零回归、入口差异或内部错误。
Linux sanitizer 与两宿主预览包独立安装通过，编辑器按范围跳过。已合入 `f129ba39`。
同一 PR 提交的本机 Mach-O 行表、macOS CI 默认忽略的后端真实 LLDB 断点和四项 CLI
调试检查共 6 项通过，零失败或忽略。

## 78. R1 类声明的错误恢复与共享递归预算

从 `a884ab8d` 独立复审 class-family。最小字符串委托目标 `by ""` 的原版 Parser 返回
`InvalidLexemeStream`。原 CLI 单文件 `check` 退出 2。修复复用声明恢复的 owner 栈，只消费
目标字符串，使用真实关闭引号或已验证的词法终止事件停止。空串、嵌套插值、非法转义及
相邻终止事件均保留后继 supertype、body 或文件声明。委托分支不逐项二分 owner 索引。
修复候选对已声明两个接口的单文件和项目均退出 1，输出恰一条 L0078 JSON 诊断。

空变体但有成员的 enum 原本零诊断，现报 L0073 并保留真实成员区。payload 缺 `)` 时
保留外层分号和右花括号，显式分号穿过嵌套泛型及函数 TypeRef。变体与参数的 Span 止于
实际消费位置。缺失 companion 常量以及跳过非法初始化 token 时保留调用方的显式分号、
两个真实 closer 和后继成员。两公开入口用相同 oracle 核对常量诊断、成员与源码范围。

非法嵌套 class 和 companion 的成员体接入既有 Parser 递归预算。class 的第 1025 层
原来仍可返回 AST，修复后两入口在该深度返回 `NestingLimitExceeded`，第 1024 层
仍返回带 L0077 的恢复产物。未测原版进程栈溢出阈值。

独立复核先确认了嵌套 TypeRef、单条声明分号与零宽错误子节点的范围缺口，再核对
修订后的普通 region 路径、终止事件游标与显式 stops。父类型解析移动到同职责子模块，
除委托字符串恢复外保留原分派。主构造器、interface 的父类型边界、软修饰词、重复
companion 与 malformed TypeRef 自身范围仍是后续审查项。本批保持 R1 开放。

隔离候选 frontend 全量 183 个结果组、2725 项通过，零失败或忽略。相关源码逐字接入
`f129ba39` 后，分支复跑 class-family、接口委托和递归边界 43 项，Parser 内部 19 项，
CLI `check_cli` 6 项，LSP 57 项，零失败或忽略。frontend 全目标 Clippy、格式、文档、
Rust 尺寸和 diff 门禁通过。`class.rs` 从 1088 降至 943 行，尺寸额度未增加。
双宿主及 native 发布门禁以 PR 的精确候选为准。

[PR #359 双宿主门禁](https://github.com/Halckon/Koven/actions/runs/42452943514) 在 `37ec9d43`
上通过。两宿主 conformance 均为 137/154，验收程序 0/6，零回归、入口差异或内部错误。
Linux sanitizer、两宿主生成资源及预览包独立安装通过，编辑器按范围跳过。已合入 `6e5934b5`。
本机同提交补跑 6 项 LLDB 检查，含 Mach-O 行表、macOS CI 默认忽略的后端真实断点及
4 项 CLI 调试检查。`parser_interface_delegation` 的 9 项仅本机执行，见 §79 的补接。

## 79. R1 类声明头的 body 边界与诊断

在 `37ec9d43` 上复现主构造器、类型参数和父类型列表吞类体，以及非法默认值吞外层
`}`。让真实类体 opener 与外层 closer 穿过对应 header/TypeRef 解析。保留函数类型、
内层平衡 brace 及非法默认 lambda 的 owner；未闭合的非法父类型调用继续保留原有硬
brace 规则。独立复核先复现候选把硬 stop 改软后吞类体的回归，已恢复基线并补见证。

Classifier、构造器字段和声明头受影响的 TypeRef 使用真实消费末端，不跨错误节点前的
trivia。重复 companion 现在在同一 classifier 的第二个关键词报告 L0077。构造器和
枚举 payload 分隔恢复保留词法 poison 的单一根因，在已恢复的真实逗号处不重报缺字段。

隔离候选的三组定向测试共 49 项，frontend 全量 183 个结果组、2731 项，零失败或忽略。
frontend 全目标 Clippy 与格式检查通过。后续还需核验无类体时的下一文件声明、软修饰词、
连续 TypeParameter poison、父类型后继 poison 与非法分组委托目标的 owner。独立最小探针
已实跑记录上述边界。R1 保持开放。

核对 PR #359 两宿主日志，`parser_interface_delegation` 当时只本机实跑。现在加入既有
Parser stage Cargo 命令，唯一普通目标从 136 增至 137，Cargo 调用仍为 23。新增静态
选择先失败再通过，最终两条静态与一条成功路径选择检查、`bash -n` 通过。

从 `6e5934b5` 建立实际分支，逐字接入上述候选。当前分支通过 49 项定向及 19 项
Parser 内部检查，零失败或忽略。frontend 全目标 Clippy、格式、文档、Rust 尺寸与 diff
门禁，以及同样三项 CI 选择检查通过。该分支的双宿主矩阵待推送后执行。

[PR #360 双宿主门禁](https://github.com/Halckon/Koven/actions/runs/42455437503) 在 `91e6e413`
上通过，macOS 与 Linux 日志均实际执行接口委托专项。两宿主 conformance 为 137/154，
验收程序 0/6，零回归、入口差异或内部错误。Linux sanitizer、两宿主生成资源与预览包
独立安装通过，编辑器按范围跳过。精确提交本机补验 Mach-O 行表、后端真实 LLDB 断点及
四项 CLI 调试检查，共 6 项通过。已合入 `4b311957`。

## 80. R1 类成员恢复的软起点与外层闭合

在 `91e6e413` 复现两条类体 owner 偏差。错误区后的 `own fun` 被跳过 `own` 并多报
L0072，`own borrow fun` 还丢失首 marker 和原重复修饰词诊断。成员恢复在本地
delimiter 和 lexical owner 回到 baseline 时复用正式 receiver 词形，只看固定两个
token。保留重复／逆序诊断和首 marker，并验证内层组和字符串插值中的同形 token 不提升。

非法嵌套 classifier 缺名称时，真实外层 `}` 保留给父类体，缺名称的 L0067 仍以空 Span
报告。文件入口精确验证父类体与后一顶层声明的源码范围。

另在隔离候选验证了连续词法 poison 后的类型参数、嵌套父类型实参与函数型参数恢复，
以及父类型后没有后继时保留文件 Error Item。计数回归捕获早期候选 556 个 raw lexeme
发生 52626 次查询，最终隔离候选的前向和反向访问均纳入预算并通过。该候选全 frontend
2739 项通过，复核仍确认部分分隔位置的 poison 没有当前结构内可达 Error node。
本批分离并保留该候选，继续按 Guide 01 补 AST 表示和下游检查。

无类体时的文件边界另有最小红测。普通类构造器吞 `public class Sentinel`，未闭合的
interface 类型参数吞 `value class Sentinel`。对照确认合法 `value` 名称、visibility
字段与平衡非法默认值正常留在其 owner。非法分组委托目标仍有外内逗号归属缺口。

独立复核确认成员候选与原已审阅逻辑一致，未带入 TypeRef 或 poison 游标改动。隔离
候选 frontend 全量 183 组、2733 项通过。逐字接入 `4b311957` 后，实际分支 97 项
定向与 19 项 Parser 内部检查通过，零失败或忽略。frontend 全目标 Clippy、格式、
文档、Rust 尺寸和 diff 检查通过。双宿主矩阵待推送后执行。

[PR #361 双宿主门禁](https://github.com/Halckon/Koven/actions/runs/42457860370) 在 `d15fdc69`
上通过。两宿主 conformance 为 137/154，验收程序 0/6，零回归、入口差异或内部错误。
Linux sanitizer、两宿主生成资源及预览包独立安装通过，编辑器按范围跳过。精确提交
本机补验 Mach-O 行表、后端真实 LLDB 断点及四项 CLI 调试检查，共 6 项通过。
已合入 `70cdd8d9`。

## 81. R1 类构造器与分组委托的文件 owner

在 `d15fdc69` 上，缺右括号的文件级类构造器会吞后续 `public class`。文件分派显式
传递 owner，只有没有真实 closer 的构造器才在本地基线交还文件 stop。保留 `public val`
字段、合法 `value` 名称与平衡默认值。无类名、父类型、委托目标或函数类型返回时，也
保留同级已识别文件起点和零宽诊断。合法 `value class` 在文件分隔处使用精确双 token
识别，不把普通 Identifier 当文件起点。

普通分组使用一次惰性扫描，忽略已验证字符串和插值 owner。已配对的非法委托目标和调用
后缀保留完整括号内区域，诊断后仍交还外部逗号、后继父类型和类体。未配对的右花括号
切断分组，孤立右方括号不弹无关 opener。回归覆盖缺 closer、真实 closer、嵌套默认值、
词法终止事件和外层文件根的源码范围。

独立复核先后发现物理配对误把平衡构造器中的非法类提升为文件根、孤立右花括号跨界、
以及角符号被后继比较运算误闭合。普通分组候选已修复前两条，定向实跑覆盖其红绿结果。
角符号候选保持隔离，`interface C<T` 仍可能吞下一文件声明，实际红测证明后继
`val later = 2 > 1` 的 `>` 不能作为类型参数 closer。当前未改类型参数列表流程。

最终隔离候选 frontend 全量 183 组、2742 项通过。逐字接入 `70cdd8d9` 后，实际分支
106 项定向及 19 项 Parser 内部检查通过，零失败或忽略。frontend 全目标 Clippy、格式、
文档、Rust 尺寸和 diff 门禁通过。

PR #362 合入最新主干后的精确头 `80cd7fb2` 基于 `4ae149a6`。
[实际 CI](https://github.com/Halckon/Koven/actions/runs/42461329571) 的两宿主 Check、Clippy、
定向组合、生成资源、Preview 构建及独立安装通过，Linux native sanitizer 与精确头门禁通过。
两宿主 conformance 均为 137/154，零回归、入口差异或内部错误，6 个验收程序仍属已登记的未实现。
Editor job 按路径范围跳过。macOS CI 按既有权限边界忽略的真实 LLDB 断点用例，已在精确头
本机显式复跑，与 Mach-O 行表及四项 CLI 调试检查合计 6 项通过。PR 已合入 `d5d3c8f5`。

## 82. R1 父类型槽位与非法委托目标的词法错误节点

基线 `80cd7fb2` 的两组红测分别表明，`class C: async, J {}` 的保留字未在父类型列表中
产生可达错误节点，`class C: I by β async, J {}` 的委托错误 marker 只覆盖 `β`。两条路径
均复用原声明恢复游标。父类型槽位以 invalid 或保留字起始时，使用原恢复区建立
`TypeRef::Error`，名称解析跳过它，类型检查不产生接口继承边。非法委托目标已有 Error
marker 时，其后以词法错误起始的恢复区更新目标、委托子句及父类型项的源码范围。

红绿回归断言词法码、可达错误节点、实际消费范围、后继父类型、类体及文件根。另核对合法
委托目标的名称 marker 保留，文件根前的多字节 invalid token 具有真实范围。两项独立静态复核
核对了 owner、诊断及下游读取，未发现新增回退。基于 `80cd7fb2` 的合批候选 frontend 全量
183 组、2747 项通过，零失败和忽略。frontend 全目标 Clippy、格式、文档及 diff 检查通过。
接入 `d5d3c8f5` 后，实际分支的类声明和接口委托共 60 项定向检查通过，尺寸门禁通过，
无新增超限文件。候选双宿主验收随下一批 PR 执行。

合法父类型或合法委托目标后的独立词法错误节点、嵌套角括号内逗号及下一文件声明的恢复
仍开放。`interface C<T; public val sentinel = 2 > 1` 的基线红测已确认，比较的 `>` 被
误认为缺失的类型参数闭合符。显式分号的处置见 §83。


## 83. R1 文件类型参数的显式分号

在 `bf8e6750` 上，`interface C<T; public val sentinel = 2 > 1` 的首根错误覆盖了比较符。
文件级 class-family 类型参数在 `;` 后识别明确的文件声明前缀，再确认普通分组与词法 owner
已回到当前 `<` 的基线，向现有关闭诊断提供分号处零宽 Span，并把分号留给文件循环。
未完成上界、嵌套 TypeRef 和原声明恢复扫描沿用同一边界。`T;>` 沿用原局部错误恢复，
带真实 closer 或仍未闭的普通组保留内部分号。函数及独立声明入口没有此作用域。

独立复核先后确认了内层 TypeRef 丢失普通组、逐层重扫分号后注释、以及前根 opener 干扰
当前错配 closer 的反例。共享普通组处理只从当前 `<` 单向推进，字符串与插值沿原 terminal
事件规则，上一文件根的栈不会进入此作用域。原构造器/分组委托的全流配对保留。
缓存当前分号的判定，避免同一分号上嵌套 TypeRef 重扫后继；深度 16→32、注释 64→128
的操作计数在缓存前 5936→19872 失败，修复后通过线性增幅断言。

显式分号及后继前缀优先确立文件分隔，后续孤立 `>` 不逆向归还。仅有换行的下一声明、
普通分组内的裸 `>` 与比较运算符角色、以及父类型列表嵌套角符号的恢复仍需分别验证。

同一批候选的 frontend 全量 183 组、2755 项通过，另跑类声明与接口委托 67 项，零失败和
忽略。最新候选通过独立静态复核，复核者未重复运行 Cargo。格式、文档、尺寸和 diff 门禁
通过。双宿主实跑以本批 PR 的精确 head 为准。

PR #364 合入 `6d87040b` 后，精确头 `45a5c71f` 的
[双宿主 CI](https://github.com/Halckon/Koven/actions/runs/42470307101) 通过。两宿主 Check、
Clippy、定向组合、生成资源、Preview 构建与独立安装均通过，Linux native sanitizer 与
精确头门禁通过。两宿主和本机 conformance 均为 137/154，零回归、入口差异和内部错误，
6 个验收程序仍属已登记的未实现。Editor job 按范围跳过，macOS CI 已登记的真实 LLDB
断点在同一提交本机显式补跑，连同 Mach-O 行表与四项 CLI 检查共 6 项通过。
PR 已合入 `34311941`。

## 84. R0/R2 调用参数诊断来源与类型身份消费者

在 `bf8e6750` 上，非 lambda 实参筛出唯一重载后，两个 Unit 入口均在 `true` 报 L0084，
但 label 覆盖完整 `callback: (Int) -> Int`。旧 Single oracle 要求所选参数名的精确 Span。
Unit 源码候选保留声明参数的 source-qualified symbol。按具名或位置映射取得选中参数，lambda
上下文从现有名称事实读取精确名称范围。普通实参诊断和没有源码 symbol 的外部/函数值路径
沿用原 label，失败调用继续不发布描述符。

原精确诊断测试迁至 MemoryDocument 和 Rooted。跨文件新增同名重载的成员调用回归，分别
调换具名实参和源文件输入顺序，验证错误处及 label 的实际源文件、精确 Span 和失败 call
事实。独立复核核对参数 symbol、替换与实参映射，复用已有 `unit_symbol_span` 查询。

隐式借用、显式 `borrow` 和 `own` 的规范类型身份测试一并迁移。原顶层运行时值用例在 Unit
会报 L0167，故将声明放入合法函数体，保留三个 TypeRef 及身份/参数模式断言。原 Single 与
两个 Unit 入口先用同一局部输入核对等价，再将此测试切到两个 Unit 入口。

基于 `45a5c71f` 的合批候选 frontend 全量 183 组、2780 项通过，零失败和忽略。
前端全目标 Clippy 与格式通过。接入 `34311941` 后，实际分支的 `type_callable` 与
`multifile_type_checking` 定向共 304 项通过，零失败和忽略。远端验收随下一批 PR 执行。

R0 对 `45a5c71f` 按 §2 的同类源码模式重新计数。排除定义后的匹配行数如下，保留语法计数
的局限，包含测试或字符串，未覆盖仓外调用和全部别名/宏。

| 旧入口 | frontend 文件 / 匹配行 | codegen 文件 / 匹配行 |
|---|---|---|
| `check_types` | 86 / 147 | 15 / 18 |
| `check_ownership` | 46 / 58 | 14 / 17 |
| `lower_scalar_file_with_entry` | 无 | 4 / 8 |

CLI 单文件由 bootstrap 构造 Unit 描述，project 走 `project_build`，LSP 由 `unit_session`
持有 Unit 名称与类型输入。三者及 frontend `analysis` 未检出旧名字。codegen 的八条
`lower_scalar_file_with_entry` 匹配分布在四个测试文件，含输入身份负控、可空 Rc 路径与只读
方法旧路径拒绝见证。仍需按实际测试意图处置剩余消费者，计数下降不代表迁移退出。


## 85. R0/R2 错误 lambda 结构的参数模式消费者

原 Single 测试用两个顶层运行时值检查错误 move 约束和参数数量。将两条 initializer
置于合法函数体，保持原 `inspect(item)`，加同体的合法共享借用 lambda 作为正控。
实际运行了同一输入的原 Single、MemoryDocument 和 Rooted 检查，三者均报告两条 L0084，
结构失败的参数均无已采纳模式。双 Unit 测试另核两个真实箭头的精确 Span 及合法参数的
Borrow 模式。此后只保留双 Unit 测试，继续通过真实名称事实定位参数符号。

三笔合批提交的 `type_callable` 与 `multifile_type_checking` 共 304 项通过，零失败和忽略。
前端全目标 Clippy、格式、文档及 diff 检查通过，双宿主验收以合批 PR 的精确 head 为准。

PR #367 的精确头 `460651ca` 基于 `34311941`。[双宿主 CI](https://github.com/Halckon/Koven/actions/runs/42472433769)
的 Check、Clippy、定向组合、生成资源、Preview 构建与独立安装以及精确头门禁通过。
两宿主 conformance 均为 137/154，零回归、入口差异或内部错误，6 个验收程序仍为已登记的
未实现。Linux native sanitizer 通过。Editor job 按范围跳过。macOS CI 已登记的真实 LLDB
断点在同一头本机补跑，连同 Mach-O 行表和四项 CLI 调试检查共 6 项通过。已合入 `f83aff80`。

## 86. R1 文件级类型参数内的普通分组闭合

在 `460651ca` 上，`interface C<T break f(; public val hidden = 2 > 1)>` 的组内比较符
被当成类型参数列表的 closer。文件级类族的类型参数恢复仅在可证配对的普通组内忽略
这一 hard stop。真实普通 closer 回收本地栈深度，保留组后逗号、参数、文件分号与后继根。
插值中的异形 closer 只可弹出当前词法 owner 内的栈项。未配对组仍归还外层 `>`，函数、成员
函数及非文件级类族的类型参数不进入这条普通配对路径。

配对候选按 opener 的局部上下文逆序计算。每个普通分隔符计算八种祖先形状集合的边界，
真实子闭合后跳过子区间，祖先 closer 及孤立右花括号保留给其 owner。惰性预索引的初始化
为 O(n + 8m)，其中 m 是普通分隔符数，临时表 O(m)，常驻候选 O(n)，局部恢复栈 O(d)。
现有构造器和委托的普通分组配对仍使用其原索引。恢复读取到配对组内 `>` 后，只在当前
`<` 的普通/词法基线确认有换行的真实文件起点，并在名称或 TypeRef 恢复退栈时复用该判定。

独立复核先后发现并复测了异形分组栈、名称恢复重复进入、插值弹外部栈、前一文件根干扰及
函数调用方回退。仓内永久对照全部 7776 个长度 5 的普通分隔符序列，独立模型另穷举到
长度 8，和逐 opener 正向恢复一致。32→64 个混合组的操作计数通过线性增幅断言。

## 87. R1 词法错误父类型的已闭合类型实参

在 `460651ca` 上，`class C: async<A, B>, J {}` 会把内逗号交给外部列表并额外报告
L0017。strict trial 原本已识别严格闭合的 `<...>`，仅因没有调用后缀而返回 NoMatch。
保留同一识别结果中的真实 `>`，使父类型槽位的词法错误区域可一次跨过严格闭合的实参。
候选不产生子 TypeRef 或伪造名称，Error Span 覆盖实际源码。后继父类型、类体和文件根
保持原 owner。未闭合列表走原恢复，typed-call 的后缀要求和深度门禁保持原义。

独立静态复核核对了 O(1) 边界查询、词法根因、实际消费范围及 typed-call 回退。回归覆盖
保留字、invalid Unicode、嵌套列表、连续词法错误以及未闭合列表的外部逗号。

两项合批候选基于 `460651ca`，frontend 全量 183 组、2792 项通过，零失败和忽略。
全目标 Clippy 和格式检查通过。接入 `f83aff80` 后，实际分支的类声明、接口委托与 Parser
内部检查合计 103 项通过，零失败和忽略。下一批 PR 执行双宿主验收。

未涉及配对组的隐式换行、未能严格配对的父类型实参，以及合法父类型后的独立词法错误
节点仍在复审范围。合法委托目标见 §88。

## 88. R1 合法委托目标后的词法错误范围

在 `e6e633d2` 上，`class C: I by delegate async β, J {}` 只保留原有两条 Lexer 诊断，
但消费的 `async β` 在父类型项中没有可达的错误范围。回归先在父类型项范围断言上失败。
由父类型项单独保留这个实际恢复区，同时保留 `I` 的 TypeRef、`delegate` 名称及
`by delegate` 的范围。外部逗号、`J`、类体和后继文件声明仍由原 owner 消费。非法
委托目标仍在已有 Error marker 记录恢复区。

独立复核发现 `async<A, B>` 会把内层逗号交给父类型列表，`async({ -> 1 })` 会把
内层花括号交给类体。两项定向回归先出现额外 Parser 诊断。委托恢复复用既有的严格
类型列表与普通分组配对，范围在最后实际消费的 token 处结束。缺少配对证明时仍走原
恢复边界。未闭合普通分组遇文件声明起点的归属仍需另外复审。

三项合批前端全量 183 组、2795 项通过，零失败和忽略；前端全目标 Clippy、workspace
全目标 `cargo check`、格式、文档和 Rust 尺寸门禁通过。
第一次全量测试期间，同一 target 被定向复现重建，7 个编译契约测试未找到匹配的 rlib。
上述全量结果来自独立 target 串行重跑，编译契约测试在内。

PR #368 的 head `97cc36f0`、base `f83aff80` 的[双宿主 CI](https://github.com/Halckon/Koven/actions/runs/42476250509)
以合并快照 `9a152d95` 运行定向检查，已核对源码树等于 head。两宿主的 Check、Clippy、
定向组合、预览构建及独立安装均通过，精确 head/base 的 Code CI Evidence 与 CI Passed
均成功。Editor lexical contracts 按范围跳过。两宿主各通过 16 个生成资源用例，conformance
均为 137/154，零回归、入口差异或内部错误。6 个验收程序仍未实现，P2 基础组 58/74，
低于 70 的目标。Linux 13 个 native ASan 干净程序及检测负控通过。在精确 head 本机
补跑真实 LLDB 断点、Mach-O 行表和 CLI 调试产物共 6 项通过。已合入 `59396543`。

在该 head 的只读探针确认，完整父类型后的 `I async Junk<A, B>, J {}` 仍会把后续
父类型与类体交给文件错误区。泛型内部的 `I<(async)?, B>` 和 `I<async by field, B>`
会把内层 `B` 提升为外层父类型。完整类型后的未配对 `(` 还可能吞掉下一文件声明。
早期尾随恢复候选在独立复核中暴露内层逗号和花括号归属问题，未进入此 PR。

## 89. R0/R2 外部调用与 Abort 消费者

两个 `type_callable` 见证从旧 Single 迁到现有的 MemoryDocument、Rooted helper。
标准 `error` 保留外部身份、Borrow、Nothing 和 Abort，源码同名函数不继承 Abort。
无效调用维持 L0121/L0084 及实际主范围，且不发布成功调用事实。
自定义外部调用仍核对三个真实外部 ID。两条顶层运行时初始化移入函数体，使输入遵守
Unit 准入。迁移前用同一合法输入核对旧 Single 与两个 Unit 入口，再移除两个测试的五处旧入口调用。

独立复核发现，`stopExternal` 在 Unit 语句中只检查无 Abort 会放过错误的返回类型。
已在同一描述符补上 Borrow 与 Nothing 断言并复核。实际分支的 `type_callable` 与
`multifile_type_checking` 304 项通过，零失败和忽略。

按 §2 的匹配口径，当前 frontend `check_types` 为 86 文件/140 行、`check_ownership`
为 46 文件/58 行，codegen 分别为 15/18 和 14/17，旧 lowering 入口为 4/8。
CLI/LSP 生产目录仍无旧检查器直接语法匹配。该计数包含测试或字符串，剩余匹配
仍需按真实消费者意图区分，R0 未退出。

## 90. R3 Unit 数据流的捕获查询

在 `f83aff80` 上，形成 lambda、检查逃逸与跨线程诊断，以及逐循环边和活 binding
检查共享捕获时，按 lambda 身份反复筛完整的 Unit 捕获表。复用数据流入口原有的
独占/借用捕获索引构建，按 source-qualified lambda 身份存原描述符下标。保持每个
lambda 的捕获顺序，以及循环首个 L0164 按全表发布顺序选取的主 Span。

双源文件各有重复的局部 AST identity。对 32、64 个 lambda（每个两个捕获），既有
线性筛选分别访问 2048、8192 个描述符，索引查找分别访问 64、128 个，结果及原始
下标逐项相同。该计数仅衡量捕获查询读取，不含索引构建、类型检查或整体编译时间。
循环正反例核对具体 primary text。数据流单测 40 项、ownership iteration / closure /
multifile suites 428 项通过，零失败和忽略。

独立复核比较 99,520 个候选顺序、筛选资格与 current/prior 集合组合，首命中与原全表
遍历一致。已覆盖无捕获短路，仍沿用既有的边与 holder 顺序。稀疏捕获下仍有差集遍历
成本。Callable provenance 的捕获存在性见 §91。跨线程 closure 描述符查找仍待复审。

基于 `f83aff80` 的下一批两项合并候选 `4479ce15` 通过 frontend 全量 183 组、2781 项，
零失败和忽略；全目标 Clippy、格式、文档及 Rust 尺寸门禁通过。

## 91. R3 Factory 返回分类复用捕获索引

在 `1dee161f` 上，每个可发布的 lambda factory 返回都对完整捕获表做存在性扫描。
`finish` 现从同一捕获索引按完整 `UnitExpressionId` 查询；原错误门禁、单一返回
要求及 pointer/closure 摘要的内容和排序保持原义。无捕获与有捕获的两个文件夹具拥有
相同的本地表达式 ID；只按本地 ID 查询的变异丢失了无捕获 factory 的 pointer 摘要，
真实回归因此失败。正常索引通过该夹具。32/64 个 lambda 的存在性查询访问零个捕获
描述符，此计数仅针对索引查询，不测整体编译时间。

独立复核比对了索引输入、source-qualified 身份、摘要门禁和发布顺序。Factory 目标
匹配与跨线程 closure 描述符表是其他查询，不包含在本切片。

实际交付工作树的数据流单测 40 项与四个定向集成套件 450 项通过，零失败和忽略。
前端全目标 Clippy、格式、文档与 Rust 尺寸门禁通过。

PR #369 的 head `990a3edf`、base `59396543` 的[双宿主 CI](https://github.com/Halckon/Koven/actions/runs/42480499647)
以合并快照 `2f71205b` 运行定向检查，已核对源码树等于 head。两宿主 Check、Clippy、
定向组合、Preview 构建及独立安装均通过。精确 head/base 的 Code CI Evidence 与
CI Passed 均成功，Editor lexical contracts 按范围跳过。两宿主 conformance 均为
137/154，零回归、入口差异或内部错误，6 个 v1 程序仍未实现。两宿主 P2 基础组均为
58/74，低于 70 的目标。两宿主各 16 个生成资源
用例通过。Linux 13 个 native ASan 干净程序及检测负控通过，macOS 生成资源报告按
平台跳过 ASan/LSan 校准，保留资源计数检查。精确 head 的本机 LLDB
断点、Mach-O 行表与 CLI 调试检查共 6 项通过。合入前一版 `fc998cdf` 的 frontend
全量为 183 组、2796 项，零失败和忽略。已合入 `6d315265`。

## 92. R1 闭合父类型后的词法错误与外层 owner

在 `59396543` 上，完整父类型后的词法错误会把本应留给类的父类型列表和类体交给
文件错误恢复。TypeRef 解析报告实际语法结束，只有到达该边界的非委托父类型才接管
紧邻词法错误。保留原 TypeRef 和精确消费的恢复 Span。分段字符串复用现有声明尾随
词法判定与 Lexer 终止事件。

已配对普通组使用现有局部配对索引，严格角列表使用 strict trial 的真实 closer。
内部逗号与花括号保持局部归属；无配对证明的普通 opener 不吞已知后继文件或成员，
不能严格证明的角列表不让未归属逗号生成父类型。File、Declaration 和 Member
owner 显式区分；外层成员分号、析构、companion 和 receiver 前缀仍交给外层。

候选复核先后复现嵌套普通组花括号、严格角列表逗号、分段字符串、嵌套 classifier
吞外层成员、独立入口丢分号及未配对组后丢成员。定向回归覆盖两入口和原诊断 Span，
32→64 项的 significant token 访问计数在既有线性门槛内。

TypeRef 自身未闭合时的恢复另有边界，例如 `I<(async)?, B>` 和 `I<async by field, B>`。
有真实角闭合但严格试探因实参内词法错误无证的 `async Junk<A, β>, J` 仍需复审，
当前记录为不确定的错误区，不把 `J` 的缺失写成验收通过。委托目标后任意错误表达式
的更宽恢复亦保持开放。

本地 `16bba7fe` 和接入 `6d315265` 后的 `d4ddceac` 均通过 Parser 内部及定向集成
检查 128 项，零失败和忽略。全目标 frontend Clippy、格式、文档与 Rust 尺寸门禁通过。

## 93. R0 前端内部旧检查器调用分流

以 `59396543` 源码和 §2 的直接调用语法为基线，剔除两个公开定义后，frontend
`src` 下的 41 条 `check_types`、13 条 `check_ownership` 匹配均位于测试入口链。
核对了 `iteration.rs`、`checked_file.rs`、`checker.rs`、`origins.rs` 和
`borrow_result/source.rs` 的 `cfg(test)`，包括其子模块。统计不覆盖别名与仓外 API。

| 测试职责 | `check_types` 文件/行 | `check_ownership` 文件/行 | 迁移约束 |
|---|---:|---:|---|
| Single 循环与 drop planner | 18/28 | 2/2 | 对比原有限图、cleanup 顺序和条件退出，先补 Unit oracle |
| 独立 instance replay | 7/7 | 7/7 | 消费现有 Single 清理模型，动态路径证据等价后再迁移 |
| Single 验证器的真实产物变异 | 3/3 | 3/3 | 保留对旧产物的负控，已有 Unit 验证器由不同测试入口覆盖 |
| 原始 origin 与 formed rollback | 2/2 | 0/0 | 私有 Single 合同随对应实现退役 |
| 借用返回来源对照 | 1/1 | 1/1 | 在迁移前确认双链差异仍有意保留 |

公开 `tests/` 的消费者需要逐项区分普通语言合同与旧产物见证。内部调用分流属于
当前源码静态证据，未把 `cfg(test)` 内的见证或仓外 API 算成可直接删除的入口。

## 94. R1 父类型内部实参与函数类型参数的恢复

在 `48c060a6` 上，`I<A async, B>` 会把后继父类型和类体交给文件错误恢复。
`I<(async)?, B>` 可把内部逗号升级，`I<async by field, B>` 还可发布伪委托。
父类型的 TypeRef 带显式声明 owner。泛型实参和函数参数用自己的实际停止符恢复，
保留已解析 TypeRef，把额外消费的范围挂在原类型路径段或参数上。真实 `>` 后的
委托继续可达；缺少实际 TypeRef 结束证明时，内部 `by` 不会产生委托子句。

已配对函数参数组把实际 `)` 的 raw 位置作为扫描上界，内部普通配对及严格角列表
跳转不得越界，嵌套函数不得借用该 closer。无真实 `)` 的情况仍保留实际箭头
的恢复机会。类型元素的分号留给声明调用方。已验证的 TypeRef 元素起点保留合法
`value`、`deinit`、`own` 类型名；实际文件及成员前缀在缺失 closer 时仍可达。
未证实的角列表仍保守。只有类型元素内紧邻词法错误的数值比较恢复走原元素逗号，
外层 `Junk<1, B>, J` 的不确定角列表不会生成伪父类型。

独立复核逐轮查出并复测错误组越界、外层分号、未闭合后继声明、上下文类型名及
重复前缀查询。边界 memo 保留相同位置的结果，32→64 层并同步倍增 trivia 的测试
通过既有线性操作数门槛。最终 Parser 定向及内部检查 138 项通过，零失败和忽略。
前端全目标 Clippy、workspace 全目标 check、格式、文档和 Rust 尺寸门禁均通过。

含损坏实参且严格试探无法证明归属的尾随角列表，以及委托目标后的更宽错误表达式
恢复在本节基线仍开放，见 §92。后续修复范围见 §100–101 和 §103。

在精确提交 `225e8a94` 上，完整前端 2815 项通过，无失败或忽略。本机 LLDB 6 项通过，
覆盖源码断点、Mach-O 行表和 CLI 最终产物。用该提交构建的 codegen 测试二进制另验证
107 项 SSA verifier、38 项 refine subset、1 项非法 SSA 的 LLVM 前置拒绝，以及 5 项
native 发布保护。后者覆盖普通及 const 的来源错误不 reserve、真实 LLVM ENOTDIR 后
保持现有文件或保持缺失、清理 sibling、移除注入后的实际 link/run，以及非法入口和
commit 失败路径。该范围是本机已有测试的复跑。

[PR #370](https://github.com/Halckon/Koven/pull/370) 合入 `600eca4f`。双宿主
[CI 42485669713](https://github.com/Halckon/Koven/actions/runs/42485669713)
对 head `225e8a94` 与 base `6d315265` 执行。合成提交 `932c0310` 的两个父、CI
证据门中的身份与上述 head/base 一致；测试树 `22450bfdea5301a0208e4633f2cb729def3236e7`
与本地 head 及实际 merge tree 一致。双宿主 Check/Clippy、定向、预览包生产及独立安装
通过。两端 conformance 均为 137/154、0/6 既知未实现整程序、零 regression；P2 58/74，
仍低于目标 70。两端 generated-owner 各 16 项通过；Linux ASan 13 个干净样例通过。
generated-owner 总验收在 Linux 为 pass，macOS 为 partial，明确记录 ASan 校准不支持、
LSan 仅计数器；Editor lexical 按变更路径跳过，Linux sanitizer 专项未在 macOS 运行。

## 95. R0/R2 lambda 参数模式与隐式参数消费者

普通 callable 回归改走原 AST 的 MemoryDocument 和 Rooted Unit 入口。主干并行合入的
#366 已迁移显式参数模式，合入后复用其 helper。两入口各自核对
声明和 lambda 参数模式、函数类型模式，以及 headerless `it` 的七组源码锚点、模式与类型。
锚点同时比对真实左花括号字节和原 AST 的完整范围，符号通过同次名称分析的 source unit
与本地 ID 关联。非法上下文、空 body 与返回不匹配各保留确切诊断和原锚点，不发布模式。

迁移前在同一输入上保留 Single 和双 Unit 对照，定向 4 项通过。Unit 的非法 lambda 用例
使用合法的函数外壳，避免把 Unit 的顶层声明限制误作目标诊断。独立复核确认原锚点和
参数身份得到保留。仍需要 Single 专属事实的见证保留。

## 96. R0/R2 泛型上界与成功重载试探消费者

泛型上界案例保留五个错误码、失败实参锚点、成功调用清单及选中的声明、类型实参与
返回类型。成功 lambda 重载复用 #366 在两个 Unit 入口对同次名称产物的重复检查，再核对两个调用的原 AST
范围、调用描述符确定性，以及已提交 lambda 参数的 Borrow 和 Int 事实。Single 的失败
trial 哨兵见证仍独立保留，它覆盖的是封存后 `TypeKind::Error` 合同。

两条回归在移除旧调用前与 Single 同输入对照，callable 套件 21 项通过。独立复核核对
失败 trial 不进入成功调用清单，以及成功事实的原 AST 与同次名称解析身份。

## 97. R0/R2 nullable 诊断与具名多 lambda 试探消费者

已有的 overload 无匹配、歧义和 nullable bottom 两组对照保留同源双 Unit。L0123、L0124
定位到原调用处 callee 的完整 Span，且两次失败不发布调用描述符；L0084 仍定位到各自
不兼容的源码实参。具名多 lambda 先以原 fixture 让旧入口与两个 Unit 入口同时通过，
再移除旧入口并加强夹具。失败候选按源码顺序先写前两个 lambda 的模式，在第三个
lambda 上因 `String + Int` 失败。两个 Unit 入口各自核对胜者的三组模式、类型与
source-qualified 符号，唯一调用描述符映射 [2, 0, 1]、实例 [String, Int] 和 Long 返回。

独立复核核对候选顺序、实参遍历及失败前的事实写入。合入 #366 后复用其具名 trial
helper 及重复产物断言，保留本批失败在前的夹具、类型和实例检查。

## 98. R0/R2 泛型实例身份与待定调用消费者

同源双 Unit 分别按原 AST 源码匹配七条调用描述符。显式和推导 identity 共享实例
key 并指向真实声明；member 实参按 owner 在前、callable 在后为 [Int, Long, String]，
参数和结果使用已替换类型。嵌套可空推导、lambda 推导与 callback 推导保留原返回 oracle；
普通函数值调用保留 FunctionValue 目标和空实例实参。

原 deferred 夹具的顶层运行时值在 Unit 触发五个 L0167。将相同表达式放入合法函数后，
旧入口与两个 Unit 对照通过。最终按同次 names 的 source unit 限定三个原 AST 调用；
generic 发布唯一描述符，callable reference 与 safe member 调用分别保留精确
OverloadSelection 与 MemberAccess 原因，无伪造描述符。独立复核核对真实目标及待定原因。

在初始基线 `6d315265` 上，`type_callable` 有 13 次旧入口调用。主干并行合入 #366
后剩 9 次，本批集成后剩 2 次，属于同一失败 trial 用例，验证旧 `TypedFile`
封存后的 `Error` 哨兵和重复产物，暂保留。合入主干后的 callable 与 multifile 定向共
304 项通过，无失败或忽略。


后续复核用原歧义及无匹配源码对照 Single、MemoryDocument 与 Rooted。三者都报
L0124、L0123，且不发布调用描述符。Single 会把未提交 lambda 表达式与参数类型
封存为 `Error`。两个 Unit 入口均为这两类 lambda 节点保留事实缺失，仅对实际
失败调用发布 `Error`。因此保留旧封存 oracle，同时让两个 Unit 入口各重复分析
同一份 names，断言两个真实参数均无类型或模式事实，并核对两个 lambda 与两个调用
的精确形状。实际 `type_callable` 和 `final_type_admission` 共 84 项通过，零失败和忽略。

## 99. R4 对象写入路径和失败原子性

独立复审发现 Inkwell 0.11.0 的对象直写先要求输出路径能转换为 UTF-8。在受支持的
Linux 上，非 UTF-8 父目录可合法预留 sibling，随后调用会 panic。本机通过 test-only
路径注入重现 panic；该 macOS 文件系统拒绝创建非 UTF-8 目录。复核本机 LLVM 21 的
直接写入实现另确认，成功打开后遇到持续写入错误会终止进程，绕过 Rust 临时文件清理。

普通/const、legacy/view 继续通过同一 SSA、布局和 LLVM 验证，再由同一 TargetMachine
生成对象缓冲区。Rust 使用原始 Path 写已预留 sibling，返回结构化 Backend 错误，
成功才 rename。独立复核核对了缓冲区实际长度、写入错误与发布顺序。该方案在写入
期间持有完整对象缓冲区，未测量峰值内存与性能差异。

本机非 UTF-8 注入用例检查结构化 ENOTDIR、保留旧文件及无残留；原 ENOTDIR 矩阵覆盖
普通/const 与 legacy/view。Linux 新增真非 UTF-8 目录下的 object/link/run，以及
字符设备 `/dev/full` 的 ENOSPC、旧文件保留、无残留与同路径重试；待该批 PR CI 验收。

[PR #371](https://github.com/Halckon/Koven/pull/371) 已合入 `d2a81954`。
[正式 CI 42489501602](https://github.com/Halckon/Koven/actions/runs/42489501602) 对
head `c30d9226`、base `65eaf1a7` 成功。合成提交 `d6f9ea84` 的父为上述两者，
测试树 `06e49b38932293add9d5dc17a71e3e11f1a6d82d` 与本地 head 及实际合并树一致。
两宿主 Check/Clippy、定向、预览生产与独立安装、精确 CI 证据均通过。

Linux 日志确认真非 UTF-8 父目录的 object/link/run、`/dev/full` 的 ENOSPC 与重试；
两端均实际通过非 UTF-8 错误路径注入。本机集成头的前端 304 项和 native 4 项通过，
相关 all-target Clippy 通过。两宿主 conformance 均 137/154、0/6 既知未实现整程序、
零 regression，P2 仍为 58/74。Linux ASan 13 个干净样例，generated-owner 16 项且
总验收 pass；macOS 16 项通过但校准为 partial（ASan 不支持、LSan 仅计数器）。
Editor lexical 按路径跳过；Linux sanitizer 专项未在 macOS 运行。

## 100. R1 父类型错误区内的损坏角列表

在 `65eaf1a7` 上，`class C: I async Junk<A, β>, J {}` 的严格试探因词法错误
没有登记真实 `>`，尾随恢复把 `, J` 一并吞入。保留字占据类型原子时也能复现；
类体和后继文件声明仍保留。先用同一 Parser 的原 AST 和 Lexer 诊断验证最小失败。

声明层父类型槽或尾随错误区在严格证据缺失时，按需复用类型结构识别建立独立 closer
缓存。仅把单个非法字符或保留字视为损坏类型原子，保留 nullable 及函数参数模式结构；
候选必须有真实闭合。数值/字符字面量、分号、声明起点和未闭合形状没有新增证据。
父类型内部 TypeRef 元素继续使用 strict closer，保留外层 `>` 和缺右括号时的
`->` 恢复机会。typed-call 仍读原 strict 结果，查询前后结果和递归预算不变。

独立复核发现过新 closer 会抢占嵌套 TypeRef 的外层标点，收窄到声明层后复核通过。
正常和损坏角列表、直接错误父类型槽、精确 lexer 诊断、后继父类型/成员/文件根和
未证角列表负控分别有测试。64→128 层的反向构建 token 访问数和状态计数在既有
线性量级门槛内。声明恢复只记录边界，未增加可接受语言语法。

暂时去除嵌套元素护栏时，缺右括号的函数 TypeRef 用例丢失实际返回类型 `B`；
恢复护栏后同一断言通过。未启用新 Guide。

## 101. R1 委托子句后的词法错误 owner

在 `781b1ab3` 上，`I by delegate async Junk<A, β>, J` 把错误区内部逗号
提升为父类型分隔符，原 `J` 和类体的归属回归失败。闭合委托的词法尾随错误改用
同一父类型 suffix owner 和已配对组证据。合法目标保留 `by delegate` 原范围，
后续词法范围另存；已有 Error 目标仍把后续范围并入该 marker。

委托原有的未闭合 `async<A, J` 逗号停止只作用于第一个未证明的 `<`，
不回退文件、类成员或组的停止条件。连续 poison 与分段坏字符串先按各自 Lexer
owner 消费，随后识别的 `<` 同样适用原逗号停止。核对 File/Declaration 嵌套
classifier 后的 `own fun`、显式分号、分段坏字符串和已有 Error 目标。
普通非词法错误的表达式拒绝规则保持既有合同；直接无效委托目标后的更宽恢复仍开放。
仓外探针另确认直接 `I by β<A, B>, J` 仍会提升内部 `B`，产生 L0077/L0017；
该缺口独立于已闭合委托目标的尾随修复。

独立复核发现过未证角列表回退吞成员、坏字符串后的角列表吞外逗号及反复查表开销，
修正后复核通过。两个尾随入口及委托前缀复用按 raw token 下标的惰性位图；位图对已排序
Lexer opener 与 token 单向并进，扫描每次常数查询。32→64 用例覆盖多个独立委托和连续
坏字符串两种排列。父类型槽仍只预消费普通 poison。

合入 `d2a81954` 的集成树上，Parser 内部 39 项及 class family、接口委托、上下文 TypeRef、
递归边界与两项词法错误矩阵合计 117 项通过，零失败和忽略。

[PR #373](https://github.com/Halckon/Koven/pull/373) 合入 `e4d6dbbe`。
[正式 CI 42492956821](https://github.com/Halckon/Koven/actions/runs/42492956821)
对 head `9f7ebce7` 和 base `d2a81954` 成功。合成提交 `30d8993d` 的两个父
与上述身份一致，测试树 `fb47a3635ba52b768641d9970d2d9806244d1f4d` 与本地 head
及实际合并树一致。两宿主 Check/Clippy、定向、预览生产与独立安装及精确证据门均通过。

两端定向日志实际包含严格查询隔离、嵌套函数箭头、委托逗号和连续坏字符串计数用例。
两宿主 conformance 均 137/154、0/6 既知未实现整程序、零 regression、P2 58/74。
Linux ASan 13 个干净样例、16 个生成资源用例且总验收 pass。macOS 16 个生成资源
用例通过，校准仍为 partial（ASan 不支持、LSan 仅计数器）。
Editor lexical 按路径跳过；Linux sanitizer 专项未在 macOS 运行。

## 102. R3 Unit 借用运输的 Abort 与不返回边

主干 #366 后的同一工作树上，本机重跑 `unit_direct_call_loan_ends` 93 项及
`multifile_ownership_checking` 的 `iteration::` 21 项，零失败和忽略；后一个 suite
有 134 项未选。四个在 #366 本机验收中只编译的 frontend 用例实际运行通过。

- 独占参数前缀的 Abort 不产生回卷或命名 owner 的 drop，普通返回对照仍保留。
- 临时父字段已取得来源借用时，后继 Abort 不产生父析构；父构造不完成时也不发布来源。
- 不返回后缀与不可达调用不发布 direct-call loan end 或 drop。
- 嵌套 while 的 break/continue 不释放外层 for provider，Abort 无 cleanup edge，
  正常耗尽仍有独立边。

这里执行的是前端事实检查，测试没有运行源程序的 Abort，也未覆盖完整 native 资源矩阵。

## 103. R1 直接无效委托目标的分隔符归属

在 `9f7ebce7` 上，`I by β<A, B>, J` 只把 `β` 放入 Error 目标，内部 `B` 可被
提升为父类型，并出现额外的 L0077 与外层恢复诊断。保留 Lexer 或 L0078 的实际根因，
让已判定 Error 的目标继续持有真实闭合角列表和普通组。首 token 就是 `<` 时也由同一次
扫描判断归属。无证角列表保留旧外逗号规则，Error 目标自身与原委托跨度仍连续。

独立复核逐轮锁定无证非首角列表、后接 poison、模糊软成员前缀、File 同行完整
`value class` 前缀与同行 `deinit(), J`。后者真实红测产生多余成员/缺体诊断。
最终严格成员前缀仅用于 Member 的 Error 目标；同行 `deinit()` 需紧随真实 body opener，
换行后的实际析构仍归外层。合法 Identifier 与普通表达式后缀拒绝行为保留。

验证覆盖 File/Declaration 同源结果、根诊断顺序、Error marker/委托 Span、配对组、
未证角列表及实际后继父类型和成员。更宽的合法目标后非词法表达式仍是独立开放范围。

本机复审工作树的 Parser 内部及定向共 160 项通过，零失败和忽略。


后续在 `e3c4ace1` 上确认两条独立回归。已有目标的非法角列表或嵌套组会提前结束
错误区，吞掉真实后继父类型或类体。缺目标或已有目标后的换行成员前缀也会被消费。
修补复用闭合组、严格角列表和恢复 owner，保留原 L0077 和目标 `Present`，对未证实
角列表继续保留外逗号。Member 前置识别只交还换行后的可识别前缀，裸 `own` 仍是
合法目标拼写。普通非法尾部中的 `deinit` 同时要求换行和完整 `() {` 才移交外层。

独立复核锁定两条实际失败的修补中间态。错误尾部的 `deinit(), J` 曾丢掉 `J`，
同行 `+ deinit() {}` 曾把内层类体误交为外层析构。最终 File 和 Declaration 两入口
均断言诊断、目标、真实后继成员及分隔符归属。内部操作计数在 32→64 段非法尾部时
满足原线性门槛。最终 Parser 内部及定向集成共 133 项通过，零失败和忽略。
随后 `cargo test --locked --offline -p lang-frontend` 完整运行 2898 项，零失败和忽略。
前端全目标 Clippy、格式、文档及 Rust 尺寸门禁通过。该提交待与下一批合并验收。

## 104. R0/R2 F1/F2 completion 与流事实测试消费者

`lambda_completion` 四个测试的六份源码保持原样。迁移前，同一 fixture 的旧 Single、
MemoryDocument 和 Rooted Unit 都通过精确诊断码列表对照。移除该测试模块对旧 Single
的调用后，两种 Unit 分别验证原 SourceId 与入口身份，再按原 AST 表达式 ID 检查
创建 lambda 可继续、局部 return 终止，并检查实际存在两种节点。
保留缺 return、分支写入使 smart cast 失效、正控及 lambda 自己的非法 return 四项。
独立复核确认该范围没有旧 `TypedFile` 专属事实；不据此推断诊断载荷或仓外 API 等价。

同一工作树的 lambda completion、control branch join 和 while flow 共 25 项通过，
零失败和忽略；与 Parser 合计 185 项。前端全目标 Clippy 通过。

## 105. R0 codegen 的旧单文件消费者与生产回退分流

以 `9f7ebce7` 的 codegen 树为基线，直接调用语法检出 `check_types` 15 文件/18 行，
`check_ownership` 14 文件/17 行，`lower_scalar_file_with_entry` 4 文件/8 行。
核对注册链，全部位于 `cfg(test)` 模块或其子模块。SSA 的 `lower_frontend` 整个模块
由 `cfg(test)` 门控，旧入口 re-export 也只供测试；当前 CLI/LSP 的 src 没有这两种
旧 checker 的引用。此处的 Single 文件 lowering 与生产 Unit lowering 的 legacy fallback
分别记账，后者仍属 R4 路由复审。

| 旧见证 | 处置边界 |
|---|---|
| lowering 入口身份、readonly receiver 拒绝、borrow result | 旧产物合同的直接负控，保留至对应合同退役或有双入口等价证明 |
| canonical callable、closure/source、instance 计划与注册 | 直接读旧分析或 lowering 的内部结构，逐个比较 Unit canonical identity 和发布顺序 |
| Range、while/If、Rc nullable、临时与条件 cleanup | 保留对照、路径及资源顺序，不能仅以 Unit 诊断码替换 SSA oracle |

计数是直接语法匹配与注册追踪，未覆盖仓外调用或整个 Rust 名称解析图。现有
`lowering_dependency_tests` 的词法守卫检查指定生产核心路径，另有正反控制；守卫不会
解释 cfg_attr、宏展开、include 或扫描范围外 re-export。

本机 `lowering_dependency_tests` 14 项通过，零失败和忽略。

## 106. R4 借用绑定校验归属与兄弟函数对照

独立复核找到生产 Unit 在逐函数 refine 路由前按整个单元决定是否运行旧绑定扫描。
CLI 实测 `6f791a21` 上，字段借用、打印、随后 return 的程序构建退出 2
（UnsupportedNode），追加一个未调用、含 swap 的函数后退出 0 并输出 `hi`。
原生回归先在有/无该 sibling 的输入上复现，故障定位到与借用已无交叠的 return。

首版将旧整函数扫描移到旧绑定入口，误拒别名建立前含 `if` 的 P05-for 变体。第二版
修复两条原生正控，但去掉的前置扫描仍保护已知未支持的 RangeView、普通借用与 carrier
别名边界。PR #375 的 `c58be8ae` 在 [Linux](https://github.com/Halckon/Koven/actions/runs/42496593270/job/128781164928)
和 [macOS](https://github.com/Halckon/Koven/actions/runs/42496593270/job/128781164978)
均实际通过两条原生正控，也分别失败于同样的 27 条 codegen 拒绝测试，后续验收未执行。

修补保留原 CFG 前置开关，门禁启用时仅跳过两类有专门检查的形状。直接字段绑定匹配 typed 投影
与 owned 来源字段，由细化层消费绑定、来源和生命周期事实。稳定根 Map 别名匹配同一
来源及完整 provider 计划，旧路径仍核对直接来源、活动 shared loan、CFG 传递与最终 verifier。
未改动 RangeView、carrier 别名、普通调用返回的绑定前置或拒绝测试的 oracle。

编译单元借用结果声明准入与 Map 迭代前置的原边界保留。另一个借用返回转发候选
`view(n + 1, source)` 在有/无未调用 swap 时均无法构建，但错误在 UnsupportedNode
和 InvalidModel 间变化。普通借用调用结果的局部绑定另在 `c58be8ae` 上复现三种
`if` 位置（绑定前、存活期间和最后读取后）都受未调用 swap 影响。无 sibling 时
三者构建退出 2，有 sibling 时构建运行退出 0。局部绑定、转发与 Map 迭代前置分别复审。


后续 Map 对照确认，`for` 遍历 `when` 的临时 Map 在前端检查中通过，旧整单元
后端预检却不接受其源节点。加入未调用的 `swap` 会绕开预检，实际细化路径可运行。
未调用的 Map 字段遍历也受该开关误拒，而真正调用后两种布局均为 `UnsupportedNode`。

两条红测锁定上述差异。修补删除生产 Unit 的旧 Map 整单元预检，把后端检查留给实际
规划并到达的路径。旧路径在发出操作前仍核验 provider，细化层核对同一条 typed/owned
迭代描述符、来源、类型和退出动作，最终 verifier 保留。独立复核确认普通函数、
lambda thunk、Basic/Const 的真实入口仍有阶段检查。SPEC-0331 的历史整源码后端能力
政策在此退出，整个编译单元的前端验证及旧 Single 的门禁保留。
本机两条定向回归已通过，覆盖两种布局的真实 native 输出与实际调用时的拒绝位置。
接入 `59d3c280` 后，前端定向 217 项和 codegen 定向 10 项通过，零失败和忽略。
codegen 全目标 Clippy 通过。正式 CLI 对照覆盖单文件和拆分项目，8 个正例均构建运行
且逐字节匹配 stdout。4 个实际到达的字段遍历负例均保留 `UnsupportedNode` 并未生成
产物。单文件退出 2、项目退出 1 与旧 CLI 的对应入口一致。

本机实际执行 map/refine 路由矩阵 13 项、Map 原别名事实 1 项、multi-borrow 6 项、
borrow result 5 项、RangeView 路由 1 项、依赖方向 14 项，共 40 项，零失败和忽略。
对象写入修复后的前置提交 `6f791a21` 另实际运行 LLDB/Mach-O/CLI 最终产物 6 项，
零失败和忽略，包含实际停止在源码断点和 Unicode 多根项目。

修补版在本机按双宿主失败日志的精确测试名复跑原 27 项，并运行两条原生正控，
共 29 项通过，零失败和忽略；lang-codegen 全目标 Clippy、fmt、文档与 Rust 尺寸门禁通过。
收紧来源身份前启动的全 lib 本机运行已主动结束，完整修补版矩阵以新双宿主 CI 为准。


[PR #375](https://github.com/Halckon/Koven/pull/375) 合入 `e3c4ace1`。修补头
`d0608aae` 与 base `e4d6dbbe` 在 [CI 42498326935](https://github.com/Halckon/Koven/actions/runs/42498326935)
验证，合成提交 `e4b2a405` 和真实合入树均匹配 `2a86432c`。双宿主 Check/Clippy、
定向测试、预览包生产及独立安装、CI Passed 与 Code CI Evidence 均通过。
两个宿主均实际通过两条 native sibling 对照、委托角列表回归和四条 lambda completion。
codegen lib 实跑 Linux 2709 项，macOS 2706 项通过，零失败。macOS 的源码断点
测试因 debugserver 权限标记忽略。两端 conformance 均为 137/154、0/6 已知未实现
整程序、零回归。generated-owner 各 16 项通过，Linux sanitizer 专项通过。编辑器
词法门禁按变更范围跳过，Linux sanitizer 专项在 macOS 跳过。

## 107. R4 普通借用返回转发的实参控制流

在 `e4d6dbbe` 上，转发实参 `-n`、`n + 1` 或值 `if` 时，未加入无关函数的输入返回
`UnsupportedNode`。追加未调用的 `swap` 函数后，实参跨分支，入口来源 loan 经 block
参数重命名，最终借用返回验证器拒绝 `ReturnType`，表层退化为无 Span 的 `InvalidModel`。
比较和位运算则有真实成功路径，旧 AST 整函数扫描会误拒不含 `swap` 的输入。

本地提交 `775bf23e` 让普通转发在当前实参 CFG 上查找第一条实际分支。
可合并的无条件跳转沿单路径走到当前出口。保留跨实参分支的精确 `UnsupportedNode`，
直接 `Map.requireValue` 仍由自己的入口验证。独立复核找到 `when` 的无条件 Goto 以及
直接 Map 的对照，最终测试同时保留直线 `when` 并在嵌套 checked arithmetic 处拒绝。

该提交的独立 target 上 15 项定向测试通过，零失败和忽略。两种 Unit 入口分别核验
实际错误 Span、SSA 来源与 LLVM。正式 CLI 运行 7 个正例，输出均为 `kept` 加换行。
两个跨实参分支变体均构建退出 2，并返回 `UnsupportedNode`。codegen 全目标 Clippy、
格式、文档和 Rust 尺寸门禁通过。该切片留在下一批，完整双宿主验收仍待该批 PR。

## 108. R4 普通调用结果局部绑定的实际结束点

独立的前端产物探针核验同一普通 `BorrowedStorage` 根借用的四种摆放。
绑定前的 `if` 先于获取。绑定后、读取前和同块最后一次读取后的两种 `if`，都位于
前端发布的唯一 `AfterStatement(函数块)` 之前。显式内层块把结束点移到内层的
`AfterStatement(println(item))`，随后外层的 `if` 位于已证明的结束点之后。
最后一次源码读取自身不提供结束证明。

在 `775bf23e` 上写下覆盖两种 Unit 入口的三条回归，首次执行在 MemoryDocument 即失败。
借用前和内层退出后的
正控仍误报 `UnsupportedNode`，存活边界在追加未调用 `swap` 后错误绕过旧门禁。
修补后，普通局部调用绑定按最近 callable 判定原语兼容路径。无关顶层函数和未调用
lambda 的 `swap` 均不能绕过门禁。只有已证不交叠的完整 `if` 语句获得例外。
获取前按普通块的语句顺序证明，获取后还要求同源、唯一且有序的 `AfterStatement`。
证明可沿无条件普通块传播，不穿过分支、循环、lambda 或实参求值。绑定活跃时仍在原
`if` Span 报 `UnsupportedNode`。保留其余语法类别和实参门禁，包括实参内的 `if`
与负号。RangeView、Map、窄化来源、inline 借用和普通返回转发保留各自入口。

独立复核找到外层前置分支、无条件 sibling 块，以及负整数字面量的精确参数边界。
补齐对照后，本机 codegen 定向矩阵 67 项通过，零失败和忽略，覆盖 #375 首轮的 27 项、
实际同函数字段 replace、Map 与 RangeView 的组合及两种 Unit 来源。


正式 CLI 用独立拥有的 String 实跑了 15 个正例。三种布局分别是单独函数、未调用
顶层 sibling 和未调用 lambda，各覆盖获取前与结束后的普通块组合。全部 stdout 与
预期逐字节一致。六个借用活跃负例构建退出 2，稳定返回 `UnsupportedNode`，且没有
生成目标文件。SSA 另查实参前分支支配借用调用，以及结果 loan 与来源 loan 按顺序
结束并支配作用域后的分支。独立复核提出的 nullable receiver 字段投影对照在旧 CLI
与当前 CLI 的两种布局均为相同的 `UnsupportedNode`。codegen 全目标 Clippy、
格式、文档和 Rust 尺寸门禁通过。


[PR #376](https://github.com/Halckon/Koven/pull/376) 合入 `59d3c280`。候选 `df1afc57`
合入 base `e3c4ace1`，在 [CI 42500943395](https://github.com/Halckon/Koven/actions/runs/42500943395)
测试的合成提交 `8bc95356` 与实际主干合入树均为 `ce6b3eb9`。两端 Check/Clippy、
定向测试、预览包生产与独立安装、CI Passed 与 Code CI Evidence 全部通过。日志
确认六条新增回归在两端均执行成功。codegen lib 的 Ubuntu 2715 项、macOS 2712 项
通过，零失败。macOS 因 debugserver 权限忽略一项源码断点测试。双端 conformance
为 137/154、0/6 已知未实现整程序、零回归，generated-owner 各 16 项通过。
Ubuntu 专用 sanitizer 通过。编辑器词法门禁与 macOS 上的 Linux 专项按条件跳过。

同候选树的本机 `cargo test --locked --offline -p lang-codegen --lib ssa::` 实跑
1941 项通过，零失败和忽略，另有 772 项未入该筛选。另行对照普通同步
`observe(view(source))`，独立语句 `if` 在调用前或后时，两种 sibling 布局都在前端
以 L0164 拒绝，未进入后端。纯直线版本两种布局均构建运行。

[PR #377](https://github.com/Halckon/Koven/pull/377) 合入 `e1b0c2dc`。主干基线
`59d3c280` 和 head `5ce6512f` 在 [CI 42504367608](https://github.com/Halckon/Koven/actions/runs/42504367608)
验证，合成提交 `accf6e86` 和实际合入树均匹配 `5430b7f2`。双宿主 Check/Clippy、
定向测试、预览包生产与独立安装、CI Passed 和 Code CI Evidence 通过。
双宿主日志均实际运行委托后缀、overload trial、临时 Map sibling 与到达字段 provider
的五条指定回归。本机前一生产切片的扩大 Map 套件 629 项通过，零失败和忽略。
Editor lexical 按改动范围跳过。Linux native sanitizer 专项通过，macOS 按平台跳过。


## 109. R4 旧路径中的数值区间、Map consume 与委托 receiver

基线 `59d3c280` 的六条普通 Unit 源码分别组合三种操作与两种真实旧路由形状。
前者是数值区间 `for`、`Map.consume`、接口委托 receiver。后者是 Borrow
`String?` 形参或循环内重新赋值外层已收窄的 nullable owner。每组只追加一个未调用、
含 Map 迭代的函数，前端均成功。有 sibling 时六例原生构建运行且 stdout 匹配，
去掉 sibling 后六例均在到达的目标操作处报 `UnsupportedNode`。

修复将三个整单元 gate 替换为逐 callable 的细化资格。细化接手后构建失败直接报错。
当前还需兼容路径的函数与 thunk 使用各操作已存在的 descriptor、来源、layout 和 ownership
检查，再由整程序 SSA verifier 验证。强制旧路径的退役见证仍要求可细化用例在三个旧
入口被拒绝。两组嵌套 Map 查询只将不检查函数体的输入工厂恢复默认路由，仍核对原始
输入、同轮前端事实、调用方和 scan 的所有入边、loan 生命周期、清理与真实 SSA/LLVM。

六条 native 回归在基线均失败。修改后本机六条全部通过，每条分别链接并运行有/无
sibling 的输入且核对退出、stdout 和 stderr。两组嵌套 Map 查询、九条匹配
`retired_` 的既有边界测试通过，零失败和忽略。该结果覆盖本机 macOS，双宿主验证
需在批次 PR 的集成提交上执行。首版扩大 SSA 验证为 1939 项通过、2 项失败；
失败均为上述嵌套查询全量 pin 所致。修正 pin 后两条逐项复跑通过，未将首版记为全绿。

## 110. R4 Borrow 内联 nullable 的只读判空

`5ce6512f` 的 CLI 对照中，Borrow `String?` 判空在有无 Map sibling 时均报
`InvalidModel`。直接返回比较结果也复现。以同样方式比较普通类句柄可以运行，
对 Borrow MoveOnly value class 的内联 nullable 则同样报 `InvalidModel`。

Unit 已产出 `NullableLoanIsNull`，原 operation verifier 只接受句柄布局。
现有 verifier 继续要求活动 Shared loan 和 Boolean 结果。LLVM 依据已验证的表示
读取同一 storage，句柄读指针，内联表示读 niche 指针或显式 tag，缺席分支不读 payload。
不新增 proof view、take 或 ownership 转移。

本机原生回归覆盖 null、空 String（非空 niche）、已有 String owner 的借用，以及
显式 tag 的 MoveOnly value class。两条均先有 CLI 失败证据，修复后链接、运行并逐字节
校验输出。LLVM 断言分别锁定字段 0 的 pointer load 与 i1 load，检查函数中其余
读只对应打印的本地字面量 storage。原普通类句柄的 borrowed nullable temporary
定向测试 1 项、inline nullable 布局套件 6 项及 codegen 全目标 Clippy 均通过。


[PR #378](https://github.com/Halckon/Koven/pull/378) 合入 `a8657f58`。主干基线
`e1b0c2dc` 和 head `e0ffd973` 在 [CI 42507181254](https://github.com/Halckon/Koven/actions/runs/42507181254)
验证，合成提交 `0f8fa2de` 和实际合入树均匹配 `8f043aaf`。双宿主 Check/Clippy、定向测试、
预览包生产和独立安装、CI Passed 及 Code CI Evidence 通过。Editor lexical 按变更范围跳过。
该集成提交本机 SSA 扩大套件 1943 项通过，零失败或忽略。

## 111. R4 普通类 nullable If 的旧路由

主干 `e1b0c2dc` 的 CLI 对照中，含外层 nullable String 循环重赋值的函数自然留在
旧路径。函数后续对自有 `Node?` 判空并调用普通 Borrow 参数。无关 nullable `when`
函数存在时原生运行输出 `present`，移除该函数后，合法的收窄实参报 `MissingFact`。
两种输入均通过前端，基线 Unit 回归精确定位到实参 `node`。

沿用逐 callable 兼容准入，同时保留原 `if` 形状、来源、借用、清理与 verifier。
两条本机定向回归分别核验原生有无 sibling 对照和有空对臂的强制旧路径拒绝。
恢复旧整单元 gate 时，新增强制用例可错误构建并使测试失败。修复后两个检查均通过。
原 `when` 内替换 owner 的 SSA/原生两条检查和原退役检查亦通过。
带额外 Borrow nullable 形参的组合还涉及旧 proof carrier 资格，另行复审。

## 112. R0/R2 成员 receiver 测试消费者

将原 Single `Holder<T>` 的成员 receiver 测试迁到既有 MemoryDocument 和 Rooted Unit
helper。原 fixture 字节保持一致，四个 receiver mode 按成员名核对。旧用例对顶层
`use` 无 receiver 的隐含检查改成显式断言。裸字段及隐式调用绑定到确切 Holder
声明，显式调用绑定到本 source unit 的 `holder` 表达式。既有 Unit 跨成员、默认方法和
object receiver 对照仍独立保留。

迁移前原 Single 测试及相邻 Unit 测试分别实跑通过。迁移后本机 `type_checking` 95 项、
`multifile_type_checking` 283 项通过，零失败和忽略。


## 113. R4 细化层的 MoveOnly Borrow nullable 判空

普通类 `Node?` 的自有证明与另一 Borrow `String?` 的存在性比较同处一个 callable
时，前端通过，旧路径在普通类 `if` 的 proof carrier 处拒绝。保留真实 source 和
loan，细化层接受该签名，通过 `NullableLoanIsNull` 只读原 Borrow storage。
判空不建立借用 payload 的 view。原 MoveOnly Borrow presence 负路由源码移到
正例，另用实际需要 payload view 的源码保持负路由断言。

独立审查发现可达 `Map.remove` 会为同型 `String?` 登记 MapResult 布局，初版
候选在此组合被 verifier 拒绝。最终实现对已登记 MapResult 只读字段 0，仍要求
活动 Shared loan 和 Boolean 结果。原生回归覆盖 MapResult 的存在/缺席、普通
Borrow 读取和短路 RHS，另有同形未登记聚合及独占 loan 的 SSA 负控。

初版全 codegen 2724 项通过、1 项有记录的 LLDB 权限忽略，原生有序 Map 碰撞
测试一次超时。该项在最终候选单独复跑通过；初版全量不计为全绿。
细化候选的 nullable 扩大套件 209 项通过。集成复核另复现普通 Unit 中同一
MapResult 比较受 nullable `when` 影响的拒绝。兼容路径对 MoveOnly MapResult 也读取
原 loan 的缺席状态，并保留 Copyable MapResult 的原路径。两路原生均通过。
合流后 codegen 定向 52 项及两个 frontend 套件 378 项通过，均零失败和忽略。
codegen 全目标 Clippy、格式和文档门禁通过。

[PR #379](https://github.com/Halckon/Koven/pull/379) 合入 `edaaa9b2`。基线
`a8657f58` 和 head `09f68669` 在 [CI 42548767914](https://github.com/Halckon/Koven/actions/runs/42548767914)
验证，合成提交 `8146b057` 与实际合入树均匹配 `fb3e860b`。双宿主 Check/Clippy、
定向测试、预览包生产与独立安装、CI Passed 及 Code CI Evidence 通过。两边日志均
实际执行四条 heap nullable If、MapResult verifier 及 Unit 成员 receiver 测试。
本机精确 head 的 nullable 扩大套件 209 项通过，零失败和忽略。Editor lexical 按变更范围跳过。

## 114. R1 文件类型参数的隐式换行

主干 `a8657f58` 上，文件级缺失 `>` 的 `interface C<T` 会吞入下一行常量声明，
并将比较符当成类型参数 closer。真实 CLI 中，显式分号对照保留下一声明；
合法多行类型参数与同一常量比较可以通过完整前端。

文件类型参数的恢复按显式分号或当前 token 前的换行识别完整文件前缀。普通分组和
词法 owner 须先回到当前 `<` 的基线。限定类型路径在 `.` 后还未取得段名时也查询
同一边界。保留显式分号、真实 closer、分组内比较和普通软标识符的对照。

独立复审核对了 `pkg.`、分行 `value class`、`borrow fun` 和缺少比较符的尾部恢复。
新用例先失败再通过，并核对文件根的完整范围。换行的嵌套退栈曾重复扫描同段 trivia，
深度 16→32、注释 64→128 的实测访问次数由 14476 增至 49196。只在当前文件
类型参数作用域缓存真实游标对应的首个 significant raw 位置后，分号和换行两组
均通过相同的线性倍增断言。原 token 位置、trivia 与 lexical owner 保留。
本机类声明、上下文 TypeRef 与接口委托三个定向套件共 125 项，另加倍增检查 1 项，
全部通过，零失败或忽略。
最终 R1 生产代码的完整 frontend 套件实跑 183 组、2903 项通过，零失败或忽略；
该次编译保留迁移前的类型消费者，迁移后结果见 §115。前端全目标 Clippy 通过。
同一生产代码构建的 `kovenc` 实跑三份源码，换行和分号缺 closer 均只报零宽 L0010，
合法多行列表与后继常量比较退出 0 且无诊断。

## 115. R0/R2 名义类型深度与接口闭包的测试消费者

原 Single 压力用例的源码生成代码逐字节保留。相同 AST 和环境进入 MemoryDocument
及 Rooted Unit。原无诊断、Leaf 传递接口闭包 64 项的断言迁移到两个入口，并核对
接口集合和两入口顺序。沿 `deep` 形参的实际类型，逐层核对 96 层 Box 声明身份和
单实参，再确认最内层是 Int。保留各入口原有 nominal 与 enum 压力测试。

原 Single 和新增双 Unit 压力用例先在同一快照各自运行通过。移除旧测试后，本机
`type_checking` 95 项及 `multifile_type_checking` 283 项全部通过，零失败或忽略。

## 116. R0/R3 待交付 owned 实参的退出验收

原 Single 的五份源码和生成模板逐字保留，迁至 MemoryDocument 与 Rooted Unit。
检查实际待交付临时值对应的 source-qualified ID，逐项锁定后继实参内部
`return`、`break`、`continue` 的析构边。普通提交、`Nothing` 调用及新增的
显式 `error()` 各自验证无该临时值的额外析构事实。原 Single 与迁移对照先同时
实跑通过，随后两套所有权测试共 186 项通过，零失败或忽略。

五个 CLI 探针实际通过前端、构建及运行，核对普通提交与各控制转移的资源析构顺序。
正式 native 用例复用真实 `deinit` 标记，对 break 和 continue 分别核对一轮和两轮，
并以尾部标记检测误续行。Abort 先核对两个资源的构造完成标记，随后仅报告错误及
SIGABRT，没有析构输出。原生资源套件 18 项及最终加强后的新增定向用例均通过，
零失败或忽略。此处验证用户可观察的析构次数与顺序。

下一批与 `edaaa9b2` 合流后，本机前端定向套件及操作计数共 690 项，原生资源、
待完成构造及循环资源套件共 30 项，均通过且零失败、零忽略。前端和 codegen
全目标 Clippy、格式及文档检查通过。双宿主证据以本批实际 PR 为准。

[PR #380](https://github.com/Halckon/Koven/pull/380) 合入 `4243ebe5`。基线
`edaaa9b2` 与 head `0d86e1d4` 在 [CI 42551850856](https://github.com/Halckon/Koven/actions/runs/42551850856)
验证。合成提交 `4de11f94` 的父提交匹配该基线和 head，CI 树与实际合入树同为
`f51ba611`。双宿主 Check/Clippy、定向测试、预览生产及独立安装、CI Passed 和
Code CI Evidence 均通过。两端原始日志均核对六条关键回归实际通过，覆盖文件恢复、
软前缀、操作计数、名义类型压力、实参清理事实及原生退出顺序。Editor lexical 按改动范围跳过。

### R3 复制字段实参的临时父对象

以 `53f1227f` 为基线，`read(Holder(1).number, ...)` 把 Copyable
字段转换为 `Borrow Int?` 后，Unit 发布包装槽的 Shared loan，却在正常返回
与后续实参提前退出时都遗漏完整父 owner 的析构。定向回归在修改前实际失败。

调用清理复用 Field/Group 父对象定位和原 pending temporary 机制。
只有已检查的同调用、同实参 Shared loan 和 nullable 转换合同可以把
父对象加入调用清理。真实 loan 目标仍是包装槽，字段父观测只消费
目标确为父对象的 loan。MemoryDocument 与 Rooted 覆盖裸父和分组父、
正常返回、`return`、`break`、`continue` 及 Abort 不析构。
独立复核额外验证混合精确/复制实参按完成顺序逆序清理，未求值的父不登记。

最终前端单元及三组集成共 895 项通过，零失败、忽略或过滤。前端严格
Clippy 通过。旧 Single 两种字段参数均缺少临时父支持，单列为旧入口
能力边界。Unit 清理事实与后端验收分别记录。

### R4 临时父字段的借用与原生清理

以 `b9672e4c` 为基线。精确 Borrow 和 nullable 复制的公共检查通过，
原生构建报 `UnsupportedNode`。已有父析构事实因 borrowed temporary
准入限制无法进入 Refine，普通调用与借用结果调用也需要同一字段处理。

准入和构建共同核对 source-qualified 调用、实参、字段链及父清理。
精确路径保留实际字段 loan；nullable 路径读取 Copyable 字段，
结束整条字段 loan 链，再借用新的包装槽。分组父保留原清理身份。
正常返回与提前退出先结束活动 loan，再消费前端的父析构计划。
借用结果调用的辅助字段只由内层调用清理，真正来源继续交付给外层。

最终 SSA 借用、资源析构、Range 调用前缀和 Refine 路由四组共 93 项
通过，零失败或忽略。SSA 直接核对嵌套 inline 字段的真实 loan、
子到父结束顺序，以及包装值到实际调用槽的关联。六组公开程序在
单文件和项目入口均构建并执行，覆盖正常返回、`return`、`break`、
`continue` 及具名辅助参数。严格 workspace Clippy、格式、尺寸、
文档及 CLI build 通过。独立复核覆盖准入和构建接线及清理边。

### R4 无分派调用的已完成前缀

以 `736a49ed` 为基线，调用的后继实参必然 `return` 或 Abort 时，
公开检查接受，原生可因缺少 nullable 参数的 SSA 类型报 `MissingFact`。
无 callee 实例时，已完成的 Borrow/Value 实参仍按已选参数类型求值。
Refine 只补这些实际前缀实参的 nullable 类型需求，未求值后缀
不引入参数 ABI 类型。SSA 回归核对前缀包装、callee 实例缺席及
后缀 Boolean nullable 类型缺席。

临时字段的已完成前缀以同源 loan 和无分派事实进入 Refine。
Return 仍消费前端父析构，Abort 允许无析构事实。独立复核确认
普通返回仍由 SSA verifier 拒绝残存 MoveOnly owner。原生回归实际
对照精确字段、nullable 字段，以及 Return 与 Abort 的构造、退出和
析构行为。

最终 SSA 借用、调用前缀及 Refine 路由集合分别通过 7、55 和 41 项，
零失败或忽略。四组公开程序在单文件和项目入口均实际构建、执行，
核对正常退出、Abort 信号和资源输出。严格 workspace Clippy、
CLI build、格式、文档及尺寸检查通过。

## 117. R0/R3 二元操作结束与重绑后的退出析构

以 `edaaa9b2` 为基线。原 Single 的 String 拼接与比较源码、重绑的普通块与 if/else
变体逐字保留。两个旧测试先在同一快照实跑，之后迁到 MemoryDocument 与 Rooted Unit。
按源码中的实际表达式与名称解析得到 source-qualified 身份。String 的四个操作数必须
恰在对应 `AfterBinaryOperands` 逆序析构，核对目标和值来源，排除其它点的重复析构。
重绑核对全部 `ControlTransfer` 事实，精确比较 return、逆声明顺序和当前值来源；
多余的提前退出析构同样会失败。独立复核提出该额外边检测后，已补强并复核。

使用真实 native 入口编译、链接和运行独立的 `deinit` 场景。左右重绑与提前 return、
正常落尾组成四条运行路径。各路径都核对旧值析构、退出标记、second、当前 first 的
完整输出顺序。此处的 native 证据是资源类型的可观察析构，前端原用例为纯内存类型。

相邻的 nested Borrow/Inout 用例继续保留为旧产物见证。旧 `loans_ending_at` 查询
实际 CallReturn 结束事实，Unit 同名查询与 `end_span` 只标明建立描述符的调用。
Unit 普通直接 Borrow 有独立的实际结束切片；Exclusive 前缀切片限定为提前放弃，
未覆盖正常 CallReturn 的 Exclusive 结束。现有 Unit 描述符用例与 Single 结束用例
分别验证其所属合同。

最终本机 `ownership_checking` 29 项、`multifile_ownership_checking` 157 项和
`resource_deinit_native` 18 项通过，均零失败、零忽略。前端与 codegen 全目标 Clippy、
格式、文档和 Rust 尺寸检查通过。本片为下一批候选，双宿主 CI 尚未运行。

## 118. R2 稳定流事实的声明符号查找

原 BodyChecker 初始化为每个有名称的 `var` 在线性符号表中查找声明。一个文件
有 M 个此类声明、S 个符号时，该部分最坏检查 O(M × S) 个候选。检查器已有
按 source unit、声明字节范围和值命名空间的 BTreeMap 索引，改为复用同一个
`symbol_at`，此部分每声明一次 O(log S_total) 查询。循环或写回触发的重放仍
按原路径重建索引。未以此推断整个类型检查的复杂度。

原赋值和捕获负控保留，另加同名变量的遮蔽正控。内层 `var` 被闭包捕获，外层
同名 `var` 仍可收窄。MemoryDocument 与 Rooted Unit 同时核对原诊断码和精确
声明使用位置。独立复核提出两个相同文本的诊断可能互相掩盖，现直接比较两处
负控的 source-qualified primary span。静态核对了索引、同源、Value 命名空间、
缺失标识符恢复与 replay 初始化，未发现待处理问题。

本机两个类型套件共 378 项通过。生产改动的完整前端套件共 183 组、2898 项通过，
零失败、零忽略。精确 span 加强后的定向用例单独复跑通过。前端全目标 Clippy、
格式及文档检查通过。

## 119. R2 普通调用的实参完成性与类型形状

独立审查发现，普通调用只看声明返回类型判断能否继续。泛型重载唯一匹配后，
实参中的 return 会让 callee 根本不执行，外层分支合流仍错误丢失另一条路径的非空证明。
值分支还误把该调用的 Unit 当成交付给上层的值。两例都在当前源码先以 L0084 失败。
把 return 替换为正常 Int 实参会执行 inout 写回，须继续报后继 nullable 使用错误。

尝试把未完成调用的 expression type 统一改成 Nothing，扩大检查在容器及普通构造的
产物校验中失败，独立复核又确认会破坏链式调用、成员解析和外层重载的静态形状。
现保留已选择声明及表达式的静态类型，普通调用从实际 receiver/callee 与实参完成事实
决定是否有正常出口。仅有正常出口的表达式检查实际交付值是否满足 expected；
不完成的值 body 以 Nothing 进入分支合流。实参中的 if 条件若提前退出，if 的公开
完成性也须反映该条件。条件组合先在同一修补快照失败，再补上游完成性。

双 Unit 定向覆盖非空证明、值分支、链式函数值、成员及外层重载，并保留真实 inout
写回负控。原生值分支用例实际编译、链接、运行，true 返回 1、false 返回 2，
只有 ok 输出，证明提前退出不执行被调函数。独立探针核对后续实参的 L0084、
待定 L0165 及既有容器/所有权 primitive 描述符仍保留。

另有全候选失败的恢复路径。当前调用包装层和条件流重建仍会按源码 inout marker
失效，可能在 L0123 后附加 L0084。试探快照本身已恢复流事实。该事务外恢复边界
继续审查，不计入本切片的合法调用完成性证据。

本片消费已发布的 operand 完成事实，新增的上游检查限于 `if`。独立探针确认带和不带
else 的条件提前 return，以及普通条件仍可能到达函数末尾的 L0088 对照。另实测
`(when (return 1) { else -> 2 })` 仍可在同一调用组合中误报 L0084。带 subject 的
when 只从分支统计完成性，需要单独核对 subject、分支/nullable 描述符和后端消费。

最终本机完整前端 183 组、2901 项通过。后端直接调用前缀 4 项、Map provider
退出 76 项、原生资源 18 项通过，均零失败、零忽略。前端和 codegen 全目标
Clippy、格式和文档检查通过。上述是基于 `edaaa9b2` 的下一批本机证据。

## 120. R2/R3 when subject 的提前退出

普通调用组合中 `(when (return 1) { else -> 2 })` 原报后续 nullable 使用 L0084。
原待定写回夹具中的 `when (return)` 以及在 subject 内嵌一个私有已知提前退出的
when，均把死 arm 的待定赋值错误带到调用者。三条红测实际复现。现在公开完成性
只统计 subject 可完成时的正常出口。私有恢复另以 subject 和 arm 的实际可达出口
合并待定键，并保留非穷尽 unmatched 路径。死 arm 仍检查源码，原 `Unbound` 诊断保留。
正常 subject 的 inout 对照仍报 L0084。独立复核的活 arm、非穷尽及全部覆盖对照
验证了待定键传播与清除。

提前退出的 nullable subject 不发布用于进入 arm 的证明。Refine 准入先检查 subject
本身，refine 与兼容 lowering 都使用公开完成事实，在结果槽、nullable plan 和 arm
之前降低 subject 一次，并核对实际 `Diverged`。原生最小用例在只有前端修补时
仍 UnsupportedNode，随后走真实构建和运行验证。带 `null` 与 `else` 的 nullable
对照在普通返回时保留 descriptor。selector 自身提前退出是另一个已识别的完成性
边界，本片验证的是 subject 求值。

三套本机前端类型与所有权用例 538 项通过，零失败、零忽略。原生对照实跑三份输入。
直接 return、带完整 null/else 形状的 nullable 返回类型，以及同一函数先执行活的
owned nullable when 后再提前退出，均只输出 ok。用于构造兼容路径的 nullable Boolean
局部夹具先在既有初始化边界失败，最终换用已支持的 nullable 类参数。

## 121. R0 合批后的剩余直接调用

以合入 `4243ebe5` 后的候选 Rust 树清点。按已跟踪 `.rs` 中的直接调用形状计
文件数和匹配行，排除同一行的定义。引用可能来自测试字符串。注册边界见 §93、§105。

| 范围 | check_types | check_ownership | lower_scalar_file_with_entry |
|---|---:|---:|---:|
| frontend src | 31 / 41 | 13 / 13 | 0 |
| frontend tests | 54 / 87 | 33 / 45 | 0 |
| codegen src | 15 / 18 | 14 / 17 | 4 / 8 |
| 其他 crates | 0 | 0 | 0 |

这三名未检出显式 as alias。此范围未覆盖宏展开、跨行间接引用或仓外调用。
两条普通析构用例已移除旧路径。后续按真实断言分流测试消费者，现有旧身份负控和
正常 Exclusive 结束事件保留。生产 Unit 兼容 lowering 仍按各路由合同复审。

同树本机合流验收运行完整前端 183 组、2906 项。后端运行直接调用前缀 5 项、
nullable when 18 项、Map provider 退出 76 项和原生资源 19 项。以上均零失败、
零忽略。nullable when 首次过滤没有匹配，验收以更正过滤后的 18 项为准。
前端与 codegen 全目标 Clippy、格式与文档检查通过。

[PR #381](https://github.com/Halckon/Koven/pull/381) 合入 `ec8a7b9f`。基线
`4243ebe5` 与 head `57d9cac2` 在 [CI 42556170476](https://github.com/Halckon/Koven/actions/runs/42556170476)
验证。合成提交 `03e9921f` 的父提交匹配该 base/head，CI 树和实际合入树同为
`de8eb0b0`。双宿主 Check/Clippy、定向测试、预览生产及独立安装、CI Passed 和
Code CI Evidence 均通过。两端原始日志均实查六条回归通过，覆盖调用与 when 的
提前退出、待定写回、重绑与析构。Editor lexical 按变更范围跳过。

## 122. R2 被拒调用与条件流重建

在 `4243ebe5` 上，两组全失败重载会先报 L0123，再在后续非空读取误报 L0084。
普通调用恢复无条件应用语法 `&`，而短路条件会重新扫描写入。只修直接失效后，
`&&` 红测仍复现。现在使用同一 source-qualified 调用身份识别不提交的写入。
条件重建只省略该调用本身的标记，仍遍历 callee 和实参。标记随候选和循环试探回滚；
循环的检查前扫描及旧 Single 入口继续使用原保守语法路径。

独立复核发现部分明确拒绝的调用仍以 Deferred 锚供最终准入。成员 lambda、
普通参数适用性拒绝、具名参数错误与待定实参并存的原语，以及确定不兼容的 super
模式均保留原锚并明确标记。已知普通函数值的非法显式类型实参，以及由分组传递
或经成员接收者、索引接收者及下标传播的已拒 callee，也不提交写入。真正待决的
候选仍保守，已有上下文错误不会撤销
已选择调用的影响。

双入口测试覆盖普通调用、`&&`、`||`、成员、前置适用性、已知 callable 和
callee 链。在被拒父调用的 `&x` 与已成功实参调用的 `&y` 同时存在时，
只在后继 y 上保留 L0084。原语测试保留 L0120 且无成功事实。当前
`type_checking` 99 项、`multifile_type_checking` 283 项、`type_ownership_primitives`
18 项通过，零失败与忽略。原语自身的公开提前退出、selector 提前退出及旧
Single 回收另有合同边界。

最终候选的完整前端 183 组、2908 项通过，零失败和忽略。前端全目标 Clippy、
格式、文档和 Rust 尺寸检查通过。上述为 `4243ebe5` 上的本机验证。

## 123. R0/R3 Phase 3 成败语料消费者

原 `ownership-pass/basic.ko`、`ownership-fail/use-after-move.ko` 与原诊断 sidecar
逐字保留。迁移前在同一快照实际运行 Single、MemoryDocument、Rooted 三路。
两条 Unit 路径继续核对成功用例诊断为空，以及失败用例的诊断码、起止字节、顺序
和末尾换行逐字节一致。解析、名称和类型仍各自要求无诊断。

迁移后 `ownership_checking` 30 项、`multifile_ownership_checking` 156 项通过，
零失败、零忽略。独立复核确认保留文件发现和逐项断言。旧 helper 仍有其他消费者，
本片只完成这组语料的迁移。

## 124. R2/R3 when selector 的独立复审

复审者从冻结的 #381 候选 `57d9cac2` 在仓外构建前端和后端，并实际发射对象、
链接及运行。另从其 base `4243ebe5` 构建前端复跑两条 R3 阻断。
以下为确认发现，修复与验收另记。

| 合同 | 实际证据 |
|---|---|
| 公开完成性保留逗号前已有匹配边，截断 selector 退出后的未匹配边 | `when (flag) { true, pred(return 7) -> return 2 }` 当前误报 L0088，subjectless 同样。共享 arm 改成普通值，或 selector 改 `pred(7)`，仍有正常出口并应报 L0088。普通标量 refine 的已有值分支对照两路输出 `2\n7\n` |
| 私有恢复保持 subjectless selector 的真假路径 | `flag() && pred(return)` 的 false 路径可 miss，true 路径无后继。当前死 arm 的待定赋值仍污染调用者 |
| 已匹配分支需要的纯内存 owner 必须保持 live | 无 deinit 的 own 参数只在先匹配的 arm 被读时，当前错误发布 FunctionEntry drop。局部 owner 在声明后被释放。后端报 InvalidModel。把提前退出 selector 移到首项的对照实际输出 `7\n7\n` |
| 短路 false bypass 仍需检查 move | `consume(x)` 后，`when { flag && pred(return 7) -> 2; else -> { observe(x); 3 } }` 漏 L0131，原生 InvalidModel。改 `pred(7)` 恢复 L0131 |

own 参数提前析构和 false bypass 漏报都在 #381 的 base 实跑复现，属基线既有阻断。
词法资源析构的原生对照通过，纯内存 ASAP 的缺陷由此独立定位。
`when` 的回向活性需按 entry 和 alternative 递推，在 selector 正常出口
分别合并匹配 arm 与继续匹配的活性。基础所有权的短路 false 边须单独复核。

另有下游边界。已支持的 `in asInt(return 7)..2` 报 MissingFact，正常对照
`in asInt(1)..2` 实际输出 `2\n2\n`。外层 canonical nullable descriptor
的嵌套 arm 当前完成性错误，须与所有权计划对齐。兼容 lowering 对动态 selector
的正常对照也不支持，按既有能力边界单列。

## 125. R3 when 匹配路径上的 owner 活性

`30886cde` 基线上，逗号前匹配的 body 借用纯内存 owner，后续 selector
提前 return 时，前端红测只取得错误的 FunctionEntry drop，原生红测报 InvalidModel。
改为逐 entry、逐 alternative 回向传递。每项正常出口分别合并匹配 body 和后续 miss，
提前退出只截断该 selector 的正常后继。String subject 的隐式读取留在每个候选的
正常出口。分支入口事实仍保存对应 body 的活性。

独立复核发现 while 条件在每条分支都替换退出边仍需读取的 owner 时，旧循环头初值
会导致固定点无法收敛。修前实际限时 8 秒退出 142。现从空集迭代，条件出口继续
注入 live-after，保留 break/continue 的各自目标。两种 Unit 均实跑通过。

本机所有权及迭代前端 388 项、后端 comma when 5 项及循环资源 9 项通过，
零失败和忽略。原生 subjectful 和 subjectless 对照实际编译、链接并运行，精确
验证先匹配时读取 owner，后续提前退出时不执行 callee。最终差异经独立复核。

与 `ec8a7b9f` 合流后，五套类型、原语和所有权前端共 591 项通过，
零失败、零忽略。

## 126. R3 普通布尔短路的正常后继

在 `152c57ea` 上，RHS 提前 return 会使普通 Unit 丢失 `&&` 的假旁路和
`||` 的真旁路。可达分支读取已移动 owner 时漏 L0131。单纯合并两边又会
引入实际不存在的路径。双入口红测实证必走 RHS 后的死语句、不可达分支以及
没有条件假出口的 while 会误报。while 有实际 break 时，后续读取仍须检查。

类型检查在试探全部结束后，按 source-qualified expression 发布布尔正常结果。
各 profile 以表达式表插入顺序构建，查询不递归扫描子树。Basic 复用已有的分组、
取反字面量必走 RHS 判断。独立复核发现重新初始化已移动 owner 后的虚假旁路，
新增红测实际在 `true &&` 上报 L0131。修复后两种必走形式继续，动态形状
保留可能绕过初始化的 L0131。Const 保留原结构结果及独立执行计划。

数据流、活性、析构计划、默认 refine 与 Unit 兼容 lowering 使用相符的正常边。
Basic 正常求值 RHS 使用 BranchExit 0，旁路使用 1。已知不完成的 RHS 只发布
实际退出的清理。true-only while 的回边供下一轮，不虚构条件假出口，实际 break
仍进入正常后继。旧字面量旁路的既有清理合同保留。

独立复核另确认，callable 实例规划按词法位置收集调用，动态 RHS 退出造成的死 arm
仍可能索要 callable provenance。该形状在 `152c57ea` 的所有权遍历已无该 provenance，
保留为下游规划复审项。Const Conditional `||` 的 refine builder 既有边编号
也需与专用计划单独对账。

本机三套前端定向用例 385 项通过，零失败、零忽略。三条新增原生测试覆盖
`when` 两种旁路、六组 if/while/嵌套短路在默认与强制兼容路由的执行结果，
以及两条必走 RHS 的 owner 移交和析构。析构夹具首跑因遗漏 `deinit()` 的
括号在解析失败，修正后两路均输出相同的两组 take/drop 和最终 ok。
最终逻辑差异经过独立复核。最后一轮完整前端本机运行因复核新发现而提前停止，
本节以修复后的定向实跑和后续双宿主候选证据为准。

[PR #382](https://github.com/Halckon/Koven/pull/382) 已合入 `ac9a7d81`。
[双宿主 CI](https://github.com/Halckon/Koven/actions/runs/42562208549) 的合成提交
`964e977e` 与实际合入树均为 `0fd9108e`。Check/Clippy、定向、预览生产和独立安装、
CI Passed 与 Code CI Evidence 均通过，Editor lexical 按范围跳过。两端原始日志
各核实 11 条指定回归通过，包括调用退出、写回、析构和本批新增短路用例。

## 127. R2/R3 when selector 的完成性与写回

在 `ec8a7b9f` 上，两种 Unit 入口均把逗号后 selector 提前 return 的路径误计为
可继续，导致 L0088。新红测直接比较公开 expression completion。修复在 selector
停止时仅截断尚未匹配的路径，保留前序 alternative 已送达的 arm。普通匹配仍
可进入有值 arm，正常未匹配出口仍要求后续 return。value-context 仍执行语法穷尽
和原分支类型检查，再为新增无正常结果的 selector 形状发布 Nothing。提前退出的
subject 保留已有静态类型。

私有写回单独跟踪 match/miss 待定键。subjectless selector 复用现有 Boolean 真/假
边，穷尽且正常返回 Boolean 的 when 也向外发布实际 arm 的真假待定出口。短路
死 arm 和嵌套 when 死 arm 原错误污染调用者的 L0165，修后只保留源头 Unbound。
活 arm 仍传播写回，公开 nullable smart-cast 的有写入和无写入对照保持原合同。
独立复核核对试探快照、Const 和非穷尽边界。

refine 准入接受已验证不完成的 Nothing when，构建不分配结果槽。Range selector
以已验证的端点求值顺序传播退出。旧原生红测报 MissingFact，修后下限提前退出不求值
上限，上限提前退出保留已完成的下限调用，正常范围匹配仍返回原值。独立原生探针
使用带析构的端点和外层 owner，验证 `in ..`、`!in ..<` 的实际释放顺序。
canonical nullable 两组实测 typed/owned entry 完成性分别对齐为 `[false,false]`
和 `[false,true]`。独立对照保留死源码 L0112、L0084 和缺 else 的 L0111。

本机 types 两套 384 项、Unit 所有权两套 181 项和两条新增原生回归通过，零失败与
忽略。前端与 codegen 全目标 Clippy 通过。原生对照使用默认路由，动态 selector 的
兼容 lowering 仍按既有能力边界审查。
后端 nullable when 定向 18 项通过，零失败与忽略。旧大文件拆出 selector 检查和
私有边归并，Rust 尺寸检查通过。

## 128. R0/R3 循环资源待定的 Unit 消费者

原循环夹具逐字保留。迁移前原 Single/Rooted 断言实跑通过，另以已构建的 Unit
前端直接验证 MemoryDocument 与 Rooted 都发布 ResourceLifetime。迁移后循环夹具
只走这两种 Unit，分别要求无诊断、ResourceLifetime、无 drop，并拒绝所有权准入。
相邻条件资源差异见证仍核对 Single 不待定，同时将 Unit 观察补为两入口。

原 readonly 否决沿用相同阶段和空 drop 断言。独立复核指出验证需消费 ownership
产物，修为回调接收产物本身。最终 `ownership_resource_deinit` 31 项通过，
零失败与忽略。

## 129. R4 实例规划的调用与 lambda 进入事实

独立仓外探针在 `ec8a7b9f` 证实死分支的 callback 调用通过前端后于 native
UnsupportedNode，活分支对照原生输出 live 和 ok。在 `f324b736` 的 when 组合中，
定向红测同样在死 callback 报 UnsupportedNode。另一个红测在死 lambda 的资源构造
处被析构实例规划拒绝。

Unit 所有权产物保存实际进入求值的 source-qualified call 和实际检查 body 的 lambda。
实例索引先校验 source/AST ID，随后跳过未进入的调用。实参或 receiver 退出之前已进入的
调用仍保留 route，实际调用的 Nothing 目标也不被当作死调用。活调用继续检查原 callback
来源和完整实例。析构规划排除未进入 lambda 内的资源根，已形成的 lambda 仍受原资源
与 hidden deinit 限制。Basic 与 Const 消费各自通过准入的同一份所有权分析。

本机 182 项前端所有权与 Const、13 项 callable 规划及 7 项 comma-when 测试通过。
新原生回归覆盖 Basic、Const、强制兼容及死资源 lambda。选中活 Resource callback 的
SSA 和 LLVM 对照仍调度 hidden deinit。活来源不明的规划负控保留。独立复核未发现
本片阻断。旧大文件只增长进入事实字段与交接，Rust 尺寸检查另核。

在实参求值中退出且后续仍有 callback 的已进入调用，会在既有特化流程中索要后续来源；
已单列前缀路由复审。其他词法扫描型类型规划仍按自身能力边界验收。

与 `ac9a7d81` 合流后，五套前端 605 项通过。后端 comma-when 8 项、callable
规划 13 项、Unit 短路与前缀 3 项通过，均零失败与忽略。前端与 codegen 全目标 Clippy、
格式、文档和 Rust 尺寸门禁通过。

PR #383 首轮 [Linux 定向门禁](https://github.com/Halckon/Koven/actions/runs/42565646073)
的 codegen 全量有 10 项失败。三项 Range 为真实准入回归。refine 准入仍会检查 Const
计划为 Never 的 RHS，因死调用无 route 而改走兼容路径。准入现与构建读取同一份 Const
计划，并继续核验左项和活 RHS。六项 Map 与一项 sequential 的旧 SSA 断言仍要求发出
只被死调用引用的函数。改按原声明 Origin 精确核验函数有无，保留活调用身份、Map owner
传递、逐返回析构、Unit 紧随调用及 Abort 的断言。Linux 首轮相应原生 Map 正控仍通过。
本机 Range 定向 6 项、Map 定向 42 项及 sequential 前缀 1 项通过，零失败、零忽略。
修复后分别复核了 Const 准入和 SSA 断言。

[PR #383](https://github.com/Halckon/Koven/pull/383) 已合入 `62ca42cf`。
[双宿主 CI](https://github.com/Halckon/Koven/actions/runs/42567269515) 的 head
为 `1822b960`，合成提交 `50d74fc8` 与实际 main 合入树同为 `41d20fba`。
两端 Check/Clippy、Targeted、预览生产及独立消费、CI Passed 与 Code CI Evidence 均通过。
Linux 后端 2743 项、macOS 后端 2740 项通过，均零失败。macOS 真实 LLDB 用例按权限边界
忽略 1 项，Editor lexical job 按变更路径跳过。两端日志均核对 selector、Range、调用进入
与强制兼容死 callable 回归的实际通过记录。Linux native sanitizer 通过。

## 130. R3/R4 Const 短路析构边编号

在 `ac9a7d81` 上，Const subjectless when 的条件 OR 在跳过 RHS 时应释放仍由
调用者持有的 String。原生红测实际报 InvalidModel。所有权计划以 0=true、1=false
编号，OR 的正常 RHS 是 1。refine 改为从同源的 Const 计划读取 RHS 编号，以互补
编号消费 skip 的清理事实。Basic 保持 RHS 0、skip 1。

本机实际编译链接运行 AND、OR 两种形状。各自覆盖正常 RHS、旁路与 RHS 实参
提前 return，精确断言结果和输出。提前退出的 RHS 不调用 callee。Const SSA 套件
27 项、前端 Const 所有权 23 项通过，零失败与忽略。codegen 全目标 Clippy 通过，
独立复核核对真实清理点、Const 提前退出及 Basic 隔离。

## 131. R3/R4 已进入调用的 callback 实参前缀

在 `ac9a7d81` 上，具名实参按源码顺序执行 tick、return，再遇到未求值 lambda 的
原生红测于实例规划报 UnsupportedNode。所有权在源码实参产生正常值时记录该实参，
planner 仅为完成的 callback 登记来源，已进入的外层调用仍保留 route。活但来源不明
的 callback 沿用原 UnsupportedNode，独立原生正控核对方法 receiver 与正常具名调用。

callback 实参本身提前 return 且外层为 subjectless when 时，第二个原生红测复现
refine 准入失败。refine 现在按源码实参顺序检查实际前缀，已知不完成的实参本身仍
求值，不检查后继实参的 lowering。没有正常值的 callback 走既有的退出求值，不构造
view。保留实参借用与临时值的原退出清理路径。

本机原生验证后续 lambda 未形成、callback 实参自身 return、已建立 inout 的前缀退出
以及活 callback 正常执行。13 项 callable 规划与 185 项 Unit/Const 所有权通过，零失败、
零忽略。前端与 codegen 全目标 Clippy 通过。独立对照实跑 Basic、Const 与活 receiver，
并核对源码实参顺序与 ABI 槽位分离。已进入但不 invoke 的通用实例调度、receiver 自身
退出以及已形成 callable view 的兼容前缀仍需复审。

## 132. R3/R4 普通成员 receiver 先退出

在 `cad9f6de`，`make(return 7).apply(lambda)` 通过前端后于原生构建报 MissingFact。
同源 typed receiver 已明确无正常结果，所有权没有建立 receiver loan。refine 准入、
构建与 Unit 兼容调用现先消费该事实，实际求值 receiver 并核对退出，不进入成员分派。

原生测试分别核对默认 refine 与强制 Unit 兼容路由。receiver 前缀确实打印，早退路径
不调用 make、普通实参函数或 callback；正常 receiver 对照依次打印 receiver、argument
与活 callback。13 项 callable 规划和 3 项 Unit 短路与前缀测试通过，零失败、零忽略，
codegen 全目标 Clippy 通过。独立复核核对了阶段身份和活 receiver 原准入。Range carrier、
borrow-return 专用入口与 nullable proof 仍按各自原有边界。

## 133. R3/R4 已求值前缀与实际进入 callee

Guide 10 要求按源码顺序求值并应用实参合同。在 `e526236a`，`dormant(tick(), return 7, lambda)`
的红测确认默认 SSA 仍生成永远不会执行的 callee，两条 lowering 路由共用该实例计划。
所有权现按源码身份合并可能进入 callee 的状态，在处理其 `Nothing` 结果前记录。
实例规划保留前缀的路由、委托和已完成 callback 的 view，明确无法进入时跳过目标函数体。
Recipe frontier 仍核对当前边的 receiver 与继承合同，停止遍历该目标模板。

独立复审发现 nullable when 已提取 owner 的前缀仍要求 callee ID，新增红测复现
MissingFact。兼容路径现在只在有分派时验证该 ID，保留早退处的析构。
已有 callback view 的前缀须先结束 view 和根 loan，再释放临时 closure。
带括号的捕获 lambda 另复现 MissingFact，refine 用现有临时值登记保留原实参身份。

本机原生通过 3 项测试，覆盖默认 SSA 省略目标、强制兼容路径保留 tick 输出、
带和不带括号的捕获 closure，以及 nullable when 的提前析构与 null 正控。
14 项 callable 规划通过，两组均无失败或忽略。独立复核核对了 return/break/continue
的 pending loan 顺序与分组临时值登记；本轮原生用例实跑 return。

具名 `Nothing` callee 的正控覆盖实例计划。其 native ABI 仍待独立修复。
未访问的后缀调用由 §129 的进入事实处理。Nullable If 和借用结果的专用准入范围按原合同保留。

与 `62ca42cf` 合流后，联合用例区分未进入的后缀、已进入但早退的前缀和实际分派，
并分别核对已完成与未完成的实参。同一合流树的三套前端测试共 472 项通过。
3 项前缀原生、14 项 callable 规划、10 项逗号 when 及 1 项本机 LLDB 真断点通过，
均零失败、零忽略。独立复核确认两组调用事实和进入 lambda 的来源都保留。

具名 `Nothing` 的独立探针在 `563049a1` 上通过公共 CLI check，原生 build 以
UnsupportedNode 退出 2。后续回归同时要求早期实参输出、真实 callee 输出、
callee 内 abort 源位置，以及跳过后续实参和栈析构。

PR #385 首轮[双宿主 CI](https://github.com/Halckon/Koven/actions/runs/42570876471)
在同一条 two-phase receiver 断言失败。原断言先查找只被无分派前缀引用的 `update` 定义，
已通过的零 exclusive activation 检查之后命中 `unwrap`。改为要求该定义缺席，
前缀 `read` 恰好调用一次，并保留 LLVM 验证。独立复核核对了 class、value class
及 return、break、continue、abort 各路径，邻近的正常激活与 CFG 合流对照继续覆盖。

PR #385 精确头 `d37dabbf` 的[实际双宿主 PR 门禁](https://github.com/Halckon/Koven/actions/runs/42572582506)
通过 Check/Clippy、定向测试、预览包生产及独立消费和最终 CI Passed。
定向测试 macOS 2747 项通过，1 项 LLDB 因调试权限忽略；Linux 2750 项通过，零忽略。
本机 LLDB 真断点证据见上。合并提交 `415ba277` 的树与门禁中的合并树一致。

## 134. R4 具名 Nothing 的普通直接调用

在 `62ca42cf` 上，公共类型阶段接受具名 `Nothing`，实例计划保留真实调用；
原生红测在声明返回 ABI 时报 UnsupportedNode。typed SSA 现在区分普通无值返回和
无正常返回。直接调用沿原路径求值、激活 receiver 并交付实参，随后以 Unreachable
终止该块。LLVM 发出实际 call 与 unreachable，abort 的消息和源码位置仍由 callee 产生。

verifier 要求无返回 call 是块的最后一条指令，且仅该形状可接 Unreachable。
无返回的声明不能有正常 Return、存储结果或借用结果。函数地址、thunk、进程入口和
析构钩子不能以空返回槽将其作为普通 Unit 签名接收。独立复核发现析构钩子旁路和 Refine
提前拒绝；新增负例复现注册旁路，现注册与 verifier 重检共用修正，Refine 准入和路由共用
具名调用判断。普通 `error()` 保持原路由。

本机 3 项具名回归通过。顶层调用在默认和强制 Unit 兼容路由保留前缀输出，方法保留
receiver、实参与 callee 的顺序，均核对 SIGABRT、stderr 源位置和未执行的后续副作用。
另以递归具名调用核对 Refine 路由与 SSA 无正常后继。52 项 deinit 相关测试、109 项 SSA
验证测试和 1 项内建 error 原生对照通过，零失败、零忽略。全工作区全目标 Clippy 通过。

独立探针 `fun stop(): Nothing { loop { continue } }` 在进入后端前于 Unit 类型阶段
报 L0088。循环完成性需单列 R2 核对，间接 `Nothing` 的 callable ABI 仍需复审。

## 135. R2/R3/R4 循环完成性与穷尽 when 前缀

`8151374d` 上，两个 Unit 公共入口都为无 break 的 `loop {}` 报 L0088。
普通完成性现在在当前循环 frame 单独收集可能执行的 break，与待定写回的布尔裁剪隔离。
语法和类型检查仍访问死语句。语句、调用实参、when selector 和短路控制先消费前序
完成性。while 复用结构布尔边，假边及本层 break 才能离开循环。
这份普通证明不折叠字面条件。失败候选及回边试算回滚带源 identity 的事实与 frame。

独立复审确认 `loop` 接受后，穷尽 Boolean when 的分组 selector 与穷尽后的 else
原会在 Refine 重建伪 miss 或 break。有限域第一次耗尽时，Unit 类型阶段现发布 arm 和
selector 的可达前缀。所有权、活性、析构计划、Refine 准入和构建共享该边界。
源顺序及原 arm、selector 的索引保持，前缀中已匹配的分支继续独立执行。

Const Refine 的 `Never` 计划原会从固定左值再建两条边，导致新版完成性暴露 MissingFact。
构建现在实际求值左侧并按计划进入唯一后继。Refine 的 if/while 红转绿，
原生 AND/OR 对照验证后续跳转未执行且返回值正确。独立复核核对原 loan/view 的边界。

本机三套 Unit 前端 474 项、4 项控制完成性 SSA、28 项 Const SSA、10 项 comma-when
和两个新增原生用例均通过。SSA 计数含一条重复覆盖的 Const 完成性回归。
原生覆盖死 arm 与死 selector 的重复消费、真实分支返回和资源逐次析构。
所有权计划另核对仅死 arm 使用的 owner 在绑定后的 AfterStatement 析构。
全工作区全目标 Clippy、格式、文档结构与 Rust 尺寸门禁通过。
旧 Single 检查器的普通循环完成性仍需随真实消费者复审。

与 `415ba277` 合流后的独立复核核对已完成实参、实际分派与无返回终止，
以及 receiver/临时借用退出、Const 固定边和穷尽 when 的可达前缀。
本机合流树的 4 项控制完成性 SSA、13 项原生控制流、1 项两阶段 receiver、
3 项具名 Nothing 和 16 项 callable identity 测试通过，零失败、零忽略。
具名 Nothing 计数与控制完成性 SSA 共享一条回归。
同一合流树的 Unit 类型、所有权与 Const 所有权三套共 474 项通过，零失败、零忽略。

PR #386 精确头 `081549cd` 的[实际双宿主门禁](https://github.com/Halckon/Koven/actions/runs/42577215257)
通过 Check/Clippy、定向测试、双宿主预览包生产及独立安装、Code CI Evidence 与 CI Passed。
15 项通过，编辑器词法门禁按改动范围跳过。`main` 合并提交 `e87de0ac` 的树与
CI 检出的 `f0182570` 一致。

## 136. R4 间接 Nothing 的 callable ABI

在 `081549cd` 上，公共 CLI check 接受无捕获 lambda、共享捕获 closure 和借用
callback 中的 `() -> Nothing`。三者原生 build 均退出 2，lowering 报
UnsupportedNode。三个原生回归均在生成目标文件时失败，错误位置落在实际 lambda。
红测要求保留 callee 输出、SIGABRT 与原始 error 源位置，并跳过调用之后的输出。

待核合同覆盖函数指针、具体 closure 和 callable view。无正常返回必须通过已验证的
callable 签名传递，普通 Unit 无值返回继续独立验证。调用按原顺序求值并交付实参，
实际间接调用之后无正常清理或后继。SSA 验证器需拒绝把可正常返回的调用当作
Unreachable，也需拒绝把可正常返回的 thunk 绑定到无返回签名。LLVM 适配需核对
实际调用、callback trampoline、无返回属性与终止指令。

本批实现保留前端 callable result 的无返回事实。声明、函数指针、具体 closure、
callable view 与 thunk 共用完整签名。SSA 验证不允许正常 Unit thunk 形成无返回指针，
也拒绝无返回调用之后接正常后继或正常调用后伪造 Unreachable。
LLVM 发出实际 indirect call、noreturn 与 unreachable。借用 callable view 的指针
trampoline 同样保留该合同。无捕获、无参数 thunk 的保留 lowering 路径已实跑。

独立复核发现普通和专用调用在中止实参后泄漏构建期 loan。普通调用、List size 和
Map receiver/key 的红测均在候选上复现 InvalidModel。表达式构建入口现在在
Diverged 后丢弃该表达式新增的 pending loan 记录，已生成的控制转移清理不变。
本机原生验证 while 正常绕过、普通嵌套调用、List 与 Map 前缀，以及 Const 实参
break 的资源计数和未实际分派。Unit initializer 与 Map action 的两条 SSA 负控先
复现误接纳，再验证运行时消费者拒绝无返回签名。独立复核已核对修正后的签名和 loan 边界。

本机间接 Nothing 原生 7 项、SSA verify 112 项、borrowed generation 28 项和
Unit closure 10 项通过，零失败、零忽略。相关拒绝过滤器的 3 项通过，与 SSA 和
generation 计数有重叠。
全工作区全目标 Clippy、格式、文档结构与 Rust 尺寸门禁通过。


## 137. R4 普通调用实参内的循环与前缀临时值

在 `50bdf49a` 上，公共前端接受先借用新 String，再在后续实参的 if 分支中求值 while 的程序。
原生红测在 while 语句报 UnsupportedNode。普通声明调用已通过其专用前缀准入，
循环只查询旧通用能力，仍拒绝被合法实参借用的临时 owner。默认路径的红测和带
Group 的强制兼容路径红测分别复现同一拒绝。

构建在已有直接前缀校验成功后，只把已求值的实参临时源身份登记在当前调用帧。
循环资格仍从既有通用能力或现存调用帧取得，检查所有待携带的临时 owner。
透明 Group 的别名按当前 CFG 中相同的 owner ValueId 识别，已离开调用帧的身份不能
开通后续循环。CFG 携带、实参交付、控制转移借用终止与析构仍由原槽位和前端事实决定。

两项原生回归通过，分别走默认和强制兼容路径，实跑循环内 return、循环正常绕过后的
return 和调用实际交付三条边。间接 Nothing 原生 7 项、旧 Unit 循环 7 项、
while proof 4 项通过，均零失败、零忽略。独立复核确认透明 alias 重绑定与无关 owner
拒绝，格式、文档与 Rust 尺寸门禁通过。
同一候选的全工作区全目标 Clippy 和 CLI 构建通过。

PR #387 精确头 `498b4b6b` 的[实际双宿主门禁](https://github.com/Halckon/Koven/actions/runs/42581057194)
通过 Check/Clippy、定向测试、双宿主预览包生产及独立安装、Code CI Evidence 与 CI Passed。
15 项通过，编辑器词法门禁按改动范围跳过。`main` 合并提交 `f2bef461` 的树与
CI 检出的 `187de685` 一致。其[主干编辑器门禁](https://github.com/Halckon/Koven/actions/runs/42583721121/job/129048425878)
完成实际安装与词法测试。上一主干 `e87de0ac` 的两次编辑器安装均因上游下载 HTTP 504 失败，
该提交的双宿主编译器与预览包证据均通过。

## 138. R4 值分支没有语法尾表达式时的终止交接

在 `498b4b6b` 上，前端接受后续实参中的
`if (outer) { loop { return } } else { true }`。原生红测在 loop span 报 MissingFact。
兼容层在构建值分支前就要求尾表达式身份，尚未获知该分支是否正常继续。

兼容层现将语法尾表达式读取为可选项，继续用已知尾身份识别 String? 的 null
物化。实际 lowering 得到 Diverged 时不生成分支正常出口或尾值交付。正常继续并
交付值时仍需原表达式身份，正常 Unit 仍无法交付要求的值。
独立复核核对了退出、继续、null 尾值桥接、临时值边界和 nullable 专用准入。

强化后的原生回归在默认与强制兼容路径通过，验证退出边不分派、继续边传入正确
Boolean 并打印前序借用的 String。另有 19 项 return-control 测试及 String? guard
定向测试通过。lang-codegen 全目标 Clippy、格式、文档和 Rust 尺寸门禁通过。


## 139. R0/R1/R2 Phase 2 夹具迁移与 L0088 闭合身份

`50079c7b` 的旧 Single 夹具基线通过。将原 10 份成功和 11 份失败源码移到真实
MemoryDocument 与 Rooted raw type 入口，保留解析、名称、源码身份和全部有序
错误码及字节范围断言。两路对照只发现两项差异。顶层普通 val 的 L0167 符合
Guide 05，侧车补一行并保留三项 L0142。L0088 的原侧车指向真实右花括号，
Unit 实际输出在括号后，修正生产者并保留原基准。旧入口的独立身份与恢复见证仍执行。

恢复独立复核发现两种 span 歧义。缺外层闭合但最后子块闭合时，原类型诊断停在
最后有效 token，Parser 插入点在实际 EOF。缺 while body 时，错误节点又可借用
真实函数右花括号范围，不能用 child 与 block 的结束位置相等判断缺失。两种情况
先由同一真实恢复入口红测复现。Parser 现在只为缺闭合的 block 保存其实际恢复点，
按新 StatementId 顺序写入稀疏表。函数体 L0088 共享选择器读取此事实，真实闭合时
定位右花括号。解析结构和诊断保留，返回类型 label 保留。
迁移所属合同见 [SPEC-0341](0341-shared-lowering-core.md)。

最终恢复回归覆盖真实闭合、带 CRLF 尾部空白的 EOF，以及词法错误抑制 L0010
时的 EOF，均核对返回类型 label。复核确认稀疏表有序且型参试探不会写入。
原 block dispatch 复杂度测试归入现有 separator 模块，保留原操作计数与断言。
前端单元 397 项、type_checking 102 项、multifile_type_checking 287 项、
parser block continuation 11 项、CLI check 6 项及 LSP 单元 56 项通过，零失败、
零忽略。前端全目标 Clippy、格式、文档和 Rust 尺寸门禁通过。

本机 LSP stdio 格式化 integration 经 Cargo 启动时，在候选和同环境 `498b4b6b`
基线均报响应超时。直接运行各自生成的同一测试二进制，两边均通过。
候选本机协议探针也实际完成单文件与项目初始化、格式化、更新、关闭和退出。
Cargo 下的本机超时原因未定位。[PR #388 精确双宿主 CI](https://github.com/Halckon/Koven/actions/runs/42585077455)
中，Ubuntu 与 macOS 原始日志均实际执行并通过该 stdio 测试，各 1 项且零忽略。双宿主定向
与预览包生产及独立消费门禁通过。合入 `d193a65f`，树与 CI 合并候选 `04150257` 一致。
[实际主干 CI](https://github.com/Halckon/Koven/actions/runs/42589677593) 独立跟进。

## 140. R0/R3 Phase 3 结构与闭包消费者

延续 §123 和 [SPEC-0341](0341-shared-lowering-core.md)。在 `8162d27e` 的
Single 基线上验证两对结构与闭包成败夹具，以及 nullable when 的未选分支不移动
owner 用例。原四份源码、两个失败侧车和 nullable when 输入均保留，使用相同
环境改走 MemoryDocument 和 Rooted。两种入口均在进入所有权前核对源码身份和
前置阶段。失败侧车继续逐字核对有序诊断码与字节范围，并校验来源文件名。

独立复核确认保留 Single 的结构投影、闭包捕获、借用终止与析构见证。结构、
闭包、所有权及 multifile ownership 共 277 项通过，零失败、零忽略。前端全目标
Clippy 通过。本批只改测试消费者，R0/R3 的阶段退出仍开放。

## 141. R2 常量依赖失效与普通类型恢复

在 `0c5fbc99` 的跨文件 Unit 红测中，另一文件函数的非法 `List<Nothing>` 签名
让独立常量环的 L0157 消失，只剩 L0125。最终回归同时覆盖独立整数除零的
L0158。全单元签名短路改为按实际常量注解范围标记无效依赖。错误注解继续
接受普通 initializer 检查，但不进入环检测和求值。

独立复核复现初版跳过依赖者的普通诊断。现仅把实际检查过的 initializer 标为
已处理。失效后仍未检查的图节点继续进入普通恢复，保留 L0085 和跨文件私有
引用 L0154，同时跳过常量资格与求值。反例覆盖非法注解形成的环、无效注解与
求值失败各自的依赖者、输入文件重排和两种类型能力均不可发布。独立复核已
核对普通恢复、SCC 过滤和常量事实发布边界。

五个受影响的类型、常量选择和所有权集成套件共 358 项，前端单元 397 项通过，
均零失败、零忽略。前端全目标 Clippy、格式、文档和 Rust 尺寸门禁通过。
表达式自身资格不成立且同时引用失效依赖的诊断优先级仍需单独复审。

## 142. R2 求值失败与下游常量资格

在 `1da3b00e` 的跨文件红测中，提供者除零只报 L0158，吞掉消费者独立的
Boolean 相等式 L0156。普通类型及常量资格在求值前检查。通过这些检查的节点
即使没有值，其类型仍可供依赖者验证。只有依赖值齐全才进入求值，值失败在
SCC 建图前沿反向边传播，阻断派生环诊断。

独立复核确认按实际依赖值准入，普通类型及资格失败仍即时传播。回归覆盖
短路 RHS 中的独立资格诊断、多跳值缺失、环内资格、派生环抑制，以及缺值读取前
本可触发的取模错误。输入文件重排保持结果，错误单元不发布常量能力。

五个相关集成套件共 360 项通过。强化后常量资格 21 项与前端单元 397 项通过，
均零失败、零忽略。前端全目标 Clippy、格式、文档和 Rust 尺寸门禁通过。

## 143. R1 父类型与委托恢复的账本对照

在 `2060a194` 实跑 class family 88 项和 interface delegation 28 项，零失败、
零忽略。§92 的 `I async Junk<A, β>, J` 已由 §100 修复并实际回归，核对
L0002、L0001、尾随词法范围、真实后继 `J`、类体成员和下一文件声明。§101
记录的直接无效委托目标残项已由 §103 覆盖。§103 后续也验证了合法目标后非法
普通组及角列表的恢复，File 与 Declaration 均保留原目标、L0077 和真实后继。

仍需按未闭合或无证分组的实际输入审查边界。`Junk<1, B>, J` 缺少可信的类型
列表证明，现有负控验证内部片段不被提升为新父类型。本节复跑范围为两套件，
R1 的其它 AST 归属合同继续开放。

在同一基线新增 `I by delegate + { -> 1 }, J` 红测。File 和 Declaration 中，
原先把显式 lambda 当类体，产生附加成员诊断并丢失后继 `J`。复用严格 lambda
header 和现有花括号配对索引，只有后续真实逗号或类体 opener 提供佐证时才把组
保留在错误尾部。普通扫描仍处理组内 lexer 终止事件，L0077 及原目标范围保留。

独立复核找出直接尾随路径的换行归属差异，红测验证初版吞掉原类体成员。现直接
尾随仅在同行启用，已有非法运算符后的右操作数仍可换行。回归另核对多参数
header、无后继父类型时的真实类体和坏字符串的 L0004。保守范围继续排除无法
证明右花括号归属的输入，以及右花括号后接调用等非直接 header continuation。

最终 class family、contextual TypeRef 和 interface delegation 共 127 项通过，零失败、
零忽略。supertype suffix 的 4 项内部测试通过，含 32→64 段直接尾随和运算符后
lambda 的显著 token 访问计数。前端全目标 Clippy、格式、文档与 Rust 尺寸门禁通过。


## 144. R1 显式 lambda 后接调用的委托恢复

以 `e25f3aa0` 为基线，`I by delegate { -> 1 }(), J` 在 Declaration 红测
报 L0071 与 L0013，后继父类型不在原列表。复用配对索引跨过同行的连续调用组，
末尾仍须为外层父类型逗号或类体 opener。前瞻只返回 lambda closer，主扫描继续
消费各调用组和 lexer 终止事件，保留原委托及首个非法 token 的范围。

独立复核发现两处误归属。跨行 `(` 会吞掉原类体，已由红测复现。前置未闭合
普通分组时，内层逗号不能证明外层父类型续接。修订核对每个调用与前一 closer
之间的源码换行，且只在普通分隔符和词法 owner 均处基线时采用证明。最终独立
复核通过。File 与 Declaration 正控覆盖连续调用后接逗号或类体，负控覆盖
调用缺右括号、换行及含换行注释、前置未闭合分组，并核对实际类体成员。

[PR #389 原候选](https://github.com/Halckon/Koven/actions/runs/42590241257) 的
head `e25f3aa0`、base `d193a65f` 有 15 个实际成功作业，编辑器门禁按改动范围
跳过。主干随后合入 SPEC-0341 的 PR #384。新候选以 `14297820` 为 base，
把本节修订纳入同一批，重新执行精确双宿主门禁。

新合并树本机执行前端单元 408 项及 11 个集成套件 846 项，均零失败、零忽略。
集成范围覆盖 Parser、常量、Unit 类型与所有权、直接调用 loan end。包含 32→64
组带调用链的显著 token 访问计数。前端全目标 Clippy、格式、文档与 Rust 尺寸门禁通过。

## 145. R3 借用观测的根来源索引

在 `14297820` 上，direct、receiver 与 borrow handoff 的观测谓词按候选重复扫描
已形成 owner、普通及 Map 迭代和借用结果绑定。32 个函数各形成同名但不同 ID 的
owner 并实际发布 receiver Return 前缀。原线性 contains 对 32 个不同根各查询
一次，仅这一项按源代码计算为 528 次条目比较。该数是针对旧成员查询的推导，
未测量端到端耗时或其他 loan 注册路径的操作数。

planner 首次查询时按当前源文件的 `UnitSymbolId` 建立三个只读集合，后续按树集合
查询。独立复核核对了普通与 Map 的绑定及 Place source 根、Owned 参数回退、
借用结果排除、回滚原记录和失败时的原子撤销。退出动作、计数与顺序沿用原产物。
只在需要根分类时建立索引，首次查询仍需遍历这三类事实。

带新增身份用例的基线 direct-call 套件 111 项通过。候选前端单元 408 项及四个
相关集成套件 500 项通过，零失败、零忽略。补强后再精确复跑同名 receiver，
两个入口均核对 32 个不同根与已形成 owner 集合完全一致，退出 rank 和同点 drop
见证保留。前端全目标 Clippy、格式、文档与 Rust 尺寸门禁通过。

## 146. R3 本地借用交接的初始化调用索引

在 `a257b2f0` 上，正常借用返回的 handoff 与本地绑定运输各自为每个候选调用
扫描完整绑定表。复用 §145 的惰性索引，在读取原绑定记录的同一次遍历中按
`UnitExpressionId` 保存各候选原位置。相同 initializer 的候选仍按原序查找，
两个消费者各自保留 pending、父依赖、来源 loan 与实际 token 的检查。索引不参与
动态状态或诊断发布，forwarded return 和其他描述符查询仍走各自入口。

32 个同名本地绑定的新增双入口用例在基线通过。对照检查每个 handoff 与运输的
实际来源 token、初始化调用 ID、绑定 ID、终止语句和原顺序。成本结论限于两处
初始化绑定查找，建表后只检查命中的候选，不推断整个所有权阶段的耗时。

独立复核确认同键首选顺序及两处来源筛选等价。按复核意见强化测试后，每个运输
终点直接与对应绑定 ID 的原结束点比较，另核对来源实参与 handoff 点。基线
direct-call 套件 112 项通过。候选前端单元 408 项及四个相关集成套件 501 项
通过，零失败、零忽略。强化用例精确复跑通过。前端全目标 Clippy、格式、文档和
Rust 尺寸门禁通过。

## 147. R3 转发借用调用与返回来源索引

在 `2c0d96ea` 上，包装函数的借用返回为每个候选调用遍历已转发来源 loan，
并重复归一整份返回来源表。新增独立惰性索引，按 source-qualified 实际调用保留
两个原始首匹配，返回来源沿用原透明分组规则。索引只在转发候选进入既有准入后
建立，缺任一证据仍退出。原动态 pending token 的身份、位置和实际返回边不变。

32 个包装函数的双入口回归覆盖表达式体与显式 Return、带分组的调用与来源实参。
逐个对比原转发事实、实际 token、返回来源与控制转移。该改动消除两处逐候选
全表查找，首次仍构建索引，未测量端到端耗时。

独立复核指出初版建表仍逐返回来源查 typed call。最终只用原通用 helper 解包 Group，
再按已验证 forwarded call 集合取成员。复核确认首匹配与撤销边界等价。回归另直接核对
解包后的调用 ID 和实际 loan 集合成员。带新用例的基线 direct-call 套件 113 项通过。
最终候选前端单元 408 项及四个相关集成套件 502 项通过，零失败、零忽略。前端全目标
Clippy、格式、文档和 Rust 尺寸门禁通过。

§145–147 同步已合入 PR #389 的主干 `7de209f7`。该合并树与
[#389 双宿主验收树](https://github.com/Halckon/Koven/actions/runs/42611691670) 一致。
同步只合并本账本的追加记录，R3 源码与测试无冲突。合并候选本机再实跑前端单元
408 项、相关所有权和 Parser 集成 629 项，零失败、零忽略。前端全目标 Clippy 通过。

## 148. R3/R4 未分派的借用结果初始化前缀

在 `7de209f7` 上，公共 CLI 接受 `borrow val item = view(owner, return)`，
native build 报 UnsupportedNode。真实前端产物保留 String 调用类型，同时发布
expression falls_through=false、call_may_dispatch=false，且没有 borrow binding。
原生红测在含可观察前置实参的同形用例复现，直线借用正控已构建并输出 `kept`。

Refine 仅在普通返回 caller、无 receiver 的实际声明或符号调用上，联合 typed、
owned 与空 dispatch key 准入该前缀。已执行实参走原求值与退出清理；未完成的
borrow val 不查找或建立绑定，后继语句不进入 lowering。ControlTransfer 上被
该调用借用的 MoveOnly 临时量使用同一判定，先结束 pending loan，再执行原析构。

独立复核发现外层 borrow/carrier 返回和借用临时量的不同清理门。外层 borrow
块体探针前端接纳，但仍在原声明门禁 UnsupportedNode，未到 verifier。循环内
普通 borrow val 仍由前端 L0164 拒绝。本批保留这两类边界，也不扩展仍有正常
分派的实参 CFG。原生回归检查已完成前置输出、未访问的后继实参、临时 owner
以及正常借用正控，各核对真实分配与释放。复核确认普通退出顺序和准入证明一致。

本机 LLVM 21.1.8 上，原生套件中的三个源码样例均完成真实 object、link、run 和
分配释放计数。borrow result SSA 合同 10 项通过，新增回归在 MemoryDocument 与
Rooted 两入口均过 verifier 与 LLVM，并验证无 BorrowCall、无不可达 callee 实例。
执行项零失败、零忽略。codegen 全目标 Clippy、格式、文档与 Rust 尺寸门禁通过。

## 149. R3/R4 单臂提前返回的借用初始化实参

在 `ac3628bf` 上，`view(source, if (flag) { return } else { 1 })` 的
普通 `borrow val` 初始化通过前端，原生红测在 Return 范围报 UnsupportedNode。
正常路径发布结果绑定和分派，提前返回同时发布同一调用的来源 prefix end。
准入只认可 Copyable 值参数中带 else 的显式 If 和直接无值 Return，核对实际
调用、来源实参、来源目标与 ControlTransfer。控制流构建沿用已有实参求值与
loan 清理。具名实参顺序把 Return 放在来源之前时，缺少 prefix end，仍精确
拒绝。双正常臂和检查负号也维持原准入结果。

独立复核确认准入使用前端实际已求值的 Shared owned-root 事实，正常臂恢复
pending loan，提前臂先结束 loan 再析构。普通函数内另一个借用绑定仍会受到
既有整函数检查，外层借用返回与循环仍受各自门禁约束。
本机原生回归核对两次调用的 stdout 和两次实际分配、释放。SSA 双入口回归从
条件的实参跟踪两臂的块参数，核对退出臂的来源 loan 结束、owner 析构及唯一
正常臂的 BorrowCall。
原生回归 1 项与 SSA 合同 11 项通过，均零失败、零忽略，其他项按套件名过滤。
codegen 全目标 Clippy、格式、文档与 Rust 尺寸门禁通过。

## 150. R3/R4 相离词法绑定间的退出证明

在 `03ac38b1` 上，给 §149 的条件借用初始化前后加入已闭合的普通借用块，
原生红测在真实 Return 仍报 UnsupportedNode。每个普通绑定独立检查整个 callable，
不能看到已通过自身准入的另一个 initializer 的退出证明。

按 source-qualified initializer 收集原有精确证明。在同一普通词法块的先后遍历中，
只有另外一个结果尚未建立或已有唯一同源 AfterStatement 终点时，才导入该
initializer 的 If 与 Return。分支体和 lambda 不参与这个遍历。其余绑定门禁及
第一条拒绝的原校验顺序保留。独立静态复核未发现阻断，另指出候选在报首错前会
预计算各绑定证明，密集绑定仍有重复整函数扫描和集合合并，未测量端到端耗时。

原生回归检查提前臂只输出前置绑定，正常臂输出三个绑定，分别核对 owner 的真实
分配释放。SSA 正例走 MemoryDocument 与 Rooted，并保留无关函数和未用 lambda
变体。负控让另一个结果在退出分支仍存活，以及在具名实参中先执行 Return、后
求值来源。两者都检查精确的拒绝范围。

本机 SSA 合同 12 项、三项原生测试均通过，零失败、零忽略，其余按过滤条件排除。
codegen 全目标 Clippy、格式、文档和 Rust 尺寸门禁通过。

本批与 `369fff4c` 合并时只需合并账本追加段。[PR #392](https://github.com/Halckon/Koven/pull/392)
合并树等于其[精确双宿主验收树](https://github.com/Halckon/Koven/actions/runs/42619050619)。
合并后的本批源码实跑 `unit_direct_call_loan_ends` 113 项、SSA 借用结果合同 12 项和
原生借用初始化 3 项，均零失败、零忽略。SSA 与原生测试按套件名过滤。
同一源码树的 codegen 全目标 Clippy、格式、文档和差异检查通过。

## 151. R0/R3 生产名称原语与 nullable when 消费者

以 #392 合入后的 `369fff4c` 为基线，按已跟踪 Rust 源码直接调用形状计文件数及
匹配行。排除同一行的函数定义，包含测试及字符串。范围不解释宏、间接调用或仓外 API。

| 范围 | resolve_names | check_types | check_ownership | complete_file_types | lower_scalar_file_with_entry |
|---|---:|---:|---:|---:|---:|
| frontend src | 34 / 44 | 31 / 41 | 13 / 13 | 3 / 3 | 0 |
| frontend tests | 65 / 133 | 54 / 87 | 33 / 45 | 22 / 31 | 0 |
| codegen src | 15 / 18 | 15 / 18 | 14 / 17 | 7 / 7 | 4 / 8 |
| 其他 crates | 0 | 0 | 0 | 0 | 0 |

五名未检出显式 `as` 别名。UnitResolver 按源文件使用 `resolve_names` 生成 local
scope/symbol/reference，再建立跨文件引用和声明身份。CLI 与 LSP 的公开入口经 Unit
消费该共享生产步骤。`complete_file_types` 的十处 crate src 调用均注册在测试链。
旧类型、所有权和 codegen 测试入口的归属仍见 §93、§105。保留这组公开 API 的仓外
兼容性边界，生产 Unit legacy lowering 仍属 R4。

沿用 [SPEC-0341](0341-shared-lowering-core.md) 的消费者迁移。旧 Single 基线
`ownership_nullable_when` 26 项通过。将四项原用例的七条原源码原样转到
MemoryDocument 和 Rooted。核对真实 Abort body 无正常或循环后继、Copyable
形参保留 Shared/Exclusive 权限和无 owner drop、合流前后两个不同调用的 Shared
loan 与后一 CallReturn 上唯一根析构。早逗号匹配直接核对 `WhenAlternativeMatch`
与两名不同 owner。多 alternative 原例没有完整 nullable descriptor，此处消费实际
drop fact。其余 22 个用例函数保留原文，包含 Single 独立 loan-end、提取位置和条件
owner 的见证。独立复核后的能力和调用身份断言覆盖只查诊断无法发现的回归。

候选四个集成套件共 85 项通过，覆盖 Single/Unit 名称解析、basic Unit 所有权及
nullable when。零失败、零忽略。前端全目标 Clippy、格式、文档与 Rust 尺寸门禁通过。

## 152. R0/R3 nullable when 清理与构造实参 loan 结束

沿用 [SPEC-0341 的同点结束观察](0341-shared-lowering-core.md#同控制点-call-loan-结束顺序观察2026-11-07)。
在 `fb5a8dcb` 上，六项旧 nullable when 清理用例中，`outer(Node(), when (...))`
的 Return 候选在原地查不到结束事件。旧 Single 见证通过，Unit 已形成实际 loan 和
临时 owner 析构义务。现有 direct 观察谓词没有纳入已完成的构造调用。

同轮已验证构造索引、MoveOnly temporary 类别和实际 Shared loan target 共同准入。
透明 Group 只解包以查构造 identity，事件继续保留真实实参和 owner。正常返回用现有
pending token 退休，Return、Break、Continue 从实际 unwind 动作投影。两种情况均
核对 loan 在同点普通 drop 前结束。普通工厂调用的析构事实可存在，公开事件沿用原
准入。Abort 没有正常结束或展开析构。

六项迁移保留十条原源码。双入口核对 while 根仅在 LoopExit 析构，早期具名 Borrow
在外层 CallReturn 结束，先于同点根析构。两条正常分支无局部清理。临时 subject 在正常路径共享唯一
AfterExpression 析构，在三个显式转移各有唯一真实出口析构。Abort 只保留 null 臂的
正常清理。较早的构造 Borrow 参数在退出和正常返回路径分别与实际 owner 清理配对。
其他提取、条件 owner 和原 Single 专用产物测试保留。

独立复核确认新增 CallReturn 动作保持既有 provider 退出点和同点计数。按复核补回
两条原分支空清理断言、Null/NonNull 域与公开 Rustdoc。分组构造和普通工厂形成的
owner 均保留真实析构，只有构造发布结束。Abort 负控使用同一实际前缀 loan，正常
臂只发布一条 CallReturn end 与同点 drop。五个集成套件共 515 项通过，零失败、
零忽略。前端全目标 Clippy、格式、文档和 Rust 尺寸门禁通过。

## 153. R3 跨文件构造 Borrow 的求值与来源身份

在 `39181c78` 上，使用两个包中的实际 class、声明和调用验证 §152 的构造实参合同。
形参 ABI 顺序为 Int、Node，正控在源码中先写分组构造、后写有 Return 的值实参。
相同调用的对照交换具名实参位置。只有先完成构造的退出边有 loan-end 和同点析构，
两条正常路径各有一个 CallReturn。跨文件输入重排仍保留 caller 的 call、argument、
temporary、Return 和 value origin，以及 callee 的声明与精确参数 Span。
三条事件均在同点普通 drop 前发生，跳过构造的边没有相应析构。

原构造和 Abort 两项测试原样迁至 `constructed_argument` 子模块，保留相同 suite。
此次仅增加跨文件见证，没有改动编译器实现。`unit_direct_call_loan_ends` 116 项通过，
零失败、零忽略。前端全目标 Clippy、格式、文档与 Rust 尺寸门禁通过。

独立复核后，借用结束事实逐条与原 loan 描述符及调用 AST 实参匹配，并按 owner
核对完整析构点序列和来源，避免错标来源掩盖额外析构。用本轮构建的 kovenc 在
macOS arm64 对同形双包输入执行 project check、object build、link 与本机运行。
两条提前路径中只有已构造的 owner 打印析构。两条正常路径均先打印 callee 标记，
再各打印一次 owner 析构，stdout 逐字节符合预期。该探针使用 deinit 输出，未测
实际分配次数。

[PR #392 实际 main CI](https://github.com/Halckon/Koven/actions/runs/42624664615)
在 `369fff4c` 通过双宿主 Check/Clippy、定向、预览生产与独立消费。15 项成功，
只跳过 PR 专用的 Code CI Evidence。

同步 [PR #394](https://github.com/Halckon/Koven/pull/394) 合入后的 `a4b72d35`。
[实际 PR CI](https://github.com/Halckon/Koven/actions/runs/42625817187) 的合成提交
`649d8a5e` 与实际合并提交的有序双亲和树一致。双宿主定向、预览生产、独立安装及
最终门禁通过，15 项成功，编辑器词法检查按改动范围跳过。
合流仅需合并账本追加段。完整前端测试所用 frontend 源码树为
`8f91d54d020d2bf6461d995b3f189cc37a82539d`，本次同步保持该树、Guide、
标准库、编辑器夹具和 workspace manifest/lock 不变。
本机 `cargo test --locked -p lang-frontend` 183 组、2967 项通过，零失败、零忽略、
零过滤。合流后同一源码树复跑 SSA 借用结果合同 12 项、原生借用初始化 3 项，零失败、
零忽略，其余按过滤条件排除。前端及 codegen 全目标 Clippy 通过。本机重新构建的
kovenc 通过原双包探针的 CLI check、build、link、run，stdout 逐字节一致。

## 154. R4 直接根别名的分支准入

在 `a4b72d35` 上，普通 owned local 的词法借用块结束后再进入 `if`，MemoryDocument
红测在 If 范围报 UnsupportedNode。使用同形 CLI 输入复核，前端均接受。
无关未调用 `swap` 使后端全单元开关绕过旧扫描，加入后实际运行输出正确。
实际 CLI 还核到 owned 参数和 Borrow 参数的活跃别名穿过 Return 时同类差异，
Inout 参数的有无 sibling 对照均为 UnsupportedNode。

BorrowedStorage 的直接名根要求无来源调用 loan、无父绑定、无字段或元素投影、
同源且类型一致。依据同轮已验证绑定确定真实根，仅放行已形成 owned local、
Owned 参数或 Shared 参数。真正到达的 builder 与 verifier 仍核存储和生命周期。
RangeView、carrier、直接字段和 Map provider 沿各自入口。没有改变构建器、
SSA 验证规则和 Inout 能力。

独立复核确认 Name 的实际来源来自已验证 ownership 产物，formed 历史仅用于分类，
存活由原消费者核对。两个 Unit 入口核对无 sibling、无关函数和未用 lambda，
覆盖提前结束的 local、穿过显式 Return 的 owned/Borrow 参数和 Inout 负控。
本机原生用例覆盖六次调用、正常与提前返回的精确 stdout 和六次实际分配释放。

根别名/存储合同 9 项、普通借用结果 12 项、Map 路由 15 项、RangeView 12 项及
新增原生运行 1 项，共 49 项通过，零失败、零忽略，按套件名过滤。Inout 负控核对
拒绝类别，未锁诊断 Span。Const 与 Basic 共用前置校验，新增正控直接覆盖 Basic。
codegen 全目标 Clippy、格式、文档和 Rust 尺寸门禁通过。

## 155. R4 直接子别名与 Borrow 参数的实际路由

在 `d130e4d6` 上，普通借用结果父绑定及同块子别名已结束，后继 If 仍报
UnsupportedNode。实际 CLI 另用直接根父绑定，两种布局均通过前端，有无关
`swap` 时通过构建并运行。候选修过此门后，Borrow 参数三层
子别名仍在 Return 报 UnsupportedNode。CLI 在有无 sibling 时均复现后一问题。
Refine subset 对任意 Borrow 参数别名偏好 per-family 路由，而后者只处理直接根。

直接子别名沿用同源同类型的 stable root 准入，另以原 Unit binding ID 二分查找
真实父事实，要求 BorrowedStorage 和完整 origin 相等。Name 的实际父身份来自同轮
前端已解析的绑定事实。Refine 中 Borrow 参数的 per-family 偏好限于 Map/MutableMap。
独立复核发现前置 gate 也须排除 Map 子别名，避免改变未达旧路径的拒绝。已补同一
前置排除及未调用的显式 Map/MutableMap 子别名负控，两入口均核对原 If 范围。投影、carrier、Inout 与普通借用结果各走原边界。

双 Unit 入口测试有无无关 sibling，覆盖真实 BorrowCall 父、三层 SharedReborrow
链和显式 Return。逐条对照 SSA 父子 LoanID，由 verifier 检查实际清理次序。
原生测试覆盖四次调用和配对分配释放，stdout 逐字节匹配。Const 与 Basic 共享
前置和 subset 路由，新增正控直接运行 Basic。

根别名/存储 11 项、原生 2 项、借用结果 12 项、Map 路由 15 项、RangeView 12 项
及旧 Map 借用 provider 观察 1 项通过，共 53 项，零失败、零忽略，按套件名过滤。
独立复核完成，codegen 全目标 Clippy、格式、文档和 Rust 尺寸门禁通过。

## 156. R4 局部 RangeView 与直接控制流

实际 CLI 对照中，`take` 建立的 descriptor、其直接 metadata child 在内层块结束，
以及 child 活跃时的提前 `return`，均通过前端检查。默认 build 在对应 `if` 或
`return` 报 UnsupportedNode。加入未调用的 `swap` 后三者均能原生构建并运行。
在 `e53d0755` 上，三条独立回归在相同位置失败。

Unit 借用预检对 typed carrier call 和直接同源 metadata child 复用词法证明。正常
`AfterStatement` 结束点与 `ControlTransfer` 分开判断，活跃绑定必须具有该
`return` 的精确 end，正常路径继续使用原状态。独立复核要求增加返回与正常结束
组合、晚建 child 和无花括号裸 return。已有活跃分支的 Abort、metadata 比较、
值位置、嵌套控制及循环负例保留。普通 String 根别名的旧 Int 比较负例因 §154
的准入变化改为六种原生比较对照，顺序范围的 checked Int count 也有实际原生对照。
普通别名的未经证明活跃 if 和 metadata count 仍拒绝。

八条新增 RangeView 原生测试已通过，在 Memory 和 Rooted provider 正逆序核对
有无 sibling 的 SSA 与实际输出，同时核对提前返回及正常继续的逆序析构。
一般 carrier alias、循环及其余 legacy 路由仍属 R4 对账范围，阶段保持开放。

本机扩大 Range 运行 384 项通过，三项旧普通根别名负例失败。迁移夹具后在最终
测试二进制重跑 14 项，包含八条新回归、迁移的正负例及值位置拒绝，全部通过。
borrow-result 70 项与 Map 路由 15 项通过，零失败和忽略。lang-codegen 全目标
Clippy、格式、文档和 Rust 尺寸门禁通过。完整 Range 集合未在最终二进制重跑，
双宿主证据留待批量候选。

## 157. R4 RangeView 独立 return

用 `fd641223` 的实际 CLI 核对三种输入。局部 carrier scope 后的裸返回、
同一块中的裸返回、以及活跃 child 上的 `return count(child)` 均通过前端。
默认原生构建退出 2 且报 UnsupportedNode，未调用的 `swap` 会使三种变体
构建运行成功。三个定向回归均在原实现的对应 Return span 失败。

词法相离路径允许独立的 Return 表达式。活跃绑定还要求同一直接词法块和
该 binding 对应的精确 `ControlTransfer` end。只放行 Return 本身，操作数
继续通过原检查。Refine 先完成返回值求值与交付，再按事实结束借用并析构。
独立复审核对了调用方、顺序和原冻结边界。旧裸返回负例迁移到带不可达
后继读取的原生正例；返回值内部的 `if` 仍在精确表达式处拒绝。

最终定向 25 项通过，零失败和忽略。实际 CLI 的六种输入都 check/build/run
成功并逐字节匹配 stdout，其中带值返回核对 child 读取、逆序析构、调用者
读取返回值。lang-codegen 全目标 Clippy、fmt、文档和 Rust 尺寸门禁通过。
更宽的嵌套控制流、循环及 legacy 兼容路由继续记在 R4。

## 158. R0 Guide Litmus 消费者的双 Unit 入口

在 `51aebc0c` 的原套件实跑 23 项。Guide15 的 12 段源代码和派生场景
分别走 MemoryDocument 与 Rooted Unit，核对 source ID、输入 key 和 package。
MemoryDocument 的展示名独立于声明包。两个输入仍核对诊断阶段、有序代码、
字节范围与源码切片，以及常量和所有权能力。纯 Parser 样例在公共入口检查。

旧 checker 仅在显式见证中运行。六条规范样例和两个派生输入保留原 typed
快照，Counter 保留旧 L0135、Unit L0134 与同一字段范围的独立断言。
独立复审核对各入口身份、原快照和精确诊断。更新后的 Guide 和相关前端
七个套件共 276 项通过，零失败和忽略。前端全目标 Clippy、格式、文档
和 Rust 尺寸门禁通过。本次验证覆盖前端套件，原 native 门禁不变。

## 159. R0/R3/R4 批次对齐与本机验收

[PR #395](https://github.com/Halckon/Koven/pull/395) 合入 `9369cc72`。
[第三轮 PR CI](https://github.com/Halckon/Koven/actions/runs/42633574473/attempts/3)
的双宿主 Check、Clippy、定向测试、预览构建和独立安装均通过。前两轮
macOS 独立安装因托管 runner 未分配而取消。第三轮实际下载并安装原产物
`13684479638`。CI Passed 与 Code CI Evidence 成功，Editor lexical 按范围跳过。
合入树 `aa9b8f84` 与已验候选一致。

批次 §154–158 与该 main 对齐为 `86ba8476`。独立复核确认代码各取唯一变更侧，
Spec 保留 main 的 §7、§151–153 并追加原 §154–158。共享架构页保留两侧事实。
对齐前 `51aebc0c` 的 Range 定向集合 395 项全部通过，零失败和忽略。

对齐候选的 Guide、nullable when、直接调用 loan-end 共 165 项通过。别名、
RangeView、nullable when 与 Map 的后端定向组合共 61 项通过，零失败和忽略。
前端与 codegen 全目标 Clippy、格式、文档结构及 Rust 尺寸门禁通过。

## 160. R2 失效依赖上的独立常量资格

在 `66886a08` 上，上游 Boolean 常量调用 runtime，消费者短路 RHS 的
独立 Boolean 相等式漏掉 L0156。双失效 Double 操作数原本只报上游 L0155，
作为防级联对照。修复保留普通类型恢复，按已知失效常量的引用标记受影响子树。
仅跳过受影响操作数的资格类型，独立操作数仍参与首个错误定位。推断根类型可
随上游失效而跳过 L0155，独立显式注解继续走原类型门。值求值失败不改变资格。

独立复审核对左右操作数混合、显式注解优先级和求值及 SCC 的原边界。回归
核对两种输入顺序、跨文件诊断的准确源码和字节范围、传递依赖、双失效对照，
以及失效单元不发布常量能力。五个相关集成套件共 381 项、前端单元 408 项
通过，零失败和忽略。前端全目标 Clippy、格式、文档和 Rust 尺寸门禁通过。

## 161. R2 循环常量的显式类型准入

`46a0bc14` 上，`BAD: Int? = BAD` 的下游比较在提供者排序靠后时，
先产生派生 L0156，再报提供者 L0155。已知不合格的显式类型现在沿用普通
常量资格判定，在建图时视为失效。诊断仍由普通 initializer 恢复发出。
正反例覆盖两种 package 排序、合法 Int 自环的 L0157，以及非法注解遇到
普通操作数错误时优先报告 L0085。独立复审核对了这三个边界。

五个相关集成套件共 384 项、前端单元 408 项通过，零失败和忽略。
前端全目标 Clippy、格式、文档与 Rust 尺寸门禁通过。

后续探针直接调用本版前端。提供者显式类型合格但循环 initializer
为 `BAD: Boolean = BAD + true` 时，下游先排序仍报派生 L0156 和上游
L0085，提供者先排序时只报 L0085。循环工作队列的依赖顺序仍需修复。

## 162. R2 循环残图的分量依赖顺序

在 `54ef31b6` 上，提供者循环 initializer 出现 L0085 或 L0156 时，
下游比较可因更早进入工作队列而先产生派生 L0156。Unit 在拓扑就绪节点
处理完毕后，按尚未完成且有效的节点构造残图，先稳定提供者分量再检查
分量外消费者。分量内仍在具体类型变化后重查。反向失效覆盖整分量，
最终诊断复用分区并维持原主范围及 labels 的排序。求值失败只在资格
检查后传播。旧 Single 仍只收集有环分量。

独立复审核对了边方向、缺失 provider、Kahn 与残图边界、失效闭包和
旧 Single 内存边界。跨文件回归覆盖自环、多节点环、传递别名、两种
package 排序及合法环的下游类型资格。最终五个集成套件共 386 项，
前端单元 408 项通过，零失败和忽略。Clippy、格式、文档和 Rust 尺寸
门禁通过。早先的全量前端运行在调整残图后主动终止，未作为最终证据。

独立探针另复现了同一分量内的顺序差异。两个文件互相引用时，普通
L0085 可因另一成员先提交资格结果而伴随派生 L0156。此处只闭合分量
之间的调度，分量内的普通检查与资格提交顺序继续留待复审。

## 163. R2 同一循环分量内的诊断优先级

在 `3e0beabf` 上，交换同一循环分量的两份源码位置，普通 L0085
可能伴随先提交的派生 L0156。Unit 把 initializer 普通类型检查与资格
分开。分量内先完成普通类型检查，或传播已确认的普通失败。再从同一
普通类型快照收集资格失败，传播失效后核对能独立成立的诊断。全部
资格失败互相依赖时，保留稳定排序的首个诊断。无环节点仍按原顺序
完成普通类型、资格和求值。

独立复审核对了普通类型事实清理、owner 恢复和试探只写诊断的边界。
红测分别覆盖普通错误被资格顺序干扰，以及资格阶段多报。最终正反例
覆盖同分量普通错误优先、独立操作数、纯资格失败和互依诊断回退，
并核对输入顺序变化及常量能力拒绝。五个相关集成套件共 389 项、
前端单元 408 项通过，零失败和忽略。Clippy、格式、文档和 Rust 尺寸
门禁通过。

[PR #396](https://github.com/Halckon/Koven/pull/396) 经
[双宿主候选 CI](https://github.com/Halckon/Koven/actions/runs/42650421771)
合入 `36d381e3`。双宿主定向、预览构建与独立安装、CI Passed 和
Code CI Evidence 全部成功，Editor lexical 按范围跳过。实际合入树
`4b4530ed` 与验收候选相同。[前一合入的实际 main CI](https://github.com/Halckon/Koven/actions/runs/42647893879)
也已完成，双宿主独立安装分别验证了对应于 `9369cc72` 的预览包。

## 164. R1 普通分组的词法错误归属

在 `fe1f47f8` 上，`(value async)` 的分组先结束，File 把
`async)` 提升为文件根。嵌套调用还会错用该右括号并丢掉后续实参，
产生 L0034 和 L0013。红测先锁定文件根和调用实参。普通分组在前置
或尾随 lexer poison 且存在同作用域的实际配对时，按索引收回错误区。
组内分号、伪声明及异形 closer 保持原 owner，诊断保留实际 Lexer 根因。

独立复核补出组内分号、异形 closer、插值作用域和前置 poison 的失配。
普通配对规则拆成独立模块，每个插值隔离其 delimiter 栈。文件角列表
的外层基线保持原有查询。未配对时保留外层声明，已消费前置 poison 的
缺右括号诊断在后一声明起点使用零宽范围。最终复核通过。

四个公共入口实测 AST 和诊断。覆盖插值内外的坏字符串终止事件、UTF-8
字节范围、组内逗号及外层后继。32→64 组的显著 token 访问计数满足
原线性门槛，另核未闭插值分组不能借用外层右括号。六个相关集成套件
234 项、前端单元 410 项通过，零失败和忽略。Clippy、格式、文档与
Rust 尺寸门禁通过。其他无可靠归属的分组形状仍保留在 R1 复审范围。

### R1 普通分组的语法尾部与调用方停止点

以 `1af615f0` 为基线，`f((value, other), next)` 的内层逗号被错交给
外层调用，后继实参丢失。裸相邻表达式同样复现。两处右括号完整时只报
内部 L0013。若只有一个右括号，继续把有歧义的分隔符留给调用方。

普通分组与最近外层均有配对时，才能考虑跳到分组右括号。按需线性
索引同时记录停止点，未闭合的异形子组不能遮住其停止点。已配对子组
仍可能包含调用方等待的 `else` 或块边界。按当前 caller Stops 保守
拒绝越过，配对花括号内部的局部声明留在该块。词法 poison 继续消费
原有的配对与 Lexer 诊断，插值隔离保留。

独立复核实际确认并补充了后继文件声明、`else`、块内 `val` 及独立
花括号块反例。已配对非法逗号的旧插值语料改为核对单 L0013 的真实
逗号 Span，同时核对外层字符串末段与后继文件声明。
带分号的独立下标探针仍暴露旧 `parse_index` 的 caller 边界遗漏。
普通分组已交还控制，该下标路径的既有跳转另行复审。

最终前端单元及 Parser 集成共 910 项通过，零失败、忽略或过滤。
新增 32→64 组显著 token 访问计数满足线性门槛。三组公开 CLI
输入在单文件和项目入口的退出码与唯一诊断一致。严格 workspace
Clippy、CLI build、格式、文档和尺寸门禁通过。

## 165. R1 下标恢复的右方括号与调用方边界

在 `2f49f39d` 上，`f(xs[key, "${nested[item]}"], next)` 的公共表达式入口
返回 `InvalidLexemeStream`。同一批红测复现嵌套 `]` 被误用、词法错误尾部
冒充调用实参或文件根。独立复核另复现无配对时吞调用逗号、未闭合局部
`(` 吞后继声明，以及坏 TypeRef 留下的 `]` 被误作下标 closer。

已配对下标收回真实 closer 前的错误尾部。禁用的额外 key 保留首个逗号诊断，
独立 stray 保留其自身诊断。无配对时交还调用方逗号，文件逗号后的恢复只让
已配对局部分组遮蔽外层边界。子表达式已报错且 cursor 在 `]` 时校验配对
身份，正常下标保留直接消费。无可靠配对的尾随 poison 仍需复审，已见
`f(xs[key β, next)` 可产生额外实参。

最终九个相关集成套件共 244 项、前端单元 411 项通过，零失败和忽略。
32→64 个坏下标的显著 token 访问计数满足线性门槛。独立复核实际检查
未闭合与已闭合局部分组、字符串终止、插值尾部、错位 closer 和合法下标，
另以同版 rlib 跑了 16,384 个确定性组合，未见 Parser 内部错误。
前端全目标 Clippy、格式、文档和 Rust 尺寸门禁通过。

### R1 下标恢复保留块分号

在 `1b6d63a5` 上，配对的坏下标可以越过外层块分号并吞掉后续
`val saved`。直接下标、词法错误后的嵌套下标、逗号后的成对恢复
分别用失败回归确认。嵌套两例在初版直接修复后仍实际失败。

下标把显式块分号停止位传给嵌套 key，直接到达的分号交回块分派，
相邻普通词法错误后也适用。坏 key 的配对跳转和逗号后的局部组恢复
查询同一配对索引，以分号后的明确语句起点确认外层块归属。
公共 Parser 探针另证实只识别 `val` / `var` 会遗漏 `borrow val`
及三种循环。预索引最多看两个显著 token，辨别同名普通标识符。
扩大检查发现将任何内部分号视作外层边界会破坏既有局部解构
夹具。已配对的逗号错误尾部 `items[1, 2; 3]` 仍保有内侧分号。
配对花括号的分号留在本地，字符串和插值保留各自词法作用域。
回归逐项核对错误下标范围和后续声明身份。普通分组的既有 poison
跳转及调用参数的停止点传递继续按各自合同复审。

独立复核核对词法作用域、花括号证据隔离和 lookahead 的线性界限。
下标恢复及局部解构各 18 项实际通过，零失败、忽略或过滤。
三组公开输入在 CLI 单文件与项目入口均以同组语法诊断退出 1，
且没有 `saved` 未解析的次生诊断。

后继复审探针 `inspect(items[0; val saved = 1 … ])` 的单文件检查
仍报告 L0010 及后续读取 `saved` 的 L0080。调用实参的独立 stops
路径会清除继承的分号边界，后续须与已配对坏实参的恢复合同对照。

## 166. R2 循环常量批次合并验收

[PR #397](https://github.com/Halckon/Koven/pull/397) 以 `fe1f47f8` 经
[双宿主候选 CI](https://github.com/Halckon/Koven/actions/runs/42656885729)
合入 `74851679`。双宿主 Check、Clippy、定向测试、预览构建和独立安装成功。
合成提交 `f2754e41` 与实际合入树同为 `0e2977af`。CI Passed 和 Code CI Evidence
均成功。Editor lexical 按范围跳过。macOS 定向测试的既有真实 LLDB 用例因
debugserver 权限忽略。独立安装证据的源码树和二进制哈希与各自生产端一致。
[实际 main CI](https://github.com/Halckon/Koven/actions/runs/42663516800) 单独追踪。

## 167. R1 已有错误接收者的后缀边界

`8bfc8d8d` 上的公共入口红测确认 `f(a.async[x, next)` 丢失第二实参。
文件中 `a.async[nested(` 可吞掉下一条声明，块中 `a.async[x; next` 可
吞掉分号和后继语句。原扫描只识别硬分隔符，无法证明后缀自己拥有哪个 closer。

有真实配对时，错误后缀以其 closer 收束。无配对时沿用普通下标的已证分组恢复，
调用逗号、块分号、文件 starter 和插值结束留给调用方。严格 header 与真实
配对同时存在的 lambda 留在局部，普通调用方块仍单独分派。红测另确认
`f(a.async[g<T, U>(x), next)` 可把类型实参误作调用实参。恢复只复用严格
typed-call 的已闭合列表和后缀证明，并复用正式的尾随 lambda 同行规则。
仅从本恢复区的近邻 token 确认可用的 callee 末端，孤立 postfix 标记不算
callee。比较与无局部前缀的形态仍归还外层逗号。同行无 header 的局部 lambda
逐 token 扫描至配对结束，保留词法终止事件与单一 Lexer 根因。单个同形 closer
在坏嵌套中的归属仍依最近 opener 规则。

最终源码经独立复核。直接用同一 `rustc` 构建前端 rlib 和 11 个 Parser
集成套件，178 项通过，零失败和忽略。覆盖普通下标共享 fallback、调用方
分号与块、插值、类型逗号、词法插入、截断和 trivia 矩阵。格式、文档和
Rust 尺寸门禁通过。Cargo 与远端 CI 另行验收。

## 168. R1 未配对下标在调用方边界前的词法错误

在 PR #398 候选 `8952321a` 上，`f(xs[key β, next)` 把 β 暴露为第三个
实参，`val x = xs[key β` 后接文件声明时则出现独立错误根。公共入口两条
红测分别核对实参和文件根。

有真实配对仍以配对为准。未配对时只收回同一行相邻的普通词法错误，且
下一个 token 须是当前调用方拥有的停止点。调用方 token 保持在 cursor，
缺方括号在该起点报空 L0010。终止性 Lexer 错误保持原归属。复核另复现
同行左花括号会被 Pratt 改挂成尾随 lambda，因而该形状不提供停止证据，
换行后的真实块仍可作为调用方边界。

缓存只记录同一 raw cursor 上的候选结果，调用方的停止判定仍逐层执行。
32→64 层坏下标的访问量通过线性门槛。用仓外副本去掉缓存命中路径后，
同一测试以 3796→11508 次访问确实失败。独立复核和公开入口实跑覆盖
逗号、块分号、真实块、插值、EOF 与终止性词法诊断。

九个前端集成套件 130 项、前端单元 412 项通过，零失败和忽略。
前端全目标 Clippy、格式、文档和 Rust 尺寸门禁通过。

## 169. R0 下标公共入口回归接入双宿主矩阵

PR #398 的双宿主工作区门禁会编译 `parser_index_recovery`，原定向
矩阵实际执行 `parser_super_recovery`、`parser_block_line_continuation`
及前端单元测试。下标专用 suite 原本仅有本地执行证据。将其加入两个
宿主共用的 `check_stage_integration.sh`，与相邻 Parser suite 一起运行。

用 shell 语法检查及替代 Cargo 的命令跟踪确认新 suite 恰好入选一次。
§168 对应的本地 Cargo 已实际执行 14 项下标回归。另保留前批精确证据：
`8bfc8d8d` 全前端 184 组 2994 项，`8952321a` 的 11 个定向集成套件
178 项和前端单元 411 项均通过，零失败和忽略。后者全目标 Clippy 通过。
本项实际双宿主矩阵随本批 PR 验收。

合批后的 [PR 作业](https://github.com/Halckon/Koven/actions/runs/42674832105)
在 Docs gate 暴露目标清单断言未同步。将下标 suite 加入显式新增清单，
保留原 135 个目标，确保下标 suite 恰好入选一次，定向目标总数为 138。

## 170. R2 字面短路的流事实与来源版本

在 `a90019c0` 上，旧 Single、MemoryDocument 和 Rooted Unit 都会把
`false && mutate(&x)` / `true || mutate(&x)` 的死右侧当作实际写入，
使后继已收窄的 `x` 误报 L0084。外层逻辑条件和 while 预扫描还能重新
引入嵌套死写。源码探针和新增公共入口回归均复现。

复用既有字面布尔量、分组及取反的语法证明，区分必须执行、确定跳过和未知。
死右侧仍静态检查，写入不撤销可达出口的事实。Single 还恢复右侧检查之前
的来源版本。共享扫描只排除已证明跳过的右侧，仍扫描左侧以及可能执行的右侧。
复合左操作数不参与新的常量求值。

同一夹具覆盖三个类型入口。检查已证短路、嵌套条件重建、while 条件和循环体、
when 来源见证、死右侧静态错误，以及未知、必执行和混合路径上的真实写回。
独立复核确认保留现有完成性及 pending 边。

最终六个前端集成套件 395 项、前端单元 412 项通过，零失败和忽略。
前端全目标 Clippy、格式、文档及 Rust 尺寸门禁通过。只在仓外副本
移除 Single 版本恢复后，两个 Unit 仍通过，Single 的 when 来源
见证重新报 L0084，证明该回归独立约束版本。
另在修后实测 `(false && flag) && mutate(&x)` 的三个入口仍误报
后继 L0084，作为复合短路证明的下一处复审边界。

## 171. R2 复合布尔左式的可能写入

在 `0b5c2b25` 上，`(false && flag) && mutate(&x)` 已证明不执行外层
右侧，但三个类型入口仍会撤销 `x` 的非空事实。复合回归分别覆盖确定为假、
确定为真、取反、嵌套条件、循环预扫描及 when 的来源版本。反向控制保留
复合左式内部实际执行的写入。

复用既有结构布尔组合函数，并在可能写入使用的语法视图中加入字面真值。
按表达式子节点先于父节点的顺序构表，一次线性扫描后按 AST ID 查询。
Single 的重放共用同一 AST 的惰性表，Unit 每个 source 分开持有。
两份 rooted 文件交换输入顺序，复用相同局部 AST 编号和字节位置，
只有可能写入的文件报 L0084。真实写入路径保留，普通及 Const 完成性、
死源码的静态检查和条件重建沿用已有合同。

最终六个前端集成套件 397 项、前端单元 412 项、后端短路隔离与提前退出
定向 1 项均通过，零失败和忽略。前端全目标 Clippy、格式、文档和 Rust
尺寸门禁通过。独立复核确认缓存重放和源码身份，枚举 3280 棵小型布尔树
未见可达正常值被遗漏。本机 single 和 project 短路对照均实际执行，
只输出一次左侧效果和 ok，未调用跳过的写入函数。字面短路提交
`0b5c2b25` 的后端短路集合 80 项通过，零失败和忽略。

## 172. R2/R4 非空读取与可写存储交接的独立反例

用 §170 和 §171 的真实源码尝试 single/project native build，nullable
收窄后续读取夹具遭遇 lowering 拒绝。公共 native API 已将两条路径区分：

- `fun read(x: Int?): Int = 7` 的调用 `read(7)` 在实参 `7` 处报
  `MissingFact`。最小例没有非空判断，普通 nullable 参数的临时 Borrow
  注入与后端路由需单独复审。
- `if (x != null)` 内对 `inout Int?` 调用 `mutate(&x)`，在 call span
  报 `UnsupportedNode`。Unit 将 `&x` 的 operand 发布为 `Int` 和
  non-null use，而声明存储为 `Int?`。移除调用后的同一可变局部和非空
  读取能生成并执行本机产物。

当前 `kovenc check` 还接受了两种错误可写交付：用收窄后的 `Int?` 变量
交付 `inout Int`，以及用 `Int` 变量交付 `inout Int?`。两例 exit 0，
无诊断。待复审可写 place 的声明存储类型及其匹配。上述 native 反例均
保留，短路的动态执行对照不代替 nullable 交接验收。

## 173. R2/R3 可写实参与普通读取的类型事实

基线 `de94e7e3` 的三条类型入口均接受两种 nullable 存储错配。新增
`type_callable` 回归在修前实际失败；待定调用 `take<Unbound>(&x, &y)`
在同一基线也未对不同的 `Int` / `Int?` 存储报 L0084。

在源码实参中只把 `&` 直接使用的名称标记为可写存储，透明分组继续传递该
类型，成员接收者和索引仍按值读取。最终实参匹配和重载试探核验 place 的
存储类型相等。Unit 复用已记录的 place 分类，待定泛型证据只用存储类型，
避免用 nullable 或字面量转换把两个类型当作同一 `T`。普通读取仍能使用
收窄后的类型和 non-null 事实。

三个类型入口均核验直接和函数值调用、分组具名实参、泛型及普通和 lambda
重载。七个前端集成套件和前端单元共 853 项通过，零失败、忽略或过滤。
后端 `--lib inout` 39 项通过，零失败或忽略，2760 项过滤。原生回归在
`if (x != null)` 中先读取非空值，再实际调用 `inout Int?` 清空同一变量。
CLI 单文件和项目的本机运行均依次输出 `narrowed`、`mutated` 和 `null`，
退出 0。旧编译器对该合法调用仍报 `UnsupportedNode`。独立复核核对了
试探回滚、缓存和发布的实参类型。

组合反例仍开放。两次字面短路跳过 `mutate(&x)` 后再读取 `x` 时，原生
构建由原调用处的 `UnsupportedNode` 转为在后继非空读取处报 `MissingFact`。
去掉短路可运行上述写回，去掉后继读取时短路也可生成对象。需要继续对账
Basic 的可达前驱和收窄证明。§172 的普通 nullable Borrow 注入仍未修复。
旧 Single 对被拒绝的 `inout` 调用仍可能撤销已有非空事实并产生额外诊断；
该调用未发布有效描述符。

## 174. R4 Basic 短路与后端收窄证明

在 `31d36974` 上，字面 `false && clear(&x)` 与复合左式
`!(left() && false) || clear(&x)` 后的非空读取均通过前端，原生生成
却在后续 `x` 处报 `MissingFact`。两个原生回归均实际失败。Basic
lowering 为已知跳过的 RHS 建了写回边，该边在合流时撤销了已有证明。

类型检查复用按源隔离的语法证明，只发布逻辑 RHS 必跳过的事实。
refine 和兼容 lowering 在左侧完成后消费 Basic 跳过边，保留左侧副作用
和提前退出。refine 准入继续核验保守 ownership 的 RHS 事实；builder
只按执行点将未进入的 RHS 与对应退出边标为不可达。独立复核发现提前
剪枝准入会漏登记字段替换清理，最终候选保留原准入检查，并用带
`inout` 的函数验证字段替换计划仍能生成原生程序。Const 入口继续
消费其原有执行计划，未知和必执行 RHS 仍沿用既有路径。

五组前端单元和集成测试共 894 项通过，零失败、忽略或过滤。最终后端
`short_circuit` 83 项通过，零失败或忽略，2719 项过滤。组内的
早退见证也单独通过，覆盖 refine 与兼容路线。既有资源计数用例继续
核对 Basic skip cleanup。旧候选的后端全量已停止，结果不计入验收。
严格 workspace Clippy、格式和差异检查通过。原组合反例通过 CLI
单文件与项目构建及执行，跳过两次写入，保留非空读取，随后实际写入
一次，依次输出 `narrowed`、`mutated` 和 `null`。最终独立复核确认
原准入路径、清理执行点及早退均保留。

Basic 所有权和实例规划仍保守处理死 RHS；本批仅调整执行路径。
§172 的 nullable Borrow 临时注入仍待复审。另实测普通
`mutate(target = &(x))` 通过公共检查，但在原生生成时报
`UnsupportedNode`，待核对分组实参的 loan 事实与 lowering 准入。

## 175. R4 分组 Inout 实参的原始 loan 身份

在 `79196577` 上，`forward(target = &((x)))` 与函数内的
`clear(value = &((target)))` 通过公共检查，但原生构建报
`UnsupportedNode`。refine 的准入与构建各自要求 operand 本身是 Name。

复用两侧已有的直接 place 名称解析，只为定位存储穿过透明分组。
类型、exclusive loan、源码位置及调用均继续使用原实参身份。转发参数
沿用已有的独占借用，调用者的局部变量继续在借用结束后读回存储。
字段、索引和临时值仍受原有准入限制。

后端 `--lib inout` 40 项通过，零失败或忽略，2763 项过滤。
严格 codegen Clippy 通过。CLI 单文件与项目均执行两级调用并输出
`cleared`，退出 0。独立复核核对了原外层 operand loan、转发与
局部写回的不同路径。以同一代码树显式运行本机 LLDB 源码断点及
栈帧回归，1 项通过，零失败或忽略，2802 项过滤。

## 176. R3/R4 Nullable Borrow 注入的路由和来源核对

以 `b2f8e7f3` 的本机 CLI 和 Basic Unit API 核对 §172 的剩余反例。
`fun read(x: Int?): Int = if (x == null) 0 else x` 的调用 `read(7)`
与 `val n: Int = 7; read(n)` 都通过检查，原生构建分别在 `7` 和
`n` 处报 `MissingFact`。前者的 Shared loan 指向源码合格的 Temporary，
后者指向 `n` 的根 Place。给调用方加无关 `val anchor: Int? = null`
后两种实参均可运行；值参数对照和 `read(null)` 也能运行。应同时复审
临时包装和具名值的拷贝，不能依靠无关声明选择后端路径。

另实测 `fun view(source: Int?): borrow Int? from source = source`。
`borrow val item = view(n)` 在 `n: Int` 与 `n: Int?` 两种情况下均
通过检查、在当前 native 路径均报 `UnsupportedNode`；`view(7)`
作为同一绑定的初始化则报告 L0162。这里必须另行核对来源是原存储
还是 nullable 转换生成的临时。已有 nullable 存储上的非空读取也不能
仅凭表达式收窄类型判成转换。暂未修复这组反例，现有原生拒绝不能
充当前端来源正确性的证据。

## 177. R0 旧入口直接调用增量核对

沿用 §151 的四个范围及五个调用名，在 `b2f8e7f3` 重跑直接调用清点。
frontend src、codegen src 与其他 crates 的数值均一致。frontend tests
的 `resolve_names` 为 66 文件 / 134 匹配行，`check_types` 为
55 文件 / 88 匹配行，各增加 1。差异唯一来自 §173
`type_callable/inout_storage.rs` 的显式 Single 对照。其 MemoryDocument
和 Rooted Unit 实跑同一输入，Single 见证可暴露旧检查器的诊断或存储
事实偏差。其余三个入口计数不变。仍需逐个处置保留测试见证，不能
从直接调用计数推断仓外 API 或间接调用已经迁移。

在 #402 合入后的 `fae780dc` 按同一口径复查。frontend tests 的
`resolve_names`、`check_types`、`check_ownership` 分别为
67/135、56/89、34/46，其余行列不变。三项增量均来自
`ownership_borrow_result.rs` 的 `checked_single`。该见证直接核对旧
Single 与两种 Unit 输入是否都拒绝把 nullable 转换当作已有借用
结果存储。已查看测试调用方，保留为有意的旧入口对照。

## 178. R1/R2 合批合并验收

[PR #398](https://github.com/Halckon/Koven/pull/398) 的最终 head
`b6cb2bb7` 经[实际 PR CI](https://github.com/Halckon/Koven/actions/runs/42675895710)
通过。双宿主 Check/Clippy、定向测试、预览生产与独立消费、CI Passed
及 Code CI Evidence 共 15 个作业成功。Editor lexical 按变更范围跳过。
逐宿主核验新增 Parser/Inout 回归均实际执行。macOS 定向套件的既有
LLDB 用例因 debugserver task-port 权限忽略，§175 另有后继本机实跑。

两端产物均核对归档 SHA256、生产与消费清单及 12 项独立执行命令。
基线 `74851679`、合成提交 `3a6c4bad`、实际合入 `94768a91`，
合成和合入树均为 `25cf3ece844d7b6b344b104ab00812085d1718a2`。
合并时无未决审阅线程；上述验收限于该 PR 候选。

## 179. R3 普通借用结果必须引用已有 nullable 存储

在 `3110cfa5` 上，公共检查接受将 `Int` 来源转换为 `Int?` 后的
借用绑定、返回和具名实参转发。新增三入口回归在修改前实际失败。
显式 `Nothing?` 到 `Int?` 以及枚举 case 到 nullable 枚举的转换也
复现。同步 `observe(view(n))` 在 `view` 的来源实参需要转换时，
原来同样会接受并把原 place 当作可延续的 loan。

Single 与 Unit 在识别借用调用之后，分开核验实际映射参数的存储。
已知类型不同且目标为 nullable 时，按原有 Name/Group 引用查声明
存储，不把收窄读值当作新存储，也不扫描整张非空事实表。声明类型
相同的真实 nullable、`when` 的枚举单 case 收窄和普通非空分支
均保留绑定、转发或同步 handoff。显式结果绑定及返回同样核对
目标类型；失败绑定释放初始化调用保留的 loan，后续赋值无级联诊断。

最终七组前端单元和集成测试共 719 项通过，零失败、忽略或过滤。
负例精确区分 L0162 与同步 L0164，并核对拒绝后无借用结果事实。
严格前端 Clippy 和 CLI build 通过。公共 CLI 单文件与项目均复核
来源错误拒绝，独立审查覆盖来源、诊断、释放和调用扫描边界。

普通 `Borrow Int?` 的临时包装仍另行处理。若内层合法返回
`borrow Int` 而外层同步消费者参数为 `Int?`，转换发生在外层，
现有直接同步交接事实仍有表示差异，不能复用本节结论。

## 180. R3/R4 普通 Borrow 的 Copyable nullable 临时值

在 `3992f39b` 上，`Borrow Int?` 接受字面量、具名 `Int` 及转发
的 `Borrow Int`，但原生构建因路由报 `MissingFact` 或
`InvalidModel`。具名转换后在同一次调用借用 `&n` 原本还报 L0135。
先复现这两个失败，再以真实 nullable 存储和先借用 `&n` 的反向顺序
作负控。

普通调用对需要 nullable 转换的 Copyable place 先检查并执行普通
读取，再发布同一 operand 身份的 Shared Temporary loan。后续实参可
借用原变量，反向顺序仍会拒绝读取。真实 nullable 的 place 在收窄
后仍有原 loan。Direct 对已发布的 Temporary 核验 inline payload
与 Copyable，再借用新包装槽；refine 仅在目标相等时复用原参数 loan，
目标不同则要求同源 Temporary 事实并从原 loan 读值。原句柄
MoveOnly temporary 的唯一析构义务继续转给 wrapper。

初版修复后，带后续提前返回的 Direct 前缀仍因实参类别是 Place 报
`UnsupportedNode`。准入与构建改用同一 Copyable inline 判据，实际
pending loan 的观测记录覆盖 CallReturn 和 ControlTransfer。
两条原生路线均验证提前退出不分派、继续路径只调用一次。
独立复核先发现该缺口，再复核修复后的实际 loan 和来源校验。

最终六组前端单元和集成测试 732 项通过。三个后端定向集合分别
通过 4、5 和 5 项，均无失败或忽略，分别过滤 2802、2801 和 2801 项。
严格 workspace Clippy、CLI build 通过。普通参数、先读后写和后续早退
三组程序均通过实际 CLI single/project 构建及输出对照。本次原生覆盖
`Int?` 与 `Boolean?`，并保留旧 MoveOnly 句柄回归。精确类型的
nullable 枚举及 `Char?` 探针仍报 UnsupportedNode。§179 的内层
合法借用结果被外层转换为 nullable 的组合仍另行复审。

## 181. R4 短路与分组 Inout 批次合并验收

[PR #399](https://github.com/Halckon/Koven/pull/399) 的 head `3110cfa5`
经[实际 PR CI](https://github.com/Halckon/Koven/actions/runs/42682599112)
通过双宿主 Check/Clippy、定向、预览生产和独立消费。CI Passed 与 Code
CI Evidence 实际执行并通过。共 15 个成功作业，Editor lexical 按范围
跳过。双端日志分别核对五个 SSA 回归均实际通过，包含两条路线的早退。
macOS CI 的既有 LLDB 用例按 debugserver 权限忽略，§175 有本机证据。

两端预览归档实算 SHA256 与生产和消费清单相符，各 12 项独立命令、
哨兵及清理成功。基线 `94768a91`、合成提交 `0292e636`、实际合入
`2b1e8f1e`，合成和实际合入树均为
`49021804d6a2d2c9bf397422b520334d1d15fe4b`，无未决审阅线程。

前一主干 [push CI](https://github.com/Halckon/Koven/actions/runs/42682434283)
中 macOS Check/Clippy 被标记取消且无步骤记录，随后定向与预览跳过。
该记录仍缺少主干验收，不能以 #398 或 #399 的 PR 结果代替。

另以 §180 候选实跑 §179 的镜像边界。精确 `Borrow Int` 参数消费
`borrow Int` 结果能原生运行；只把外层改为 `Borrow Int?` 后，
公开检查仍通过，原生报 `InvalidModel`。下一轮须分别核对内层
来源 lease、实际复制点及外层临时 loan 的结束。

## 182. R3/R4 嵌套借用结果的 nullable 复制边

在 `4879cf78` 上，外层 `Borrow Int?` 消费合法的 `borrow Int` 结果时，
公开检查接受而原生生成报 `InvalidModel`。若后续实参再独占借用同一根，
前端也会误报 L0135。精确 `Borrow Int` 保留直接续借合同。

Single 与 Unit 只允许将 Copyable payload 复制到外层 nullable 临时槽。
原始来源 loan 保持到读取完成，已有父借用继续有效。外层 Shared loan
使用原 operand 身份并指向新槽。普通借用结果仍不允许从包装临时槽
转发来源。旧文件 lowering 对这项 inline nullable 交接显式拒绝。

独立复核发现两条清理边偏差。内层无关实参须在其 CallReturn 先清理，
来源最后使用的析构才移到实参 AfterExpression，发布顺序与执行一致。
捕获来源的闭包清理也可能递归释放同一根。两个 planner 在整个内层清理
批次保护该根，再按实参完成边清理。两条前端回归在修复前实际失败。
SSA 断言追踪内层调用的结果与来源 loan，验证无关析构先于读取，
来源析构在两项 loan 都结束之后。

最终六组前端单元和集成测试 723 项通过，零失败、忽略或过滤。
原生定向 3 项与 SSA/既有借用调用回归 6 项通过，零失败或忽略，
分别过滤 2807 和 2804 项。四组程序经 CLI 单文件和项目实际构建并
运行，分别核对基本复制、后续写回、资源作用域析构及无关实参析构。
独立探针另核对具名、分组和临时闭包、已有父借用和后续活跃来源。
严格 workspace Clippy、CLI build、格式、尺寸及文档结构检查通过。

闭包回归只验收前端清理事实，原生仍受既有函数值实参路径限制。
Unit 通用 direct loan-end 观测仍不覆盖嵌套借用结果实参，此处动态验收
使用实际 Refine SSA 与 CLI。另以本机探针确认，内层借用结果生产者
的其他 `Borrow Int?` 参数接收 `Int` 时仍报 `InvalidModel`，即使
外层是精确 Borrow。传 `null` 或已有 nullable 槽的对照可运行，
这一独立的参数包装遗漏留后续处理。

## 183. R3/R4 普通 nullable 借用存储批次合并验收

[PR #400](https://github.com/Halckon/Koven/pull/400) 的 head `4879cf78`
经[实际 PR CI](https://github.com/Halckon/Koven/actions/runs/42689614938)
通过 15 个作业，Editor lexical 按范围跳过。两宿主的定向日志分别
确认本批 11 项借用回归实际执行并通过。macOS 的既有 LLDB 用例因
debugserver 权限忽略，§175 另有前一代码树的本机运行证据。

两端预览归档实算哈希与生产、消费清单相符，各 12 项独立命令、
哨兵及清理通过。基线 `2b1e8f1e`、合成提交 `7cac65d6`、实际合入
`53f1227f`，合成和实际合入树均为
`8c55319f1396a98f0b0ed27047f6cac051f7b94a`，无未决审阅线程。
§181 的 [#399 合入后主干 CI](https://github.com/Halckon/Koven/actions/runs/42689444198)
随后成功。另启动了 [#400 合入后主干 CI](https://github.com/Halckon/Koven/actions/runs/42694684754)。

## 184. R4 借用结果生产者的辅助 nullable 实参

在 `c314c867` 上，`view(source: Int, aux: Int?): borrow Int from source`
的非来源实参接收 `Int` 时通过公开检查，原生构建报 `InvalidModel`。
外层精确 Borrow 和 nullable 消费者均复现。把辅助实参换成已有
nullable 存储或 `null` 后可运行。新增原生回归在修改前实际失败。

来源调用 lowering 继续按源码顺序读取每个实参，并用已选描述符
填充 ABI 参数槽。只有非来源槽、Copyable payload 和同一调用实参
的同源 Shared Temporary 事实同时成立，才复用普通 nullable 包装，
借用新槽并在内层调用返回时结束该 loan。具名映射或分组常量可有
不同 operand/backing ID；实际来源槽和已有 nullable Place 保留原路径。

最终原生矩阵含六种实参组合，覆盖外层精确与 nullable 结果、具名
求值顺序、转发借用、已有 nullable 存储、`null` 和分组常量。
最终 SSA 借用调用与 Range 调用前缀各六项通过，零失败或忽略，
各过滤 2805 项。加入分组常量前的扩大借用结果集合 43 项通过，
零失败或忽略，2768 项过滤。严格 workspace Clippy 和 CLI build
通过。八组公开程序在单文件和项目入口均实际构建、执行并核对
输出，另包括具名类字段读取。

独立复核确认匿名临时类的字段投影仍有另一条前端边界。精确
`Borrow Int` 会发布 backing owner 的 CallReturn drop；转换为
`Borrow Int?` 时没有该事实。两种匿名投影在基线原生路径均报
`UnsupportedNode`，该资源边另行复审。

## 185. R3/R4 嵌套 nullable 借用批次合并验收

[PR #401](https://github.com/Halckon/Koven/pull/401) 的 head `03b885ce`
经[实际 PR CI](https://github.com/Halckon/Koven/actions/runs/42695805642)
通过 15 个作业。两宿主分别执行并通过本批 9 项指定回归。
Editor lexical 按范围跳过，macOS 的既有 LLDB 用例因 debugserver
权限忽略，§175 另有前一代码树的本机运行证据。

两端预览生产、独立消费清单与归档实算哈希匹配，各 12 项命令、
哨兵及清理通过。基线 `53f1227f`、合成提交 `eb5f8eb8`、实际合入
`1af615f0`，合成与实际合入树均为
`7f7508857236850ccbc0b8ee18a983ed2cfca0f8`，无未决审阅线程。
[#400 合入后的主干 CI](https://github.com/Halckon/Koven/actions/runs/42694684754)
已成功，另已启动 [#401 合入后的主干 CI](https://github.com/Halckon/Koven/actions/runs/42701685162)。

下一批临时字段及无分派前缀的主干合流树为
`e4e9944c1380a0802bddb39bb70205910da40dda`，与隔离预演一致。
合流后复跑 SSA 借用、临时字段、嵌套 nullable 和标量前缀，分别
通过 8、2、4 和 1 项，零失败或忽略。严格 workspace Clippy 通过。
早一预演另核对 131 项前端合同和两组组合程序的双 CLI 入口实跑。

## 186. R3/R4 临时字段和无分派前缀批次合并验收

[PR #402](https://github.com/Halckon/Koven/pull/402) 的 head `8ae8f772`
经[实际 PR CI](https://github.com/Halckon/Koven/actions/runs/42701973621)
通过 15 个作业。双宿主各执行并通过本批 7 项指定回归。Editor lexical
按范围跳过，macOS 的既有 LLDB 用例因 debugserver 权限忽略。

六份预览产物经独立下载，双宿主生产与消费清单同实算归档哈希一致，
各 12 项命令、哨兵及清理通过。基线 `1af615f0`、合成提交
`ae0e1478`、实际合入 `fae780dc`，合成与实际合入树均为
`0e786d14becbdd314ea5bd7e3f2c715c50b20155`，无未决审阅线程。
[#401 合入后的主干 CI](https://github.com/Halckon/Koven/actions/runs/42701685162)
已成功。另已启动 [#402 合入后的主干 CI](https://github.com/Halckon/Koven/actions/runs/42707291414)。

## 187. R1 普通分组和下标批次合流验收

普通分组 `1b6d63a5` 与下标 `09f66d06` 在 `fae780dc` 上合流为
`1a3e5ca5`，实际树 `7b60f770b07f58a6e391e2e36252ff04afcfbbf4`
与三方合并预演一致。两片的 Parser 源码在合流前后相同。

合流树前端单元及全部 Parser 集成 72 个套件共 914 项通过，零失败、
忽略或过滤。严格 workspace Clippy 与 CLI build 通过。六组输入分别
通过单文件与项目实跑核对诊断，覆盖普通分组及错误下标。已配对的
错误下标保留局部解构的旧 oracle，含内部数字和普通 contextual 名称。

## 188. R1 调用实参里的下标与块边界

在 #403 候选 `8dc4f167` 上，公共块入口实跑复现
`f(xs[key; val saved = 1 …])` 吞掉 `saved`，原 CLI 探针另报次生
L0080。嵌套调用必须保留块上下文，又须允许 `f(xs[key; 2], next)`
继续在配对的错误实参内部恢复。

调用传递仅针对后接明确块语句的分号停止点。沿用 §165 的语句前缀
识别，嵌套表达式在同一分号退出时复用查询。实参的 Pratt 与 TypeRef
右操作数、下标配对与逗号尾部恢复、外层调用均遵守这一条件。
独立复核追加的 cast、type test 反例先实跑失败，再覆盖缺类型和
未闭合泛型类型。回归逐项核对表达式范围、后继声明和局部数字分号
对照的两个实参。通用调用错误扫描与普通分组 poison 的独立旧路径
仍各有实跑反例，后续分别处理。

最终三个定向套件 69 项通过，零失败、忽略或过滤。严格 workspace
Clippy 与 CLI build 通过。普通缺界、cast 和嵌套泛型三种探针各走
单文件和项目，仍以语法错误退出 1，两入口诊断码一致，均无次生 L0080。

在 `897b2f4b` 源码上补跑前端单元及全部 Parser 集成，共 72 个
套件 916 项通过，零失败、忽略或过滤。公共 Parser 探针复核 cast 与
type test 已保留声明，通用调用扫描和普通分组 poison 仍复现旧行为。

## 189. R1 调用通用错误扫描的块分隔符

在 `b4726152` 上，错误实参或调用尾部跳入通用扫描后，局部
圆括号或方括号栈可屏蔽后接明确块语句的分号。公共块入口红测
证实 `f(: junk[key; val saved = 1 …])` 会吞掉 `saved`。

通用扫描在同一词法作用域内用配对花括号保护真正的局部块。仅在
外层词法 owner 为空且已离开配对块时，才把明确分号交回块分派。
逗号和硬闭合符仍走原判定。独立复核核对了配对区间、字符串结束
事件及常数时间查询。测试覆盖前置实参错误、尾部错误、未配对下标、
嵌套圆括号和局部花括号结束后的边界。对照保留配对块及插值中的
分号、数字分号、未配对逗号和后继实参。

普通分组词法错误的独立探针仍复现吞声明，已固定前置、尾随和缺
操作数三个形态。直接块 initializer 的旧配对 oracle、调用内数字
分号及内层 lambda 保留为对照。

最终三个定向套件 71 项，扩大前端单元与全部 Parser 72 个套件
共 918 项通过，零失败、忽略或过滤。严格 workspace Clippy、CLI
build、文档、格式和 Rust 尺寸检查通过。三组公开探针在单文件及
项目入口均以相同语法诊断退出 1，无 `saved` 未解析的次生错误。
angle 配对测试移至相邻测试文件，去除外层缩进后正文逐字相同。

## 190. R1 调用内普通分组与已确认的块分隔符

在 `a10a561e` 上，公共块入口实跑证实，调用内普通分组的前置
poison、尾随 poison 及缺操作数均可越过后接局部声明的分号。
独立复核另指出非 poison 逗号尾部的配对跳转，探针证实其嵌套
方括号同样可吞掉声明。

调用内普通分组复用配对索引的块分隔证据。相邻普通词法错误走
与下标共享的单游标缓存，Error 截止于该错误段，并把分号留给块。
两种配对查询都在仍未越过的分号前停下。回归核对恢复前缀、词法
根因唯一及后继 AST 声明名称。直接块 initializer 的既有配对、
数字分号及内层 lambda 继续保有局部归属。跨行 poison 的探针
已保留声明，现有空范围 L0034 另留诊断复审。坏接收者后的
`value.async[key; …]` 和 `value.async(key; …)` 另经公共 Parser
实跑复现吞声明，属于 error-postfix 的独立 raw 跳转。

[#402 合入后的主干 CI](https://github.com/Halckon/Koven/actions/runs/42707291414)
已完成，双宿主定向、预览生产及独立消费、Editor lexical 和最终
汇总均通过。PR 专用 Evidence 按范围跳过。

最终三个定向套件共 77 项，前端单元与全部 Parser 72 个套件
共 921 项通过，零失败、忽略或过滤。严格 workspace Clippy、CLI
构建、文档、格式和尺寸检查通过。前置、尾随和缺操作数三组 CLI
输入均经单文件和项目入口验证，语法诊断一致，词法根因各一条，
无 `saved` 未解析的次生错误。

## 191. R1 错误接收者的配对后缀

在 `a8471402` 上，公共块入口红测证实，坏接收者后的配对调用
和下标可把后接局部声明的分号一并跳过。错误后缀现在复用已有
配对索引的分号检查。只有块上下文或继承块停止位时才启用，
配对不能越过已确认分号时沿用扫描回退。

独立复核确认回退保留块分号与最后显著 token 的错误节点范围。
回归核对直接块、调用实参、`super`、实际局部声明节点和单条
词法根因。对照覆盖后继实参、数字分号、配对 lambda 与插值。
若同一错误后缀在分号之前另有逗号，回退仍按既有规则将无归属
证明的逗号交还外层调用。

最终四个定向套件共 87 项，完整前端单元及 Parser 72 个套件
共 923 项通过，零失败、忽略或过滤。严格 workspace Clippy、CLI
构建、文档、格式和尺寸检查通过。调用内下标、调用内调用、直接
下标和包含显式 lambda 的四组源码，分别通过 CLI 单文件和项目
入口核验。两入口的语法诊断一致，词法根因各一条，无后继
`saved` 未解析诊断。

## 192. R1 普通分组批次合并及调用恢复合流

[PR #403](https://github.com/Halckon/Koven/pull/403) 的 head `8dc4f167`
经[实际 PR CI](https://github.com/Halckon/Koven/actions/runs/42709905397)
通过 15 个作业。双宿主各实际通过 13 项指定下标、普通分组、资源
及既有回归。Editor lexical 按范围跳过，macOS 既有 LLDB 用例因
debugserver 权限忽略。

六份预览产物经独立下载，双宿主生产和消费清单同实算归档哈希
一致，各 12 项命令、哨兵及清理通过。基线 `fae780dc`、合成
提交 `7f448a26`、实际合入 `c97c5a9b`，合成与合入树均为
`7303ee1fecb7461ae5473df09cdfac07a3671f45`，无未决审阅线程。
[合入后的主干 CI](https://github.com/Halckon/Koven/actions/runs/42714892193)
已启动。

§188–191 调用恢复四片随 `d031ea14` 合流为 `5ed61b8e`。
三方合并预演、实际合流及本机 923 项测试所覆盖的完整树均为
`6a1ef0cf979cbea1221c75f1b1a034cd365ed291`。

## 193. R2 Single 调用恢复与非空事实

在 `c97c5a9b` 上，Single 拒绝不匹配的 `inout` 存储后仍按
源码标记撤销非空事实，使后续读取多报 L0084。直接调用和函数值
调用均实跑，MemoryDocument 与 Rooted Unit 仅报告实参错配。
独立复核另确认恢复类型不能代替调用准入。唯一候选的硬错配可与
待定实参同时存在，普通和 lambda 重载的纯待定预检也可返回
Error 恢复类型，`replace` / `swap` 有相同的待定优先路径。

Single 在外层期望类型检查前记录调用自己的写回判定。明确拒绝
时保留旧事实，纯待定仍保守失效。即时更新和短路条件重建共用该
判定，候选回滚与重查同步清理。已选调用被外层拒绝返回类型时，
其实际写回仍生效。被拒外调用也不会抵销已执行的内层写回。
循环预扫描继续使用尚未解析时的保守规则。

三个定向套件共 146 项通过，零失败、忽略或过滤。独立复核与
公共类型 API 实跑另固定了下一片边界。已拒绝但返回待定恢复值
的内层调用作为外层 callee、分组 callee 或实参时，Single 仍
撤销外层 `x` 的事实并多报 L0084，两种 Unit 入口均无次生诊断。
该边界涉及恢复值跨表达式传播。

完整 frontend 的 184 个测试目标共 3041 项通过，零失败、忽略
或过滤，含 12 项文档测试。严格 workspace Clippy、CLI 构建、
格式、文档及尺寸检查通过。公开 CLI 单文件和项目各实跑两个
对照。被拒调用的唯一 L0084 位于实参，有效写回的唯一 L0084
位于后续读取。公共入口仍使用 Unit，独立 Single 回归由 Rust
测试直接覆盖。

## 194. R2 普通调用传播已确认的恢复状态

在 `1c02c527` 上，公共类型 API 复现了两种反向偏差。已拒绝的
内层调用保留 Deferred 恢复类型时，外层 callee、分组 callee 或
实参会错误激活写回。仅待定的内层重载使用 Error 恢复类型时，
外层则过早保留非空事实。两种 Unit 入口分别保持正确对照。

Single 用明确的 Pending / Rejected 状态取代写回布尔判定，保留
原恢复类型。普通 callee 和实参读取其来源，透明分组不丢失状态，
已确认拒绝优先于待定。后继事实检查只撤回外层调用自己的写回，
此前在实参中实际执行的调用仍有原效果。独立复核核对试探与缓存
回滚、诊断位置和透明分组查询的成本。六组正反例同时覆盖三个类型
入口。三个定向套件共 148 项通过，零失败、忽略或过滤。

另以公共 API 实跑定位后续边界。Member 接收者、Index 下标和
`replace` 的待定实参仍可丢失内层拒绝，纯待定重载传给
`replace` 也会被误当成明确拒绝。四组探针均保留两种 Unit 对照，
后续需分别核验各运算何时产生自身诊断以及何时沿用输入的恢复状态。

最终 frontend 184 个测试目标共 3043 项通过，零失败、忽略或
过滤，含 12 项文档测试。严格 workspace Clippy、CLI 构建、格式、
文档与尺寸检查通过。CLI 单文件和项目均实跑拒绝 callee、纯待定
实参两组，前者唯一 L0084 位于内层原始实参，后者唯一 L0084 位于
后续读取。公共入口使用 Unit，Single 的实际状态由同源码 Rust
回归直接核验。

## 195. R2 投影与所有权原语传播恢复状态

在 `28f2cd06` 上，Member 接收者、Index 的待定下标及
`replace` 的待定实参均实跑复现丢失已确认拒绝。纯待定重载传入
`replace` 则会被误当作拒绝。公共类型 API 探针和定向红测已固定
两类差异，两种 Unit 入口提供同源码对照。

投影查询只沿已经检查且带恢复类型的操作数读取状态。已确认拒绝
优先，独立产生的 Error 不由仅 Deferred 的输入解释。顺序下标
自身的 L0128 与 nullable Map 查询自身的 L0130 均保留，Error
包装下的 Pending 下标仍保守。两种所有权原语共享收尾判定，形状
和显式类型的本地硬错优先，保留原恢复类型和实际内层写回。
独立复核通过。三个定向套件共 151 项通过，零失败、忽略或过滤。

另一个实跑边界是 `Map<Int, Int>` 的 Deferred key。Single 仍
发布具体 `V?` 和 MapGet，后继调用看不到拒绝的来源。两种 Unit
入口仅报原始错配，Single 还多报后继 L0084。公开描述符探针
实跑确认，具体 key 三入口都有 MapGet，纯待定与已拒 key 仅
Single 有 MapGet。Unit 对纯待定保留暂定 `V?`，对已拒 key
返回 Error。后续需核对 MapGet 准入，保留暂定类型和已知
nullable 或 MoveOnly value 的本地诊断。

[#403 合入后的主干 CI](https://github.com/Halckon/Koven/actions/runs/42714892193)
已完成，双宿主定向、预览生产与独立消费、Editor lexical 和最终
汇总均通过。PR 专用 Evidence 按范围跳过。

完整 frontend 184 个测试目标共 3046 项通过，零失败、忽略或
过滤，含 12 项文档测试。严格 workspace Clippy、CLI 构建、格式、
文档及尺寸检查通过。CLI 单文件与项目各实跑 Member、`replace`
和 Map 本地硬错三组，分别只在原始操作数或 Map 查询报 L0084
或 L0130。公共入口使用 Unit，Single 的效果由同源码 Rust 回归
直接核验。

## 196. R2 MapGet 描述符与 key 准入

在 `ed06d87d` 上，公共类型 API 对照和定向红测证实，Single
会为待定 key 发布 MapGet，也会将已拒 key 包装成具体 `V?`。
两种 Unit 入口仅在 key 已确认时发布 MapGet。

Single 的下标和 `.get` 共用发布判断。待定 key 保留供诊断使用
的暂定 `V?`，直接实参和透明分组仍携带待定状态。拒绝的 key 返回
Error。下标重查清理自身旧状态，查询自己的 L0130、L0136 和
key 类型错误优先于待定恢复。外层期望类型的硬错也保留，已执行
的内层 key 写回仍生效。

最终四个定向套件共 170 项通过，零失败、忽略或过滤。独立复核
核对重查与缓存、局部诊断的精确位置、透明分组和实际内层写回，
未发现阻断。另实跑确认暂定值经 `!!` 或 Elvis 包装后仍可使外层
普通调用发布描述符，列为独立后续复审。

## 197. R1 调用恢复合入与 R2 Single 批次合流

[PR #404](https://github.com/Halckon/Koven/pull/404) 的 head `1c7fc66e`
经[实际 PR CI](https://github.com/Halckon/Koven/actions/runs/42715152484)
验收。双宿主各实际通过 9 项指定 Parser 回归、严格 Clippy 和
最终门禁。Editor lexical 按范围跳过，macOS 既有 LLDB 用例因
debugserver 权限忽略。六份预览产物独立下载校验，双宿主各 12 项
独立消费、哨兵和清理通过，生产与消费清单的归档哈希均与实算一致。
无未决审阅线程。基线 `c97c5a9b`、合成提交 `1434b4da`，实际合入
`6ce85b07`。合成与合入树均为 `8bd014eba55b7feda60a8759e57e36ca5ec0811b`。
[合入后的主干 CI](https://github.com/Halckon/Koven/actions/runs/42720825998)
已启动，双宿主 Workspace Check 与 Clippy 已通过，定向运行中。

§193–196 的四个类型修复在 `6ce85b07` 合流为 `f6107a29`，
测试树为 `af9d747458b0c0096bd6d4905c498d5cbdcdc40c`。合流前后类型源码
与直接回归逐字相同，Parser 与该主干一致，双方历史账本完整保留。
合流树 frontend 184 个目标共 3057 项通过，含 12 项文档测试，
零失败、忽略或过滤。严格 workspace Clippy、CLI 构建、文档门禁
通过。CLI 单文件和项目各实跑拒绝调用、有效写回、拒绝下标 key
和拒绝 `.get` key 四组，均仅在原实参或应失效的后继读取报一条
L0084。公共入口使用 Unit，旧 Single 事实由直接回归核对。

## 198. R2 Single 非空断言与 Elvis 的恢复和正常出口

在 `f6107a29` 上，三入口公开类型 API 对照显示，待定 Map 查询
经 `!!` 会让 Single 提前发布断言及外层调用，Elvis 也会提前发布
外层调用。已拒绝的左侧和右侧硬错误可使 Elvis 多报后继非空读取。
两个 Unit 入口保留原始诊断，不发布这些待定描述符。直接类型回归
在旧 Single 上确实失败。

`!!` 保留暂定的非空类型但不发布断言。Elvis 两侧静态检查均保留，
已拒绝状态优先于待定。两者沿用原恢复类型、试探回滚及实际写回。
已完成 fallback 的写回仍参与正常合并。独立复核发现，实参里的
提前 return 由检查器单独记录，Elvis 的普通完成标志会漏掉它。
三入口实跑确认，右侧仅退出路径上的写回可使 Single 多报 L0084，
左侧退出时也可在应有 L0085 之外污染另一条正常路径。Elvis
现在使用已记录的完成性合并事实和版本，静态操作数诊断继续保留。

最终五个定向套件 181 项通过，零失败、忽略或过滤。最后一个左侧
退出回归之前，完整前端 184 个目标共 3058 项和严格 workspace
Clippy 已通过。另有三入口实跑反例：布尔条件重建仍把 Elvis 仅
在退出分支发生的写回算入正常路径，三入口都给合法读取报 L0084。
该路径复用语法写入扫描，需要与循环保守失效分开审查。

## 199. R2 布尔条件只合并能到正常出口的写回

在 `224d180e` 上，Single、MemoryDocument Unit 和 Rooted Unit
均把 Elvis fallback 在提前 return 前发生的写回带入布尔条件的
正常路径，对原本合法的非空读取报 L0084。两种 Unit 入口还会
使已有 `when` subject 版本失效。三入口独立探针与两条正式红测
区分了这些差异，以及真正正常完成的 fallback 写回。

布尔条件的写入扫描读取类型检查已经确认的正常完成性，略过只在
退出路径上的右侧。两种 Unit 入口复用 source-qualified pending
退出事实，Elvis 合流也据此保留正常旁路的版本。Single 对无正常
出口的短路 RHS 恢复进入前的版本。普通完成事实继续用于下游，
静态 operand 检查和原始诊断保留。

子循环可能捕获 break 或 continue，扫描进入子循环后保持保守，
离开后恢复外层规则。循环头和循环体的预扫描仍使用语法候选。
子循环内只退出 callable 的 fallback 仍可能被保守计入，需另行
验证退出目标后收窄，不能丢掉子循环可观察的写回。

`224d180e` 的完整前端 184 个目标共 3059 项通过，零失败、
忽略或过滤，严格 workspace Clippy 通过。当前候选六个定向套件
209 项通过，零失败、忽略或过滤。新增回归覆盖两个入口方向、
复合布尔的必走与旁路、条件内赋值、保留正常写回、子循环捕获
break、离开子循环后的相邻表达式及 `when` 版本。
独立复核核对了实际完成性生产者、试探回滚、循环跳转边界和
正常写回的负例，未发现阻断。格式、文档、Rust 尺寸与 diff 门禁通过。

## 200. R2 条件写回与扫描内循环的退出目标

在 `33f17a59` 上，子循环中只退出 callable 的 fallback 仍在
三个类型入口使正常读取多报 L0084。用例也确认被子循环接住的
break 写回必须保留，外部循环的保守预扫描仍应失效旧事实。

已检查的无正常出口 RHS 在扫描内有循环时，只在可能跳到当前
循环时保留该段写入。迭代扫描按 Guide 06 区分最近循环 body
及其输入，内层自身接住的跳转不算给外层。记录点在左侧之后，
避免截断之前写回。Single 在布尔合流以同一正常写回集合选择
来源版本。正式红测另外确认，旧 Single 即使修正了普通读取，
仍会在 `when` subject 见证上丢失版本。

最终六个定向套件 211 项通过，零失败、忽略或过滤。三入口覆盖
return、捕获的 break/continue、内层自行捕获的 break、保留
左侧写入、for source 跳到外层循环和 `when` 版本。独立复核核对
DFS 生命周期、三个版本生产者和回滚，未发现阻断。语法上不可达
的顺序跳转仍可能保守计入，后续须依赖已验证的顺序可达性。

`33f17a59` 完整前端 184 个目标 3061 项、严格 workspace Clippy
和 CLI 构建通过。公开单文件及项目入口接受退出路径和保留来源
版本两个正例，正常写回与被捕获 break 各只在后继读取报 L0084。

本机纵向探针另核对 [Guide 15 的 nullable 阶段边界](../../guide/15-conformance-and-staging.md#nullable-的阶段交接)。
Elvis 值与提前返回均通过类型/所有权检查，native 仍按规范延后。
简单 Elvis 及含 inout 的组合返回 UnsupportedNode；另两个提前
return 形态经两入口返回 MissingFact，错误分类需在 R4 审核。
等价的显式 if 对照经两入口均成功构建运行，输出 `bypass` 和
`return`。本次未增加 Elvis lowering 或声称已覆盖双宿主原生。

## 201. R2 合批前验收与 R4 基线对照

`89428235` 的树 `ce8d1d3a517a3603d4b036ef852d88176e6795b3` 上，
完整前端 184 个目标共 3063 项通过，含 12 项文档测试，零失败、
忽略或过滤。严格 workspace Clippy 和 CLI 构建通过。单文件
和项目各实跑六组，接受退出路径、保留 `when` 版本及子循环只
return 三组，正常写回、被捕获 break 及子循环 break 的
`when` 读取各报唯一 L0084。公开入口使用 Unit，Single 同源码
对照见 §198–200 的直接回归。

[PR #404 的主干 CI](https://github.com/Halckon/Koven/actions/runs/42720825998)
在 `6ce85b07` 已成功。双宿主定向、预览生产及独立消费、Editor
lexical 和最终汇总通过。PR 专用 Evidence 按范围跳过。

§200 的两个 MissingFact 在 [#404 的 macOS 预览产物](https://github.com/Halckon/Koven/actions/runs/42715152484)
实跑复现。核对二进制 SHA-256 为
`7267328e659281279746162b794b784c86bf17616dbff500cbcfad3296ef4c1f`。
同一基线的简单 Elvis 值报 UnsupportedNode，显式 if 对照正常
构建运行，输出 `bypass` 和 `return`。R4 待查项是已延后能力
在组合中的错误分类，Elvis native 实施仍按 Guide 15 分期。

## 202. R2 Single 批次合入与 Boolean flow 合流

[PR #405](https://github.com/Halckon/Koven/pull/405) 的 head `386e21fe`
经[实际 PR CI](https://github.com/Halckon/Koven/actions/runs/42722572074)
通过。双宿主严格 Check/Clippy、定向和最终门禁通过，两端日志各
逐名确认 13 项 Inout 和 1 项 Map key 回归。Editor lexical 按范围
跳过，macOS 既有 LLDB 用例因 debugserver 权限忽略。六份预览
归档经独立下载，双宿主各 12 项消费命令、哨兵和清理通过，生产
及消费清单的归档哈希与实算一致。无未决审阅线程。

基线 `6ce85b07`，合成 `2e27355f`，实际合入 `144793e1`，
合成与合入树均为 `9a88f98fea12e0a6ab9927c3ee43a953bfd01f14`。
[合入后的主干 CI](https://github.com/Halckon/Koven/actions/runs/42727311444)
已启动，前置文档、格式、Editor 和三个 guard 均通过。

§198–200 三个功能提交与该主干合流。双方共用的历史账本逐字
保留，主干新增 §197 在本批追加内容之前。合流只增加 §197 和
本节证据，crate 内容与 §201 的 3063 项本机验证树逐字一致。

## 203. R4 兼容路由内联 nullable 局部初始化的诊断

§200 的红测在 `89428235` 将 MissingFact 定位到先执行的
`var x: Int? = 7`。含 Elvis 的 callable 超出 Refine 子集，
Compatibility 随后尝试包装已成功降低的局部值，但这条路由
没有默认登记该内联标量局部类型。单独的 `left ?: 0` 已在
Elvis Span 返回 UnsupportedNode。

先复用既有 inline nullable 表示判定，再将分类限制为真实
Compatibility 路由下的具名局部初始化。只有 initializer 已降低为
Value、其前端类型精确匹配 Boolean 或整数的 nullable 内层，且
目标类型映射不存在时，才在绑定 Span 返回 UnsupportedNode。
已登记的映射继续走原适配，String、名义类型、Map 和 Range
保持各自的专用登记与判定。独立复核核对了实际路由、已登记
映射及其他适配位置，未发现阻断。

后端两组共 15 项通过，覆盖正常 CFG 与 SSA/LLVM 见证及 9 项
原生泛型/资源用例。既有泛型 nullable null 映射负控仍通过。
严格 codegen Clippy 和 CLI 构建通过。两种提前退出形态及单独
Elvis 值经本机单文件和项目入口检查通过，构建均返回
UnsupportedNode 且未发布文件。显式 if 对照两入口均原生运行，
输出 `bypass` 和 `return`。Elvis native 能力继续按 Guide 15 分期。



## 204. R2 Boolean flow 批次合入与 R4 合流

[PR #406](https://github.com/Halckon/Koven/pull/406) 的 head `c76fbbfe`
经[实际 PR CI](https://github.com/Halckon/Koven/actions/runs/42727705752)
通过。双宿主严格 Check/Clippy、定向和最终门禁通过，两端日志
均逐名确认六项新增回归。Editor lexical 按范围跳过，macOS
既有 LLDB 用例因权限忽略。六份预览归档独立下载，两端各 12
项消费命令、哨兵和清理通过，生产及消费清单与实算哈希一致。
无未决审阅线程。

基线 `144793e1`，合成 `dbb1c85a`，实际合入 `cc1e5b41`，
合成和合入树均为 `aa23d91496ca0374fcbf65581304be99c12653a7`。
[合入后的主干 CI](https://github.com/Halckon/Koven/actions/runs/42732776388)
已启动。[#405 合入后的主干 CI](https://github.com/Halckon/Koven/actions/runs/42727311444)
已通过双宿主定向、预览、Editor lexical 和最终汇总，PR 专用
Evidence 按范围跳过。

§203 的 R4 代码与 `cc1e5b41` 合流，编译器源码及测试与
`aaddfcae` 的本机验证树逐字一致。合流保留主干插入的 §197
及追加的 §201–202，并保留两边原始记录。

## 205. R0 借用结果的阶段拒绝消费者

迁移 `borrowed_result_source_move_stays_a_frontend_diagnostic_before_ssa`
到 strict MemoryDocument Unit。保留原 provider 和调用源码，直接
核对 parse/names/types 零诊断、ownership 唯一 L0135，且主位置
是 `consume(source)` 的实参字节。相同源码的公开单文件和项目
均拒绝且只有 L0135，项目诊断另保原文件身份。

独立复核确认本用例检查的是语义准入。旧 lowering 入口的 ownership
拒绝由原专用负控覆盖。调用方 CFG 与块返回的 Legacy UnsupportedNode、
全文件发射未选 Map wrapper 和 nullable factory 边界仍使用原
辅助入口，属于各自的路径见证。

本地 codegen 借用结果合同 12 项通过，2806 项过滤。frontend
借用 continuation 和结果套件分别 14、12 项通过，无过滤。
三组均零失败、忽略。

## 206. R0 Map slot 返回消费者与直接调用清点

只读 Map 包装保留原始两条声明和空 `entry`。在 strict MemoryDocument
Unit 中显式选择原 `view`，按声明 Span 确认实际函数，核对
`MapRequireValue` 的 slot loan 直接进入 BorrowReturn，且无 Copy
或 SharedRetain。只选择空 entry 时按同一声明 Span 确认 view
未发射。独立复核另核对旧 whole-file 发射见证仍单独通过。这样
分别验证普通借用返回合同和两条路径的实际实例规划边界。

按 §151 的调用形状在 `cc1e5b41` 重跑清点。较 §177 的
`fae780dc`，frontend tests 的 `resolve_names` 由 67 文件 /
135 行增至 68 / 138，`check_types` 由 56 / 89 增至 57 / 92。
新增三处来自 Inout 的三入口对照及 Map 的两处 Single 见证。
其他计数一致，含 codegen src 的 `check_types` 15 / 18、
`check_ownership` 14 / 17、`lower_scalar_file_with_entry` 4 / 8。
§205–206 的迁移保留共享旧入口给独立负控，因此直接调用计数
本身无法度量迁移完成度。此口径仍不覆盖间接调用或仓外 API。

最终候选直接复跑 `require_tests`，三项通过、零失败或忽略。
空入口对照按独立复核意见检查实际函数 origin，不靠缺少借用
元数据推断未发射。

## 207. PR421 续接 owner/borrow 主干批次的冲突修复

输入为 PR421 `4e201f4b` 与已合入 #420 的 main `3b7a9a7a`。六处 refinement
冲突按实际模块归属解决：保留根级 statements 与既有 emitter operation 分派，同时接入
新主干 receipt 的初始化、alias/statement、layout、terminator、fold 与 finish hooks。
新增 folding 子模块与 main 正文一致；owner 故障枚举及线程局部状态恢复到 main 的
canonical build helper，仅保留一份。旧宿主下独立 checkpoint 正文不变，消费者指回
其实际宿主；未用复制 fault 状态或删除故障分支解决编译问题。新主干增长后恢复的职责文件
不算作原薄壳退役数量的再次验收，也不表示 E3 完成。

前一轮 head 的 [CI run 43456413689](https://github.com/Halckon/Koven/actions/runs/43456413689)
双平台 Targeted Tests、Check/Clippy 均成功，但 macOS Preview Consumer 被取消，汇总
记录 abandoned；该 job 的 runner 名为空、无步骤且无可取日志。只据这些数据不能确认
取消根因，更不能把未执行记为通过；新 main 输入亦不能复用旧 head 全绿结论。

| 本轮验证 | 实际结果 |
|---|---|
| `cargo check -p lang-codegen --all-targets --features loan-observation-test-support` | 退出码 0 |
| 新增测试名称有界选择（feature） | 98 passed / 0 failed / 55 ignored，退出码 0；ignored 为 CI-only，未伪造 CI 环境本机执行 |
| 原有 receipt 协议及 BorrowReturn/branch-forward/nullable 分类定向回归（feature） | 78 passed / 0 failed / 0 ignored，退出码 0 |
| 默认配置 Map require、nullable 分类与 owner return 定向回归 | 10 passed / 0 failed / 0 ignored，退出码 0，含 single/Unit native require 入口 |
| `check_borrow_faults.py --list-only`（指定 LLVM 与既有 target） | 62 项库存验证通过；只编译并列出，没有执行。首次未指定 target 的尝试超时，不计通过 |
| 默认及 feature 的 all-targets Clippy（`-D warnings`） | 均退出码 0 |
| fmt、文档与 Rust 尺寸门禁 | 均退出码 0；emitter 957、statements 702、canonical fault helper 252 行 |
| 独立只读审查 | 核对冲突边界、receipt hooks、单份故障状态与 checkpoint 保全，未发现本轮具体问题；不替代新主干全部实现及 CI-only 行为验收 |

待新 head 的双宿主与 Preview CI 真实验收；本轮不归档 Spec，不把清单核对、编译或忽略项
当作完整回归通过。

## 208. R1 调用 poison 的诊断与索引闭合归属

独立复核在 `cc1e5b41` 用公开单文件和项目入口复现 `f(0 β)`
及 `f(0 async)`。Lexer 错误之前多了一条指向同一起点的空
L0034。正式 Parser 红测同样失败。真实下一实参及普通非法
token 的两种 L0034 继续遵循 Guide 07。

调用仍将 Invalid／ReservedWord 交给 Pratt 恢复为 Error value，
同时保留真实逗号、后继 argument 和外层闭合符。索引恢复若只看
Parser 诊断增量，会把 error postfix 消费的内层 `[` 的配对
`]` 当作索引自己的闭合符。改用已消费 raw 范围内的 Lexer
锚点与原 Parser 诊断共同判断，再走原配对和无配对回退。原锚点
验证数组在有 Lexer 诊断时原地累计，准备 O(n)，每次查询 O(1)；
无 Lexer 诊断时不分配。独立复核核对范围边界和配对顺序。

隔离 target 的调用、块续行、索引和词法 poison 压力四套共 81 项
通过，零失败、忽略或过滤。覆盖同／跨行、保留字与非法字符、
跨行分组的两条后继声明、内外层配对、Error key Span 和尾项。
扩大前端 Lexer／Parser／Source 验收在运行。

## 209. R3 部分构造的循环退出与 Abort 本机对照

在现有 normal／return 用例之外，按 Guide 10、11 的 operand
求值和 Abort 合同复审两个构造入口。普通类和 `arrayOf<Leaf>`
先将 MoveOnly 前缀交付给待提交构造，后续实参分别执行
break、continue 或 `error("construction-stop")`。单文件及项目
共 12 组均通过 check 和 build，再比较真实进程输出和退出状态。

| 退出 | 两类构造的实际运行 |
|---|---|
| break | 退出 0，按顺序输出 jump、prefix、exit，prefix 析构一次 |
| continue | 两轮各输出 jump、prefix，最后输出 exit，退出 0 |
| Abort | SIGABRT，stdout 仅 jump，stderr 首行为 `abort: construction-stop`，带实际源码位置 |

独立负对照 `Pair(prefix, prefix)` 的两个公开入口均只报告 L0131。
主位置是第二实参，标签保留第一次移动与第二形参声明位置。

本机 arm64 CLI 产物 SHA-256 为
`f2bcb4cf86d9302814e5a44f84b576e75d770cb6e931ea0d36e57b88c17164c5`，
来自 §203 的构建。生产源码与 #407 head `2155d5f3` 一致，
两提交的 crate 差异只涉及消费者测试。本节证据限这 12 组
直接循环退出、两组所有权负控及本机环境。更广的循环、资源
故障和双宿主动态矩阵仍按 R3/R4 门槛验收。

## 210. R5 本机 conformance 与已合入双宿主基线对照

在 macOS arm64 用 §209 所述 CLI 和 `conformance/baseline-aef3f4f8.tsv`
执行完整语料回归门禁，证据标记 `7c18d202`。154 个 probe 中 137 个
达到预期，分别为 121 个原生运行、13 个诊断和 3 个 Abort。
六个 v1 验收程序均属已登记未实现。23 项已知未实现包含这六个
程序，内部错误、入口差异、回归、意外通过、未审阅和错误输出
均为零。P2 基础 A 组 58/74，目标至少 70 项。

独立下载 [#406 实际 PR CI](https://github.com/Halckon/Koven/actions/runs/42727705752)
上合成提交 `dbb1c85a` 的 Linux 和 macOS conformance 证据。
三份产物分别通过 `--verify-evidence`，两份托管产物另通过
`--compare-evidence` 的同提交双宿主验证。三份 `matrix.tsv`
逐字节相同，覆盖 160 个用例的单文件和项目结果记号，SHA-256 为
`c774542544b88775c12e95b9e8437362cbaa72d450257785e368303cb27579e6`。

本机与托管基线的对照范围是结果矩阵，各自产物的原始记录单独
验证。该次覆盖用于确认候选变化未改变语料分类。R5 的同一冻结
候选、sanitizer 以及完整验收程序仍需单独收口。

## 211. R1 扩大验收与 R4 本机 LLDB 断点复跑

§208 的扩大前端检查完成。隔离 target 的 frontend lib 及
75 个 Lexer、Parser、Source 集成目标共 967 项通过，零失败、
忽略或过滤。用同一代码树构建公开 CLI，SHA-256 为
`5f0c2586fed22f9b4b959b690645747c0626a30e27e14a0b175a1d79459f41b3`。
单文件和项目分别验证六种错误输入。非法字符只报 L0001，
保留字只报 L0002，真实缺分隔符和普通非法 token 各保留 L0034，
跨行分组按序报告 L0001、L0010，嵌套下标各保留两个 L0001。
两入口共 12 组退出 1，诊断序列与预期一致。

#407 精确 head `2155d5f3` 的本机 codegen lib 在普通运行中
通过 LLDB 行表定位，实际进程断点用例按权限默认忽略。单独执行
同一个测试二进制的
`llvm::debug_tests::lldb_hits_a_koven_source_breakpoint_and_reports_the_frame --exact --ignored`，
1 项通过，零失败或忽略，2818 项过滤。断点命中并得到
`app at debug.ko:4:5`。测试二进制 SHA-256 为
`cc114a9a9b3666f77dcaf1f931b2e0a254384efc17c90c8480a01c77ab4cdaa2`。
该证据覆盖本机真实调试器启动；托管 CI 的权限边界仍单列。

## 212. R4/R5 已合入主干的动态资源基线

[主干 CI 42732776388](https://github.com/Halckon/Koven/actions/runs/42732776388)
在 `cc1e5b41` 实际完成双宿主定向、预览生产及独立安装、Editor
lexical 和 CI Passed。PR 专用 Evidence 按范围跳过。

独立下载两个 generated-owners 归档并核对干净编译器提交、完整
16 例清单、输入哈希、诊断及 native 进程结果。各宿主 8 个合法
程序和 8 个拒绝程序与重新计算的 oracle 一致。合法程序对应 24
次预期分配，逐例 counter 运行成功，非法程序未产出 LLVM。Linux
另核对合法程序的 ASan/LSan、检测器校准、检查器变异和最小化
复现。macOS 的资源计数通过，ASan/LSan 校准按既定平台限制
标为 partial。

另核对同一主干的 Linux x86_64 native-sanitizers 归档。
LLVM 21.1.8 下 12 个 ASan/LSan 开关对照的退出状态、输出和
故障类别符合预期，报告能定位实际测试函数和四字节泄漏。
Map 正控与 12 个 provider 程序的 ASan/LSan 输出和退出结果
通过，provider 的源码、IR、输出 oracle 及函数清单哈希一致。
此为已合入主干的动态证据，生成 Koven LLVM IR 的 UBSan 未覆盖。

## 213. R3 两个构造前缀与嵌套词法析构顺序

复用 §211 的本机 CLI，以两个带析构标记的工厂实参扩展 §209。
普通类用逆声明顺序的具名实参，数组用相同的 operand 顺序。
两个入口各实跑普通类和数组的正常提交、break、continue、
return、Abort，共 20 组通过 check、build 和真实进程对照。

两种构造都先按源码顺序完成 B、A。break、continue、return
先按 A、B 逆序清理未完成的构造前缀，再清理循环体内资源。
break 和 continue 保留外层资源至循环后，return 先清理外层
资源再回到调用者。正常提交后普通类按字段逆序清理，数组按
元素逆序清理。Abort 只留下已完成求值的标记，SIGABRT 退出，
无析构输出。初版工厂试图从借用 String 参数移出值，两入口
均正确报 L0133；改用 `own` 形参后执行上述矩阵。

§211 同树的 frontend all-targets Clippy 以 `-D warnings` 通过。
本节构造对照覆盖 macOS arm64 的可观察输出和退出，资源计数
及 Linux 动态证据仍以各自实际测试范围为准。

## 214. R2 已检查的循环跳转可达性与条件写回

以 `69b76b5b` 的编译器源码为基线，两种公开入口实跑发现
`return` 之后的 break/continue 和前一实参已 return 的后继
break 都会让 Elvis fallback 的写回污染正常布尔合流，后继
`return x` 误报 L0084。正式回归覆盖三个类型入口，旧实现
先在 Single 的断言失败。

Single 和 Unit 分别记录原 AST 表达式身份与 source-qualified
身份。已检查的正常合流排除不可达跳转。Unit 仍独立计算局部
循环完成性，Single 与 Unit 在检查不可达代码内部时维持保守的
静态诊断。调用扫描依已检查的完成性保留 callee 及可求值的
实参前缀，包含实际退出的 operand。循环预扫描仍以语法保守
收集。独立复核核对了跳转身份、trial/replay 回滚、静态假分支、
不可达创建点的 lambda 及嵌套循环出口。

六组前端套件共 370 项通过，零失败、忽略或过滤，前端全目标
Clippy 与 CLI 构建通过。公开单文件和项目各 7 组，四组无
可达写回通过，真正能到循环出口的两组均只报 L0084。
另有一组把写回后 return 与可达 break 放在互斥分支，当前
两入口仍报 L0084。该路径需要将写入与各跳转边关联，单列为
后续 R2，不能以跳转可达性代替分支内效果。CLI SHA-256 为
`e2bd71ba0fda89436522721a334c3b4ecb9b380d16df67dc5b48a40a17b01cd0`。

## 215. 2026-11-14 主线对齐与阶段复核

远端 main 固定在 `51a3aad3`，合流提交为 `bf25f740`。
保留 #421 新增的 §207，本地历史证据顺延为 §208–214。
#407 已合入 `fbfb5a3a`。#419 推进 Map 迭代及 nullable borrowed
cleanup 的细化层迁移。#420 接入 owner/borrow 凭证批次，#421
完成本批测试宿主退役及凭证协议合流。

[主干 push CI](https://github.com/Halckon/Koven/actions/runs/43551796637)
实际通过双宿主 Check/Clippy、定向测试、预览生产、独立消费、
Editor lexical 和最终汇总。PR 专用 Evidence 按触发范围跳过。
§208–214 保留原实测提交身份，合流后的验证另记。

| 阶段 | 对齐后判断 |
|---|---|
| R0 | 公开入口及主要产物已有清单。近期大批模块退役后仍需按现行树复核旧入口消费者及保留理由。 |
| R1 | 已合入多批语法恢复修复。调用 poison 去重和索引闭合归属随本地前端批次验收。 |
| R2 | F11 已登记的复现已有修复证据。布尔合流的条件写回仍有具体误拒，下一步继续核对分支与退出边。 |
| R3 | 已增加 owner/borrow 凭证和构造析构实跑证据。仍需在同一候选树复核循环、退出边和阶段准入。 |
| R4 | Map 迭代、可空清理及测试宿主退役已推进，Unit lowering 仍存在 Compatibility 路由。继续核对保留家族、负例和动态资源证据。 |
| R5 | 最新 main 双宿主 CI 提供了候选基线。第一轮冻结树纵向复审待收口，v1 冻结后的发布复审仍待启动。 |

## 216. R2 按分支保留可到达循环出口的写回

在 `bf25f740` 上重现 §214 的互斥分支误拒。正式回归先在 Single
对 `if` 的写回后 return 臂与无写回 break 臂报 L0084。对照覆盖反向
分支、裸 return、when 分支、真正写后 break，以及两臂之前的
实参写回。最终回归通过 Single、MemoryDocument Unit 和 Rooted Unit。

共享扫描复用已检查的分支完成性，为 if 与 when 的各分支分别
决定保留的写入。循环预扫描仍保守。初次复测发现 Single 在未
创建的 lambda 内丢失应有的诊断，该上下文恢复保守的分支检查，
原回归随修正版通过。修正版十组前端定向套件共 452 项通过，
零失败、忽略或过滤。分支正常完成后再由后继语句 return 的
组合仍需按顺序控制流核对。

前端全目标 Clippy（`-D warnings`）、格式、文档、Rust 尺寸
和 diff 门禁均通过。独立复核核对了分支完成性、循环目标、语法
预扫描与不可达 lambda，复跑 `type_callable` 的 40 项均通过。

## 217. R2 嵌套分支写回与后继 return

在 `5588afae` 上实跑，内层一臂写回后正常完成并在后继
语句 return，另一臂 break。旧扫描把这两条路径混在一起，
导致后继非空读取误报 L0084。

扫描在循环跳转处捕获抵达该跳转的写入前缀。互斥分支分别扫描，
正常出口的写入在分支结束后合并，无正常出口的块只留下已由
循环跳转捕获的写入。循环退出时按表达式身份补齐，语法预扫描
仍保守收集。独立复核发现初版在嵌套循环重复收集写入，去重后
复核通过，28 层循环回归验证后继仍能看到真实写回。

10 组前端定向套件共 453 项通过，零失败或忽略。回归覆盖
Single、MemoryDocument Unit、Rooted Unit，以及真实写后 break
和跳转前的共同写入。独立复核另外登记了既有精度边界，包括
when 后续筛选、右侧提前退出的赋值和仅能 return 的循环体。

前端全目标严格 Clippy、格式、文档和尺寸门禁通过。

## 218. R2 赋值操作数提前退出

在 #425 合入的 `28a3c290` 上复现。赋值右侧执行 break，另一路
return，旧扫描仍把赋值目标带入循环出口，后继非空读取误报 L0084。

共享扫描按目标、右侧、写入的顺序收集。已检查的任一操作数无正常
出口时，保留已执行的操作数效果，省略未到达的写入。语法预扫描
继续保守收集。回归对照正常赋值后 break，以及右侧先通过 inout
实际写回再 break。

Map 下标赋值另有直接复现。Unit 检查接收者与键时不产生 MapGet，
扫描需消费两个实际操作数的完成性。键先 break 时排除未求值的
右侧，有真实写回的键仍使后继事实失效。普通赋值的排序对照也
覆盖右侧一臂 break、另一臂可求值为 null 后再 return 的形态。

最终八组前端定向套件共 426 项通过，零失败、忽略或过滤。
新增用例经 Single、MemoryDocument Unit 和 Rooted Unit 运行。
独立复核核对了下标接收者与键的顺序、跳转前副作用和语法预扫描。

## 219. R2 when 筛选写回与正常出口

在 §218 工作树上实跑，较早的 when 分支匹配并正常完成，后续
条件若求值则写回，随后所有分支均直接 return。旧扫描把后续
条件写入计入较早分支的正常出口，导致后继非空读取误报 L0084。

对本次扫描所进入循环中的 when，带 else 且筛选操作数可正常
完成时，按源码顺序记录能
到达正常分支出口的筛选前缀。筛选期间已到达循环跳转的写入
仍由当前循环捕获。省略 else 时保留可能全部不匹配的路径，
语法预扫描保持保守。正式回归经三个类型入口核对正常臂、
else、真实 break 及没有 else 的对照。六个定向套件共 409 项
通过，零失败、忽略或过滤。

本次使用分支体完成性，筛选条件已覆盖整个剩余域而令后续分支
不可达的场景，仍需要结合现有可达性事实复核。
独立复核还指出同一筛选条件的匹配与未匹配边可能有不同的写入。
例如 `other || (write(&x) && false)` 只在条件为 false 的路径
写入。该形态与剩余域耗尽的对照留待实际复现和分边验收。

## 220. R2 穷尽后的 when 分支可达性

复现 Boolean subject 的筛选已覆盖整个域，末尾 else 的分支体
自身可正常完成，但任何输入都无法到达该分支。旧扫描把它当作
正常出口，误保留只通向 return 的筛选写回并报告 L0084。

Single 与 Unit 在检查完 entry 后，将其已知不可达性登记到现有
分支完成性账本。静态检查仍会验证分支体，扫描使用真实入口可达性。
独立复核核对了 StatementId、trial 回滚、循环重算和不可达 lambda。
六组定向套件共 410 项通过，零失败、忽略或过滤，新增回归覆盖
三个类型入口。单个筛选条件真假边有不同写入的情形仍待处理。
已用 #425 本机 CLI 在单文件和项目入口重现该真假边问题，两端
只在后继非空读取报告 L0084。现有 Boolean outcomes 记录真假
可达性，仍需为匹配边和未匹配边区分写入来源。

## 221. R2 循环体仅从 callable 退出的写回

实跑确认，while 循环体内写回后直接 return 时，旧扫描把写回
带入外层循环的正常后继，误报 L0084。零次内层迭代的路径
并未执行该写回。

共享扫描消费两种检查器已有的循环块完成性。没有正常尾边时，
在退出当前循环帧前截去体内普通写入，保留已在该帧的 break
或 continue 捕获的写入。while 条件和 for 源表达式在体内
截断范围之前。语法预扫描仍保守收集。

八个定向套件共 427 项通过，零失败、忽略或过滤。新增用例
覆盖 while 与 for 的直接 return，以及真实 break 和 continue。
独立复核未发现漏记真实写入，另指出静态不进入的循环体以及
§220 的 when 真假边仍需分开验收。

## 222. R2 短路筛选的匹配、未匹配与跳转

三个类型入口的回归覆盖无 subject、带末尾 else 的 when 中的
单表达式筛选。红测确认 `other || (write(&x) && false)` 的
写回错误进入匹配分支。另一个红测确认，筛选右操作数内的 break
会错误捕获左侧短路跳过右操作数时才发生的写入。

正常筛选按布尔结果区分写入，分支体沿匹配边扫描，后续筛选沿
未匹配边扫描。循环内可能跳转的逻辑右操作数只继承能进入它的
左侧写入，结束后恢复普通表达式的保守合并。右侧没有跳转时，
使用惰性的源码位置索引跳过对左侧的额外扫描。

独立复核枚举 20,000 个真假结果边，并核对源码范围、循环捕获
和两个分支后继，没有发现真实写回漏记。多条件 entry、
subjectful 比较以及无末尾 else 的筛选维持原有保守收集。

最终带跳转索引的八组定向套件共 430 项通过，零失败、忽略或
过滤。前端全目标严格 Clippy、格式、文档结构和 diff 检查通过。
[#425 主线 CI](https://github.com/Halckon/Koven/actions/runs/43593425424)
在 `28a3c290` 已完成且成功。

## 223. R2 静态不进入的 while 循环体

在 §222 工作树上复现 `while (false)` 内的写回污染外层
循环正常出口，后继非空读取误报 L0084。复用布尔结果缓存，
已检查的写回扫描只在条件可能为 true 时进入循环体。
条件仍用于收集自身的副作用及跳转，语法预扫描继续保守收集。

回归覆盖字面 false、分组取反和逻辑组合，以及条件恒为 false
但条件求值先写回的负控。三个类型入口共用同一组诊断预期。

八组定向套件共 430 项通过，零失败、忽略或过滤。严格
Clippy、格式及文档结构门禁通过。独立复核核对了条件的结构
判定、不可达循环体的静态检查以及语法预扫描入口。

## 224. R0 主线旧入口的直接消费者复查

在 `28a3c290` 按 §151 口径统计直接调用。codegen src 的
`check_types` 为 15 个文件 18 行，`check_ownership`
为 14 个文件 17 行，与 §206 一致。排除函数定义，
`lower_scalar_file_with_entry` 仍为 4 个文件 8 处。
相较旧清单，readonly receiver 的负控因 #421 的模块合并
进入 `single_readonly_method_tests.rs`，仍由测试配置启用。

| 直接调用 | 测试职责与处置 |
|---|---|
| `lowering_entry_contract_tests.rs` 5 处 | 保留 Single 的诊断、源与分析身份门禁，以及 receiver 能力差异负控。 |
| `single_readonly_method_tests.rs` 1 处 | 保留 readonly receiver 能力边界的明确拒绝对照。 |
| `rc_nullable_while_bounded_tests.rs` 和 `rc_nullable_while_observation_tests.rs` 各 1 处 | 保留 Single 与 Unit 的 nullable Rc 循环对照及 lowering 归属见证。 |

以上入口的宿主均由 `cfg(test)` 启用。CLI/LSP src 无直接
`check_types` 或 `check_ownership` 调用。frontend tests
的 `check_types` 为 68 个文件 105 行，`check_ownership`
为 46 个文件 71 行，仍须逐个按实际合同处置。此次清点不含
别名、间接调用和仓外 API，也不涵盖生产 Unit 的 Compatibility。

## 225. R0 保留的 Single 分析与所有权对照

在 `28a3c290` 的测试源中复查两组仍使用旧检查器的直接消费者。

| 消费者 | 保留的合同 |
|---|---|
| `final_type_admission/complete.rs` | `complete_file_types` 的借用身份和分析实例隔离。相同源码重新检查也必须得到不同的分析身份。 |
| `final_type_admission/static_recovery.rs`、`failed_overload_trials.rs` | 保留现行恢复边界的 Single 证据，和 Unit 分别断言诊断根、源码身份及失败候选事实。 |
| `final_type_admission/simple_assignment.rs`、`unit_statement_map_write.rs` | 用真实 Single 产物保留既有赋值诊断和能力边界，与 Unit 发布的写入事实对照。 |
| `unit_borrow_loop_edge_evidence.rs`、`unit_borrow_zero_loop.rs` | 同时观察 Single 和两种 Unit 的循环证据。Single 观测开启前后必须有相同诊断、借用结果、loan 和结束事件。 |
| `unit_loop_no_carry_boundaries.rs`、`unit_loop_break_arm_no_carry.rs`、`unit_loop_no_carry_evidence.rs` | 保留两个所有权入口的未封闭退出、清理、分析身份和阶段拒绝对照。 |

本次为静态处置。以上见证依赖各自真实的 Single 产物，保留至对应
旧合同退役。带 loan observation feature 的目标按各自实际运行
登记。普通前端默认套件的空目标不能证明该 feature 的覆盖。

## 226. R4 可空兼容路径的主线本机证据

在 `28a3c290` 的生产 Unit 入口复查分流。`refine::route`
按单个 callable 的完整准入决定是否进入细化层。通过准入时，
orchestrate 补齐类型需求后调用 Refine。当前生产中仍存在
Compatibility，供未满足完整准入的混合体使用。
`ForcedPerFamily` 由测试配置启用，测试的强制拒绝不能单独
作为生产 Compatibility 都会拒绝的证据。

复用 `native_nullable_when_tests/replacement_if.rs` 的普通
native 源码和独立固定输出。公开 CLI 的单文件及项目均成功
check 和 build，两个实际进程退出 0，stdout 61 行逐字节符合
原 oracle，stderr 为空。单文件增加调用原 `entry` 的
`main`，项目直接选择 `app.entry`。覆盖可空标签、重绑后的
四种 null 比较写法及每种的四种输入组合。

CLI SHA-256 为
`04a50ee10856e06fb1ad665e19fa95d22cf753d82875b78095d337308827ea1f`，
出自 #425 head `e6f70af4`，与合入主干树
`4e7db50c54331796ebb82f78782cc390e3579f5d` 一致。
本轮实跑限定 macOS arm64 的公开行为，未读取动态路由标记，
也未对这 16 个组合另跑 sanitizer。

## 227. R2 if 条件写回与后继 return

单文件与项目 CLI 对同一外层循环复现 L0084。内层 if 的
短路条件只在随后 return 的路径写回，旧扫描也把该写回捕获
到另一条 break 边。定向红测另复现了经调用实参 return 的
条件写回污染。条件表达式自身的完成性不足以区分这两条边。

已检查扫描在循环内根据条件正常求值的真假结果分别捕获
分支写入。无 else 的 false 路径直接参与正常合流。条件
内部已经抵达 break 或 continue 的写入保留在原循环帧。
正常合流按写入表达式身份保序去重，避免嵌套条件重复放大。
语法预扫描继续保守收集，when 保留原有筛选策略。

修复后八个定向套件 546 项通过，零失败、忽略或过滤。回归
覆盖三个类型入口、带和不带 else、实参 return 与 break，
以及 28 层条件合流。独立复核检查了实际写入、正常完成性
缓存、循环捕获和合流，并用 40,000 个短路树对照逐路径模型。
嵌套层各有不同写入时仍可反复复制增长中的写入集合。

上一提交 9251cd2d 的前端默认全量为 222 个目标、3769 项通过、
零失败、12 项 CI 专用 fault-injection 忽略。另核验 §226 同
主线源码树的 macOS conformance 包。相对仓库基线无回归，
121 项原生运行、13 项诊断、3 项 Abort 符合预期，23 项为
已知未实现。能力探针通过 137/154，综合程序通过 0/6。

原始 Elvis 用例修复后通过两个 CLI 入口的 check，build 仍报告
UnsupportedNode。保留同一循环与短路路径的显式 if 探针另见
MissingFact，登记为后续 lowering 复审输入。简化的原生对照
覆盖嵌套分支保留原值和写回后 return，两入口构建及运行输出
一致。原生对照只覆盖 macOS arm64。

## 228. R4 原生窄化值跨分支写回

以 #428 合入的 `467f88b5` 为基线。§227 的显式 if 原生红测
在后继 `return x` 的 x 上报 MissingFact。类型产物已发布该非空
使用。refine builder 的共享窄化槽表在生成另一路 inout 写回时
删除了此值。另一个原因是布尔字面量的 Lowering 仍生成两个
后继，令不会执行的分支参与合流。

Copyable 窄化的可用性现在随实际 CFG 后继传播，并在合流处
保留各入边共同证明的符号。每次新证明重新定义同一个逻辑槽，
由现有 SSA 构造产生块参数。写回只使自身路径上的窄化失效，
表达式读取保存当时的值，后续操作数重检不会改写已读取的结果。
布尔字面量求值后只接真实后继。循环头的语义事实仍由前端
循环预扫描与重查保证，lowering 使用现有 SSA 循环值传递。

独立复核发现重复比较会丢失共同证明，单纯复用旧内值又会
在回边读取写回前的值。两条独立证明和写回后重检均加入原生
回归。另复现了共享槽在二元表达式中重读的问题，回归断言
先读到 7、右侧重检得到 9，结果为 16。独立复核还实跑了
常量 if/while 下的 owner 替换，核对了 return 和 break 输出。
Elvis 原生支持仍依 Guide 15 分期。

macOS arm64 的单文件与项目 CLI 均通过 check、build 和执行，
三条 stdout 与独立 oracle 逐字节一致，stderr 为空。两项原生
定向回归通过，无失败或忽略。独立进程探针另通过普通 loop、
continue、while 每轮重检、回边 inout 写回和恒定条件 owner
清理。当前证据限于本机，R4 的通用双宿主验证仍需随候选进行。

补充验证覆盖 `while(false)`、`while(true)` 与内层 `if(false)`。
计数原生回归执行两次分配和两次释放，逐指针检查重复释放，
并断言可见输出仅为 `after`。写回回归增加了 `wrote` 标记，
四条固定输出证明仅返回路径执行写回。macOS arm64 的
`ssa::refine_tests` 92 项全部通过，包含 map 条件、nullable、
控制退出、refine 与 legacy 准入对照。两项写回回归另行通过。

复用 SSA verifier 套件核对支配、边入参、所有权和 loan，98 项
通过。branch、arm loop 与 fresh loop receipt 定向测试共 9 项
通过，本地默认忽略 38 项 CI 专用故障注入。这些忽略项仍需由
候选的远端门禁验证。

macOS arm64 公共 conformance 串接最终 lowering 候选，指定既有
`conformance/baseline-aef3f4f8.tsv`。独立校验结果包通过，154 行
探针矩阵与 #428 候选的同宿主结果逐字节一致，零回归。137/154
能力探针通过，6 个综合程序仍为已知未实现。编译器二进制
SHA-256 为 `c480193f6357231b22f987df430c9789faa31b7d396bc5afd40f21dff657f836`。
本地候选基于 `467f88b5` 加 §228 未提交改动，证据中的 commit 字段
仅记录构建基线。最终严格 Clippy、格式、Rust 尺寸与文档检查通过。

对同一条 `return x` 增加 SSA 结构见证。默认 Unit lowering
生成的返回值来自合流块参数，两个前驱分别传入不同
`NullableTake` 定义的结果。验证按真实声明 Span 选中函数、
按返回语句 Span 定位出口，两项原生定向回归均再次通过。
这同时验证了该混合控制流能通过 refine 准入和构建。

[#428 合入后的主线 CI](https://github.com/Halckon/Koven/actions/runs/43604465124)
在 `467f88b5` 通过双宿主测试、预览构建、独立安装和 CI Passed。
下载该提交的两端 conformance 归档并复核一致性，本地 §228 候选
的 154 行探针矩阵与该主线结果逐字节一致。主线门禁证据对应
`467f88b5`，候选的双宿主验证另待交付。

为同一写回回归增补默认路由观察。加入未调用的 Elvis 函数前后，
按真实声明 Span 找到 `run`，两种输入均由 refine 完成 lowering，
原生执行各自匹配同一份四行输出。测试复用现有路由观察器。
另用 macOS CLI 调用该 sibling，前端 check 仍通过，build 报
UnsupportedNode，目标文件未产生。这个对照证实无关声明不干扰
已支持函数，且进入调用链后仍按 Guide 15 保留 Elvis 原生边界。

增加同一编译单元中实际调用 `MutableMap<Int, Int>.remove` 的
对照。可达实例的结果布局在函数 lowering 前统一登记，写回函数
仍走 refine，但其 `Int?` 非空取值从 `NullableTake` 改为
`MapResultUnwrap`。测试用真实函数 Span 定位并分别断言两种
SSA 操作。macOS 原生及 CLI 实跑均输出 Map 结果 42 的标记，
随后四行写回行为逐字节一致，stderr 为空。两项定向回归通过。

## 229. R4 非空合流的非法 SSA 负例

从严格 Unit 前端产物经默认 lowering 构建同一 `Int?` 在两条
互斥路径分别判空、最终返回非空值的程序。验证原始 SSA 通过，
随后只将一条入边的块参数实参替换为另一条入边的同类型值。
边目标、实参个数与类型均保持原样。Verifier 精确报告一条
`NonDominatingUse`，位置和来源均指向被修改的前驱终结指令。
该定向负例通过，完整 SSA verifier 定向套件 99 项通过，零失败或忽略。

## 230. R4 写回路径的原生调试信息

复用 §228 的单文件与项目 Mach-O arm64 可执行文件。两份
dSYM 各自与对应可执行文件的 UUID 一致，`dwarfdump --verify`
均报告无错误。以 LLDB 在两种入口分别运行，顺序命中保留值返回、
写回函数、早返回、保留值返回，随后进程退出 0。单文件的帧源
位置依次为 `control.ko:16:25`、`control.ko:1:42`、
`control.ko:10:17`、`control.ko:16:25`。项目帧落在包内
`case.ko` 的同一列，行号各加一。用于验证的 dSYM 和日志保留
在本地临时目录。此次动态调试验证覆盖 macOS arm64。

## 231. R4 写回路径的本机 AddressSanitizer

从 §228 的原始写回与未调用 Elvis sibling 两种源码生成 LLVM
IR。临时导出将入口改名为 `entry`，并加入一次 String 分配、
输出与析构，以覆盖堆路径。复用现有 `sanitize_address` 标记器，
并在 Apple Clang 21.0.0 生成的 IR 中核实 `write` 与 `run`
两者均含实际 ASan 读写报告调用。两份插桩可执行文件在
macOS arm64 以 `detect_leaks=0` 运行，退出 0，含 `owned`
标记的 stdout 与各自预期逐字节一致，stderr 为空。

Homebrew Clang 21.1.8 的 ASan 产物与独立 C 对照均在限时内
无输出。Apple Clang 可执行同一对照，但其运行库明确拒绝
`detect_leaks=1`。本次只建立本机地址检查证据。可达 Map
变体的临时导出未通过解析，仍由 §228 原生回归覆盖，Linux
ASan/LSan 仍需随候选 CI 执行。临时导出代码已恢复。

## 232. R4 折叠字面条件后的普通借用转发边界

本地后端全量发现 §107 的借用返回转发负例失效。
`if (true) { 1 } else { 2 }` 降为单后继后被直线实参检查误当
作普通 Goto，导致原应拒绝的 wrapper 被接受。定向复跑确认
该用例在修复前没有产生预期拒绝。

refine 内部给折叠了源码条件的基本块记录来源类别。普通
借用转发检查在原实参 Span 返回 UnsupportedNode，发射器
仍消费真实的单后继 CFG。独立静审核对了相关 Goto 消费者。
嵌套恢复的 carry 检查使用真实 CFG 且重新核验最终 SSA 边、
根实参与前驱关系，因此继续消费折叠后的实际边。

普通多参数借用返回 8 项、写回 2 项和分配/析构计数 1 项
通过。旧全量会话基于修复前的测试二进制，其失败留作红测证据。

随后全量报告另一组普通借用转发负例，以及两项既有 Map
所有权结构测试。Map 夹具特意保留 `if (true)` 的两个源码臂，
用来核对各自的组件使用、真实 SSA 分支及合流。最终将折叠
限定在无需保持独立源码 if 分支的条件位置，Map 夹具和其验证
保持原状。对最终候选复跑上述四项失败用例所属的定向套件、
两组写回证据及计数析构，共 14 项通过，零失败或忽略。

## 233. R4 与 SPEC-0454 主线对齐

工作分支快进到远端 main `cc83d9ec`，保留 §228–232 的候选
改动。#424 只为通过既有预检的直线标量函数补充迁移触发，
写回候选继续消费已准入函数的事实。对齐后，写回与非法 SSA
证据、Map 两项和借用转发两项共 7 项通过。SPEC-0454 的
标量准入、跨文件顺序、Compatibility 共存及原生结果 9 项
通过。两批测试零失败或忽略。已启动带 CI 同款
`loan-observation-test-support` feature 的后端全量。

## 234. R4/R5 对齐后的 CLI 与 LSP 入口

在 `cc83d9ec` 加本地候选上重建真实 CLI。原写回程序在单文件
与项目入口均通过 check、build 和执行。两端 stdout 与三行
固定预期逐字节一致，stderr 为空，dSYM 均通过结构验证。

用同一工作树构建 `lang-lsp`，经真实 stdio 协议在单文档与
rooted source-set 分别打开该输入。编辑版本 2 将短路末项由
false 改为 true，使实际写回能够到达 break，两个会话仅在
后继读取 x 发布 L0084，UTF-16 范围同为第 15 行第 31 至 32
列（从零计数）。恢复版本 3 后两端发布空诊断，随后正常退出。

LSP 套件 56 项单元测试通过。stdio 格式化测试首次在默认
环境超时，独立本机复跑通过。对齐候选的后端全量与 CLI
conformance 仍在执行，主线 CI 另按 `cc83d9ec` 记录。

在同一 CLI 上补跑格式化后的原生行为。先将 `kovenc format`
的输出写到独立源码，再执行 `format --check`。对格式化结果
再次格式化，源码逐字节一致。随后 check、build、实际执行均
成功，进程输出与未格式化源码的三行 oracle 一致，stderr 为空。

同一候选补验三文件、两包的公开项目入口。把原写回函数
移到 `helpers/write.ko` 并经 `import helpers.write` 调用，
循环与程序入口分别放在 `app/run.ko` 和 `app/main.ko`。
项目 check、build 和执行退出 0，输出符合原三行 oracle。
dSYM 行表包含三个源码文件。LLDB 实际顺序命中
`run.ko:16:25`、`write.ko:2:49`、`run.ko:10:17` 与
`run.ko:16:25`，进程退出 0。跨包调用的写回与调试位置
均经 macOS arm64 纵向确认。

## 235. R4 内层分支写回复验

以主线 `cc83d9ec` 为基线，使用远端同款
`loan-observation-test-support` feature 复跑候选。8 项定向测试通过，
零失败或忽略。覆盖内层写回后 return、另一条 break 边保留原值、
写回后重检、可达 Map 共存、普通借用转发拒绝边界、非法 SSA 入边
及实跑析构计数。R1/R2 修复已由 #428 合入，合入提交为 `467f88b5`。

独立复审实跑发现，内层 `if (false)` 和 `if ((!true))` 的不可达
赋值与 break 仍会参与窄化合流，前端 check 通过，构建在后继读取
报 MissingFact。直接源码 if 在条件块已有 Copyable 非空证明时
也使用实际字面后继，保留控制来源；无此证明的源码 if 保持显式
分支。这保留 §232 中 Map 两条臂的结构检查。新增原生变体同时
执行跳过不可达写回的分支与另一条真实写回后 return 分支。
修订后，两个原生写回用例、两个 Map 结构用例、两个普通借用转发
负例及 SSA 支配负例均重新通过。带相同 feature 的严格 Clippy 通过。
独立复核核验了实际后继、不可达臂的析构登记，以及 Map 结构保留条件。

PR #429 首次 Ubuntu 全量以 3226 通过、3 失败、55 忽略结束。
不涉及非空证明的字面 `while` 和无主语 `when` 恢复原结构边，
折叠限于短路操作数以及已有 Copyable 非空证明的控制点。原
SSA 借用与 Boolean 规划断言均保留。3 项失败已用同一 feature
在本机逐项复现，修改后均通过。独立复审核对了条件入口、
分组和取反传递、原写回路径与折叠来源。

## 236. R0 跳转边界与接口委托测试入口迁移

从 #429 合入提交 `21490e9c` 开始复查 `type_checking.rs`。
两组语言合同改用同一 AST 的 MemoryDocument Unit 与 Rooted Unit，
复用既有 helper 核对 source identity。跳转用例保留原三条 L0142
的消息与主范围，并断言 Unit 的顶层运行时 val 在关键字报 L0167。
委托用例保留两条合法计划、三条非法目标诊断与两条冲突诊断，并
核对非法目标主范围。原文件外目标夹具另含不合法的运行时顶层
val 与无 receiver 的 this。首次 Unit 运行同时报告 L0167 和
L0166。改用合法顶层常量后，仍引用构造器外的真实符号，三条
委托诊断均按原顺序出现。

`type_checking` 103 项、`multifile_type_checking` 288 项通过，
零失败、忽略或过滤。该文件的旧检查器仍有直接调用，包括旧
分析身份与 deferred reason 见证。本批只迁移上述两组。

## 237. R4 合入后的相邻路径与表示边界

从 `21490e9c` 重建 macOS arm64 CLI。独立探针验证四组行为：
Copyable nullable 的 inout 参数判空返回；`while (false)` 内写入；
无主题 `when` 的 false 选择器内赋值并 break；真实赋值后重检与未
赋值路径合流。四组在单文件及项目入口均 check、构建并运行成功，
退出 0，stdout 均精确为一行 `ok`，stderr 为空。

含 `null -> 0; else -> x` 的 `when (x: Int?)` 探针在两个入口
均通过 check，原生构建明确返回 UnsupportedNode。现行 nullable
when 兼容 lowering 的准入要求 NullableHandle 和 HeapOwner 或
SharedOwner。此探针登记为 inline nullable 的实现边界，与本批
已通过的无主题 when 区分。

## 238. R0 数值诊断与 nullable when 的旧测试消费者

在 `21490e9c` 上继续迁移 `type_checking.rs` 的三处 legacy
helper 调用。数值边界用例沿用原源码，两种 Unit 入口均保留
非法 return、非法非空断言、无符号负数、溢出和禁止隐式转换的
原诊断序列，并断言顶层运行时 val 的额外 L0167 位置。

nullable when 的域测试核对 null alternative 的匹配与剩余域、
else 的输入与 body 域以及穷尽后的空域。Unit 用实际的读取见证
核对 else body、源码中的 x 和 Int 类型。成功重载用例同时核对
只发布一个计划及读取见证，两者均归属于选中的 Int 分支。失败
试探的独立断言继续由现有 `nullable_when_unit/mutation.rs` 覆盖。

Single 的 native_eligible 布尔值仍由旧类别用例检查。Unit 的
descriptor 发布来源类别和分支域，native 准入由后端独立核对，
见 §237，故本批没有用 Unit 类型检查替代该旧入口的资格断言。
本机 `final_type_admission` 63、`multifile_type_checking` 288、
`type_checking` 103 项通过，零失败、忽略或过滤。

## 239. R0 名义类型与枚举旧入口见证

在 `21490e9c` 基线上，将两组旧 `TypedFile` 断言并入同源码的
现有 MemoryDocument / Rooted Unit 回归。枚举用例继续核对
L0114 的使用点与根标签，另从合法成员 type-test 的 TypeRef
追到带 source-unit 限定的 case symbol、Shape 根类型和 Int
payload。名义类型用例保留两条 L0091 的源范围及最终准入拒绝，
并核对两个 `Box<Int>` 使用共享实例、Left 和 Right 声明身份
不同，以及字段类型确实指向 Box 的类型参数。

`type_checking.rs` 仍保留旧入口专属的外部 SourceMap 与环境
身份拒绝、deferred 原因、native_eligible 和原分析器恢复与
确定性对照。Unit 已覆盖超大整数恢复和 128 case / 96 条件
压力输入，两个 Single 对照目前分别保留真实旧诊断以及重复
检查的 enum case 表。该文件的其他两组重复测试已由以上 Unit
断言承接。

三个套件最终通过 452 项，零失败、忽略或过滤，其中
`type_checking` 101、`multifile_type_checking` 288、
`final_type_admission` 63。两项测试数差异来自合并既有重复用例。

## 240. R0 关联常量消费者分流与 R4 双宿主产物对账

关联常量用例拆分可见性与缺失成员的合同。MemoryDocument 和
Rooted Unit 对 private class companion、private object 常量均
在成员名报告 L0154，同一 companion 内的私有访问仍合法。
外部访问夹具改用函数体，满足现行顶层声明规则。缺失成员与
禁止从接口继承常量的旧类型恢复见证仍按原源码保留。现行
Unit 对这两例在名称阶段报告 L0080，无法进入同一类型恢复合同。
真实 CLI 对三类非法用例在单文件和项目入口都只报告一条相应
诊断。三个常量套件本机共 62 项通过，无失败或忽略。

下载 [#429 PR CI](https://github.com/Halckon/Koven/actions/runs/43617059887)
的两端 conformance 证据，验证证据包、双宿主结果一致性和
与 #428 合并后同宿主矩阵的逐字节一致性。构建提交为 PR
临时合并提交 `8d9c0ad2`，其 Git tree 与 main 合并提交
`21490e9c` 同为 `51a20d0aafa5224f01f9b7fc5d336bd6481de80a`。
探针能力 137/154，综合程序 0/6，23 项已知未实现，内部错误、
退化与双入口差异均为 0。PR 证据的提交身份仍为 `8d9c0ad2`。
[#429 合入后的 main CI](https://github.com/Halckon/Koven/actions/runs/43621284008)
的双宿主编译与 Clippy 已通过，采集此记录时测试仍在执行。

## 241. R0 常量资格与关联读取迁移

将两个 `type_constants.rs` 测试迁到 MemoryDocument / Rooted Unit。
常量资格沿用六组源码和原 L0156 主范围，覆盖短路 RHS 的运行时
调用。含运行时顶层 val 的用例额外断言 L0167。所有拒绝输入均
验证不发布部分常量事实。关联读取保留五种声明命名空间与双
声明顺序，将运行时读取放入合法函数体。验证读取表达式的 Int
类型与 Temporary 类别，引用同源 LIMIT 声明，值为 7，并保留
两个声明、两次读取的断言。

本机六个前端套件共 514 项通过，无失败或忽略。修改的两个
测试目标通过严格 Clippy。main 合并提交 `21490e9c` 的 Linux
conformance 证据包通过独立校验，137/154 探针与 PR Linux 矩阵
逐字节一致，综合程序仍为 0/6。采集时 main macOS conformance
步骤尚未开始。

## 242. R1 词法错误后 else 的分组归属

在 `21490e9c` 基线上，新红测确认 `if (flag) (value β else fallback)`
在配对跳转中吞掉调用方 else，保留字 async 同形。恢复复用相邻
词法错误缓存，把可见的调用方 else 留给 control parser，普通
分组的 Error 截止于词法错误，缺右括号定位到 else 前。

独立复核给出内外两层 if 及闭合控制块反例。表达式 control body
先认领本层 else，把错误尾保存在可达的 Error statement。闭合
控制块保留原语句，再登记紧邻的错误范围。两个嵌套回归分别核对
innerFallback、outerFallback 和词法 Error，既有分号、下标、调用
实参及完整分组对照通过。仅处理同一行相邻词法错误；跨行隔开的
错误仍按原边界。修订后独立静态复核未发现阻断。

前端单元 848 通过、12 项既有忽略，四个 Parser 套件 125 通过，
零失败。严格 Clippy 通过。重建的 CLI 在单文件及项目入口运行
闭合控制块的 β 和 async 变体，两端各只报告相应的 L0001 或
L0002。main 的 Ubuntu 测试矩阵已通过，macOS 仍在运行。

## 243. R1 扩大验证与 LSP 增量恢复

本批所有 71 个 Parser 集成目标通过。CLI check 6 项、LSP 单元
56 项与真实 stdio 格式化 1 项通过，零失败或忽略。新增操作计数
用例复用既有计数器，比较 32 与 64 个带注释的普通词法错误，
确认命中该扫描且显著 token 访问数满足双倍加常数的预算。

重建 LSP 后经真实 stdio 验证 standalone 与 rooted source-set。
两者 version 1 均无诊断，version 2 在嵌套 if 的闭合 then 块后
分别插入 β 或 async，只发布 L0001 或 L0002。零起点 UTF-16
范围分别为第 0 行第 76–77 列和 76–81 列。version 3 撤回编辑，
两端均清空诊断并在 shutdown 后正常退出。此证据来自
`21490e9c` 加本地 R1/R0 差异。

## 244. R1 跨行词法错误与 else 恢复

扩展 §242：上一批 CLI 对跨行词法错误同样只报预期词法诊断，
但 AST 红测确认换行或带换行注释会丢失内层 else。显式等待
else 时，扫描越过 trivia 和普通词法错误，且只在首个其他 token
确为 else 时消费错误段。前端不跨过任何普通语法 token 或字符串、
插值边界。分号和下标的一般相邻扫描策略保持原样。

红绿用例核对内外两层 else、原控制块子节点与 Error 实际范围，
覆盖闭合块后的换行、带换行注释，以及分跨两行的 β 与 async。
相邻扫描的成功与拒绝路径各用 32/64 操作计数覆盖。拒绝范围
缓存由不可变 token 流决定，跨行错误逐项恢复时可复用拒绝结果。
独立静态复核核对了缓存、试探与终止边界。

本机前端单元 850 项通过、12 项既有忽略，四组 Parser 定向
125 项通过，零失败。严格 Clippy 通过。最终构建的 CLI 在
单文件与项目输入均只报告 L0001 和 L0002。

## 245. R3/R4 交接复跑与合入后的双宿主 conformance

在 `21490e9c` 加未提交 R0/R1 差异上重跑
`lang-codegen --features loan-observation-test-support --lib handoff_contracts`。
22 项通过，零失败或忽略。覆盖 basic / const 的 source、输入、
名称、环境、类型和 ownership 身份混用，合法重排与复用，以及
失败前隔离目标文件、部分写入清理和重试。

从 [main CI](https://github.com/Halckon/Koven/actions/runs/43621284008)
下载 `21490e9c` 的 Linux 和 macOS 原始 conformance 产物，分别
完成完整性校验，并通过双宿主对账。137/154 能力探针通过，
6 个综合验收程序仍未通过，两端矩阵分别与对应 PR 矩阵逐字节
一致。

同一运行的 Linux generated-owners 归档按源码模型逐例重算：
8 个正常程序、8 个所有权拒绝程序，正常路径共 24 次预期
分配。核对输入哈希、诊断留存与无 LLVM 产物的拒绝路径，以及
合法程序的 counter、ASan、LSan 进程状态和字节输出。校准通过。
Linux native-sanitizers 归档记录 LLVM 21.1.8、12 个开关对照、
Map 正控和 12 个 provider 程序通过，Koven LLVM IR 的 UBSan
仍未覆盖。采集时 macOS 生成资源验收尚在运行。

## 246. R4 路由与验证器复跑

在 §245 相同本地候选的 CI feature 二进制上运行 SPEC-0454
标量子集 9 项、非法 nullable 入边 1 项、原生写回 2 项、Map
所有权转移 2 项，均通过。标量子集含实际 native 执行及 Abort，
且以混合程序验证 refine 调 Compatibility。另跑 `ssa::verify_tests::`
11 项通过，其中包含上述入边负例。合计 24 项不同测试，零失败或忽略。

生产入口仍由 `refine::route` 决定 callable 路径。命中 refine 时
构建错误直接上报，闭包 thunk 要求 refine，普通 callable 的
Compatibility 分支仍处理已登记的混合能力。Inout 参数与接收者
进入该分支时明确拒绝。SPEC-0454 的已验收范围仅为直线标量，
其余控制、nullable 和基础设施退役仍需按实际家族核对准入、
构建和回退。§7 据此刷新退出状态。

## 247. R4 Compatibility 的隐式空 nullable 分支

同一 owned nullable when 重绑后直接调用 Borrow 函数，显式
`if (x != null) { observe(x) } else {}` 可构建，去掉空 else
后前端 check 通过而构建在窄化读取报 MissingFact。正式红测复现。
Compatibility 的 heap proof 准入现在按条件极性选择正反臂，
复用既有空臂判定。缺失的非空臂仍无证明入口，原非空臂的
同源 Borrow 校验、空臂清理及末端 verifier 保持生效。

替换用例共 3 项通过，零失败或忽略。隐式分支用例按来源定位
Compatibility callable，并执行同一份四种比较、各四种输入
组合的原生输出 oracle。独立静态复核核对极性、证明身份与析构
交接。严格 codegen Clippy 通过。重建 CLI 的单文件及项目均
通过 check、构建和真实执行，stdout 五行一致，stderr 为空。

同一 main CI 的 macOS arm64 generated-owners 产物逐例核验
通过。8 个合法、8 个拒绝，正常路径共 24 次预期分配；核对
输入哈希、所有权诊断、拒绝路径未产生 LLVM、counter 输出和
真实退出状态。macOS detector 校准为 partial。采集时
`21490e9c` 双宿主 Targeted Tests 均已通过，预览仍在运行。

## 248. R4 隐式空臂的析构序列与主线基线闭环

将 §247 的隐式空臂接入原 exact cleanup walker。按源码身份
核对旧值与替换值各自的 nullable owner 和不同证明。原值为空时
两份 owner 各清理一次；原值非空时先结束旧证明并清理旧值，再
按替换值判空。替换值非空只在调用后结束证明并析构，替换值为空
无多余证明。三条路径与显式空臂用例对照一致。原生用例另以精确
函数身份确认 Compatibility 路由。CI feature 下 nullable when
15 项通过，零失败或忽略。新增回归放在专用子模块，父模块保持
在 Rust 文件行数门禁内。

[#429 合入后 CI](https://github.com/Halckon/Koven/actions/runs/43621284008)
在 `21490e9c` 已完成并成功。Linux 与 macOS 的双宿主测试、
预览生产与独立安装、最终 CI Passed 均成功。该主线证据与本地
§242–244、§247–248 的后续修复候选分开记录。

## 249. R4 nullable 空值臂提前返回

在 §247 候选上，将空值臂改为打印并 return。正式红测此前
报 MissingFact，位置仍为非空臂读取。根因是 heap proof 准入
额外要求空值臂为空，尽管证明只送入非空臂。移除这一条件，空值
臂复用原条件清理和分支 lowering，实际 return 不参加正常合流。
独立静态复核核对每边快照、ControlTransfer 与 BranchExit 的
互斥，以及保留的同源 direct Borrow 准入。

CI feature 的 nullable when 16 项通过，零失败或忽略，
codegen all-targets 严格 Clippy 通过。原生断言验证空值臂
输出后提前返回，跳过后续标记；非空臂继续消费原借用证明。
重建 CLI 的单文件与项目同样输出 replace、replacement-null、
done，退出 0，stderr 为空。该提前返回用例中的替换值为空，
动态输出没有单独证明非空资源在 return 的析构顺序。

## 250. R3/R4 空值臂提前返回的存活资源析构

在 §249 的回归中加入带可观察 deinit 的 Guard。Guard 在判空前
创建，正常路径还要借用它，因而在空值臂 return 时仍是存活资源。
实跑确认空值臂按 replacement-null、drop-guard、
caller-after-return 顺序输出。非空臂保留 observe 与 keep-guard，
并在 after-if 后、after-when 前退出 Guard 的词法作用域并析构。
首次编写的正常路径预期误用了纯内存 ASAP 顺序，经 Guide 10
核对后改为词法作用域顺序。两条路径均只析构一次。

按实际函数身份确认走 Compatibility，并验证 SSA。单文件与
项目 CLI 均退出 0，完整 11 行 stdout 与预期逐字节一致，stderr
为空。codegen CI feature 的 nullable when 套件复跑 16 项
通过，零失败或忽略。本轮只增强回归，未修改生产 lowering。

## 251. R3/R4 空值臂 Abort 与 return 的动态对照

复用 §250 带存活 Guard 的源码，只将空值臂的 return 改为
error("nullable-stop")。按源码身份确认 run 仍走 Compatibility，
且已验证 SSA 中有 Abort。单文件和项目 CLI 均通过 check 与
构建，原生产物由 SIGABRT 终止，stdout 精确为 replace、
replacement-null 两行，无 drop-guard 或调用者标记。stderr
保留唯一错误文本及实际调用位置，单文件为第 14 行第 49 列，
项目带 package 头后为第 15 行第 49 列。

另用等于 null 的相反极性源码实跑 return，输出与 §250 的
11 行 oracle 一致。CI feature 的 nullable when 套件共 17 项
通过，零失败或忽略。本轮没有生产代码变更。

## 252. R4 内层写回后读取与返回

在 #429 已合入的内层写回复现上复跑原三条实际路径，并增加
inout 写入 9 后重新判空并从内层返回的变体。break 仍读取原值
7，跳过写回的路径仍读取原值 7。另在 §250 的空值臂中增加
内层分支，重新写入 heap nullable 根后直接 return，存活 Guard
在调用者恢复前析构一次。指针根另带 deinit 的探针在前端仍报告
deferred codegen facts，因此上述 stdout 仅观察 Guard 的析构。

四个源码经 macOS arm64 CLI 单文件和项目入口分别 check、build
与运行，均退出 0，stdout 与各自 oracle 逐字节相等，stderr 为空。
回归保留原路由断言，并分别检查普通与内层 return 变体的
Compatibility 路由、SSA 验证与真实运行结果。

独立审查另实跑 x != null 和 null == x 两种极性，通过 Bool
参数分别执行写回后 return、非空调用后继续、空值且跳过写回后
继续。两种源码均退出 0，输出一致，三条路径各观察到一次 Guard
析构。静态复核确认非空 proof 只进入对应臂，提前退出不进入合流。

## 253. R3/R4 nullable 资源写回后从内层返回

[#430](https://github.com/Halckon/Koven/pull/430) 已交付 R1/R2 的
Parser 恢复和 Unit 入口回归。其精确头双宿主通过，合入后的
[主线 CI](https://github.com/Halckon/Koven/actions/runs/43631274287) 也已通过。

在合并提交 `32240166` 上复现带 deinit 的 nullable 资源在
when 非空臂内赋值时报 UnsupportedNode。旧借用证明仍有效，
原准入拦截了尚未执行的赋值。候选要求同源普通根赋值及 RHS
完成时的 Named 旧根析构事实，随后复用原赋值路径。RHS 可借用
旧值，完成后先结束旧借用并析构旧值，再重绑。重新判空后从
内层 return 时，Guard 和新值按声明逆序各析构一次。

独立复审核对赋值、RHS 和析构目标的同源身份，以及原有成员、
索引和复合赋值门禁。按复审补强，RHS 实际读取旧字段并在完成前
打印独立标记。macOS arm64 的单文件与项目入口均编译并执行，
15 行 stdout 与 Guide 10 的预期顺序逐字节一致，均退出 0，
stderr 为空。两条正常边带不同 proof 的合流继续单独复审。
最终源码的 nullable when 后端套件通过 18 项，零失败或忽略。

## 254. R3/R4 不同 nullable 根的内层写回与返回

基线为 `d39d0d54`，包含 #430 的 R1/R2 Parser 恢复与 Unit
入口回归，以及 #431 的单根资源替换后提前返回。新用例保留外层
nullable when 的借用，在内层 when 写回另一根、重新判空、
借用两根并 return。基线前端检查通过，后端在重新借用内层根
的位置报 MissingFact。

Compatibility 复用现有 heap nullable if 的非空边证明，并按
既有 CFG 参数携带其他根的 When owner 与 loan。待证明的根
必须没有活跃 proof，已有 If/While proof 仍受原准入限制。
本次复用现有前端事实、SSA 校验和 return 清理，未改语言规则。

独立复审追踪两根的实体和 loan 身份，以及新 If proof 结束与
return 清理的边界。按复审补强，借用函数输出各自的 label。
断言覆盖 RHS 读取旧值并完整结束、旧值析构、新旧根身份、
return 逆序析构，以及跳过写回和原内层为空的正常路径。

本机 macOS arm64 的 nullable when 套件 19 项通过，零失败或忽略。
同一用例经 CLI 单文件和项目入口检查、构建并执行。两个程序均退出 0，
22 行 stdout 与预期逐字节相等，stderr 为空。codegen all-targets
严格 Clippy、Rust 格式、Rust 尺寸和文档结构检查通过。

## 255. R1/R2 分组 when selector 的分支边界与类型上下文

在 `50f12468` 上分别复现换行后的配对分组被上一臂吞作调用，以及
`((null)), true` 中 null 丢失主体期望类型。Parser 用已索引的闭括号
和紧随其后的箭头确认后继分支，并隔离嵌套 when 自己的 selector。
类型检查沿用比较表达式识别分组 null 的规则，保留外层分组、内层
分组和 null 的类型；混合臂继续保留 nullable 流事实。

独立复审定位了嵌套 selector 继承外层停止规则的问题。回归覆盖
嵌套 selector 的跨行实参、原分支体内的合法跨行调用、物理箭头、
双 Unit 入口的有限域覆盖和缺少 nullable 上下文时的诊断。四组
frontend 测试共 430 项通过，零失败或忽略，严格 Clippy 通过。
换行后的复合或未配对 selector、现有后端专用 nullable 描述符对
分组 null 的接受边界仍保留为后续复审项。

## 256. R3/R4 内层 nullable 写回合流后返回

在 #432 合并树上复跑原来的双根提前返回对照。macOS arm64 单文件
与项目均检查、构建并运行，22 行 stdout 逐字节一致，stderr 为空，
进程退出 0。新增用例在内层一条正常边写回外层 nullable 根，另一边
保留原根，合流重新判空后从内层返回。基线的两公开入口都通过前端，
原生构建报 UnsupportedNode。新增原生回归的红测也在内层 when
范围报告同一错误。

写回边已在替换时结束旧借用，保留边的旧 When proof 不能穿过合流。
普通 if 和 nullable when 复用正常出口 proof 校验，在持有单边
proof 的实际前驱结束借用，按已有 SSA 参数传递各边当前 owner。
独立复审核对 proof 来源、别名及待交付实参校验、实际前驱和
owner 参数。提前返回继续使用已有 ControlTransfer 清理。

最终回归用第三个 nullable when 重新判空，在非空臂借用当前值并
return。macOS arm64 单文件与项目各 32 行输出逐字节一致，覆盖
写回非空、写回 null、保留原值和初始外层为空。内层写回后直接
return 的对照两入口各 28 行输出一致。两组均退出 0，stderr 为空。
相邻的判空 if 非空臂同时借用当前值并 return 仍可触发 MissingFact，
留作单独复审。R1/R2 分组 selector 也已通过 CLI 双入口 check。
后端使用 CI 的 loan observation feature 运行 nullable 过滤套件，
265 项通过，零失败或忽略。

### PR 双宿主复验中的 Parser 栈回归

[#435 首轮 macOS 测试](https://github.com/Halckon/Koven/actions/runs/43670948815/job/132682055593)
的 64 层未闭合下标测试在 libtest 工作线程发生栈溢出。本机
arm64 调试构建也复现。将两种 postfix 分支边界判定放到先返回的
非递归 helper，保留求值顺序、错误和分支归属。独立复核确认语义
等价。反汇编显示本机调试构建的递归栈帧由 6960 减到 6672 字节。
同一压力测试在默认测试线程栈通过，手动缩到 2000000 字节仍溢出。
完整 frontend lib 测试为 850 通过、12 项既有 ignore、零失败。

## 257. R4 判空 if 非空臂借用后返回

在 [#435 合并树](https://github.com/Halckon/Koven/commit/8fd09265a7f2d7d1030f5014265cfab73e68975b)
上，§256 用例把最后一次判空改为 `if (x != null) { see(x); return }`。
单文件和项目均通过前端，原生构建报 MissingFact。新增后端测试
在借用实参报告同一错误。原 heap If 入口仅识别单调用分支体。

首个本地候选继续校验已发布的判空、借用目标、实参及调用资格，允许
单个直接根借用之后紧接无值 return。ControlTransfer 先清理待结束
调用借用，在具名 owner 析构前结束该 If proof。返回终结指令要求
对应证明已结束。独立静态复核核对 proof 的唯一入口、原有值
return 前置限制与有关析构顺序。

macOS arm64 实跑 `x != null` 与 `null == x` 两种极性。两份源码
各经单文件和项目 check、build、运行，四个进程均退出 0，输出各
30 行，与预期逐字节一致，stderr 为空。写回非空、写回 null、
保留原值和初始外层为空均有动态证据。借用与 return 之间插入
另一条语句的单文件探针在首个候选上仍能复现 MissingFact。
另用不同根的外层 When loan 保持存活至内层 If return，单文件和
项目各 19 行输出与预期一致，均退出 0，stderr 为空。内层当前
owner 先析构，外层 owner 随后析构，最后恢复调用者。
首个本地候选启用 loan observation feature 的 nullable 过滤套件共 266 项
通过，零失败或忽略。Rust 格式、尺寸与文档结构门禁通过。

### 连续借用之后的普通语句

在首个候选上，借用与 return 之间加入 println，两个公开入口均
通过前端并在原生构建报 MissingFact。新增回归用两次连续 Borrow
调用验证证明保留到最后一次调用，随后运行可观察语句再返回。

按源码和根对已发布窄化使用建立一次跨度索引。当前非空臂最后
一个同根使用必须落在连续的合格直接 Borrow 调用前缀内。末次
调用完成并执行表达式边界清理后，只结束与该调用绑定的 If
proof。之后沿普通 lowering 执行剩余语句和 return，其他根的
When loan 保留。

独立复核发现：写回后内层 If 的窄化使用能让外层尝试以独立资源
调用作为借用前缀。候选先检查每个实参的同根窄化事实，再校验
实际调用与 loan；不匹配时回到普通 lowering。新增测试覆盖
先调用独立资源、写回 nullable 根、再使用内层 If 重新证明。

最终候选先将零实参、构造及所有权原语交回已有派发，再查看通用
调用描述符。独立首句回归增加零实参临时资源构造对照。单文件
和项目入口均完成 check、build 和运行。独立资源、零实参构造、
借用后普通语句及有值 return 四组的 stdout 分别为 10、11、32、
32 行，对应入口逐字节一致，stderr 为空，均退出 0。实跑验证
独立资源保留到作用域退出、临时构造先于后继调用析构、写回后
实际读取新资源，以及调用方收到原有返回值。
最终后端 nullable 定向套件启用 loan observation feature，共 268 项
通过，零失败或忽略。独立资源和零实参构造复用同一条回归测试，
两者均核对实际 Unit 路由、SSA verifier 和原生输出。

## 258. R4 字符串借用结果的比较与 when

基线 `3782b8d6` 上，`borrow val s = view(owner)` 后的字符串
等值比较与字符串 when 在单文件、项目均通过前端，但在预检报
`UnsupportedNode`。比较的原生回归也实际失败。

预检使用已验证的源文件与绑定符号标识读取字符串结果的等值
比较、其直接 if 以及字符串 when。正常退出沿已有借用范围，
提前返回使用前端发布的同绑定结束事实。独立复审发现正常端点
与提前返回并存时，旧门禁遗漏了作用域外后继。修补后，仅将已
证明的返回从正常端点候选中移出，再恢复对作用域外控制的准入。
原有 String 比较和 when lowering 直接消费活动 shared loan。

macOS arm64 与本机 CI 工具链容器内 Linux x86_64 在同一代码
候选实跑五组，覆盖 `==`、`!=`、具名与分组、when 多 selector、
提前返回、正常退役后再次消费原 owner 及返回 when 结果。
20 个公开入口产物退出 0，stderr 为空，10 对 stdout 逐字节
一致。四个对照均在前端拒绝借用期间的 owner 消费，给出 L0135。
两个持久原生回归另覆盖实际指针分配与释放计数。

## 259. R4 Copyable nullable 主体的 null 和 else

基线 `cc2d15d6` 的 `Int?` 主体可通过前端，单文件和项目构建
均报 `UnsupportedNode`。持久回归先在同一基线上复现。
本批消费同源类型描述与所有权计划，按原绑定类别核对根，主体只
求值一次。非空边复制整数或 Boolean payload，原 nullable 包装
仍可用。Map 查询的 present 边沿用该表示的 unwrap；写回清除
分支的窄化，旧 MoveOnly 路径沿用原有所有权校验。

独立复审复现将窄化名作为普通 `own` 或 `Borrow` 实参时被
MoveOnly 门禁截获。Copyable 交由普通实参路径准备，已验证的
分支 payload 可供 Borrow 使用，避免从原 nullable 参数 loan
直接借用不同 ABI 的 inner。回归覆盖分组 Borrow、临时主体、
提前返回后的后继、inout 写回、Map 的 present/missing 和外层
MoveOnly view 在不同分支中的恢复。

最终候选的 macOS arm64 与 Linux x86_64 CI 工具链容器各运行
两组源码的单文件和项目入口。8 个产物均退出 0 且 stderr 为空，
4 对跨宿主 stdout 逐字节一致。macOS 的 nullable when 和
Refine 路由套件分别通过 41、54 项，Linux 的默认配置 nullable
when 套件通过 41 项。严格 Clippy、格式、尺寸及文档结构检查通过。

另用最终 macOS 二进制在两种入口复现 `Boolean?` 的 `null`、
`true`、`false` 三 selector 分支构建报 `UnsupportedNode`，
前端检查均通过。前端现有 nullable when 描述只发布两臂形状，
普通标量 when 的准入也没有覆盖该 nullable 类型。该用例继续留在 R4。

## 260. R4 Boolean nullable 穷举和混合 selector

§259 留下的三臂例在已有两臂修复上仍可通过前端但无法构建，
新增原生回归首先复现 `UnsupportedNode`。本批沿用前端记录的
穷举范围及分组 selector 边界，读取已检查字面量并复用原 when
CFG、nullable presence 检查及 Map result 表示。主体始终只求值
一次。非空边可读取原主体的已复制 payload，混合 null/non-null
入边在合流时失去窄化。普通 Borrow 实参按已发布的非空使用事实，
从当前边的 payload 建立同步借用。

独立复审以两轮循环复现初版错编。循环前判定非空，第一轮在
`when` 后写空，第二轮曾错误输出 `present`。修补后仅同一个
`when` 的 null 未匹配边保存该次主体的 payload，循环回边和
不同分支上的旧符号窄化无法替代实际检查。回归断言按顺序输出
`present`、`null`。另一个回归覆盖首实参借用窄化值、次实参
提前返回及正常后继，观察四次调用各析构一次。独立复核未发现
新的阻断项。

macOS arm64 和 Linux x86_64 CI 工具链容器各实跑五组的
单文件与项目入口。20 个产物均退出 0，stderr 为空，10 对
跨宿主 stdout 逐字节一致。两端各通过 44 项 nullable when
回归，macOS 另通过 54 项 Refine 路由回归与严格 Clippy。
格式、尺寸和文档结构检查通过。

用最终 macOS CLI 再核对两个待处理的 MoveOnly inout 用例。
`String` 整值替换和带 deinit 的类整值替换，单文件及项目
入口前端均通过，构建均报 `UnsupportedNode`。下一项需验证
RHS 完成后旧值析构、新值写入以及返回调用方后的使用与析构。

## 261. R4 MoveOnly inout 整值替换

基线的 `String` 与带 `deinit` 的类整值替换均通过前端检查，
构建报 `UnsupportedNode`。回归先复现失败，再覆盖 RHS 读取
旧字段、提前 return 保留调用方原值、经转发正常替换和析构顺序。

所有权阶段在 RHS 正常完成后按同次赋值与参数身份发布
`ReplacedInout`。细化层消费该事实，通过 `LoanReplace` 在
独占参数存储中析构原值并写入新值。Copyable 保留原操作。
复审发现原 verifier 可继续使用被替换的旧 SSA 值。修补要求
callee 的替换基于正式 Inout 参数的 loan，调用方从原 place
取回替换后的值再继续。持久负例覆盖旧值消费、非参数 loan
替换、共享 loan 与不匹配的类型。

具名 MoveOnly 参数的独占访问由前端检查，SSA 在实参求值完毕
后的调用块中才物化根存储。后续实参提前退出时，从正常清理路径
析构原 owner。堆后备字段在原地址更新，调用结束后外层 handle
仍可用。复审覆盖分块实参、转发、重叠借用和堆字段兼容路径。

macOS arm64 与 Linux x86_64 CI 工具链容器各实跑六组，均覆盖
单文件和项目入口。24 个产物均退出 0，stderr 为空，12 对跨
宿主 stdout 逐字节一致。RHS 标记位于旧值析构之前，调用方读取
新值后再析构新值。后续实参提前退出只析构当时的原 owner。
另用重叠实参确认前端给出 L0135。两端分别通过 46 项 Inout
回归和 18 项 SSA 替换校验，macOS 另通过 44 项 nullable when
及 54 项细化路由回归。前端的多文件所有权与构造测试分别通过
164、8 项，覆盖正常 MoveOnly 替换发布唯一旧值事实、Copyable
与 RHS 无正常出口时的事实边界。前后端严格 Clippy、格式、
尺寸和文档结构门禁通过。

本候选的 macOS CLI 在两种入口复验借入 temporary 后的后参
return、普通借用 String 返回，两者输出符合预期。借入 String?
后判空读取在两种入口仍报 `MissingFact`，借入 Rc 参数读取
payload 在两种入口仍报 `UnsupportedNode`。这两组已通过前端
检查，继续列入 R4。

## 262. R4 借入 String? 判空后的只读 payload

§261 的候选在判空成功后缺少 payload 视图，新增原生成对回归首先
在 `println(value)` 上报 `MissingFact`。本批按已接受的
[ADR-0041](../../adr/accepted/0041-borrowed-nullable-string-payload.md)
接入 `NullableStringLoanBranch`。非空边产生依赖原 shared loan 的
`String` 子借用。LLVM 的判别和后续读取继续使用原地址。

SSA 校验要求共享的 niche `String?`、唯一的非空前驱和可用的父
借用。借用来源随 CFG 参数保留。负例覆盖 exclusive 来源、先结束
父借用、以及跨基本块使用已结束父借用建立新视图。

复审发现纯判空路径也生成了非空借用，影响 Map 返回值的封装表示。
现在仅在存在同一符号的窄化使用、实际存储为 String niche 时生成
子借用。Map presence 的原有两条回归继续通过。另补齐祖先来源
经 CFG 重绑的追踪，并对 `BorrowCall`、`MapRequireValue` 验证
父借用活跃性。每个来源回归先验证合法图，再在前一块结束父借用，
精确核对派生指令的拒绝。独立复核通过这些修补。

macOS 与 Linux 的 CLI 另复验了前缀 `!` 和 `||` 假边窄化的合法用例，
两种入口当前仍报 `MissingFact`。同一 macOS CLI 补查
`!(value != null)` 的 else 读取，以及
`value == null || read(value)` 的右操作数和 else 读取。四个入口
检查均通过，构建均报 `MissingFact`，未产出可执行文件。
组合条件需分别核对短路操作数入口与最终分支入口的借用。
借入 Rc 参数读取 payload 也已在 macOS 复现 `UnsupportedNode`。
扩展的 macOS Map provider 套件途中结束，专项中保留真实的
Map presence 兼容路径回归。

同一生产代码在 macOS arm64 和 Linux x86_64 CI 工具链容器各运行
11 组单文件与项目入口。44 个原生产物均退出 0，stderr 为空，
22 对跨宿主 stdout 逐字节一致。String 组区分 null 与空字符串，
核对反向判空、分组条件、短路 RHS 实际读取、分支内重复读取和具名
源跨调用重用。两端分别通过 11 项 nullable String、21 项所有权
verifier 和 44 项 nullable when 回归。macOS 另通过 54 项细化
路由回归，严格 Clippy、格式、尺寸和文档结构检查通过。

## 263. R4 借入 String? 的直接取反判空

新增原生回归首先在 `!((value == null))` 的非空分支报
`MissingFact`。Refine 准入与构建共享前端所发布的判空事实。
分组保留极性，取反次数的奇偶决定目标交换。非空边继续使用
ADR-0041 的子借用，原生测试覆盖 then、else、双重取反、null、
空字符串及同一具名值的重复读取。

独立静态复审核对了比较极性、非 niche presence 路由、普通分支
收尾与普通 return 结束子借用。复审发现两种退出组合仍有缺口，
macOS 和 Linux 的单文件与项目入口均重新复现：

- 空值分支提前 return 后，在唯一正常后继读取 String 的代码通过
  前端检查，构建报 `MissingFact`。直接判空和取反判空均可复现。
- 分支内读取后执行 `return source`，函数签名返回
  `borrow String from source`，构建报 `UnsupportedNode`。
  直接判空和取反判空均可复现，当前用例尚未验证该专用返回路径的
  借用收尾。

两组问题和 §262 的 `||` 仍在 R4 范围。

同一候选在 macOS arm64 与 Linux x86_64 CI 工具链容器复验
15 组单文件和项目入口，含非空分支内普通提前 return，及返回后
调用方再次借入同一个源。60 个产物退出 0，stderr 为空，30 对
跨宿主 stdout 逐字节一致。

## 264. R4 借入 String? 的短路右操作数与假分支

原生回归首先在 `value == null || read(value, stop)` 的右操作数
报 `MissingFact`。修补按短路前驱交接共享 String 子借用。
对于 OR，左操作数假边进入 RHS，整体假边一定经过 RHS。子借用
可以在这两处使用，到达同时接收旁路的整体真边前结束。
AND 保留整体真边的对应行为。分组与取反继续按实际边交换。
非 String 的原有窄化与类型化的常量 RHS 计划继续沿原门禁。

独立复审指出，嵌套 if 收尾会把外层兄弟分支尚未激活的借用误判为
本层遗留。单文件与项目均复现 `InvalidModel`。修补按目标块把待
激活借用保存在所属分支，进入该分支时才激活。收尾继续拒绝本层
未消费的记录。补丁已复核。

复审另确认两条真前驱共同证实非空时依然缺少合流证明。
`(gate && value != null) || value != null` 的真边读取在双宿主的
单文件与项目均通过前端检查，构建报 `MissingFact`。该问题仍在
R4 范围，与 §263 的返回路径缺口一同跟踪。

最终候选在 macOS arm64 和 Linux x86_64 CI 工具链容器各实跑
20 组单文件与项目入口。80 个产物退出 0，stderr 为空，40 对
跨宿主 stdout 逐字节一致。新增用例核对 null 跳过 RHS、空字符串
与非空字符串、RHS 读取后假分支重复读取、判空位于右侧、两个
同类型的独立来源，以及兄弟分支内嵌套 if 和 while。双宿主各
通过 12 项 nullable String，macOS 另通过 54 项细化路由和
严格 Clippy。缺少真边证明的非法对照在 macOS 两种入口均由
前端以 `L0084` 拒绝。

## 265. R4 短路合流的共同 String 借用与验证成本

`((gate && value != null) || value != null) && read(value)` 的
持久原生回归先在 `read` 的实参上报 `MissingFact`。现在对两条
可达前驱按有资格的 shared String 子借用和同一源符号求交。前驱
各自结束其余借用，并以同一个合流槽传入各自的借用。单独存活的
前驱继续使用自己的槽。短路 RHS 与最终分支消费相同的前端判空
事实，两个同类型参数的身份不互换。

独立复审发现，多组合流串联会保留每个前驱私有的借用名称。
macOS 初版在 8 组构建耗时约 2.8 秒，12 组和 16 组均超过
35 秒。验证器现在可以选用尚未被占用的入块参数名称，统一重命名
已经不可见的来源及其依赖记录，在保留每条路径的关联和活跃状态
的前提下合并等价状态。持久原生压力用例覆盖 16 组同时存在的证明。

复审另用交换回边指出，SSA 参数的当前绑定不能用于反推借用的
直属来源。每条引用现在随边保留直属来源，派生引用继续累积祖先。
结束借用按引用携带的来源失活同源引用和后代。新增单测检查交换
后的两条独立借用及一条派生引用，静态复核通过。

最终候选在 macOS arm64 和 Linux x86_64 CI 工具链容器各实跑
26 组单文件与项目入口。104 个产物退出 0，stderr 为空，52 对
跨宿主 stdout 逐字节一致。新场景覆盖 OR 真边、AND 假边、外层
短路 RHS、while 入口，以及两种同类型来源只有一个参与合流。
macOS 的同一个 16 组输入最终三次构建为 1.35–1.64 秒，每次实跑
输出 32 行。双宿主均通过 15 项 nullable String 和 22 项所有权
测试，macOS 另通过 54 项细化路由、两组各 4 项的 provider 测试
和严格 Clippy。

另用普通直接判空函数核对非空字符串字面量的实参适配。
`inspect("payload")` 传给 `inspect(value: String?)` 在 macOS 的
单文件与项目均通过前端，构建报 `MissingFact`。该实参路径和
§263 的返回路径仍在 R4 范围。

## 266. R4 空值提前退出后的 String 借用

新增原生回归首先在 `if (value == null) { return }` 后的读取位置
报 `MissingFact`。Refine 保留有同源、控制后非空使用事实的
shared String 子借用。正常前驱需各自能提供证明，普通显式返回按
原清理路径结束借用，隐式返回在结束捕获来源前先结束活跃视图。

独立复审给出双前驱反例。两个外层 if 分支各经一次空值提前返回，
后继使用报 `MissingFact`。修补让 if 与短路共用同源借用合流。
不同前驱各自传入借用，同源且已被所有前驱持有的外层槽直接保留。
第二次复审复现了在循环内以新证明替换外层槽导致的
`InvalidModel`。优先保留共同现有槽后，内层新增证明只在所属
路径结束。补丁已复核并增加零次迭代、回边和循环后读取回归。

无 subject 的 `when` 在唯一继续的分支内经过空值提前返回，
随后在外层读取时，macOS 的单文件与项目仍报 `MissingFact`。
专用 `borrow String from source` 返回组合仍报 `UnsupportedNode`。
两项作为独立路径保留在 R4 范围。

最终候选在 macOS arm64 和 Linux x86_64 CI 工具链容器各实跑
33 组单文件与项目入口。132 个产物退出 0，stderr 为空，66 对
跨宿主 stdout 逐字节一致。新增组覆盖直接与取反判空、两种
前驱方向、双前驱、循环内 continue、零次迭代与循环后读取。
双宿主各通过 18 项 nullable String 和 22 项所有权测试。macOS
另通过 54 项细化路由和严格 Clippy，格式、尺寸和文档门禁通过。

## 267. R4 无主体 when 的正常后继与选择条件

原生回归先复现无主体 `when` 分支内通过空值 guard 后，在外层
读取报 `MissingFact`。单一正常前驱直接交接证明，多个正常前驱
通过已有的同源 String 子借用槽合流。仍可能未匹配的语句
`when` 将该路径一同交给合流，准入按对应分支状态求交。

独立复审发现，选择条件本身若执行空值 guard，内层 if 已建立
证明，外层 when 却把匹配和未匹配路径都恢复为选择前的状态。
旧候选的单文件和项目通过前端检查，构建报 `MissingFact`。
修补后，每个匹配条件记录自己的前驱及借用，后续条件只继承
实际未匹配路径。选择条件提前返回时，前面的匹配前驱保留。
末尾未匹配边也在 BranchExit 清理前结束无后续用途的新借用。
借用是否来自外层按槽身份判断，避免中途结束一个借用后把后续
槽位误当作外层借用。修补已独立静态复核。

直接用 `value != null` 作无主体 when 的选择条件在当前前端
不提供对 arm 的直接 String 窄化。修补沿用此前端行为。

最终候选在 macOS arm64 和 Linux x86_64 CI 工具链容器各实跑
40 组单文件与项目入口。160 个产物退出 0，stderr 为空，80 对
跨宿主 stdout 逐字节一致。新增场景包含唯一正常臂、各臂独立
判空、多 selector 匹配、后续 selector 提前退出、无 else 的
未匹配边及继承的外层借用。macOS 前端在两种入口均以
`L0084` 拒绝缺少共同证明的两组对照。
双宿主各通过 21 项 String、22 项所有权和 44 项 nullable when
测试，macOS 另通过 54 项细化路由与严格 Clippy。格式、尺寸和
文档门禁通过。

## 268. R4 借入 String? 实参的临时包装与退出清理

独立调用者仅执行 `inspect("payload")` 时，两种入口通过前端
检查，构建报 `MissingFact`。混合测试中已有 nullable 值会提前
选择细化路径，使该缺口难以观察。Refine 准入现在将借入
`String?` 时实际发生的 `String` 临时值包装计入调用方。
生成继续使用既有适配和临时槽更新，调用先结束借用，再消费前端
的 CallReturn drop。

独立复审确认，后续实参提前退出时，原有 pending owner 门禁也会
拒绝包装临时值。修补只接收同源 Temporary loan、同一调用及实参
的同步 SharedLoan 合同和已确认的 String 包装。普通返回和循环
跳转沿既有路径在 ControlTransfer drop 前结束 loan。最终复核确认
没有把这一准入扩至使用专用终止指令的 BorrowReturn 或
RangeReturn。借用返回源的对照用例有无该包装均报
`UnsupportedNode`，留由专用返回合同继续审查。

字面量使用静态存储，分配身份回归使用产生动态存储的
`.clone()` 临时值。逆序具名调用分别验证形参映射和两个堆分配
各释放一次，顺序为 `[1, 0]`。后续实参提前 return 的用例核对
两个调用中只有一次到达 callee，两个临时值按 `[0, 1]` 释放。

最终候选在 macOS arm64 与 Linux x86_64 CI 工具链容器各实跑
49 组单文件与项目入口。196 个产物退出 0，stderr 为空，98 对
跨宿主 stdout 逐字节一致。新增用例含字面量、具名及分组、函数
产生的临时值、最后一个实参阻止派发，以及提前 return、break
和 continue。双宿主各通过 24 项 nullable String 与 22 项所有权
测试。Linux 另通过 44 项 nullable when 回归，macOS 另通过
54 项细化路由和严格 Clippy。格式、尺寸和文档门禁通过。

另用可继续读取的具名 `String` 调用同一个借入 `String?` 参数。
macOS 的单文件与项目均通过前端检查，原生构建报
`UnsupportedNode`。此例需要保持具名源的所有权，留在 R4。

## 269. R4 具名 String 借给 String? 参数

独立原生计数回归先确认具名 `String` 调用借入 `String?` 参数
无法生成。准入现在核对已选调用的参数类型、实参身份及同源 shared
loan。对于 `borrow val`，前端绑定事实所记录的原来源必须与调用
loan 相符，生成器使用该绑定对应的实际 SSA loan。

这一适配使用同一存储地址。SSA 的 SharedReborrow 允许从
StringOwner 到以同一 StringOwner 为 inner 的 niche
InlineNullable shared loan，保留原来源和结束义务。
同类型原路由仍适用，反向窄化继续依赖 ADR-0041 的非空边。
验证器回归覆盖跨块转交、拒绝反向转换及父 loan 先结束。
原生计数用例验证只有显式 clone 产生一个堆分配，调用完成后
同一 owner 仍可移动并释放。

借用别名的准入差异由独立复审指出。修补前，macOS 单文件和项目
通过前端检查，构建报 `UnsupportedNode`。另用同一来源同时作为
shared 与 inout 实参，macOS 两种入口均由前端报 `L0135`。

最终候选在 macOS arm64 与 Linux x86_64 CI 工具链容器各实跑
55 组单文件与项目入口。220 个产物均退出 0，stderr 为空，110 对
跨宿主 stdout 逐字节一致。新增用例覆盖调用方继续读取或移动、
借用形参转发、借用别名、具名空字符串、Inout 写回和后续实参
提前返回。直接嵌套 `inspect(view(value))` 在 macOS 两种入口由
前端报 `L0164`，本轮的原生别名路径先建立了 `borrow val`。

双宿主各通过 26 项 nullable String 和 23 项所有权测试。Linux
另通过 44 项 nullable when。Linux 首次并行测试在子进程退出后
停在等待状态，保留该次日志。随后在相同容器对同一测试二进制
以单线程实跑上述三组，均通过，之后终止挂起进程。macOS 另
通过 54 项细化路由和严格 Clippy。格式、尺寸和文档门禁通过。

## 270. R4 判空借用与直接借用返回的退出清理

在前一批候选中，`String?` 参数判空后读取 payload，再按
`borrow String from source` 返回另一参数，前端通过而 Unit
预检报 `UnsupportedNode`。持久原生回归修复前在借用返回标记处
失败。预检确认每个返回事实的源单元、函数体范围、声明指定的
`String` 参数根及名称引用，Refine 继续验证实际函数体。

独立复审发现使用词法子借用时会先结束父视图。在进入 Refine
的前提下，直接别名和调用结果两种写法均复现 `InvalidModel`。
返回入口 loan 的路径依次结束待调用借用、词法绑定和非空视图，
再处理 owner 析构；返回的入口 loan 交还给调用方。复审复核了
修正后的顺序及准入范围。持久回归同时检查空和非空分支、
词法子借用、调用方读取结果并消费来源，以及两次动态分配的
释放次序。

不含 ownership primitive 的窄化词法绑定仍报 `UnsupportedNode`，
由既有绑定预检拦截。同一候选另确认在借入 String 临时值后，
下一实参的求值若直接返回入口 loan，单文件与项目构建仍报
`UnsupportedNode`。这两个用例保留为 R4 后续。

macOS arm64 与离线 CI 工具链镜像内 Linux x86_64 各实跑四组
单文件与项目入口。两种判空形式及两种词法子借用形式共生成
16 个产物，均退出 0 且 stderr 为空，8 对跨宿主 stdout 逐字节
一致。原生计数按 Guide 10 的最后使用规则断言：nullable 实参
最后一次调用结束后先释放，返回借用的来源由调用方移交给
`take` 后释放。
旧 nullable when 负例中的直接借用返回签名现在通过预检，组合
函数体继续报 `UnsupportedNode`。测试核对错误所在的函数声明范围。

macOS 定向套件分别通过 nullable String 27 项、borrow result
108 项、Refine 54 项、nullable when 44 项。Linux 容器通过
nullable String 27 项、所有权 verifier 23 项、nullable when
44 项。上述套件均零失败、零忽略。

## 271. R4 窄化词法绑定和调用实参中的借用返回

复现 270 节的两个待修例。直接绑定 `borrow val item = value`
和调用绑定 `borrow val item = view(value)` 均在前端通过，
默认入口构建报 `UnsupportedNode`。局部放开预检后，调用绑定
还会被生成端的旧门禁拦截，普通提前返回则复现
`InvalidModel`。

预检与生成端共用 `NarrowedString` 判定。它消费同一源单元内
的绑定根、非空类型证明、绑定类型和调用的借用来源实参映射。
Refine 仍需用活动 payload view 建立子借用。普通返回与直接
返回入口借用共用清理顺序，先结束待调用借用和词法子借用，
再结束父视图并消费前端析构事实。

借入字符串临时值后，后一实参求值中直接返回 `from` 指定的
入口借用时，准入允许该临时值走同一退出清理。原生计数用例
同时检查跳过调用的退出路径、调用真正执行的路径，以及调用方
读取结果后仍可消费入口来源。三个堆分配各释放一次，顺序为
提前退出的实参、正常调用的实参、入口来源。

独立复审核对了精确绑定来源、子借用先结束及返回借用的保留。
复审提出的转发用例在前端通过后仍报 `UnsupportedNode`，
由直接返回入口借用的声明准入边界拦截。带判空视图的
`return view(source)` 作为 R4 后续用例。

macOS arm64 和离线 CI 工具链容器内 Linux x86_64 各实跑八组的
单文件及项目入口。32 个产物均退出 0、stderr 为空，16 对
跨宿主 stdout 一致。macOS 上直接绑定与调用绑定两种写法
各用单文件和项目检查了活跃子借用期间写回来源，四次均在
前端报 `L0135`。后续转发例的活跃视图和已结束视图两种形态
也在 macOS 两个入口复现了构建拒绝。

双宿主均通过 29 项 nullable String、23 项所有权 verifier、
44 项 nullable when，均零失败、零忽略。macOS 另通过 54 项
Refine、严格 Clippy、格式、尺寸和文档检查。

## 272. R4 判空视图与入口借用转发

复现 271 节的转发用例。视图活跃、视图结束、两条返回边都转发，
以及后续实参求值中提前转发四种形态均通过两个入口的前端检查，
构建时在声明准入报 `UnsupportedNode`。持久原生回归在同一
位置实际失败。

声明预检、退出临时值准入和生成端共用来源核对。它匹配已验证的
来源 loan、调用的共享来源形参和精确实参、转发结果类型，以及
源码中对声明入口参数的名称引用。生成端在能证明是该入口的
情况下使用现有退出顺序，结束待调用 loan 和词法子借用，再结束
判空视图并按前端事实析构。返回的 loan 仍由 SSA verifier 核对
是否唯一来自声明入口。

独立复审核对入口来源、同一入口作为辅助实参时的依赖、以及待
求值调用同时借用子视图和临时字符串时的退出路径。macOS arm64
与离线工具链容器内 Linux x86_64 各实跑四组单文件和项目构建，
其中一组项目将 provider 放到另一个源文件。16 个产物退出 0，
stderr 为空，8 对 stdout 逐字节一致。源码将转发来源换成另一个
参数时，两宿主两个入口均报 `L0162`。原生回归确认 nullable
payload、提前退出的实参和返回来源的释放次数和顺序。

另在 macOS 两入口复现返回调用自身的辅助实参含 `if`（两臂
普通值或一臂提前返回）时构建报 `UnsupportedNode`。将同一
控制流移到调用外，两入口通过。现有调用实参直线路径检查
拦截这两种用例。复审还确认非窄化词法结果跨布尔分支受旧绑定
预检限制。两个控制流边界留在 R4。

双宿主均通过 nullable String 30 项和 nullable when 44 项。macOS
另通过 Refine 54 项、borrow result 111 项，Linux 另通过所有权
verifier 23 项，全部零失败、零忽略。严格 Clippy、Rust 尺寸和
格式、文档结构及 diff 检查通过。

## 273. R4 转发调用实参的分支与退出

复现 272 节记录的实参分支问题。两臂正常合流和一臂提前返回
在单文件和项目均通过前端，生成端对调用的直线路径检查报
`UnsupportedNode`。当转发的来源经同一入口事实确认时，生成端
核对继续边的调用来源 slot 就是入口 loan，并核对没有需要交付
的额外来源 loan。其余转发仍走原门禁。旧直线路径测试的活动
loan 集合不能检查 CFG 的块参数，相关测试改由 SSA verifier
核对，并单独断言转发调用使用的参数和交付的结果 loan。

早先求值的辅助临时值在后续实参提前返回时由前端事实清理。
生成端先结束已开始的临时 Borrow，再析构 owner。独立复审
发现将辅助形参声明为 `String?` 时，原路径会传入
`Loan<String>`，导致 verifier 拒绝。调用路径复用既有的具名
重借用和临时包装，仍用同一声明入口交付结果。
原生测试覆盖正常合流、单臂提前返回、普通 Borrow 和 own 临时值，
以及 nullable Borrow 的具名重借用和临时包装。计数断言提前
退出与正常调用的辅助分配均各释放一次，返回来源最后释放。

另以完全不会派发且没有其他入边的 provider 复现两个实参分支
均提前返回的情形，两种入口前端检查通过，构建仍报
`UnsupportedNode`。该用例另带最先执行的 `tick()`，作为后续
核验参数前缀是否执行、callee 是否保持不可达的回归输入。

复审复核了继续分支与提前返回边上的借用顺序、源码实参映射、
wrapper 的临时 owner 以及旧 CFG 检查。macOS arm64 和离线
工具链容器内 Linux x86_64 分别实跑七组的单文件与项目入口。
28 个产物均退出 0 且 stderr 为空，14 对 stdout 逐字节一致。

双宿主各通过 nullable String 32 项、所有权 verifier 23 项及
nullable when 44 项。macOS 另通过 Refine 54 项、borrow result
113 项及两项旧边界回归。严格 Clippy、格式、尺寸、文档及 diff
检查通过。

## 274. R4 所有实参分支提前返回时的调用前缀

复现 273 节的完全不派发样例。两个公开入口均通过前端，构建报
`UnsupportedNode`。外层 provider 没有其他入边，与返回分支中
实际调用的 provider 使用不同的声明。

Refine 的准入与生成共用判定，核对同源的类型落下事实、所有权
派发事实和 source plan 的无 dispatch key。命中时复用已有的
按实参源码顺序求值路径。发散之前的前缀仍执行，后继实参和
callee 保持不可达。内层直接返回或转发仍按入口来源验证，先
结束待调用 loan，再消费前端析构事实。对 nullable `when`
收窄实参，独立复审发现准入与生成的结果类型边界不一致。
观察与消费均实跑复现。生成端复用同一无派发判定，并将
nullable when 的外层函数准入放宽至同类型 Borrow String 入口
返回。source plan、视图和所有权事实逐项校验继续生效。旧的
nullable when 借用返回负例改为原生验收，核对独立 owner 析构和
返回借用持续有效。

回归使用两个逆声明顺序的具名前缀检查执行顺序，核对后继
实参和不可达 provider 未生成，并检查 Borrow、own、
nullable Borrow 的临时资源释放次数。nullable `when` 用例
分别核对观察和消费，资源析构先于调用方打印返回借用。
macOS 两个入口将实际内层返回换成另一个来源时均报 `L0162`。

下一项另在 macOS 两入口复现：普通 `borrow val item = view(source)`
的词法借用跨布尔分支时，直接退出普通函数和转发借用退出均在
构建时报 `UnsupportedNode`。在同一函数加入独立 `swap`，
两个入口均可执行；将 `swap` 放在无关函数中，两个入口仍报
`UnsupportedNode`。保留探针以核对普通绑定预检的函数范围。

最终 macOS arm64 和离线工具链容器内 Linux x86_64 各实跑十一组
单文件及项目入口。44 个产物均退出 0、stderr 为空，22 对
stdout 逐字节一致。macOS 另实跑 null 臂正常合流、非空臂消费
后从后续实参提前返回的混合形态，单文件与项目输出均符合预期。

双宿主均通过 nullable String 33 项、nullable when 45 项、
所有权 verifier 23 项和单独的 abandoned nullable when 回归。
macOS 另通过 Refine 54 项和 borrow result 115 项，全部零失败、
零忽略。严格 Clippy、格式、尺寸、文档及 diff 检查通过。

## 275. R4 普通 String 借用结果跨布尔 if

复现 274 节的两种提前返回。普通结果绑定跨语句级布尔 if 时，
单文件和项目均通过前端，旧绑定预检在构建时报
`UnsupportedNode`。同一函数中的无关 swap 可切换预检路径，
另一函数中的 swap 不影响原失败。

普通 String 结果绑定的预检按声明、封闭源单元内的语句作用域和
if 父子臂关系核对。活跃路径的直接 Return 只由对应 binding 的
`ControlTransfer` 事实准入；相邻未建立的臂、建立前和已结束后的
后继按各自词法路径处理。只有已证明的返回结束事实时，外层
后继无带借用的普通前驱。空结束集合不能单独提供该证明。
条件运算符沿语法树处理，不进入调用参数、循环或另一个
控制表达式的内部。来源绑定和 SSA 的所有权验证保持生效。

先复现条件比较与短路运算、臂内声明、相邻臂嵌套控制、只有
提前退出的臂，再用原生回归核对独立旗标、短路副作用、正常
合流和提前返回，以及九个分配各自的释放顺序。独立复审核对
了精确的结束事实及原有实参、循环边界。legacy
`lower_scalar_file` 的旧拒绝用例保留，两条 Unit 入口的旧布尔
if 用例改为 verified SSA 验收。macOS 两个入口对活跃借用期间
改写来源均报 `L0135`。

macOS arm64 和离线工具链容器内 Linux x86_64 各实跑八组的单文件
及项目入口。32 个产物均退出 0、stderr 为空，16 对 stdout
逐字节一致。下一项在最终 macOS 编译器的两个入口复现：普通
String 结果绑定跨布尔主体 `when` 时，前端通过，构建仍报
`UnsupportedNode`。

双宿主分别通过 12 项借用结果合同、34 项 nullable String、
45 项 nullable when、23 项所有权 verifier。macOS 另通过
54 项 Refine 和 116 项 borrow result，全部零失败、零忽略。
严格 Clippy、格式、尺寸、文档结构及 diff 检查通过。

## 276. R4 普通 String 借用结果跨布尔 when

复现 275 节的布尔主体 `when`。单文件和项目均通过前端，构建
在普通绑定预检报 `UnsupportedNode`。无主体布尔条件版本也
复现相同拒绝。

普通 String 结果绑定的语句级词法扫描共用布尔 `if`、布尔主体
和无主体 `when` 的控制结构。活跃臂直接 Return 继续匹配同一
源单元的精确绑定结束事实；臂内声明沿语法树连接外层控制和
其他臂。它不进入初始化器或可达调用的实参。原有循环门禁仍
由原路径处理。

独立复审发现，首臂已穷尽布尔域时，后续 nullable selector
和死 else 的 Return 仍被旧表达式预检扫描。两种入口复现后，
改用 typed 的同一闭域臂数和末臂选择器数。死语法只豁免表达式
门禁；所有权分析按同一闭域边界停止发布事实，死臂不作为
活跃作用域或返回路径遍历。复审再次核对了可达选择器内的
Return 仍由独立门禁拒绝。

原生回归覆盖主体单次求值、带副作用的选择器按序执行、首匹配
短路、死 selector 和死 else、提前返回、臂内绑定和合流，
核对十次分配的释放身份及次序。下一项在最终 macOS 两个入口
复现：普通 String 结果绑定跨整数主体 `when` 仍在构建时报
`UnsupportedNode`。

macOS arm64 和离线工具链容器内 Linux x86_64 各实跑六组的单文件
及项目入口。24 个产物均退出 0、stderr 为空，12 对 stdout
逐字节一致。项目嵌套用例将 provider 放到另一源文件。macOS
两个入口对 when 臂内改写活跃借用的来源仍报 `L0135`。
去掉整数主体探针中的结果绑定后，本机对照能构建并输出预期。

双宿主均通过 12 项借用结果合同、34 项 nullable String、45 项
nullable when、23 项所有权 verifier 和新增的原生布尔 when
回归。macOS 另通过 54 项 Refine 和 117 项 borrow result，全部
零失败、零忽略。严格 Clippy、格式、尺寸、文档结构及 diff
检查通过。

## 277. R4 普通 String 借用结果跨整数 when

复现 276 节的整数主体用例。单文件与项目均通过前端，构建报
`UnsupportedNode`。复用 Refine 的整数类型集合，按主体的
具体类型匹配等值选择器。`in` 和 `!in` 采用同源的前端整数
Range 描述，核对左右端点类型及 inclusive/exclusive 运算。
仍由原有生成端决定范围比较、边方向和实际求值顺序。

整数控制中的主体调用实参或范围端点调用实参含算术表达式时，
旧词法扫描在实参位置拒绝。整数控制继续扫描经类型检查的无
receiver 普通直接调用中的运算符，进入条件运算符的路径仍在
嵌套控制处停止。借用结果初始化器、实际可达的选择器内 Return
及循环的独立门禁保持。活跃臂 Return 仍需精确的前端绑定结束
事实，臂内声明使用同一父臂路径。

独立复审核对了具体类型、Range 描述、端点求值顺序、声明
作用域及 Return 门禁。原生回归覆盖主体求值一次、等值首匹配、
左端点先于右端点、右开边界、`!in` 方向、臂内声明和提前
返回，并核对三个堆分配各释放一次。外部形状覆盖全部八种
整数宽度及带类型的等值选择器。

macOS arm64 和离线工具链容器内 Linux x86_64 各实跑五组的
单文件及项目入口。含独立的 Long 范围用例，测试端点超出 Int
范围。20 个产物均退出 0、stderr 为空，10 对 stdout 逐字节
一致。macOS 两个入口对臂内改写活跃借用的来源均报 `L0135`。
另实跑确认，实际求值的普通选择器调用参数中的 Return 仍报
`UnsupportedNode`。

下一批保留两个双入口复现，分别为非观察该借用的 String 主体和
枚举主体。保留绑定的版本前端通过而构建报 `UnsupportedNode`；
两项按去掉结果绑定的本机对照均可构建并输出预期。

双宿主均通过 12 项借用结果合同、34 项 nullable String、45 项
nullable when、23 项所有权 verifier 和新增原生整数 when
回归。macOS 另通过 54 项 Refine 和 118 项 borrow result，全部
零失败、零忽略。严格 Clippy、格式、尺寸、文档结构及 diff
检查通过。

## 278. R4 借用结果跨独立 String 与枚举 when

复现 277 节的独立 String 和普通枚举主体。两个公开入口均前端
通过、构建报 `UnsupportedNode`。语句级词法预检复用现有
作用域遍历。String 选择器核对同类型和字面量叶；枚举核对
同一名义根及实际 case 身份。等值 case 必须是 Copyable 枚举的
无载荷值，类型测试核对同一根下的 case type。活跃 Return 仍
依赖对应 binding 的结束事实，互斥臂仍按语法树独立处理。

独立复审触发两项邻接复现。Copyable 枚举带括号的 case 值在
预检、Refine 准入和生成端共同展开到前端构造事实。MoveOnly
枚举提前读取 tag 的路径只检查可达臂及末臂实际选择器，沿用
类型阶段的闭域边界；实际运行不会执行闭域后的调用。复审
复核了类型、来源、分组求值和闭域。

原生 String/Copyable 枚举回归核对独立 owned String 主体、
返回借用、提前退出及三个分配的释放次序。纯内存 MoveOnly
枚举的原生回归覆盖臂内绑定、提前退出、合流和闭域死选择器。
泛型 Copyable 枚举使用实例化后的 case 类型测试。macOS 两入口
对异枚举根的类型测试均报 `L0106`。

另确认 `Packet.Full(Node())` 的 payload 若为带 deinit 的 Node，
即使去掉借用绑定和整个 when，单文件与项目仍前端通过且构建报
`UnsupportedNode`。该构造路径独立留作 R4 下一项。

同一代码候选在 macOS arm64 和工具链容器内 Linux x86_64
各实跑六组单文件和项目入口。24 个产物均退出 0、stderr 为空，
12 对 stdout 逐字节一致。两个新增原生回归及 12 项借用结果
合同、34 项 nullable String、45 项 nullable when、23 项所有权
verifier 在双宿主均通过，零失败、零忽略。

资源枚举的 macOS 单文件对照中，即使仅构造 `Packet.Empty`，
只要另一个 case 的 payload 带析构，构建仍失败。去掉 payload 的
deinit 后原构造可运行，单独构造带 deinit 的 Node 也可运行并
触发析构。

macOS 另通过 54 项 Refine 与 119 项 borrow result，零失败、
零忽略。严格 Clippy、格式、尺寸、文档结构及 diff 检查通过。

## 279. R4 资源枚举的析构可达性

macOS 上，包含带 deinit 载荷的 Packet 在单文件与项目入口均能
通过前端，随后构建报 `UnsupportedNode`。单独构造 Full、
仅构造 Empty，以及嵌套枚举分别复现失败。失败位于 Unit 的
deinit 规划，原有路径只按 class 和 value class 的字段递归。

Unit 规划将 case 类型归到其 root，复用枚举布局解析得到每个
case 的具体载荷。隐藏析构体按载荷递归登记；LLVM 仍根据活动
tag 选择载荷，按既有聚合 drop 路径清理字段。回归核对从未
构造的 case 也登记其独有析构体，且运行时不执行该析构。实跑
核对移动交付、词法逆序、载荷字段逆序及五个分配的逐指针释放。

独立复审核对了分类事实、析构签名验证、活动 tag 分派、字段
清理和测试可见性。macOS 独立探针另确认泛型函数构造和
`Payload<T>` case 类型测试可运行，资源 enum 臂内的局部借用
可随提前 return 和正常合流结束。

macOS arm64 与工具链容器内 Linux x86_64 各实跑五组的单文件
与项目入口，包括仅构造空 case、具体泛型、跨源嵌套载荷和
借用控制流。20 个产物均退出 0，stderr 为空，10 对 stdout
逐字节一致。

下一项复现已隔离：`Box(Packet.Full(Leaf()))` 中的 Leaf 带
deinit 时，macOS 两入口的前端检查通过，构建仍报
`UnsupportedNode`。去掉 deinit 的对照在两入口均构建运行，
输出符合预期。
带 `Box<Tree>` 边的递归资源枚举在两入口同样复现，前端检查
通过。该用例设定右子树先于左子树释放，作为下一批顺序回归。

双宿主 `resource_deinit` 各通过 59 项，零失败、零忽略，包含
新增的析构体登记与逐指针回归。双宿主扩大 `boxed_enum` 筛选
各 299 项通过、7 项按属性只由专用 CI 故障注入执行、零失败。
macOS 严格 Clippy、格式、尺寸、文档结构和 diff 检查通过。

## 280. R4 Box 载荷的资源析构可达性

macOS 上，普通枚举、具体泛型枚举、递归枚举树和值类作为含
deinit 的 Box 载荷时，单文件和项目均通过前端、构建报
`UnsupportedNode`。普通枚举的纯内存对照在两入口均能原生运行。

Unit 规划将 intrinsic Box 的单个具体载荷交给已有资源遍历。
访问集合按具体类型去重，递归边不会反复登记析构体。LLVM 的
既有 drop 路径执行活动 case 的析构、按字段逆序清理，然后
释放相应 Box。独立复审检查了类型身份、载荷解析、循环终止、
SSA 定义次序、LLVM 析构调用和计数 oracle。

持久原生回归覆盖内联根持有递归 Box 子树，分别核对三个
deinit 的输出和七个指针的逐个释放顺序。双宿主各实跑五组
单文件和项目入口，覆盖移动交付、临时与词法析构、递归、具体
泛型及纯内存对照。20 个产物均退出 0，stderr 为空，10 对
跨宿主 stdout 逐字节一致。普通 class 作为 Box 类型实参的
macOS 双入口对照仍由前端以 `L0117` 拒绝。

当前 macOS 两入口的后续探针中，`Box<Packet>?` 和
`Rc(Leaf())` 后调用 `owner.share()` 使用资源载荷时分别通过前端
检查、构建报 `UnsupportedNode`。两个去掉 deinit 的对照均原生
运行。另有值类与枚举经 Box 相互递归的两组用例，资源和纯内存
载荷均在构建时报同一错误，直接构造活动叶 case 的对照均原生
运行。已登记为独立布局缺口。

双宿主 `resource_deinit` 各 60 项通过，零失败、零忽略。Linux
扩大 `boxed_enum` 筛选 300 项通过、零失败，7 项由专用 CI 故障
注入步骤另行执行。macOS 严格 Clippy、格式、尺寸、文档结构和
diff 检查通过。

## 281. R4 可空 Box 的资源析构可达性

macOS 两入口复现带资源载荷的 `Box<Packet>?` 前端通过、构建报
`UnsupportedNode`。局部保留、移动给 own 参数、null 和空 case
组成第二条相同失败路径；纯内存可空 Box 与非可空递归资源 Box
对照原生运行。

Unit 析构规划对可空 intrinsic Box 沿现有的内层资源路径登记
隐藏 deinit。正常 drop 消费前端发布的所有权事实，LLVM 的
`NullableHandle` drop 在 null 边直接返回，在存在边调用相同
Box 析构。现有 Box 析构先清理活动载荷，再释放句柄。

新增原生回归区分 null 与存在的空 case。输出验证移动送入
callee 的资源在那里析构，调用方保留的值直到词法作用域退出
才析构。分配计数核对两组 String、资源类、Box 和一个空 case
Box 的七个指针均按指定顺序各释放一次。

独立复审核对了前端 drop 来源、nullable handle ABI、移动交付和
分配计数。扩大套件发现旧 SSA 对照仍要求拒绝可空 Box。原来的
空句柄输入改为验证隐藏 deinit 唯一登记且 LLVM 通过，同一测试
继续检查 inline value 和 Rc 的原有拒绝位置以及类型 arena 不变。

另确认 `enum class Tree { Tip(value: Leaf), Link(next: Box<Tree>?) }`
在 macOS 两入口仍通过前端、构建报 `UnsupportedNode`。去掉
Leaf 的 deinit 也复现。递归字段形成 nullable handle 时，当前
类型注册会同步要求完成该 Box 的 payload，并在重入时拒绝。
该布局问题继续单独跟踪。

同一生产代码在 macOS arm64 和工具链容器内 Linux x86_64 各实跑
六组单文件与项目入口，包括跨源交付、`!!` 后转移及 `when` 判空。
24 个产物均退出 0，stderr 为空，12 对跨宿主 stdout 逐字节一致。
独立 macOS 探针另通过具体泛型枚举的可空 Box；普通类的可空
自递归字段也已在两个入口复现布局拒绝。

双宿主各通过 61 项资源析构、45 项 nullable when，零失败、
零忽略，包含修订后的 SSA 对照和新增原生回归。macOS 严格
Clippy、格式、尺寸、文档结构和 diff 检查通过。

## 282. R4 可空句柄的递归布局

macOS 的单文件和项目入口均复现下面两条前端通过、构建报
`UnsupportedNode` 的路径。递归枚举字段使用 `Box<Tree>?`，带
deinit 和纯内存两版都失败；普通资源类以同类 nullable handle
保存后继时也失败。嵌套递归枚举另组合了活动空字段和有值字段。

Unit 先登记 pointer-like owner 的身份。nullable handle 可用
已声明的身份，完成最外层注册前再统一定义 owner 的 payload。
SSA verifier 仍拒绝引用未完成定义或非指针类型的 nullable
handle。SSA 对照先验证未完成定义的负例，再补上自递归载荷并
验证相同可空身份。原生回归核对 null 字段跳过清理、资源析构
输出逆序及六个指针的逐个释放顺序。

另在相同 macOS 构建复跑值类与枚举经 Box 相互递归的独立用例，
资源和纯内存对照在两个入口均继续报 `UnsupportedNode`，对应
直接叶 case 对照均运行通过。

独立复审核对了 Unit 待定义队列、SSA verifier 和 LLVM 入口，
并通过已构建的测试二进制重复执行两个新增回归。保留的测试专用
carry 布局仍同步定义 Box，本批证据适用于公开 Unit 路径。
macOS 两入口仍以 `L0116` 拒绝无指针间接层的原地递归 enum。

相同代码在 macOS arm64 和工具链容器内 Linux x86_64 各实跑
五组单文件与项目入口。20 个产物均退出 0、stderr 为空，
10 对 stdout 逐字节一致。用例覆盖纯内存与资源递归、跨源
嵌套枚举、可空自引用 class 和非递归 nullable Box。

双宿主各通过 62 项资源析构、45 项 nullable when、4 项 nullable
handle，零失败、零忽略。macOS 严格 Clippy、格式、尺寸、
文档结构和 diff 检查通过。

## 283. R4 值类与枚举经 Box 互递归

macOS 两入口实跑复现 `Branch → Tree → Box<Branch>`，纯内存
或资源载荷均通过前端、构建报 `UnsupportedNode`。活动 Link
case 同样失败，而直接构造未装箱的叶 case 正常运行。

首版在 Box 处检测回边。独立复审建立反向复现
`Branch → Box<Tree>? → Tree → Branch` 与双值类递归。两者仍
被拒绝，显示准入依赖遍历起点。最终补丁与 MoveOnly enum 的
处理对齐。取到已解析的具体字段并确认 MoveOnly 后，遇到正在
检查的同一具体值类时结束该次递归，原始遍历继续检查剩余字段。
编译仍消费前端对内联环的判定，并由 SSA verifier 检查生成布局。

两条持久原生回归实际构造两种边方向的链，核对资源析构在作用域
输出之后发生和各自三个分配的逐指针释放。macOS 两入口的直接
内联环继续报 `L0116`；有不支持的闭包载荷旁支时继续由后端
拒绝，活动 case 不会掩盖其他 case 的布局要求。

相同生产代码在 macOS arm64 与工具链容器内 Linux x86_64 各实跑
八组双入口用例。32 个产物均退出 0、stderr 为空，16 对 stdout
逐字节一致。包括纯内存和资源载荷、两个值类互递归，以及
拆为两个源文件的活动递归 case。

共享的值类准入也适用于其他已验证的间接字段。macOS 独立复审
以旧编译器作对照，另确认活动 class 递归及空 Rc 字段用例由拒绝
转为可运行，两种公开入口均复跑。活动递归 Rc 持有资源以及
List 自递归仍被后端拒绝。另准备普通资源 Rc 的共享转移探针，
两入口均前端通过、构建报 `UnsupportedNode`。

双宿主各通过 64 项资源析构和 54 项 Refine 准入回归，零失败、
零忽略。macOS 严格 Clippy、格式、尺寸、文档结构和 diff
检查通过。

## 284. R4 Rc 载荷的资源析构可达性

macOS 单文件和项目均复现普通资源类、资源枚举和可空 Rc
通过前端后构建报 `UnsupportedNode`，纯内存 Rc 对照可运行。
Unit 规划将直接和可空 intrinsic Rc 纳入已有载荷遍历，登记
具体资源析构体。SSA 与 LLVM 继续使用已有的引用计数及前端
发布的 drop 点；null 不递减计数，最后一个强引用的释放
先执行载荷析构再释放 control block。

原生回归分别将非可空和可空的共享副本交给 own 参数，穿插
null 对照，并保留原句柄到词法作用域退出。断言载荷析构恰好
一次、早于同作用域较早声明的资源。计数器还核对三个分配
各释放一次及其顺序。旧 SSA 对照中的空 `Rc<Payload>?` 改为
验证唯一析构体登记和 LLVM 验证，inline `Payload?` 的原
拒绝位置保持。

独立复审核对了前端析构点、nullable null guard、引用递减及末
引用清理的顺序。macOS 独立探针另通过可空参数持有最后一个引用、
嵌套 Rc 和具体泛型枚举载荷。活动递归 Rc 及含资源字段的泛型
class 仍停在既有后端限制。Guide 13 的 Rc 小节仍用 ASAP
描述释放点，Guide 10 对含资源载荷另规定词法作用域；本轮
代码生成继续消费前端按双轨规则给出的析构点。

后续 macOS 两入口探针确认，借入 `Rc<String>` 后读取 `.value`
通过前端但原生构建报 `UnsupportedNode`，own 参数对照运行成功。
另发现把 `owner.value.name` 交给 println 时，借入或 own
`Rc<Leaf>` 两种参数都在所有权检查返回
`InvalidUnitArgumentPlace`。这条复现发生在原生生成前。
去掉 Leaf 的 deinit 仍复现该所有权错误；借入未装进 Rc 的
Leaf 后读取相同字段则原生运行通过。Rc 内 Copyable 字段比较
能通过前端，仍在后端构建失败。

相同生产代码在 macOS arm64 和工具链容器内 Linux x86_64 各
实跑五组单文件和项目入口。20 个产物均退出 0、stderr 为空，
10 对 stdout 逐字节一致。双宿主各通过 65 项资源析构和
4 项 nullable Rc，零失败、零忽略。macOS 严格 Clippy、
格式、尺寸、文档结构和 diff 检查通过。

## 285. R3/R4 Rc 载荷字段的共享借用与最后使用

macOS 两入口复现 `owner.value.name` 作为共享实参时所有权检查
返回 `InvalidUnitArgumentPlace`。具名 Rc 的 own 与 borrow 参数、
有 deinit 和纯内存载荷均受影响。相同字段直接经普通类读取通过。

前端从已发布的 Field 和 Rc intrinsic 描述符把共享借用归到
其 Rc 句柄，保留具名根和临时根的身份。Refine 对具名 owned
Rc 先建立 payload 的共享借用，再沿字段链生成短期子借用。
验证后先逆序结束子借用，才消费调用返回后的清理事实。

独立复审发现三个绕过入口及两条最后使用缺口。合法输入的普通
字段读取不能转为消费。MoveOnly 字段作为按值实参、局部绑定
或返回值时发 `L0132`；向可变字段赋值或作为 inout 实参发
`L0134`。两类诊断均保留 Rc 根的来源。两条最后使用用例原先
在 native build 报 `InvalidModel`，分别是可空证明后读取和
后续实参调用同一 owner 的 `.share()`。规划现在在实际共享
实参求值前登记根，保留既有 Copyable 转可空的临时副本路径。

持久原生回归核对具名 Rc 经两层字段读取的输出、末句柄析构和
四个指针的顺序。另核对两条最后使用路径和空值对照共六个指针
的清理。借入 Rc 参数与临时 Rc 仍通过前端后在原生构建报
`UnsupportedNode`。普通 Borrow ABI 传句柄存储地址，现有
可空证明路径传 control block 指针；后续需核对两种表示再扩大
后端准入，并覆盖临时根的返回及提前退出清理。

同一生产代码在 macOS arm64 和工具链容器内 Linux x86_64 各
实跑八组单文件与项目入口。32 个产物均退出 0、stderr 为空，
16 对 stdout 逐字节一致。macOS 两入口另核对绑定和返回的
`L0132`、赋值和 inout 的 `L0134`，以及后续实参直接 return
时跳过被放弃的调用并正常退出。

双宿主各通过 165 项 Unit 前端所有权和 67 项后端资源析构
回归，零失败、零忽略。macOS 前后端严格 Clippy、格式、尺寸、
文档结构和 diff 检查通过。

## 286. R4 借入 Rc 的句柄槽 ABI

单文件和项目复现借入 `Rc<String>` 后读取或 clone 载荷，
以及借入 `Rc<Leaf>` 后读取字段，前端通过、原生构建报
`UnsupportedNode`。具名 owned Rc 作为对照可运行。

共享调用参数和可空证明的 LLVM loan 都指向句柄槽。Rc
生产者从槽加载 control block 指针后读取载荷或共享一个
新句柄。Refine 对借用句柄的载荷发出 `SharedPayloadLoan`，
后续字段借用或 Copyable 读取消费该子借用。

独立复审发现旧 `SharedPayloadPlace` 仍可从 loan 形成无父
依赖的裸 place。现仅接受 Value owner，已有可空证明消费者
和单文件兼容消费者均改用子借用。另一项发现是可空证明经
普通 CFG 边重绑定后会丢失来源。两种非空证明的隐式参数
都进入来源传播，反例核对重绑定父还持有子时提前结束会被
拒绝。原生回归核对保活、末句柄析构和三个指针各释放一次。

具名 Rc? 判空后的本地读取可运行；直接将证明中的 Rc
传给另一个 Borrow 参数仍有后端 `MissingFact` 复现。
临时 Rc 的载荷借用与提前退出清理也继续单独跟踪。

macOS arm64 和工具链容器内 Linux x86_64 各实跑九组双入口。
36 个产物均退出 0、stderr 为空，18 对 stdout 逐字节一致。
两宿主各通过 68 项资源析构、10 项单元 String 合同、4 项
nullable Rc、2 项 borrowed Rc clone 和 5 项 shared payload，
零失败、零忽略。macOS 严格 Clippy、格式、尺寸、文档结构
和 diff 检查通过。判空后转交的最小 `Rc<Int>` 即使不读载荷，
在本机旧编译器和本轮候选均构建报 `MissingFact`。

## 287. R4 owned Rc? 非空证明转交调用

macOS 单文件和项目用最小 `Rc<Int>?` 复现判空后传给 Borrow
参数的 `MissingFact`。Refine 准入曾排除 Rc 证明，虽然后端
调用已可传共享句柄槽。准入现在核对解组后的原符号、活跃非空
证明和参数的精确类型，构建继续使用已存在的共享 loan。

另复现原子判空取反后转交失败。准入和构建共用 Group 与 !
链的 Rc 判空识别，依据奇偶性把证明放到实际非空臂。独立
复审指出 Borrow/Inout 可空根没有相同的证明生产者，最终
准入按本地 owned 或 Value 参数核对根。组合逻辑仍由原先
的短路规则负责。

持久原生回归覆盖 else 非空臂、分组实参、重复读取、null
和提前 return，核对六个指针的逐指针释放，以及两次末句柄
析构的位置。相反边或缺少证明的调用仍由前端报 `L0084`。
后续实参对同一 Rc 调用 share 的场景也纳入双入口实跑。

独立复审另确认借入或 inout Rc?、以 return 结束空分支后在
If 之外使用的 Rc 证明，仍有后端 `MissingFact`。临时 Rc
的载荷字段原生生成仍报 `UnsupportedNode`。

同一生产代码在 macOS arm64 与工具链容器内 Linux x86_64
各实跑五组单文件和项目入口。20 个产物均退出 0、stderr 为空，
10 对 stdout 逐字节一致。单次和双次取反，以及跨源资源载荷
都纳入矩阵。扩大准入套件发现旧对照仍把借入非空 Rc 的字段
读取当作拒绝用例。已保留其源码并改验 Refine 成功，新负例
改用借入 Rc? 缺少 owned 非空证明的情形。

双宿主资源析构 69 项、Refine 准入 54 项、nullable Rc 5 项
均通过，零失败、零忽略。Linux 先实跑生产代码及资源套件，
更新原路由见证后复跑两组准入与 nullable Rc 测试；两份快照
的 crate 源码差异仅为该测试文件。macOS 严格 Clippy、
格式、尺寸、文档结构和 diff 检查通过。

## 288. R4 owned Rc? 的空臂早退与后继借用

修复前，owned `Rc<Int>?` 经 `if (owner == null) { return }`
后借传给非空参数，单文件和项目均通过前端检查，原生构建报
`MissingFact`。

准入只从唯一可继续的 If 前驱接收已有的 owned Rc 非空证明，
并核对同源、同绑定的后继非空使用事实。构建保留该路径上活跃的
shared owner loan，由实际 CFG 前驱按借用槽取交集。写入、消费
或析构会先结束受影响的视图。现有 borrowed String 与 When 的
证明规则保留。独立复审核对了合流、重赋值、转移和析构，并用
前端负例确认两条正常分支合流后失效的证明仍报 `L0084`。

macOS arm64 与工具链容器内 Linux x86_64 各实跑五组单文件和
项目入口。20 个产物均退出 0、stderr 为空，10 对 stdout
逐字节一致。矩阵覆盖取反、正反方向的正常前驱、两次借用、
跨源资源载荷、赋值后重新判空。持久原生用例核对字符串缓冲区、
Leaf 与 Rc 三个指针依次释放，并检查 Leaf 在后继借用之后、
调用方继续之前恰好析构一次。

双宿主的资源析构 70 项、Refine 准入 54 项、nullable Rc 5 项均通过。
严格 Clippy、格式、尺寸、文档结构及 diff 检查通过。
扩大验证中，两个宿主的默认测试线程栈均在 String? 的 16 组
独立合流压力用例溢出。重新编译上一份 macOS 快照也复现同一失败。
两端将测试线程栈设为 8 MiB 后，String? 套件均通过 34 项。
同一压力源码另用 macOS CLI 实跑，32 行输出均匹配。

borrow/inout Rc? 的证明生产者及临时 Rc 的载荷字段生成仍待复审。

## 289. R4 临时 Rc 的载荷字段读取与共享调用

修复前，临时 `Rc(Leaf(...)).value.num` 通过前端检查，原生构建报
`UnsupportedNode`。持久回归先在修复前复现失败，后核对三组
缓冲区、Leaf、Rc 指针按顺序释放。空的提前返回边不分派原调用，
仍执行已求值临时句柄的清理。

前端以同源的根 Rc 临时身份发布投影表达式的清理，或在共享实参
注册成功后沿既有 pending temporary 路径发布调用返回及控制退出
的清理。Refine 准入核对 Rc 描述符、owned effect 及可接受的临时
接收者。构建只求值一次，经共享 payload place 形成子借用。
借用结束后才消费发布的清理点。普通直接 field token 观测范围
保持原样，前端回归单独核对实际 Rc 调用返回的 drop fact。

独立复审发现直接 Borrow `.value` 遇后续实参提前返回仍被整体
call 的控制退出门禁拒绝。补丁按已准入的同一临时句柄放行，
并以原生早退复核。字段链的 `clone()` 也走同一共享载荷路径。
前端仍以 `L0162` 拒绝不稳定来源的借用绑定。

复审另复现复制字段作为按值实参或二元表达式左操作数时提前
析构。临时句柄现在随待清理状态保留至外层表达式的清理点。
分组与取反沿内部表达式发布条件清理点，短路根沿执行 RHS
或跳过 RHS 的边各自清理。新增原生回归核对右侧求值和调用
先于析构，以及进入分支前的清理。

macOS arm64 与工具链容器内 Linux x86_64 各核对七组单文件
和项目入口。28 个产物均退出 0、stderr 为空，14 对 stdout
逐字节一致。矩阵包括分组、取反、复制字段、Copy-to-nullable、
载荷或字段共享调用、两实参逆序清理、工厂返回及跨文件字段链 clone。
双宿主资源析构 72 项、Refine 准入 54 项、nullable Rc 5 项、
前端 direct call loan 348 项及 Rc 所有权 5 项通过，均为零失败、
零忽略。macOS 的 frontend/codegen 严格 Clippy、格式、尺寸、
文档结构与 diff 检查通过。

仍有一个可观察的析构顺序缺口。短路作为外层调用的实参时，
条件形成的 RHS 句柄只能在局部分支清理。例如
`sink(lhs && rhs, tail())` 的 RHS 执行后，当前先析构 RHS，
再执行 `tail()` 和 `sink`，最后析构 LHS。此边界需要能表示
另一条边从未形成 RHS owner 的合流。Rc 载荷为 value class
时的字段投影（具名与临时均可复现）、临时句柄构造表达式自身
包含提前退出、借入及 inout 可空 Rc 的证明也仍开放。

## 290. R4 短路右侧 Rc 的条件清理跨合流

复跑 §289 的 `sink(lhs && rhs, tail())`，原输出在 RHS 执行的
路径上先析构 RHS，再运行 `tail()` 和 `sink`。修复后两次调用
均先完成后续实参与分派。RHS 执行路径随后按 RHS、LHS 逆序析构，
跳过路径只析构 LHS。

前端将仅一条正常边形成的 Rc 临时句柄保留在共同续行的清理计划，
以真实 source-qualified owner ID 和条件标记发布外层清理事实。
前端回归核对该标记仅属于 RHS，且位于同一 `CallReturn` 点的
无条件 LHS 清理之前。后端只对已发布的条件临时身份规划可空
句柄表示，正常边交付真实句柄或 null。RHS 内提前退出时仍按
该路径已有的实际句柄清理；跨过合流后的控制退出使用合流槽。

独立复审另复现两处路径差异。Basic 的恒跳过实参会报 `MissingFact`，
已让所有权计划和准入共用普通 bypass 事实。Const 的恒执行和动态
路径会提前析构，已保留其外层表达式的待清理状态，并沿 Const
分支编号发布清理。复审实跑了 Basic/Const、嵌套短路、内外 return、
循环 break/continue、同一条件合流的 String proof 和多实例泛型 thunk。

同一候选源码在 macOS arm64 和工具链容器 Linux x86_64 各实跑
六组单文件和项目入口。24 个产物退出 0、stderr 为空，12 对
stdout 逐字节一致。完整资源析构 74 项、Refine 准入 54 项、
nullable Rc 5 项、直接调用借用结束 348 项和多文件所有权
166 项均在两端通过。macOS 严格 Clippy、格式、尺寸、文档和
diff 检查通过。

后续 R4 仍覆盖 §289 记录的其余场景，包括临时句柄形成前的
提前退出、value class 载荷字段，以及 borrowed/inout Rc? 的证明。

## 291. R4 临时 Rc 接收者与部分构造的提前退出

单文件与项目首次复跑 §290 留下的接收者 `if` 反例，前端检查通过，
原生构建均报 `UnsupportedNode`。Rc 准入现消费接收者的类型、
正常续行及实际所有权 effect。只在有正常结果的边进行载荷借用、
复制或 retain。临时 `share()` 在 retain 后释放原句柄，载荷在
最后一个句柄释放时仅析构一次。

普通类与 intrinsic 构造保留实参的续行事实。所有权按实际退出
发布已完成的构造实参及终止实参，没有为未完成构造发布 root。
Rc 与普通类的 Refine 准入验证完整交付或终止前缀。完全终止的 Rc、字段链与 String
clone 只求值接收者前缀，普通调用先处理终止实参，再决定是否
生成借用或分派。正常清理、提前退出和 Abort 沿既有规则执行。

持久原生回归通过分配身份核对提前 return、后续实参 return、
正常借用、Share 和部分构造。独立复审另实跑 15 组单文件与
项目，并从所有权产物确认终止 Outer 与外层 Rc 均无 root，
只存在已完成的 Outer 交付。命名参数按源码顺序求值，退出时
逆序析构，后续实参与外层 retain 不执行。六个 Abort 产物均按
预期退出 134，部分构造的 Abort 路径没有展开析构。

同一候选源码在 macOS arm64 与工具链容器 Linux x86_64 各实跑
八组单文件和项目入口。32 个产物退出 0、stderr 为空，16 对
stdout 逐字节一致。双宿主资源析构 76 项、Refine 准入 54 项、
nullable Rc 5 项、直接调用借用结束 348 项、多文件所有权
166 项和构造所有权 8 项通过，零失败、零忽略。macOS 严格
Clippy、格式、尺寸、文档与 diff 检查通过。

有正常续行的嵌套 `Rc<Rc<Int>>.value.value` 仍会在所有权入口
报 `invalid compilation-unit argument place`。终止的 Box 原生
准入、可空 borrowed/inout Rc 的证明及 value class 载荷字段
继续作为独立缺口跟踪。

## 292. R4 嵌套 Rc 载荷的借用根与清理

§291 的具名和临时 `Rc<Rc<Int>>.value.value` 在修复前的单文件
与项目入口均于所有权检查报 `invalid compilation-unit argument place`。
前端沿已判定的 Rc 载荷与字段链保留最外层借用根，并在每步发布同源
操作事实。持久的前端回归确认具名、临时链分别借入外层 place 或
临时 owner，临时 owner 随调用结束而清理。

Refine 以 typed/owned 描述符逐层准入。嵌套 Rc 和字段访问复用既有
shared payload 和 field loan，读取、retain 或共享实参的调用之后按
逆序结束这些借用。处理提前退出时同样先结束 loan，再按前端事实
清理。持久原生回归检查普通调用和后续实参 return，亦检查临时
外层释放后 retain 仍保有内层载荷，最后只析构一次。

同一候选在 macOS arm64 与工具链容器 Linux x86_64 各实跑八组
单文件和项目入口。32 个产物退出 0、stderr 为空，16 对 stdout
逐字节一致。两端资源析构 77 项、Refine 准入 54 项、nullable Rc
5 项、直接调用借用结束 348 项、多文件所有权 167 项、构造所有权
8 项均通过。macOS 另通过三项 payload loan verifier 对照、严格
Clippy、格式、尺寸、文档与 diff 检查。

独立复审核对借用链、混合字段、retain、同根多实参和提前退出。
macOS 两个入口的六个负例继续拒绝移动非 Copyable 载荷、inout
访问和共享实参建立后移动外层 owner，分别给出 `L0132`、`L0134`、
`L0135`。

普通类的字段接收者还有两处待收口。显式 own Holder 参数经普通
字段取 Rc 原生 `MissingFact`，直接临时 Holder 字段在所有权入口
被拒绝。复审的单层与嵌套对照均复现 Rc 载荷用作成员接收者及
value class 载荷字段的支持缺口。可空 borrowed Rc 非空分支传递
仍通过前端，而单文件和项目构建报 `MissingFact`。

## 293. R4 共享借入 Rc? 的非空证明

修复前，`borrow owner: Rc<Leaf>?` 判空后传给 `borrow Rc<Leaf>`
形参，单文件和项目入口均通过 `check`，构建均报 `MissingFact`。

Refine 在非空边复用父 loan 所指的原始句柄槽，并以共享子 loan
交付非空参数。该路径没有隐式 retain。短路和有后续非空使用的
合流按借用根与槽类型匹配证明，离开作用域时先结束子 loan。
Verifier 对照覆盖活跃子 loan 阻止父 loan 结束、已结束父 loan
无法产生证明，以及拒绝独占父 loan。

原生回归覆盖 null、分组取反、重复判空、短路、提前 return、
投影读取和显式 share。计数断言核对显式 share 的 retain 及
最后一个 owner 释放时仅析构一次。macOS arm64 和工具链容器
Linux x86_64 各实跑六组用例的单文件与项目入口。24 个产物均
退出 0，stderr 为空，12 对 stdout 与预期逐字节一致。
准入回归将共享借入从拒绝改为接受，并保留独占借入的拒绝对照。
独立复审核对多根证明合流、短路中提前退出、循环及独占参数边界，
另在 macOS 单文件和项目重跑五组既有字符串借用用例，输出保持一致。

两端资源析构 78 项、Refine 准入 54 项、nullable Rc 7 项和
nullable String 35 项均通过。nullable String 测试线程使用 8 MiB
栈。Linux 另通过前端直接调用借用结束 348 项、多文件所有权 167 项
及构造所有权 8 项。macOS 严格 Clippy、格式、尺寸和 diff 检查通过。

仍待收口的路径包括 `inout Rc?` 判空后转交、以借入 Rc? 为主体的
`when (owner)`、非空视图传入可空借参，以及从非空 Rc 视图再次建立
具名借用。独立复审的后两种探针分别在后端报 `MissingFact` 与
`UnsupportedNode`。

## 294. R4 共享借入参数作为 nullable when 主体

`when (owner) { null -> …; else -> read(owner) }` 的 `Rc?` 和
`String?` 借入参数用例通过前端检查，原单文件和项目构建均报
`UnsupportedNode`。持久原生用例在修改前也复现该错误。

所有权阶段为 `BorrowRoot` 的共享调用实参发布原有 Observe 事实。
Refine 核对源绑定及调用事实后，从原参数 loan 槽判空。非空子
loan 在正常和提前离开分支时结束，分支不 retain 或 take。
回归检查 SSA 父槽身份，并禁止额外 retain、take 或 String clone。

macOS arm64 与 Linux x86_64 工具链容器均实跑直接观察、嵌套
when 和独立借用返回三组用例的单文件与跨文件项目入口。12 个产物
均退出 0，stderr 为空，6 对 stdout 与预期逐字节一致。覆盖 null、
空字符串、重复读取、提前 return 和 Rc 载荷单次析构。

两端 nullable when 46 项、Refine 准入 54 项、资源析构 78 项、
多文件所有权 167 项通过。macOS 严格 Clippy、格式、尺寸、
文档结构及 diff 检查通过。

独立复审核对嵌套分支提前退出和独立 String 借用结果转发。
共享主体保持在原有临时借参提前清理准入之外。以共享主体传给
own 实参仍报 L0133。独占主体和具名 `borrow val` 主体当前通过
前端检查，原生构建仍报 `UnsupportedNode`。具名借用需单独处理
词法父 loan 与分支子 proof 的清理顺序。

## 295. R4 具名借用作为 nullable when 主体

`borrow val local = view(text)` 或 `borrow val local = owner` 后直接以
`local` 作为 `when` 主体，修复前的单文件和项目均通过前端检查，
构建报 `UnsupportedNode`。持久原生回归同样复现。

类型阶段发布具名共享主体身份，并随试算状态回滚。所有权阶段
保留语法绑定用于选取原 loan 槽，观察事实中的调用 loan 则按活跃
绑定的真实来源核对。借用结果预检只接受该绑定或已记录的同源
词法子绑定，提前返回仍要求该绑定的前端结束事实。

Refine 在词法绑定结束前清理其活跃的非空证明。嵌套清理按已有
前端次序交付，外层 loan 保持到子 proof 及子 loan 结束。

macOS arm64 与 Linux x86_64 工具链容器均实跑四组单文件及
跨文件项目入口。16 个产物退出 0，stderr 为空，8 对 stdout 与
预期逐字节一致。覆盖空值、空字符串、重复读取、词法父子链、
嵌套 when、return、break、continue 及 Rc 单次析构。
从具名共享绑定传给 own 实参仍由前端报 L0133。

两端 nullable when 47 项、nullable String 35 项、Refine 准入
54 项、资源析构 78 项和多文件所有权 167 项通过。macOS 另通过
nullable when 类型检查 22 项、严格 Clippy、格式、尺寸、
文档结构及 diff 检查。

独立复审确认同源但无父子关系的对照仍由原生预检拒绝，另实跑
双层 when、提前 return 和循环退出。分支内从非空载荷另建具名
借用仍可复现 `UnsupportedNode`，留待后续收口。

## 296. R4 从 when 非空证明建立具名子借用

非空臂中的 `borrow val child = subject` 与
`borrow val child = view(subject)` 均通过前端，修复前原生构建报
`UnsupportedNode`。新增持久回归在旧候选上复现。

预检把 typed nullable When 凭据与同一具名绑定的所有权事实相连。
调用形式另核对真实共享实参 loan、来源 lease 和签名。Refine 从
对应控制的非空 proof 槽建立直接子 loan 或借用返回，正常退出
与提前 return 均维持子 loan、proof、来源 loan 的先后顺序。

macOS arm64 和 Linux x86_64 工具链容器各实跑三组单文件和
跨文件项目。12 个产物均退出 0，stderr 为空，6 对 stdout 与
预期逐字节一致。覆盖空值、空字符串、重复读取、提前 return
及 Rc 载荷单次析构。

两端 nullable when 48 项、nullable String 35 项、Refine 准入
54 项及资源析构 78 项均通过。Linux 多文件所有权 167 项通过。
macOS 严格 Clippy、格式、尺寸、文档结构和 diff 检查通过。

独立复审核对 String 与 Rc 的两类生产者及正常、提前返回的顺序。
无关实参的 checked unary 与缺少已发布清理事实的参数控制继续
报 `UnsupportedNode`。循环内新建具名借用的对照仍由前端报
L0164，泛用循环清理保持原行为。从子借用移出仍报 L0133。

## 297. R4 借用返回调用的后续实参提前返回

`borrow val child = view(subject, if (stop) { return } else { true })`
在非空 `when` 臂内通过前端，修复前原生构建报 `UnsupportedNode`。
持久回归同时覆盖具名 `String?` 借用来源以及 owned `Rc?` 来源。

Drop Planner 从已注册且实际在 return 边结束的原 call loan 发布
前缀结束事实。非空来源与同轮 Observe 核对，词法借用、共享参数或
owned 来源在该边仍活跃。预检对 `NarrowedWhen` 接受实际为
Copyable 临时值的默认借参，并继续核对 call、实参、lease 与转移点。
SSA 给来源实参建立独立共享子 loan。提前退出先结束该 loan，
成功调用则将它交给结果借用的清理路径。

独立复审验证默认 Boolean 借参双入口，确认普通 Scoped 对照和
来源求值前就 return 的负例仍报 `UnsupportedNode`。回归预期按
Guide 资源词法析构顺序校正，区分载荷读取与真实析构输出。

macOS arm64 和 Linux x86_64 工具链容器各实跑三组单文件及
跨文件项目。12 个产物退出 0，stderr 为空，6 对 stdout 与预期
逐字节一致。提前返回只清理已求值的来源，调用结果未建立。同一
共享 Rc 还可由调用方再次借入，最后一个 owner 释放时只析构一次。

两端 nullable when 49 项、nullable String 35 项、Refine 准入
54 项、资源析构 78 项、多文件所有权 167 项和直接调用借用结束
348 项通过。macOS 严格 Clippy、格式、尺寸、文档结构和 diff 检查
通过。同一调用更早的实参另有临时 Rc 资源时，原生构建仍报
`UnsupportedNode`，待独立核对其临时 owner 和贷款清理。

## 298. R4 借用返回前缀中的临时 Rc 实参

同一个调用先借入临时 `Rc`，再借入 `when` 非空来源，最后一个
实参可能提前 return。原候选能运行没有提前退出的对照，含提前
退出的版本通过前端却在原生构建报 `UnsupportedNode`。

Refine 准入核对临时 Rc 的真实共享实参合同、同一调用的位置、
前端发布的临时析构点与同一 return 上的来源借用结束事实。
复用现有清理路径，提前退出逆序结束已建立的调用借用，再按
前端转移点释放临时 Rc 和 owned 来源；正常路径在 CallReturn
释放辅助 Rc，借用结果继续使用其独立来源。

独立复审核对 BorrowCall 只出现在继续路径。原生回归以析构
标记和载荷读取核对顺序，前端断言检查同一提前转移上先有临时
析构事实，再有具名来源析构事实。命名实参、透明分组和两个
提前返回点的独立探针通过。来源实参尚未求值就返回的对照保持
原生拒绝。

macOS arm64 与 Linux x86_64 工具链容器各实跑 owned、共享来源
及双临时三组单文件和跨文件项目。12 个产物退出 0，stderr 为空，
6 对 stdout 与预期逐字节一致。两个临时按实参求值的逆序析构，
共享来源在调用方继续执行并再次借入，最后只析构一次。

两端 nullable when 50 项、nullable String 35 项、Refine 准入
54 项、资源析构 78 项通过。Linux 多文件所有权 167 项、
直接调用借用结束 348 项通过。macOS 严格 Clippy、格式、尺寸、
文档结构和 diff 检查通过。

## 299. R4 inout Rc? 的非空证明与写回

具名 `inout Rc?` 在判空后转交非空借参，修复前构建报
`MissingFact`；直接作为 nullable `when` 主体报 `UnsupportedNode`。
持久对照先通过前端检查，再复现原生失败。

Refine 从独占参数所指的 Rc? 槽只读判空，非空边建立共享子
loan。非空证明可供直接借参、投影、显式 share 和具名子借用。
求值写回 RHS 后，在实际替换或转交 inout 之前结束旧证明。前端
按原调用合同发布 Observe 和已求值前缀的结束事实。

独立复审复现了短路跳过边仍持有已在另一边失效的证明，以及布尔
值合流丢失单边证明清理。短路按原槽身份处理各前驱，布尔值产生
的边沿现有视图合流结束单边证明。SSA 负例覆盖子证明活跃时替换，
以及从下一基本块读已结束的父 loan。具名子借用结束后可以写回。
写回后仍读取该借用，两个入口均给出 L0135。inout String?
原生证明仍报 UnsupportedNode。

macOS arm64 和 Linux x86_64 工具链容器各运行六组单文件和
跨文件项目入口。24 个产物退出 0、stderr 为空，12 对 stdout
与预期逐字节一致。覆盖 null、重复借入、直接替换、inout 转交、
短路、循环、投影、retain 后替换以及借用结果前缀提前退出。

行为候选两端另通过 nullable String 35 项、资源析构 78 项和
多文件所有权 167 项。为通过尺寸门禁，将判空契约与准入归入
对应模块。整理后的候选在双端重新完成 24 次原生运行，另通过
nullable when 54 项、nullable Rc 12 项和 Refine 准入 54 项。
macOS 严格 Clippy、格式、尺寸、文档结构和 diff 检查通过。

## 300. R4 inout String? 的非空证明与写回

基线在单文件通过前端检查，但原生构建对纯判空、if 非空
借参和直接 nullable when 均报 `UnsupportedNode`。持久原生
回归另复现了非空读取后克隆、写回和跨控制边继续读取失败。

Refine 与 verifier 承认从独占 String? 原槽只读判空并生成
共享子证明，沿用指针 niche。借参及局部具名子借用读取同一
载荷。RHS 先读取，替换原槽或转交 inout 前结束证明。SSA
对照要求有活跃子证明的替换失败，结束证明后的替换通过。

独立复审核对了原槽身份、共享子证明、跨块依赖和替换前的
清理。额外实跑表明，实参提前返回时仍可再次读取原槽；正常
结束子借用后可以写回。同一活跃子借用之后再写回或转交
inout 均给出 L0135，MoveOnly 提取给出 L0133。

直接在 nullable when 非空臂以主体做字符串比较、打印或
clone 仍报 `UnsupportedNode`。共享和独占参数均有此缺口。
本批验证 when 内借参、具名子借用、顺序写回与 inout 转交。

macOS arm64 与 Linux x86_64 工具链容器从相同的代码候选各
运行三组单文件、跨文件项目入口。12 个产物退出 0，stderr
为空，六对 stdout 与预期逐字节一致。覆盖 null、空串、重复
读取、克隆后替换、子借用退役及短路转交。两端定向套件覆盖
nullable when 55 项、nullable String 36 项、nullable Rc 11 项
和 Refine 准入 54 项。macOS 另通过资源析构 78 项、严格
Clippy、格式、尺寸、文档结构及 diff 检查。

## 301. R4 nullable when 内直接读取 String 主体

共享与独占 String? 参数的非空臂直接比较、打印或克隆时，
前端接受，原生构建报 `UnsupportedNode`。持久原生回归先失败，
修复后覆盖分组、空串、两侧比较、重复读、克隆以及克隆后写回。

Refine 按实际表达式识别 String 比较、拼接、println 和 clone
的读取点，生成沿现有活动共享证明读取原槽。编译器内建 println
使用已有的 String 借参 lowering，普通声明仍走独立的 Observe
合同。从非空证明取得的左操作数在右操作数求值期间保持共享读。

独立复审和主验证均复现了 `slot == clear(&slot)` 曾在生成阶段
报 `InvalidModel`。所有权检查现在在活跃读取期间拒绝同槽冲突，
持久单文件与项目负例给出 L0135。独立槽写回、先 clone 再转交、
比较完成后再转交及重新判空仍可运行。逆序 `clear(&slot) == slot`
涉及 String 与 String? 的异型相等，仍报 `UnsupportedNode`。
逆序拼接给出 L0085，转交后直接打印给出 L0084，核实前端已
撤销原非空证明。

最终代码候选在 macOS arm64 和 Linux x86_64 工具链容器各实跑
三组单文件与跨文件项目入口，12 个产物退出 0、stderr 为空，
六对 stdout 与预期逐字节一致。双端的 CLI 所有权诊断回归均
确认 L0135。行为候选的两端 nullable when 套件各通过 56 项；
macOS 另通过 nullable String 36 项、Refine 准入 54 项、
多文件所有权 167 项，以及三个受影响 crate 的严格 Clippy。
Linux nullable String 首跑使用默认测试线程栈，在既有跨块
借用验证测试触发栈溢出，复跑沿用 8 MiB 栈，36 项通过。

## 302. R4 String 与 String? 的等值比较

基线的双可空及混合等值通过前端，原生构建报 UnsupportedNode。
Map.remove 的可空字符串由标量比较路径处理，原生构建报 InvalidModel。
新增持久用例分别覆盖两种布局、两侧缺席、混合顺序、空串与缺席、
UTF-8 及内嵌 NUL、写回后读取、提前返回、具名值重复读取和临时值。

Refine 将该类型组交给 StringEqual。SSA 验证可空操作数的
StringOwner 布局和借用活跃性。LLVM 沿用原字节比较，在可空路径上
两侧都存在且长度相同才比较字节，Map result 缺席时屏蔽 payload。
所有权检查在求值右操作数时保持左侧具名读取，临时值沿二元运算
以及各退出点清理。持久 SSA 对照拒绝已结束的 loan 及 Boolean?
布局。单文件、项目的同槽写回对照均报 L0135。

独立复审发现并复验了右操作数为 Nothing 的提前返回路径及有标签
Map result。跨文件对照覆盖 String 键、Borrow、Inout 写回后读取
当前槽以及带有标签的临时左值提前退出。后续 R4 用例已在单文件和
项目复现，`h.label != null`（`label: String?`）及
`borrow val a = view(slot)` 后的判空均通过前端，原生构建报
UnsupportedNode。

同一最终代码候选在 macOS arm64 和 Linux x86_64 工具链容器
各运行三组单文件与跨文件项目入口。12 个产物退出 0、stderr
为空，六对 stdout 与预期逐字节一致。Niche 对照另验证 8 次
非空分配的逐指针释放。两端 nullable String 39 项、nullable when
58 项和 CLI 所有权诊断回归均通过。
两端扩大的 Map 原生套件各通过 148 项。macOS 另通过 Refine 准入
54 项、多文件所有权 167 项和 frontend、codegen、CLI 严格
Clippy。格式、尺寸、文档结构和 diff 检查通过。

## 303. R4 String? 字段的直接判空

基线单文件和跨文件项目对 `h.label != null` 均通过前端，但
原生构建报 `UnsupportedNode`。普通类含闭合的 String? 字段时，
布局 recipe 同样在构造端拒绝，即使未读取字段。

布局解析复用现有 String? 的 concrete type identity。Refine
检查实际字段身份和 receiver 类型，从原槽建立共享投影并生成
NullableLoanIsNull。独占 receiver 经 scoped SharedReborrow。
比较后先结束字段借用，再结束父重借用，同一字段可继续写回。
普通临时 receiver 的清理由前端发布到 AfterBinaryOperands。
每次比较只求值 receiver 一次。具名资源仍按词法作用域末尾析构。

独立复审实跑普通／嵌套类、分组、this、Rc、own 和 inout，
以及临时 receiver 的短路与提前 return。缺失构造实参给出
L0121。重新求值同一字段的 String? 不从之前的判空取得
隐式收窄，前端给出 L0084。

具名泛型 receiver 依实际 owner 与具体字段类型准入。字段模板
为 T? 时，前端临时 receiver 的清理仍沿 enclosing owner 的
MoveOnly 事实。复审重跑了泛型局部、T=String 的临时 receiver
和泛型字段 clone，并确认临时 receiver 准入仍需要精确清理事实。

macOS arm64 和 Linux x86_64 工具链容器从同一最终代码候选
各运行六组单文件与跨文件项目入口。24 个产物退出 0、stderr
为空，12 对 stdout 与预期逐字节一致。两个持久资源用例分别
逐指针核对 8 次与 6 次分配释放。单文件和项目也复验了
L0084。

后续 R4 对照包括具体 Int? 临时字段的判空及泛型资源类构造。
借用返回的 String? 绑定后判空仍在两个入口通过前端、原生构建
报 UnsupportedNode。

最终候选的双端 nullable String 各通过 42 项，nullable when
各通过 61 项。macOS 多文件所有权 167 项和 frontend、codegen
严格 Clippy 通过。前一行为候选另通过资源析构 78 项和 Refine
准入 54 项。格式、尺寸、文档结构和 diff 检查通过。

## 304. R4 借用返回的 String? 绑定后判空

基线上，借用返回可以绑定、转交普通借参及作为 nullable when 主体。
绑定后的直接判空、布尔结果和非空分支读取通过前端，但原生构建报
UnsupportedNode。单文件与跨文件项目均复现。

控制准入按 typed null comparison 的符号、可空类型、词法祖先与
同一个 place 身份核对判空及其布尔控制。Refine 对已活动的绑定槽复用
NullableLoanIsNull 和非空边的共享证明。来源是 Inout 时，
结果调用在共享 ABI 上使用现有 SharedReborrow，仅延续 from 实参
的重借用。结束时先释放 payload 证明与结果借用，再结束来源。

独立复审复现了短路右侧的 String 等值比较、外层 if 内的借用和
非空分支里的词法 payload 子借用。String? 复用普通 String 的
词法控制遍历。窄化子绑定还必须命中同源 BorrowedStorage 父绑定，
且 typed proof 指向该绑定。复核覆盖分组、两条不同父路径、再次
转交借用、孙级子借用及提前返回。同源的其他绑定不能复用当前
证明，仍报告 L0084。活动子借用期间写回仍报告 L0135。

同一代码候选的五组输入在 macOS arm64 与 Linux x86_64 容器各
通过单文件与跨文件项目的检查、构建和运行。20 个产物均退出 0，
stderr 为空，10 对 stdout 逐字节符合预期。公开 check 入口的
L0135 回归在两端通过。循环内的这类绑定判空仍报 UnsupportedNode，
有标签的 Map result 非空读取仍报 MissingFact，继续作为 R4 复现。

双宿主的 nullable String 套件各通过 46 项，nullable when 各
通过 65 项。macOS 的 borrow_result 套件 120 项通过，codegen
与 CLI 的严格 Clippy 通过。格式、尺寸、文档结构和 diff 检查
通过。两个后续复现均在单文件与跨文件项目确认。有标签的
Map result 只判存在性时可运行，失败点是非空 payload 读取。

## 305. R4 带存在标记的 Map 结果的非空借用

`Map.remove` 的 `String?` 结果带显式存在标记。基线上，借用返回后的
判空可以读取标记，非空分支读取 String 时原生构建报 MissingFact。
Refine 复用 NullableLoanBranch。Verifier 要求注册的 Map 结果布局
及 String payload，沿边保留原槽的父借用。在 LLVM 中，判空读取
字段 0，非空边接收字段 1 的地址，空边不读 payload。内联指针
niche 继续遵循 ADR-0041。独占槽可在结束子证明后写回。

新增原生回归覆盖有值、空字符串和缺失值，运行 If 内的词法子借用、
when 读取及独占写回后再次借用。独立复审核对了布局注册、字段投影、
跨块重新绑定及父子退休顺序。Verifier 拒绝未注册的同形布局、活跃
子借用期间父借用写回或提前结束。评审另在 macOS 跑了四组单文件与
跨文件项目对照，覆盖转交普通借参、when 后写回和短路后继续循环。

同一候选源码在 macOS arm64 和离线 Linux x86_64 容器各通过两组
输入的单文件与跨文件项目 check、build、run。8 个本机产物均退出
0，stderr 为空，4 对输出逐字节符合预期。两端 nullable String
各通过 49 项，nullable when 各通过 67 项，另各通过 1 项标记布局
准入测试。macOS codegen 严格 Clippy、格式、尺寸和文档检查通过。

后续复现仍待修复。借用返回的 String? 绑定后用作 while 判空条件，
两种公开入口均通过前端检查，原生构建报 UnsupportedNode。带
continue、break 和循环体析构的组合也在两入口复现。直接以借参
判空的对照在 macOS 输出 T。独立评审另复现带标记布局中的具名
String 转交 borrow String? 报 InvalidModel。后者在被调用函数
没有非空分支时仍可复现。

## 306. R4 借用返回的可空 String 用于 while 判空

借用返回绑定用于 while 判空时，单文件与跨文件项目通过前端检查，
原生构建报 UnsupportedNode。带 continue、break 和每轮析构的新增
原生测试先失败，错误 span 对应循环体内的 if。只有简单判空也无法
通过旧循环准入。

仅把由相同绑定或同源词法子绑定的 typed 判空事实直接证明的 while
加入该绑定的准入。已准入循环的完整条件复用原有运算符遍历，循环体
复用原有语句控制遍历。独立复审发现复合条件中的 Int 比较需要一同
登记，修复后复验。条件中短路右侧的独立 inout 调用按原求值顺序执行。
Refine 沿用原 while 的 payload proof 边界，父结果按已发布的词法
结束事实清理。活动借用期间写回仍报 L0135。

持久回归覆盖空、非空和循环内析构。公开入口额外覆盖显式标记布局、
普通回边、条件变假、continue、break、return 和调用者再次写回。
在同一源码快照上，macOS arm64 和离线 Linux x86_64 容器各有
四组输入通过单文件与跨文件项目的 check、build 和 run。16 个
进程正常退出，stderr 为空，8 对输出逐字节符合预期。
两端 nullable String 各通过 50 项，nullable when 各通过 68 项。
macOS borrow_result 通过 120 项，Linux 标记布局准入测试通过。
macOS codegen 严格 Clippy、格式、尺寸、文档及 diff 检查通过。
候选以 `cc2d15d6` 为基线，源码清单 SHA-256 为
`0298b3ba3593fc05678008ddb39b63e0078551ec399dde36002fdfb2fcaa8d23`。

同时存活且跨回边的独立结果借用仍触发保守准入，两种公开入口均
复现。循环体内新建具名子借用的探针仍在前端报 L0164。两者保留
为后续复审场景。双宿主两入口另复验了具名 String 转交 borrow
String? 的 InvalidModel。它只在同一编译单元内存在会选择显式
标记布局的 Map 结果时触发。在 macOS 上，两个操作位于不同函数
时，只要 Map 所在函数可达即复现。取消该调用后可执行。

## 307. R4 Map 移出 String 的可空表示一致性

在旧候选上，可达 Map.remove 使同一编译单元的 String? 统一选择
显式标记布局。具名 String 转交 borrow String? 时，复用地址的
SharedReborrow 无法满足该目标布局，原生构建报 InvalidModel。
新增持久回归先复现，再验证直接借入、已有借参转交及调用后复读
同一原值。删除独立的可达 Map 操作可以消除失败。

生产 Unit 的 Map.remove String 现沿 ADR-0033 复用 String? 的
指针 niche。Verifier 仅允许 Map 的实际 String payload 匹配该
niche 结果。LLVM 缺失边交付全零布局，命中边原样转交移出的
String，空字符串仍使用非空 sentinel。已注册的显式标记结果
仍有既有 SSA 校验和原始回归。具名 owned String? 的布尔判空
直接读取 NullableIsNull 后消费原 drop 事实，控制分支仍沿真实
证明建立视图。两处 SSA 见证继续锁定 MapRemove 的原第二结果、
源位置与析构次序。

独立复审覆盖不同生产者跨函数交付、可达与不可达 sibling、
非空和空 Map 的 miss、空文本和空 argv、独占写回，以及原值
生命周期。逐指针原生回归核验 9 次分配与释放。复核确认布尔
判空的两种极性及原 owner 的清理顺序。

最终候选以 `cc2d15d6` 为基线，源码清单 SHA-256 为
`4299496ca7fd32b0ae566e6045f14ab96cad5413fac2acc46bef9597cf3d0252`。macOS arm64 与离线 Linux x86_64 容器的两种公开
入口各通过四组输入的 check、build 和 run。16 个进程正常
退出，stderr 为空，8 对输出逐字节符合预期。两端各通过 165 项，
涵盖 nullable String、nullable when、Map.remove、析构顺序与布局
准入。macOS 的严格 Clippy、格式、尺寸、文档和 diff 门禁通过。

下一项复现是 when 的非空臂将借入的 String? 主体转交给同步
borrow String? 参数。macOS 两种入口的 check 通过，原生构建
仍报 UnsupportedNode。该用例无需 Map 即可复现。

## 308. R4 When 非空 String 作为可空借参

前端已发布 When 的非空 use witness 和普通共享调用参数 loan。
旧候选只允许该 witness 匹配 payload 形参。在同一非空臂先传
borrow String、再传 borrow String? 时，第二次调用使原生构建
报 UnsupportedNode。新增持久回归先失败，修复后覆盖有值、
空字符串、缺失及转交后再次读取。

准入与构建共享相同的同步 String Borrow 提升判断。构建在核对
原 witness、形参与所有权调用事实后，从已活动的 payload 证明
建立 ABI 兼容的 SharedReborrow。普通调用和后续实参提前返回
都先结束该短借用，再结束证明或父槽借用。提前返回的持久回归
检查调用是否被跳过，以及参数与外层资源各自的析构顺序。

独立复审实跑同源双借参、逆序具名实参、后续 return 和 abort、
独占主体写回，对共享槽的移动仍报 L0133，后续实参在同槽写入
时两种入口均报 L0135。具名借用主体加后续实参动态 return
仍可触发 UnsupportedNode。两个形参均为 borrow String 的旧
路径同样复现，作为独立的词法准入问题保留。

最终候选以 `cc2d15d6` 为基线，源码清单 SHA-256 为
`d6d4c8e27e5098627a3a9b7a6d218e3c9a11f0367e2a7d808185e49f5b8788ae`。macOS arm64 与离线 Linux x86_64 容器分别用
单文件、跨文件项目验证五组输入，共 20 个正常退出的原生产物。
stderr 为空，10 对输出逐字节符合预期。两端 nullable when
各通过 71 项、nullable String 各通过 50 项，另各通过 1 项
标记布局准入测试。macOS 严格 Clippy、格式、尺寸、文档和
diff 检查通过。

下一项复现是同一个 If 非空臂内依次调用 borrow String 和
borrow String?。macOS 两种入口通过 check，构建报 MissingFact。
单文件反转两种调用的顺序仍失败。分别只重复同一种形参类型的
对照均构建成功并输出两次 payload。

## 309. R4 If 非空臂混用 String 和 String? 借参

同一 If 非空臂里混合两个 Borrow 调用时，两种调用顺序均能通过
前端检查，但旧候选构建报 MissingFact。临时调用者追踪定位在
Unit 兼容 lowering。Refine 准入仅允许声明类型本身为 String
的具名借参提升，拒绝已证实非空的 String? 绑定，连带回退整个函数。

准入仍核对原调用的实际共享 loan。声明为 String? 的具名实参
还需匹配活动的同符号非空事实。构建优先从该证明创建短共享重借用，
沿普通调用机制结束子借用。持久回归覆盖嵌套 arm 及两种调用
次序，验证有值、空字符串、缺失和后继读取。后续实参试图写入同一
槽时仍报 L0135。

最终候选以 `cc2d15d6` 为基线，源码清单 SHA-256 为
`10c2d7d2a821a8f89bc7d821a77476733a6b0195bcd283b3c1a52ba86a6529f7`。macOS arm64 与离线 Linux x86_64 容器分别验证
共享参数、独占参数写回、owned 参数、词法绑定和后续实参提前
return 的单文件与跨文件项目。20 个原生产物正常退出，stderr
为空，10 对输出逐字节符合预期。独立复审另跑同源重复借参、
逆序具名形参、短路两侧、来源与辅助实参、赋值后重新判空、
成员调用和循环退出，12 组两入口输出一致。双宿主各通过 72 项 nullable when、50 项
nullable String 和 1 项标记布局准入。macOS 严格 Clippy、格式、
尺寸、文档和 diff 检查通过。仍登记两个独立缺口：
同根非空 String 的 owned 消费，以及将其作为 borrow String?
返回值的唯一来源。对照隔离了本批的正常调用短借用。

下一项继续核对词法借用主体的 When 调用实参提前返回。即使
前两个调用参数同为 borrow String，macOS 的单文件与跨文件项目
均在成功 check 后构建报 UnsupportedNode。

## 310. R4 词法借用主体的 When 实参提前返回

前端接受词法借用作为 When 的主体。非空臂里，直接调用先观察
该主体，再求值可能提前 return 的实参时，原生构建报
UnsupportedNode。持久回归先复现，错误 span 对应后续实参
中的 return。已有词法准入遍历覆盖分支中的直接控制语句，
但无法沿原调用定位此退出。

密封 preflight 接口复用原 nullable When 所有权计划查询，生产
与实验代理同样读取原分析结果。只有同一个 When 的真实共享
观察、同源调用位置、其后实际实参及该词法绑定的同点结束事实
共同成立时，才准入直接 return 或 If 臂尾 return。普通调用的
Borrow 前缀不改用借用返回切片。Refine 原清理链先退休调用短
借用，再结束证明及词法结果，最后按前端事实结束来源和析构。

持久回归同时验证两个同型借参和真可空短重借用，覆盖有值、
空字符串、null、后续写回及提前返回时跳过调用和后续语句。
独立复审验证逆序具名实参、多级词法子绑定、分组、短路、带值
return、两臂 return，以及已求值资源实参在退出时先于外层
资源析构。L0133 和 L0135 保持；提前返回在观察实参之前、与
观察无关的调用和另一个嵌套调用的 return 仍保留准入边界。

最终候选以 `cc2d15d6` 为基线，源码清单 SHA-256 为
`43a9590d99e9442f417783c0c159afc65a6728c7bd9a35c02466cffa6e0724e0`。macOS arm64 与离线 Linux x86_64 容器用
单文件和跨文件项目实跑五组输入，20 个原生产物正常退出，stderr
为空，10 对输出逐字节符合预期。独立 macOS 复审另外实跑 18 个
产物。同源独立绑定无法替代当前 When 的 Observe 事实。macOS
的 two-source-unit 编译检查、严格 Clippy 与格式检查均通过。
双宿主各通过 73 项 nullable when、50 项 nullable String 和
1 项布局准入测试。

下一项复现是 If 的非空 String 作为返回 borrow String? 的唯一
来源。两种入口通过 check，构建报 UnsupportedNode。直接从原
String? 槽调用的对照正常运行，可区分空串和缺失值。

## 311. R4 If 非空 String 作为可空借用返回的来源

借用返回调用以 If 非空臂的同一可空槽为唯一来源时，前端通过，
原生构建报 UnsupportedNode。持久回归先失败。已有窄化绑定
准入只接受返回 String，导致同一个槽的 String? 结果进入旧
绑定门禁。

扩展只接受已选 Borrow 形参类型与返回绑定类型都等于来源声明
的 String?，且原同源非空事实和共享调用 loan 匹配。构建复用
活动 payload proof，对唯一来源建立 ABI 兼容的 SharedReborrow。
既有 BorrowCall 交付结果并延续来源重借用，返回或词法退出时先
结束结果，再结束来源重借用及外层 proof。独立来源和辅助实参
分别按既有事实清理。直接从真正的 String 存储制造 String?
返回借用仍报 L0162。

持久回归覆盖分组实参、有值、空字符串、缺失、正常离开及提前
返回；两种路径之后均能继续访问或写回来源。独立复审额外运行
owned local、Borrow 形参、词法父子链、相邻绑定、逆序具名参数
及同根和异根的辅助实参。

最终候选以 `cc2d15d6` 为基线，源码清单 SHA-256 为
`a7ed6bfe72e9cbcfcf41a077b3a0a9011b9dc30b5b5a9d0fa137c08c6299e289`。macOS arm64 和离线 Linux x86_64 容器分别验证
五组单文件及跨文件项目，20 个原生产物退出 0、stderr 为空，
10 对输出逐字节符合预期。组合覆盖 Borrow 和 Inout 参数、owned
来源、词法父别名，以及提前退出时内外资源的析构顺序。活跃结果
和其词法别名仍拒绝独占写入，独立复审两入口确认 L0135。
两端各通过 74 项 nullable when、50 项 nullable String 和 1 项
布局准入。macOS 额外通过 120 项 borrow_result、严格 Clippy、
格式、尺寸和文档检查。

同一候选沿 CI 使用的公开一致性语料及历史 baseline 实跑并验证
双宿主证据包。两端能力探针 137/154、综合验收程序 0/6，
23 项与已登记的未实现一致，零回归、入口差异、错误输出及
内部错误。证据包用源码清单哈希标识候选。本轮 R4 实跑仍有
后续阻断，R5 发布出口未闭合。

下一项复现是将 If 非空臂的 owned String? payload 交给
own String 形参。无前置借参的最小程序也在单文件和跨文件项目
通过 check、构建报 InvalidModel；复制 payload 的对照可运行。

## 312. R4 If 中的自有 String? 交付 own String

If 非空臂将自有 String? 载荷交付 own String 形参时，前端
发布整根 Move，原生构建仍失败。计数回归先失败。Refine 的
普通 Name 准入仅按 Copyable 值或当前函数自身返回类型处理
窄化，不能代表另一个调用的 Value 交付。

准入和构建共享同一分析的非空事实、Value 参数合同和整根 Move
核对。实参求值时从当前 owner 及活动证明执行 NullableTake；
同根另开的证明须先结束。保留已拆出实参的身份，使旧 payload
完成交付或在后续实参退出时清理时，不再次消费同槽的新 owner。
旧 When 交付也使用同一提取与退休路径。

两条持久原生计数回归分别核验正常调用及提前返回、同槽重填和
二次判空。按指针核对 2 次与 4 次分配，各释放一次。独立复审
另跑同根复证、短路、具名实参求值次序、continue、break 和
abort。abort 跳过被调函数，遵循无栈析构语义。移动后读取和
从共享槽移出分别保持 L0131 与 L0133。

最终候选以 `cc2d15d6` 为基线，源码清单 SHA-256 为
`9274bd0bc6661994540714f1ac622585d4fb1fb9cf2c200d01daab90307a26be`。macOS arm64 与离线 Linux x86_64 容器各实跑
六组单文件和跨文件项目，共 24 个原生产物。两端输出和跳转行为
相同。双入口用例覆盖直接交付、嵌套同根证明、早退、循环退出、
重新初始化及实参求值期间同槽重填。
两端各通过 76 项 nullable when、50 项 nullable String 和
1 项布局准入。macOS 另通过 36 项 owned_null_cleanup、严格 Clippy、
格式、尺寸、文档及 diff 检查。完整公开一致性语料双宿主对账通过，两端均为
137/154 能力探针、0/6 综合程序，零回归、入口差异及内部错误。

同一候选重核 R0 的直接调用。CLI、LSP 与 frontend analysis 的
生产源码没有直接调用 check_types 或 check_ownership。codegen
命中的 15 个文件均在测试 cfg 内，包括生产文件里的旧对照。
Unit Compatibility 路由仍由同一 Unit 类型和所有权产物驱动。
发布的旧函数签名、仓外调用与间接函数引用仍留在 R0 清单。

下一项复现是两个独立的借用结果同时跨 while 回边，其中一个
用于条件、另一个在循环后读取。两种入口通过 check，原生构建
仍报 UnsupportedNode。 只保留一个结果的对照可执行。把另一个
结果放到循环前已结束的词法块，单文件仍报 UnsupportedNode，
作为独立的作用域对照保留。

## 313. R4 while 判空与独立的 String? 借用结果

先用两条持久回归复现全函数准入对兄弟绑定重复拦截的问题。
两个结果都持有到循环退出，以及另一结果先结束其词法块，均
通过前端，在判空条件处报 UnsupportedNode。独立复审又发现
在循环外套一层普通块仍报同错，最终持久回归保留这个嵌套
版本及已结束的兄弟，用原生输出核对零迭代、continue、break、
每轮析构和循环后写回。

准入以源文件和单位内语句身份收集已认证的判空 while。另一
String? 结果只能凭自己的唯一 AfterStatement 结束事实获得
同一普通顺序块树内的循环许可。按 Block.elements 追踪作用域，
到条件臂、循环体或嵌套 callable 停止。保留各借用身份、原有
结束位置和 SSA 回边、退出边验证。

最终候选以 `cc2d15d6` 为基线。源码清单 SHA-256 为
`b3199efcf8cfe94073e033e28fdfafb50204f026761e286dd9b509978a40219d`。
macOS arm64 和离线 Linux x86_64 容器各实跑原始同级、普通块嵌套、
闭块三组单文件／跨文件用例，共 12 个产物。对账确认同源、同输出、
无 stderr、进程退出 0，以及 Mach-O 和 ELF 目标格式。两端各通过
78 项 nullable when、52 项 nullable String 和 1 项 Map 布局准入。
macOS 的严格 Clippy、格式、Rust 尺寸、文档和 diff 检查通过。完整
conformance 和 sanitizer 未随此切片重跑。

独立复审在 macOS 核对直接和嵌套、同根和不同根，含闭块后在
循环中写回独立根。覆盖原单结果路径、活动根的 L0135 冲突和
无判空 witness、条件臂、外层循环边界原有的准入拒绝。本次
限定两个独立 String? 结果。一个普通 String 借用结果和一个
判空 String? 结果混用时，最终候选的单文件和项目均能复现前端通过、
原生 UnsupportedNode，登记为下一项。

## 314. R4 while 与普通 String 借用结果混用

普通 String 借用结果跨 String? 判空 while，或在该循环之前
从普通块退出，两种形态都通过双入口前端检查并在原生准入报
UnsupportedNode。持久回归先确认判空条件处报错。跨回边的
用例同时保留自己的字符串等值比较与循环后读取。

普通 String 的观察规则公开给同一个准入收集器。循环见证仍
来自独立 String? 结果或其同源词法子绑定的精确判空事实。
普通 String 只凭自己的 AfterStatement 结束事实取得顺序块内
的许可。等值和 when 消费自己的普通 String 观察规则；各身份和结束点由原所有权事实提供。

最终候选以 `cc2d15d6` 为基线，源码清单 SHA-256 为
`c446f94deb573a5792f9a573906a05106185ff2dec1a6237343b7cb9dc537998`。
本机 macOS arm64 与离线 Linux x86_64 容器实跑五组单文件与
跨文件项目，共 20 个产物，输出逐字节吻合且无 stderr。
两端各通过 80 项 nullable when、54 项 nullable String、1 项 Map
布局准入和 40 条 word-frequency 公共命令。严格 Clippy、格式、
尺寸检查在 macOS 通过。

同一候选以源码清单身份运行完整公开一致性语料。双宿主对账
通过，两端均为 137/154 能力探针和 0/6 综合程序，零回归、入口
差异及内部错误。综合程序维持现有未实现基线。

Linux sanitizer 门禁以默认测试线程配置复跑通过。标量 ASan、
LSan 正反控、原始 Map，以及 12 个 Resource Map 历史用例均按
预期分类。首次运行的串行 libtest 把 nocapture 日志插在测试状态
行中，导出器通过但脚本误拒；复跑使用同一冻结源码及默认线程。
这套门禁未对本节两个新增程序插桩，也不覆盖 Koven IR UBSan。

独立复审复跑 macOS 正式双入口，同时覆盖有主语 when、plain
等值与判空组合守卫、两个普通借用、闭块后写回来源，及活动根
L0135、无精确见证和同文件另一个 callable 的无关循环对照。

下一项是混合结果经过可达 return 退役。循环之后直接 return 和
循环体内可选 return 的双入口均通过 check、原生拒绝。独立复审
的单可空借用保留循环和真实 return 对照可执行，仍在正确边析构。
条件臂里的混合判空循环亦在双入口复现准入拒绝。

## 315. R1–R4 第一轮收口与合批验收

R1 的 Source、AST、Lexer、Pratt、调用和恢复矩阵在本机 11 个
suite 共 151 项通过。复验 §255 的剩余边界发现，合法的悬挂调用
`produce
(1)` 之后，下一臂以 `((true)) && true ->` 起始时被
误当上一臂的 postfix call。公开单文件和项目入口均报 L0058 和
L0065。将 selector 改成完整分组，或在上一臂末尾加显式分号，
两种入口均通过；待补 AST 回归并修复。

R2 本机标准类型、跨文件类型、循环流事实三个 suite 共 407 项
通过。这一范围仍需随最终候选复验。

R3 两条 Const/Basic 对照仍期待 Basic 给恒跳过的 RHS 发一个
loan。测试首次失败，实际为零。已按 §290 的 typed bypass 合同
将两条对照修正为双入口均无 RHS loans/drops。保留 Const 计划
和材料化观测，以及原始与恢复产物拒绝进入 Basic 验证的检查。
独立复审核对事实生产者与两条能力验证路径。本机 6 个所有权
和交接 suite 共 77 项通过。此项只更新既有测试。

合批发布等待 R1–R4 第一轮已确认阻断关闭及同一候选的相应
阶段门禁。R4 当前包括 §314 的混合借用正常/return 退役与条件
臂。候选通过后先发 PR，跟进双宿主 CI 并合入 main；R5 发布
验收沿原计划继续。

## 316. R1 复合 when 条件的分支归属

§315 的悬挂调用加复合条件先录成 AST 红测，前一臂吞括号并触发
L0058 和 L0065。Phase 1 从行首成对分组向后寻找同层真实箭头，
以现有运算符规则和索引越过配对内部、字符串及类型。函数类型
内部箭头由原严格类型预索引跳过，typed call 复用尾 lambda 的
续行判定。控制 operand 的内层箭头仍归其所属结构。

AST 回归逐项核对前一臂调用跨度、新 entry 条件和箭头跨度。
覆盖复合与多条件、if/when operand、具名泛型调用、return/super、
行首中缀 is/in、跨行 lambda 独立条件及 cast 函数类型的箭头。
扫描计数测试将悬挂调用和复合运算符各从 64 加倍至 128，计数
符合线性上限。独立复审找到控制 operand、跨行 lambda、函数类型
和裸条件恢复欠账，修完后复跑 AST 与最终 CLI。后者在无主体
when 的裸 is/in 负例各只报一个 L0107。

最终快照以 `cc2d15d6` 为基线，源码清单 SHA-256 为
`47d2776a157fed32c34653e24490a409a5dcc5a0662618cba25e6e85af6d595d`。
macOS arm64 与 Linux x86_64 各通过 48 项 parser 内部测试。macOS
另通过 155 项 R1 集成、6 项 CLI、严格 Clippy 和尺寸门禁，Linux
通过 105 项 parser 集成。四组用例在双宿主和双入口均通过 check，
其中三个复合条件共产生 12 个原生产物，逐字节输出符合预期。
独立 lambda selector 的四个原生构建报 UnsupportedNode。脱离
when 的立即 lambda 调用对照在 macOS 同样报该后端错误。本节只验
其 AST 和前端接受。非法裸 is 的四次双宿主／双入口检查各只报
一条 L0107。

同一 Rust 源码的 macOS R2/R3 阶段回归分别通过 407 项和
77 项，零失败、零忽略。完整候选的一致性和 sanitizer 沿合批门禁复验。

R4 另读取同一工作树前端发布的两个混合结果结束点。循环后直接
return 时两结果各只发布同一 ControlTransfer。循环内可达 return
时两结果各有该 ControlTransfer 和词法块的 AfterStatement。循环
位于条件臂且正常退出时两结果各有单个 AfterStatement。后续分别
验收这些真实结束边及准入与构建一致性。

## 317. R4 混合字符串借用的 while 返回边和条件臂

三条持久原生回归先在判空处报 UnsupportedNode。修复读取
§316 核对的完整 source-qualified binding end，允许带精确
ControlTransfer 的 return 以及正常续行的 AfterStatement。
普通控制扫描从该绑定的顺序块沿已准入的语句控制进入各自的臂。
臂内声明的绑定留在本臂，循环体和嵌套 callable 各自核验。正常
和提前退出使用后端既有的 path-specific 清理与 SSA 资源验证。

回归覆盖循环后直接 return、循环内 continue 后真实 return，
以及条件臂执行、空值零迭代和跳过循环。每次匹配当前迭代析构和
外层析构顺序。独立复审复跑三条用例的单文件和跨文件入口，另
核 Boolean when 中的循环、同名遮蔽、活跃来源的 L0135、未知
回边与跨臂负例。真实绑定与来源各自匹配。

最终候选以 `cc2d15d6` 为基线，源码清单 SHA-256 为
`3082da841afd6a23e245dad2346fd667f6196fa6219b55ddcd1421eb3ea97484`。
macOS arm64 与离线 Linux x86_64 容器各通过八组双入口实跑，
共 32 个原生产物，输出逐字节符合预期，目标格式分别确认为 Mach-O 和 ELF。两端各通过
83 项 nullable when、54 项 nullable String 和 1 项 Map 布局准入。
macOS 另通过 111 项 SSA verifier、严格 Clippy、格式、尺寸、
文档及 diff 检查。完整 conformance 和 sanitizer 沿合批门禁复验。

下一项双入口复现是将 Boolean 主体改为独立 String? 主体的
nullable when。正常结束事实齐备，前端通过而原生仍报
UnsupportedNode。§316 的 lambda 缺口也已缩到直接或加括号
调用 literal，带捕获时同样复现。局部命名后再调用的对照可运行。

## 318. R4 独立 owned String? when 与词法字符串借用

原始局部 owner 及参数 owner 的双入口通过 check，原生在 else
内的判空 while 拒绝。先修读同一分析的 typed／owned 计划与
已发布的作用域结束点。旧构造为 MoveOnly 主体在 else 建立
payload 视图，循环边发现这个活跃 proof 又拒绝。没有 typed
载荷读取和 owned 调用使用的自有 String? 主体改走 NullableIsNull
布尔分支，两个消费者共享判定，按原事实清理 owner。

独立复审验证了非 null 空字符串、null、正常退出、循环内
return，以及未认证循环和真实 payload 使用的准入边界。复审
另发现删掉不可达 while 会改变外层 when 的准入。最终让同一
作用域中来源独立的 when 直接消费自己的 typed 与 owned 计划，
并用已有语句控制合同处理可支持的臂内结构。原生回归同时保留
有循环和完全无循环的用例，循环与控制分别核对其直接凭证。

最终候选以 `cc2d15d6` 为基线，源码清单 SHA-256
为 `c303fb99ed66675badc7ce3cdbf0c8f360bdddf150125c8fc5180fe8425d901b`。
macOS arm64 与离线 Linux x86_64 各实跑 11 组单文件／跨文件
项目，共 44 个原生产物。对账确认源码组成、逐字节输出、空
stderr、正常退出及各自目标格式。独立复审核对 8 项正向 CLI
探针和 3 项负向准入，含无循环与不可达循环输出相同、兄弟臂
独立语句控制、同根与既有借用载荷视图。

两端各通过 85 项 nullable when、54 项 nullable String 和
1 项 Map 布局准入。macOS 另通过 111 项 SSA verifier、严格
Clippy、格式、尺寸、文档及 diff 检查。

最终候选的公开一致性语料通过双宿主对账。两端均为 137/154
能力探针、0/6 综合程序，零回归、入口差异及内部错误。过渡
候选的 Linux conformance 在固定最终树后结束，最终数据只取
重新执行的冻结候选。sanitizer 留在合批候选验收。

下一项立即调用 lambda 已复跑无捕获、共享捕获和 move 捕获。
前端接受合法无参形式，原生生成失败。局部命名再调用的共享
捕获对照正常运行。导出的所有权事实中，命名闭包有自身的
CallReturn 释放；捕获 literal 的 Basic drop 计划中未检出
闭包 temporary 清理。该组合需要同时检查所有权发布与后端调用时机。

## 319. R3/R4 立即调用 lambda 的环境与调用退出

无捕获、共享捕获和 move 捕获的无参字面量调用通过前端，原生
构建先报 UnsupportedNode。Refine 从 typed FunctionValue descriptor
和已有 thunk plan 接受直接或分组 literal，在实参前形成 callable。
前端将内层 lambda ID 登记为调用期 owner，正常返回按 CallReturn
清理。无捕获函数指针也有逻辑 SSA owner，首次只登记捕获环境时
verifier 报 MissingOwnedExit。

独立复审发现只读捕获的纯 String 来源缺少调用期保护。合并既有
独占捕获调用事实和同一 lambda 发布的共享捕获来源，沿原清理
算法先释放环境，再释放已最后使用的纯内存来源。Unit 的两个
入口均保留后续仍被读取的来源。持久事实测试修前仅看到闭包
temporary，修后按顺序看到 temporary 和源 binding。

原生 Abort 回归另发现 `error(owner.label)` 既在具名闭包也在立即
闭包触发 UnsupportedNode。消息字段准入只排除外层 error 本身的
Abort，仍检查实参中的提前退出。独立复审核对正常调用、嵌套
退出和 Abort 的边界。原生正例核对共享 Resource 的词法析构、
move Resource 的调用返回析构、共享 String 的读取，以及
error 的 SIGABRT、确切消息位置、无捕获析构及无续行。

本机 macOS arm64 与离线 Linux x86_64 均通过 3 条新增原生
回归。冻结源码清单 SHA-256 为
`d2e2c71aa2b53c7c5cdce411605d8add39bee6a26da84b4276153adc2e944d7f`。
同一编译器源码树汇集 17 组单文件及跨文件项目用例，双宿主实跑
68 个原生产物。逐项确认输入正文一致、check/build/run 成功、
stdout 逐字节相同、stderr 为空，目标格式分别为 Mach-O arm64
和 ELF x86-64。§316 中直接调用 Boolean lambda 的 when 条件
同时通过两宿主两入口。

## 320. 第一轮 PR 合批扩大验证

[PR #443](https://github.com/Halckon/Koven/pull/443) 将 §316–319
已复审修复与同批积累的 R2–R4 修复提交到 main 基线 `cc2d15d6`。
两宿主各通过 87 项闭包所有权与帧测试。§319 的候选另在公开
conformance 矩阵达到双宿主一致，均为 137/154 能力探针和 0/6
综合程序，原基线 23 项未实现，零回归、入口差异及内部错误。

扩大验证的前端 lib 有一条旧根写入回放断言仍期望 Basic 模式
记录恒短路右侧。本机复现 `false && ...` 实际只记录后继写入。
按 §290 的普通 bypass 合同更新测试预期。两个模式继续核对六组
动态、恒跳过和恒执行的条件，以及实际写入的短路与 when 分支
归属、续行清除。独立复核确认改动限于测试。定向测试通过，macOS
前端 lib 复跑 881 通过、0 失败、14 忽略。忽略项保留原门禁分流。

## 321. R3 所有权短路检查的默认栈

PR #443 的 macOS 与 Linux CI 均在 16 个独立判空证明的既有
原生回归发生栈溢出。此前本机扩大线程栈的回归通过，未覆盖
CI 默认栈。两宿主用默认栈复现 SIGABRT。macOS LLDB 堆栈
显示短路链每次递归都保留通用 AST match 的大帧。

将 AST 分派移到原 typed dispatch 之后的独立函数。短路直接
复用原 planner 和 left/right 数据流，保持恒跳过、恒执行、动态
合流与 callable 来源记录。独立复审核对原优先级和调用边界。
原 16 段用例及 stdout 断言保持不变，macOS arm64 和离线
Linux x86_64 均在默认栈复跑通过，Linux 启用 CI 使用的
`loan-observation-test-support`。

## 322. R1 循环内 when 条件的跳转与恢复

循环块中，悬挂调用后的复合 when 条件包含 `break` 或
`continue` 时，AST 红测显示前一臂丢失换行实参。零操作数跳转
进入 selector 扫描，已配对的括号先由 when 同层箭头决定归属。
普通 block 换行仍负责未配对括号。独立复审发现省略这一步会让
调用恢复吞掉后续 else，追加 AST 回归保留原臂和末尾 else。

macOS 的 parser 内部、控制流、block 续行与入口换行矩阵共
95 项通过，覆盖前一臂调用跨度、新条件和箭头、末尾 else。

## 323. R4 同一堆对象的不同 MoveOnly inout 字段

直接类字段通过前端后，普通调用 lowering 先拒绝字段实参。
接通该路径且保留旧 verifier 时，正例改报 InvalidModel。
准入与构建共用已验证的字段投影及该实参唯一的同源独占 loan。
MoveOnly 调用在全部实参正常完成后建立字段地址，无需回写
局部 owner 快照。SSA 复用借用建立时使用的直接字段互斥证明。
同一字段及未知布局的重叠保持拒绝。独立复审核对了相邻实参、
提前退出、正常 loan 退役和无凭证路径。

新 SSA 回归验证不同字段通过；将同一 loan 交给两个位置时，
断言调用指令拒绝。macOS 4 项 move-only inout 回归通过。冻结
候选在 macOS arm64 和 Linux x86_64 分别通过不同字段写入与
§322 循环跳转的单文件和跨文件项目，共 8 个原生产物，逐字节
输出一致。同字段的四次公开检查各返回一条 L0135。

## 324. 第一轮扩大门禁的逐项合同核对

按 [PR #443 上一轮双宿主日志](https://github.com/Halckon/Koven/actions/runs/44245165150)
对账。基线 `cc2d15d6`，出现以下完整失败集合的 head 为
`51845889`。原默认栈回归在该 head 已通过，两端随后各遇到
同组 21 项代码生成测试失败。更早的 suite 提前退出，未建立
每一项的首次失败 SHA。`c26047e6` 的两项定向修复与下表分别验收。

| 组 | 原失败测试 | 原合同与当前行为 | 决议及验证入口 |
|---|---|---|---|
| A | `ordinary_branch_neighbors_keep_the_unit_declaration_gate` | 单层 String 转发邻例要求原 receipt；新增同根检查绕过该门 | 恢复原负例的 UnsupportedNode、marker Span、无 receipt、未进最终 verifier |
| A | `ordinary_balanced_neighbors_keep_exact_unsupported_boundary` | 四叶树的错外层条件超出 receipt | 同上，保留全部输入和观察开关 |
| A | `ordinary_bounded_terminal_tree_neighbors_stay_unsupported` | 深度和相邻拓扑不匹配 bounded receipt | 同上 |
| A | `ordinary_nested_else_cascade_tail_wrong_outer_keeps_backend_unsupported` | else cascade 的外层条件无匹配 receipt | 同上 |
| A | `ordinary_nested_else_cascade_tail_neighbors_stay_unsupported` | else cascade 相邻形状、错内层条件 | 同上 |
| A | `ordinary_nested_then_tail_wrong_outer_keeps_backend_unsupported` | then tail 的外层条件无匹配 receipt | 同上 |
| A | `ordinary_nested_then_tail_neighbors_stay_unsupported` | then tail 的额外语句等相邻形状 | 同上 |
| A | `ordinary_nested_neighbor_trees_keep_exact_unsupported_boundary` | 三叶树错序等形状无匹配 receipt | 同上 |
| A | `ordinary_nested_four_leaf_wrong_flag_stays_unsupported` | 四叶树旗标无匹配 receipt | 同上 |
| A | `ordinary_single_continuation_neighbors_stay_unsupported` | 双空位及额外语句无 continuation receipt | 同上 |
| A | `child_borrow_branch_remains_unsupported` | 旧门禁拒绝普通 String 臂内借用，现由与该臂相符的词法结束事实清理 | 延续 §317–318 的独立 arm 合同。保留两种 unit 输入和原调用，验证异源、同源和合流后读，原生输出逐字节核对 |
| A | `multi_parameter_borrow_assertion_argument_remains_unsupported` | 旧门禁拒绝 `!!` 辅助实参 | 保留原输入，两种 unit 验证 SSA，原生核对返回 loan 仍读 lender；不扩展 Inout |
| A | `mixed_value_borrow_wrapper_control_prefix_remains_lowering_unsupported` | 旧门禁拒绝辅助实参中的负号、if、加法 | 保留三例和两种 unit，按普通实参和借用结果合同验证 SSA、原生读取 |
| A | `mixed_value_borrow_caller_cfg_remains_lowering_unsupported` | 旧门禁拒绝结果作用域跨后继分支 | 保留原两处读取及两种 unit，验证 SSA、原生输出。错误来源、活跃借用和可调用值原负例继续检查 |
| B | `rejects_recursive_nullable_owner_until_ssa_declaration_cycles_are_supported` | 旧检查要求 Node? 回边拒绝 | §282 准许间接堆回边。验证声明完成及 LLVM，缺 payload 负测保留 |
| B | `field_replace_single_nullable_recursive_class_layout_stays_unsupported` | 旧检查拒绝自递归和互递归普通类 nullable 参数 | §282 精确迁移，验证 SSA/LLVM 和普通类有限活动链的析构终止 |
| B | `finite_value_class_recursion_through_a_heap_handle_lowers` | 旧检查允许布局但仅接受 fallback | §283 的 Refine 接管有限布局；核对同一入口、aggregate/heap/Rc 布局，实跑活动堆边 |
| B | `field_replace_single_nullable_layout_keeps_nonclass_and_generic_boundaries` | 八项 recipe 中旧检查包括 String? | 仅迁移 §303 支持的 String? niche，核对 inner 为 StringOwner，另外七条和精确字段位置保持拒绝 |
| C | `enum_tag_slice_does_not_claim_general_payload_grouped_or_move_only_equality` | 旧拒绝含分组的无载荷 case | §278 仅迁移 Copyable 分组 case，原生核对命中和未命中。普通值、带载荷、MoveOnly 三条仍在原 Span 拒绝 |
| C | `retired_ownership_primitive_boundary_tests` | Int? when 原走自然 fallback，组合中的 replace 被拒绝 | §259 的默认入口进入 Refine。原生核对旧值、写回及选择结果；同一源和原强制旧路径均在 replace Span 拒绝 |
| C | `rhs_only_move_uses_published_short_circuit_cleanup_on_both_exits` | 旧断言要求静态跳过分支带 owner 清理 | §290 静态跳过无 RHS 交付；动态 OR/AND 的跳过边仍有实际 owner 清理。保留跨源输出和四次分配释放 |

A 组十个邻例的原只读检查保留。普通同根字符串返回入口仍要求
每个出口来自同一参数，并检查直接返回、多实参转发，或同一个
return 上确切的词法 String 结束事实。纯单实参转发树继续使用
原 receipt。这项准入变更与四项自然支持场景的验收更新分别复审。

同一候选还需验证最终 PR-event 双宿主 Targeted、聚合
CI Passed 与运行 SHA。旧冻结候选的 conformance、sanitizer
和 native 输出记录保持各自输入身份。

macOS arm64 以默认栈定向复跑 42 项，涵盖上表原 21 项及相关
原生、布局和 verifier 对照，零失败、零忽略。两个新增堆布局
实测核对分配和释放的逐指针身份。普通类互递归为有限活动链，
分配由内到外，各父对象先 deinit、再清理并释放其字段，最后
释放自己的指针。value class 的普通堆边只分配和释放活动 owner。
这两项仍依赖原内联环、非法 nullable 和缺少定义的拒绝负控。

## 325. 文件恢复及字段调用的扩展验收

§322 的完整函数文件回归覆盖跨行 break、continue，以及未配对
括号后的 else 和随后的顶层声明。只追加入口验收，没有改变
selector 实现。macOS 控制流、block 续行和入口换行共 48 项
通过。公开诊断核对缺失括号后的单文件和项目行为。

§323 新增带资源字段的调用回归。两个 Inout 字段之间及其后
各求值一个普通实参。提前 return 只执行已求值的前缀，保留并
清理两个原字段且不分派；正常路径按源码次序求值、替换两字段，
调用后可以读取新值，最后完成外层析构。macOS 原生回归通过。
同字段和 owner 与字段重叠在单文件、项目入口仍报 L0135。

## 326. 文件尺寸门禁和双宿主定向对账

`b8ad88fe` 的 PR-event Rust Physical Line Guard 报
`provider/borrow_result.rs` 共 1014 行。将字符串入口与转发
身份核对移入同模块的 `entry_string.rs`，两个函数仅改变模块
可见性，原逻辑逐字节不变。拆分后分别为 837 和 183 行，尺寸
检查通过。macOS workspace 严格 Clippy 和默认栈 42 项复跑通过。

拆分前、与 `b8ad88fe` 一致的编译器源码清单 SHA-256 为
`c555ca019a2150357e3e5717e48aa6cdce80fccdafdcd99d9087eec1c14e85ee`。
macOS arm64 与 Linux x86_64 各通过 42 项定向 SSA/native 回归
和 48 项 Parser 入口回归。双宿主的 5 组合法源码均跑过单文件和
跨文件项目，共 20 个原生产物，退出 0、stdout 逐字节符合预期，
stderr 为空。3 组负例共 12 次公开检查，同字段和父子重叠各报
一条 L0135，缺失括号两入口均报 L0010、L0056、L0059。
完整测试继续看当前 PR-event 的两个宿主及聚合检查。


## 327. 第一轮双宿主 PR-event 合批结果

[#443](https://github.com/Halckon/Koven/pull/443) 的最终 head
`134a72efda2fdfa543d44a92778aeb4c302571b2` 与 base
`cc2d15d6947cf413a474484cdaf30c4e8cad5f26` 由
[PR-event CI](https://github.com/Halckon/Koven/actions/runs/44252093643)
验收。两个宿主的 Targeted 检查和聚合门禁通过，包含公开入口、预览、
generated owners 与 Linux sanitizer。Conformance 为 137/154 探针、
0/6 综合程序，精确通过范围按该轮证据保留。

CI 合并候选 `cc66331b21f7881a238e1a87e01c4571a7e7ee7c` 与
实际 main 合并提交 `625f6804a9dc448832f148f6eb3d8b8bf9ad7be6`
具有相同父提交和树 `fd79bad731f6c177bb9f1c2813fccd8e6328c23b`。
此次合批关闭 §321–326 记录的 PR 阻断。生产 Unit 兼容路径的处置、
第一轮冻结候选的全阶段纵向复审以及 v1 功能冻结后的 R5 仍按 §3 执行。

## 328. R4 Copyable 类字段的普通 Inout 调用

基线 `625f6804` 接受同一普通类实例的两个 `var Int` 字段
作为普通 Inout 实参。原生回归和公开单文件及跨文件项目的
`run` 均复现 UnsupportedNode。§323 的字段凭证和构建调度
原先仅供 MoveOnly 使用。现在基于既有同源字段投影与独占 loan
识别两类目标；其余实参完成后才建立字段地址，callee 直接写堆
存储。Copyable 局部根仍沿原方式写回。

独立复审核对准入与构建一致性、提前退出不物化字段 loan、
正常结束顺序，以及同字段重叠检查。独立记录的后续验证项是
手工 SSA 将同一 Copyable 独占 LoanId 放入两个调用槽位；当前
调用端成对重叠判定只覆盖 MoveOnly，旧 Copyable 根也有此缺口。

macOS arm64 默认测试栈通过全部 58 项 `inout` 匹配用例（含 10 项
字段定向测试）。公开单文件和跨文件项目均退出 0，输出逐字节符合
预期，stderr 为空。后续实参提前 return 仅打印已求值的前缀；
正常路径按源码顺序求值并读回两个新字段值。同字段负例在两入口
各以退出 1 和唯一 L0135 拒绝。

同一编译器源码清单 SHA-256
`99a51083ee8c4bfee9a99dbf1f34c3c88db8cf76cea9d70ab1e5fb94a1d03862`
在离线 Linux x86_64 容器通过 10 项默认栈字段测试，复用与 macOS
相同的单文件、跨文件项目及两个同字段负例，退出码和输出逐项一致。
macOS codegen 全 target 严格 Clippy、格式、文档与 Rust 尺寸检查通过。
本节为定向验收，新分支尚无 PR-event CI 结果。

## 329. R4 Copyable 独占借用的调用槽位

先降低两个不同 `Int` 局部根的普通 Inout 调用，原始 SSA 通过。
仅将第二个实参替换成第一个 LoanId 后，旧 verifier 仍返回成功，
正式负例据此失败。现在独占实参先核对 LoanId 相等或别名冲突，
再对 MoveOnly 单独登记根写回；不同直接字段继续使用既有分离证明。
两个 SSA 回归都先核对合法输入，再断言篡改仅有的调用槽位后，在原
调用指令上返回对应合同错误。独立复审核对转发、堆地址和根写回的
分支，以及 CFG 合流别名集合的保守性。RangeCall 的手工 SSA 辅助
独占实参仍需另行核验，当前源码 lowering 不生成匹配的实参 loan。

同一编译器源码清单 SHA-256
`e87a2b1df0d1641aeae0bbb6185e4dfafe797168ab909a21e2996a5d0c1bbec9`
在 macOS arm64 与离线 Linux x86_64 各通过 59 项 Inout 定向测试。
Linux 复验 §328 的单文件、跨文件项目、提前退出及同字段拒绝，
stdout、stderr 与退出状态同预期。这批尚无新 PR-event CI 结果。
同一源码的 macOS 全量默认 codegen lib 复跑随后完成，3323 通过、
0 失败、71 ignored；该结果不覆盖 §331 及以后的源码修改。

## 330. R4 Compatibility 混合体的 inline nullable 类型登记

在 §328 已验收的 macOS 编译器上复用 §247 的隐式空臂与堆对象写回。
原体通过，单独的 `val count: Int? = 1` 后判空也通过。在同一
函数加入该局部和判空，公开单文件与跨文件项目的 check 仍为 0，
run 分别以 2 和 1 报 UnsupportedNode。单文件额外调用独立
`fun touch(own value: Int?): Unit {}` 后，原样的混合函数恢复
执行，输出与原体一致。现有 Compatibility 局部包装守卫读取全局
类型表，已有对应登记时即走原适配。§333 记录正式红测和
基于已形成局部变量的类型需求登记。

不可达的 `touch` 声明无法消除失败，只有可达调用带来的类型需求
有该效果。相同混合体使用 `Boolean? = true` 也报 UnsupportedNode；
改用 `Int? = null` 后报 MissingFact。正式回归应分别覆盖非空包装
和空值材料化，并保持不可达 sibling 不影响生产路由。

## 331. R4 局部 String? 的 Inout 写回

PR #443 固定版本复审指出，自有局部变量在非空臂读取时保留共享
proof，接下来的普通 Inout 实参直接借入同根，产生冲突。修复前，
`var slot = input` 在 `if (slot != null) { println(slot); clear(&slot) }`
后读回的公开单文件和跨文件项目均通过 check，原生入口分别以 2 和 1
报 InvalidModel。相同回归增加尾实参条件 return 后仍能复现。

直接根调用现共用既有 proof 退役步骤，先结束同根共享 proof，再转发
Inout 参数的 loan 或建立局部根的独占 loan。MoveOnly 仍在后续实参
完成后才执行此步骤，写回仍使用原根存储。macOS 默认栈通过 60 项
Inout 测试和 3 项写回路径测试。同一回归的单文件与跨文件项目分别
运行 null、空串、普通文本和后续实参提前 return，逐字节符合预期。
额外单文件具名 Guard 探针在四条路径均执行一次析构，提前 return
只输出前缀并析构；该探针未给字符串缓冲区加分配计数。

独立复审核对该顺序、转发与实参提前退出，并提出两个相邻边界。
修复后的 macOS 编译器上，`flag && clear(&slot)` 与
`while (flag) { clear(&slot); break }` 两项均通过前端 check，
原生生成仍因 InvalidModel 失败。待分别核对短路跳过边及循环
条件为假边的 proof 退役和状态合并，阶段退出仍开放。

同一编译器源码清单 SHA-256
`0ba64bf1485afb10e78fb5ff1a94d0584348b27db4a3e1eaaeee8e7e14dae36a`
在离线 Linux x86_64 容器通过同样的 60 项 Inout 和 3 项写回路径
测试。两宿主公开单文件及跨文件项目的 stdout 逐字节一致，
stderr 为空，退出 0。macOS codegen 全 target 严格 Clippy 通过。

## 332. R4 局部 nullable Inout 的短路和循环边

§331 仅修复实际调用边。其候选的单文件探针在
`flag && clear(&slot)` 和 `while (flag) { clear(&slot); break }`
上仍通过 check、在原生生成报 InvalidModel。短路 join 现按同一
proof slot 核对实际 RHS 后继，在缺少该 proof 的合流上结束跳过边
上仍活跃的该根共享 proof；无合流的跳过后继仍可携带原 proof。
循环入口核对同一源单元和当前可调用体内已发布的独占根 loan，
在条件与回边的共同入口结束可能被写回的旧 proof。

独立复审发现，若 `for` 在来源求值前结束 proof，
`0..<upper(slot)` 中的一次性借用会丢失。公开入口实际复现
前端通过、原生报 MissingFact。更新后的数值范围、顺序容器、
RangeView 和 Map 均先求值或取得来源，再于进入循环前清理
受写回影响的 proof。二次独立复审核对了同根来源的冲突拒绝、
常量短路无 loan、其他根的 proof 和临时工厂来源。

同一编译器源码清单 SHA-256
`0850d66bd90c428f8bbff29da742a5d3cd9307671e9434efe6a35618e0c232d5`
在 macOS arm64 和离线 Linux x86_64 各通过 62 项 Inout 和 5 项
写回测试。macOS 另通过 6 项数值 for、22 项 provider/来源退出测试
及 codegen 全 target 严格 Clippy。macOS 的临时 List 和 Map 工厂、
无关根保持与常量短路重复读取的原生探针均通过。直接以窄化的
nullable List 为 provider 的探针，连同移除短路调用的对照，在
修复前后均为 UnsupportedNode，现有 provider 边界仍开放。

两宿主均由公开单文件及跨文件项目入口运行条件与值位置的
`&&`、条件 `||`、while break、while continue 和数值 for，
共 24 个原生产物。所有进程退出 0，stdout 逐字节符合相同 oracle，
stderr 为空。跳过短路和零次循环保留原值，执行路径写回 null，
有 Guard 的用例均按原顺序析构。该分支仍待自己的 PR-event CI。


## 333. R4 已形成局部的 inline nullable 布局登记

§330 的正式红测固定混合函数实际走 Compatibility，前端通过后在
`val probe: Int? = 7` 报 UnsupportedNode。直接 null 初始化的公开
探针另报 MissingFact。沿既有签名优先、函数体在后的类型计划，复用
同源单元、同函数、正常形成的局部变量事实，登记 Boolean 或整数
可空布局。删除依赖全局表偶然已含此类型的局部守卫，由原
NullableWrap 和 NullableNull 消费已登记布局。同一作用域的 Map
事实和 K/V 校验继续走原 mapper。独立复审核对了函数路由、
作用域归属以及布局登记和生成的一致性。

新回归对 Int 和 Boolean 分别运行 non-null 与 null，再对照无签名、
不可达签名、可达签名三个状态。实际核对 Compatibility 路由、
SSA verifier、源程序输出与两个 Guard 的析构次序。

macOS arm64 通过 30 项 replacement-if、类型来源和标量路由用例，
另通过 27 项 Map promotion、nullable operation 和 Unit 类型计划用例。
公开单文件和跨文件项目用同一组预期测试 Int/Boolean 的 null 与
非空、可达签名与无签名，16 个原生产物全部退出 0 并逐字节匹配。

另有两条边界仍开放。独立 `Map<Int, Int>` 查询可以运行，加入
该调用后，含普通 `Int?` 局部的原函数在单文件和跨文件项目均通过
check，但原生报 UnsupportedNode。Map preregistration 先为同一
UnitTypeId 发布 `$map.result`，普通标量包装拒绝该结果身份。
泛型 `T?` 以 Int 实例化的探针仍在原生报 MissingFact，有无可达
`Int?` 签名结果相同。两条边界均在本修复前后复现，需分别核对
Map 结果身份隔离及泛型实例的类型事实。

同一编译器源码清单 SHA-256
`c54c9dd980327af57f9512bb8814e34282b0ae4a93b45c4c2184605897dc9335`
在离线 Linux x86_64 容器通过同一组 30 项定向测试和全部 16 个原生
产物。两宿主的 stdout 逐字节一致，stderr 为空。macOS 另验证
可达签名单独成文件且排在目标文件前后时结果一致；严格 Clippy、
格式、文档和 Rust 物理行数检查通过。该候选尚无 PR-event CI。


## 334. R4 Map 查询与普通 Copyable nullable 包装共存

§333 的独立 Map 查询与普通 `Int?` 局部分别通过。组合后在单文件
和跨文件项目通过 check，正式红测定位到普通局部包装的
UnsupportedNode。Map 类型预登记使用 `$map.result` 的存在位和载荷；
两条路径共用该类型时，Compatibility 原来的拒绝覆盖了 Copyable。
现复用既有 `NullableWrap`。Verifier 核对结果与实际载荷类型，
LLVM 写入存在位和原值，直接 null 继续由 `NullableNull` 零初始化。
独立复审核对了这条链、实际路由及所有权条件。

回归在原有签名可达性矩阵上加入可达 Map 查询，明确区分 `0`、
`false` 命中与缺键，并核对普通 Int/Boolean 可空局部初始化及 Guard
析构。macOS 通过 57 项相关定向测试，公开入口另从 Compatibility
把普通包装交给细化层的 `when`，核对整数和布尔载荷的实际值。

§333 的泛型探针进一步缩减。只包含 `T?` 普通非空或空初始化的
`run<Int>` 可正常运行；去掉 `T?` 和泛型值形参后，如果泛型函数包含
owned nullable `when`，在非空臂写回并重新判空借用，仍在两个
公开入口通过 check、原生报 MissingFact。非泛型同体、generic
when 直接读取和移出 when 后的单独写回判空均通过。后续聚焦该
泛型控制流消费者及写回后的证明身份。

同一编译器源码清单 SHA-256
`f445297c19234f951bb9fc905805bb0e0f842f97e085961eb52ec8d5e7dd07cf`
在 macOS arm64 和离线 Linux x86_64 均通过 57 项定向测试。
两宿主由公开单文件与跨文件项目运行四组命中/空值矩阵和两组
跨路由载荷检查，共 24 个原生产物，退出 0，stdout 逐字节一致，
stderr 为空。macOS 另通过原生 Map 28 项及 codegen 严格 Clippy。
当前分支尚无新 PR-event CI。


## 335. R4 泛型实例的 nullable when 写回与重新借用

§334 的正式红测在前端和初始 nullable when 通过后，定位到
写回后 `observe(x)` 的实参 `x` 报 MissingFact。Compatibility
曾按调用者含类型替换或类型实参跳过 fresh heap If proof。现在核对
根的 canonical nullable 类型在当前实例中保持同一身份，再用原
source token、完整 instance key 和 FunctionId 校验所属函数。
原窄化根、实际 owner、共享借用、目标调用和前缀边界继续验证。
独立复审另核对了 StaticSelf 和嵌套作用域边界。

回归覆盖 Int、Boolean 和两种实例共存，分别执行旧值为空、替换为空
后 return、替换为非空后重新借用。macOS 首个候选通过 58 项相关
测试，两个公开入口原生验证相同输出和 Guard 析构。另用实际泛型
Abort 验证退出 134、报告 `nullable-stop` 且无 Guard 析构。

## 336. R4 泛型 Compatibility 局部 T? 的存储需求

§335 把泛型 fresh proof 修复后，将直接 `T?` 局部放回组合体。
前端通过、未登记布局时再现 MissingFact；增加无关可达 `Int?`
签名则通过。已形成局部的规划现按该 callable 实例替换直接
类型参数，只在具体内层为 Boolean 或整数时登记 inline nullable，
仍由原 mapper 验证实际布局。独立复审覆盖了源单元、最近函数
作用域、实例替换及与 Map 结果边界的交接。回归在无可空标量
签名的前提下运行 Int/Boolean 两种实例，分别核验 wrapping
和 null 初始化，包含 Boolean 的 false 仍为 present。

最终 macOS arm64 候选在默认栈通过 59 项相关测试及
codegen 全 target 严格 Clippy。公开入口的混合实例、局部
可空布局、Const-enabled 组合和 Abort 输出均与预期一致。
早期 Linux 候选的并行测试进程挂起，原测试二进制以默认栈
串行复跑 58 项均通过，已结束挂起的进程。该次串行复跑属于
§335 首个候选，不能作为本节最终候选的验收。

同一最终编译器源码清单 SHA-256
`d338c301aa6eecc838f80f86f8619498132db89b75169484ea97c61654ae1a05`
在离线 Linux x86_64 默认并行和默认栈通过同一组 59 项，早期挂起
未重现。双宿主的单文件与跨文件项目共运行 24 个原生产物，20 个
正常进程退出 0、输出逐字节一致、stderr 为空；4 个 Abort 进程
退出 134，输出一致并核对错误报告，均未执行 Guard 析构。
当前 GitHub main 与本轮基线 `625f6804` 一致，新候选尚未有
PR-event CI。


## 337. R4 RangeCall 辅助独占实参的 SSA 合同

复查 §329 的 RangeCall 余项。直接构造有真实 RangeConstruct、
RangeCall 及 RangeReturn 的 typed SSA，来源是该签名原有的
共享容器 loan，两个辅助参数为不同独占 loan。原 verifier
接受该控制；替换其中一个参数为另一个 LoanId 后仍返回成功。
另一个红测让第一个独占实参持有活跃共享子借用、且在调用后
才结束子借用，原 verifier 也返回成功。

RangeCall 现复用普通调用的实参检查、独占重叠与 MoveOnly
写回约束，原 Range 来源和精确根权限继续验证。定向回归覆盖
Copyable/MoveOnly 两种辅助类型、独立实参到 LLVM、重复独占
实参拒绝和活跃子借用拒绝。独立复审核对执行顺序、来源与
结果配对。手写用例的 MoveOnly 为参数转发；本地 RootPlace
写回由共用的其它调用测试覆盖，本节没有单独执行其 RangeCall 组合。

同一编译器源码清单 SHA-256
`adde8dd884909e625cfcd0d2b664dd4e4d8e46ebd5708f2081faee688b23f706`
在 macOS arm64 与离线 Linux x86_64 各通过 76 项定向测试。
公开单文件与跨文件项目均执行标准 `take` 的临时资源来源，
两边的进程退出 0，stdout 逐字节一致、stderr 为空；先读保留的
元素，再按既定次序析构两个对象。macOS codegen 全目标严格 Clippy
通过。当前候选尚无新 PR-event CI。

另用新旧候选复查活跃 RangeView 的分支：取 `prefix.size` 到局部
且没有分支可运行，提前读取后再执行 `if` 则失败；条件与 View
无关且分支后仍读取 View 的用例，也在两个公开入口 check 通过、
原生报 UnsupportedNode。先用独立语句块结束 View，再进行分支
的同类型对照在两个入口均成功。该源程序边界与本节负责的
辅助独占实参验证分别记账。


## 338. R0 直接调用清单复核

按 §151 的同一口径复查当前工作树已跟踪 Rust 文件。数值为
文件数／匹配行数，排除同一行的函数定义。包含测试及字符串，
不涵盖间接调用、宏展开和仓外 API。

| 范围 | resolve_names | check_types | check_ownership | complete_file_types | lower_scalar_file_with_entry |
|---|---:|---:|---:|---:|---:|
| frontend src | 43 / 53 | 40 / 50 | 22 / 23 | 3 / 3 | 0 / 0 |
| frontend tests | 81 / 153 | 70 / 107 | 48 / 73 | 22 / 30 | 0 / 0 |
| codegen src | 15 / 18 | 15 / 18 | 14 / 17 | 7 / 7 | 4 / 8 |
| CLI src | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| LSP src | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |

相关范围未检出显式 `as` 别名。复查了长枚举/深层窄化和数值
溢出：两组生产 Unit 对照均已存在，旧 Single 分别保留旧 case
表确定性和独立溢出诊断，见 §239。另核对 `string_clone.rs` 的
Single CallReturn loan-end 及 `ownership_structural.rs` 的投影
对照，两处测试明确保留旧产物。以上是静态清点，下一轮按剩余
消费合同核对是否已有等价 Unit 证据，再决定迁移或保留。

补查三项旧入口的标识符引用，覆盖 import、限定路径和直接调用。
CLI、LSP 与 frontend analysis 源码均未引用这三个标识符。
`lower_frontend` 模块和 `lower_scalar_file_with_entry` 重导出
均受 `cfg(test)` 约束。真实生产模块防回流检查及其负例共
14 项在 §346 候选的 macOS 测试二进制通过。
`lang_frontend::type_checking::check_types` 和
`lang_frontend::ownership_checking::check_ownership` 仍是
公开 Rust API。该树的仓内标识符与依赖检查不能确定仓外调用，
这两项 API 的迁移处置继续开放。

与 RangeView 后续切片相邻的三个 frontend integration suite 也已复核。
`ownership_range_producer.rs` 的来源、递归拒绝与输入顺序当前均走 Unit。
`type_range_carrier.rs` 的非逃逸位置用 MemoryDocument 与 Project 入口覆盖，
仅保留 §12 所记的 name-invalid raw Single 恢复对照。
`ownership_range_receiver.rs` 保留三种元素布局下的 Single source loan
没有在 producer CallReturn 结束的独立断言。其 Unit 同源对照独立检查
真实根、parent/child metadata 和词法结束顺序。Unit 的
`loans_ending_at` 只按创建点筛选，不表示实际终止事件，
因此这处测试不能通过改用同名查询迁移。


## 339. R4 活跃 RangeView 跨缺省 else 的语句式 if

§337 的公开入口复现缩减为同一词法块内的 `take` 描述符、
普通一臂 `if` 和合流后同步读取。前端检查通过；单文件和跨文件项目
均在生成前报 UnsupportedNode。正式红测定位到该 if 的完整源码区间。
带显式 else 的双臂语句早有同类运输合同，见
[SPEC-0298](../../archive/specs/0298-range-binding-local-control-flow.md)。

修复允许通过既有准入检查的 Boolean 条件、Unit 结果语句省略 else。
同根来源、source loan、词法结束、语句位置和臂内调用检查仍生效；
假分支仍应用已发布的 BranchExit 清理再合流。值上下文、嵌套、
循环内 if、臂内新绑定和无终止事实的控制转移保留现有拒绝。
独立复审核对了上述边界和短路条件的共同调用点。按复审建议，
用真分支内及合流后均读取同一 View 的资源原生回归替代原先
冻结的无 else 拒绝测试。该回归运行动态真假两条路径，
并检查每个源对象的析构次数和顺序。

macOS 首次扩大到整个 Range lowering 模块时，328 项通过，唯一失败为
`scalar_counts` 仍要求拒绝 `if(true){read(part)}`。按现行语义另建常量真、假
以及合流后的资源运行见证，其余五个非 Int 表达式、while 和独立值位置 if
的拒绝证据保留。独立复核确认动态和常量回归的输出、析构预期及负例范围。
两个未更新该负例的 Linux 快照随后主动停止，未用于最终验收。
最终编译器源码清单 SHA-256 为
`4e80582fdfdb33da16080eb5996ae1c92dc73af5ba5479757ba7c3d107ab6d82`。

同一最终候选的 macOS arm64 定向测试通过 20 项，含两种新单臂回归和
保留的准入负例。公开单文件与跨文件项目均按顺序观测真分支内读取、
标记合流、合流后读取，然后按 `second`、`first` 各析构一次；
假分支跳过臂内读取。两个入口退出 0 且 stderr 为空。
原最小探针另运行成功。单文件短路条件探针只在真路径求值 RHS，
且保留相同的合流读取和析构输出。macOS codegen 全 target 严格 Clippy 通过。
§337 中先取 `prefix.size` 的对照换成正常调用臂后原生输出正确；
保留 `error` 的 Abort 臂仍在前端 check 通过、生成阶段报 UnsupportedNode。
真臂尝试 move 活跃 View 的源 owner 时，公开单文件 check 仍在实参处报 L0135。

离线 Linux x86_64 对同一编译器源码清单通过相同 20 项定向测试，
与 macOS 均用默认测试栈和默认线程。两个公开入口的进程退出 0，
stdout 和 macOS 及预期文件逐字节一致，stderr 为空。最终候选
没有重新执行整个 Range lowering 模块或新 PR-event CI。

相邻未闭合边界已在同一 macOS 编译器重现。将描述符接到
`borrow val child = (part)`，再让别名跨语句式 if，单臂和显式双臂
均通过 check，但公开单文件和项目原生生成都报 UnsupportedNode。
去掉该 if 的别名对照在两个入口均成功且输出一致。
后续单独核对 `BorrowedCarrierMetadata` 的父描述符依赖和分支清理。
该别名只在合流后读取时仍重现；将其换为 `take(part, 1)`
创建新的子描述符，保留同一分支与读取位置，则单文件原生运行成功。


## 340. R4 同块 RangeView metadata 别名跨普通 if

§339 的两个公开入口复现来自同一词法块内的描述符与直接别名。
正式红测在 Unit Lowering 报 UnsupportedNode，span 对应完整语句 if。
原词法 scope 证明只接受新描述符，因而有别名时无法放行此控制。

新分支准入复用现有类型与父引用检查。只有同块、先于子绑定、来自
同一根且拥有新描述符的真实父绑定可以证明该直接别名。父、子和
块内其他承载根依赖的绑定各自要求同一实际语句块结束事实。
普通语句 if 继续要求可正常合流及有界只读调用。短路、算术、
值位置 if、嵌套、循环及已有提前返回检查各自保持原准入入口。
回归保留旧 metadata 标量、不同根和其他控制的拒绝见证，
并替换正常语句 if 的旧拒绝见证为可观测原生用例。

独立复审发现初版会接受 `show(-n)` 的隐式溢出边。macOS 原生
探针确认 `n = Int.MIN` 时进程退出 134，报告整数溢出且没有析构。
最终 gate 仅在别名的新增普通 if 准入中拒绝运行期负号，
直接整数字面量沿用原常量路径。定向测试保留该负例。
加固前的候选在两宿主各通过 25 项定向测试，单臂、双臂的公开
单文件和跨文件项目都符合相同的读取和析构预期。
加固后的首个候选编译器源码清单 SHA-256 为
`ee53e5106d9d60b09b656526b183b571585a487e57c0d6b93cb2fe64c6a97a03`。
尺寸门禁发现 `local_if.rs` 超出 1000 行，已将 metadata 准入函数
原样移入子模块，最终编译器源码清单 SHA-256 为
`c7ea25a375531023ac375f21beb5d1063a9476578527fe5974a786426955e028`。

移动前的加固候选在 macOS arm64 通过 25 项定向测试及 codegen 全 target 严格 Clippy。
单臂与双臂在单文件及跨文件项目入口均退出 0，stdout 逐字节符合
预期，stderr 为空。真臂读子别名，随后合流再次读取；假臂分别跳过
臂内调用或输出 else 标记。两条路径都在合流读取后按
`second`、`first` 各析构一次。动态负号仍在 native 准入报
UnsupportedNode，直接整数字面量 `-1` 的对照成功。

同一最终 macOS 产物在父在外块、子在内块时复现下一项边界。
两公开入口 check 均通过，原生生成均报 UnsupportedNode。
删除内块 if 后，两入口均退出 0，输出与资源顺序符合预期。
后续核对外层父 loan 在内层分支及内层作用域退出的运输。

最终模块布局在 macOS arm64、离线 Linux x86_64 默认线程和默认栈各
通过相同 25 项，且同一源码清单的单臂、双臂各经两个公开入口
原生运行，两宿主共 8 个进程。输出逐字节一致，退出 0，stderr
为空。最终 macOS 重验动态与字面量负号及嵌套作用域对照。
codegen 全 target 严格 Clippy、格式、尺寸和文档门禁通过。
完整 Range 模块及新 PR-event CI 待后续批次。


## 341. R4 内块 metadata 别名与外层 RangeView 父描述符

§340 的单文件和跨文件项目复现加强为合流后读子、离开内块后再读父。
两个入口通过 check，改动前均在原生准入报 UnsupportedNode。
正式红测在内块语句 if 的完整 span 复现。

准入沿 AST 普通块的直接父链查找已解析的父绑定，遇到控制体
即停止。子别名和真实父各自核对唯一的词法结束点、同根、来源 loan
及先声明后借用。两份绑定合同使用同一子块的普通 if 证据。
不为内块创建新的 transport 或改写 cleanup。现有生成器消费前端
的结束事实，SSA 验证器仍检查实际依赖。算术、短路、Abort 和
不相关控制不分享本次准入。

独立复审核对了 AST 父链、事实身份与其他准入边界。复审指出
原生析构输出不足以定位子 loan 的精确结束点。补加 typed SSA
级回归，检查子借用在合流后最后一次内块读取与外块父读取之间
结束，父描述符结束排在外块读取之后。

当前编译器源码清单 SHA-256 为
`f253426b3704b0df5e9329cd64f70964b9870350d6d17dbe3de3048cb630389c`。
macOS 的 SSA 定向断言通过，普通内块和两层普通块的公开单文件及跨
文件项目原生运行与预期一致。将其中一层换成外部 if 的控制体后，
macOS 单文件仍在原生准入报 UnsupportedNode。该边界暂仍开放。

同一清单的 macOS arm64 完整 Range lowering 模块 333 项通过。
离线 Linux x86_64 的 27 项定向测试通过，含同块、嵌套块、SSA 结束
顺序及保留的准入负例。两宿主以默认线程和栈执行。两种词法深度与
分支形态经两个公开入口各运行，两宿主共 8 个进程，全部退出 0，
stdout 逐字节一致，stderr 为空。macOS codegen 全 target 严格
Clippy、格式、尺寸及文档门禁通过。Linux 全模块和新 PR-event CI
未在这轮执行。


## 342. R4 外层 if 控制臂中的 RangeView metadata 别名

§341 的单文件探针扩展为控制体直接绑定和控制体内有一层普通块
两种形态。各自的公开单文件与跨文件项目均通过前端 check，
旧候选四个入口的原生准入均报 UnsupportedNode。正式红测在
内层 if 的完整 span 重现。

准入从实际 AST 父关系定位直接的外层语句式 if 及匹配的控制臂，
核对父的唯一 scope end、真实 metadata 别名和子的唯一 scope end。
经证明的臂按实际语句逐条核验，只接受已核实的直接别名、普通内层 if
和只读 Unit 调用。外层条件和其他语句仍用普通 if 检查。
现有 lowering 与 SSA verifier 继续消费实际路径上的结束事实。
不相关的控制嵌套、Arm 退出、动态负号和其他独立准入保持原边界。

独立复审核对了实际臂身份、来源与作用域、受检查的表达式和
正常结束路径。原生回归用动态条件执行内层真、内层假及外层跳过，
覆盖 MemoryDocument、Unit 以及重排输入的 Unit。
候选编译器源码清单 SHA-256 为
`03b572d704655b75618c672e230c41746a9957c52064e351de6720b1cf7fdd15`。

macOS 定向 28 项与两个公开入口通过。外层假臂主动读取父并
在外层合流后再读，真臂在内层分支和合流后读子，外层再读父，
所有资源按 second、first 各析构一次。单文件另验证别名居于 else
臂同样成功。另一臂含显式 Abort 的对照仍报 UnsupportedNode。
外层控制臂到别名间若插入两层普通块，macOS 单文件和跨文件
项目均通过前端 check，但原生准入仍报 UnsupportedNode。
前述覆盖只代表当前控制臂证明。

同一清单在 macOS arm64 完整 Range lowering 模块通过 334 项，
在离线 Linux x86_64 通过 28 项定向测试。两宿主默认线程与栈，
两种源码形态各走单文件和项目入口，共 8 次原生运行，均退出 0，
stdout 与预期逐字节一致且 stderr 为空。macOS codegen 全 target
严格 Clippy、格式、尺寸及文档门禁通过。本轮未运行 Linux
完整 Range 模块和新 PR-event CI。


## 343. R4 外层 if 臂的多层普通块

§342 的尾项扩展为两层及四层普通块。两种源码在旧候选各经
单文件、跨文件项目通过前端 check，原生准入均报 UnsupportedNode。
正式红测在两层版本的内层 if span 重现。

沿 AST 中实际的普通块链上行，从别名块到外层 if 臂下的最外块
逐层核验。父块只跳过已检查的直接子块，其余语句继续受只读合同
约束。来源事实仍核对真实父描述符、先声明、同根以及各自唯一结束点。
外层条件和其他臂、未获准的控制与退出规则仍走现有门禁。
独立复核了实际 parent edge、结束事实以及逐层检查后才放行外层臂。
回归覆盖零、一、二、四层及动态真假路径，另保留中间未经证明声明
的拒绝。候选编译器源码清单 SHA-256 为
`537df39dd71c4d2027e94466e0876dc6166277c8ccdb0530458b5771c0ab82ff`。

macOS 的 29 项定向测试通过。公开单文件和跨文件项目在两层、
四层均执行内层真/假和外层跳过，包含中间层读父、合流读父以及
second、first 各一次析构。对同一公开单文件只插入中间声明，
前端仍通过，原生生成报 UnsupportedNode。再套一层外部 if 的
独立对照在 macOS 单文件和跨文件项目仍通过 check、原生准入失败；
额外控制臂的事实组合仍待验证。

同一清单的 macOS 完整 Range lowering 模块 335 项通过。Linux
默认并行在 165 项后无进展，最终退出 124。原二进制默认栈串行
在 700 秒前完成 332 项，也退出 124。保存该二进制，用测试清单
定位未完成的三个临时资源、循环清理与 verifier 用例后串行补跑，
三项均通过。两批无重复，合计逐个通过 Linux 清单的 335 个测试，
并与 macOS 测试身份一致。Linux 的单次默认并行验收仍未完成。

双宿主定向各 29 项通过。两种深度经两个公开入口共 8 个原生进程
均退出 0，stdout 逐字节一致，stderr 为空。Clippy、格式、尺寸与
文档门禁通过。该候选仍无新 PR-event CI。


## 344. R4 外层普通块父描述符跨多层 if 臂

§343 的多层 if 尾项扩展为直接控制体和插入两层普通块的形态。
四个旧候选公开入口均通过前端 check，原生准入报 UnsupportedNode。
正式红测在内层 if 完整 span 重现。

父查找遍历实际 AST 的候选包围作用域，遇控制体时只通过其实际
所属的直接语句式 if。准入从最内侧逐层验证已核验直接子块和 if，
每层其余语句、条件及另一臂继续满足已限定的只读控制合同。
局部控制集合仅在整条路径通过后发布。来源、唯一结束点及后端清理
继续用原有事实，loop、动态负号和提前退出保持原门禁。

独立复审核对了实际 AST 关系、父子来源、结束事实及失败行为。
原生回归覆盖内层真/假和两级各自跳过，父视图在跳过臂及最终
合流后读取。父描述符仍声明于外层普通 Block。
候选编译器源码清单 SHA-256 为
`a1b9b33a8c787955b89c16f296500e6cfeb2d9797950e43a36d51340e663f35f`。

macOS 定向 30 项通过。直接与普通块间隔的公开单文件、跨文件
项目输出与预期一致。对同一测试形态，在中间 if 的另一臂放入 Abort，
或在两层 if 间加未经证明的声明，均通过前端 check、原生准入拒绝。
将父描述符移入外侧控制臂后，macOS 两公开入口仍通过前端检查，
原生准入报 UnsupportedNode。该父作用域另待处理。

同一编译器源码清单在 macOS arm64 和离线 Linux x86_64 各通过
30 项定向测试。两形态、两个公开入口在两宿主共 8 个原生进程，
均退出 0，stdout 逐字节一致，stderr 为空。macOS codegen 全
target 严格 Clippy、格式、尺寸及文档门禁通过。上一候选的完整
Range 结果见 §343，本候选未重跑全模块与新 PR-event CI。


## 345. R4 外层 if 臂内建立父 RangeView

§344 的尾项扩展为直接控制体和臂内普通块两个形态。四个旧候选
公开入口均通过前端 check，原生准入为 UnsupportedNode。正式红测
在内层 if 完整 span 重现。

候选开始核对 ControlBody 内真实父符号与唯一词法结束点。仅当
新描述符是所验证叶作用域中 metadata 别名的真实父时，才在该臂
按有界合同准入其声明。初始化必须是确切的 typed Carrier 调用，
有相符调用、实参、实际存储根的来源 loan。调用的参数与 receiver
使用现有只读模式，操作数继续验证。外部条件和另一臂保留原门禁，
描述符和别名现有的生成、释放和 SSA 验证路径保持一致。

独立复审未发现生产逻辑阻断项，指出运行输出本身无法证明
RangeEnd 所在边。新增 SSA 回归检查子借用结束在父读取之前，
父 RangeEnd 在外层真正合流之前，跳过臂直接进入合流。
原生回归使用会输出标记的计数调用，核对只在进入外层臂时
建立父描述符及四条路径的读取和根析构顺序。
首个候选在 SSA 回归的父位于臂体、子位于同臂普通子块形态仍误拒，
两宿主均复现。父查找现包含实际所属的 ControlBody。逐句核验
覆盖该臂，且只跳过前一步已验证的直接子块。独立复审补核该增量。
最终候选编译器源码清单 SHA-256 为
`1e20913abd3d9bf272d4f3a10eee4bdf66914cf72c2b054f995fd32090f83de6`。

该清单在 macOS arm64 默认栈通过完整 Range lowering 模块 338 项。
两个源码形态的单文件及跨文件项目原生 stdout 与预期逐字节一致，
stderr 为空。含 `own` 或 `inout` 操作数的父描述符初始化在独立
单文件对照仍通过前端 check、原生准入报 UnsupportedNode。
macOS codegen 全 target 严格 Clippy、格式、尺寸和文档门禁通过。

离线 Linux x86_64 在同一清单上通过默认栈 32 项定向回归。
两种源码形态的两个公开入口共 8 个原生进程退出 0。两宿主
输出逐字节相同，stderr 为空。该清单未重跑 Linux 完整 Range
模块，未触发新 PR-event CI。

仅把外侧条件改为 `outer && middle`，两个公开入口均通过前端
check、原生准入报 UnsupportedNode。该条件的 CFG 是下一验证边界。

## 346. R4 包住 RangeView 别名作用域的短路条件

§345 冻结候选的正式红测明确拒绝 `left && rhs(right)` 的逻辑根，
报 UnsupportedNode。该条件属于已获准的外层语句式 if，但原有
借用绑定门禁仍检查其独立 AST 节点。单文件与跨文件项目亦复现。

核验外层语句与其直接子作用域后，再以原有短路操作数合同验证
完整条件。通过时只把该逻辑根加入当前包围链的暂存集合，各层
全数通过后才发布。条件必须是单层 Boolean 短路，沿用已存在的
只读调用准入，禁止 `own`、`inout`、嵌套短路及其他隐式控制流。
已有 SSA 生成仍负责条件分支和描述符清理。

独立复审核对旧门禁等价、包围链发布时点和只读调用权限。
正式原生回归用输出确认 RHS 的跳过与求值，以及父描述符仅在
执行臂创建、资源在合流后析构。短路测试无单独的 SSA 结束点
断言，上一节有裸 Boolean 条件的 BorrowEnd 和 RangeEnd 断言。
候选编译器源码清单 SHA-256 为
`7126fc059b59a0d1f7a7491776760b274bec45cf3c0bb48b92cebc4babe1ec42`。

同一清单在离线 Linux x86_64 通过默认栈 26 项定向回归。
macOS arm64 与 Linux x86_64 的 `&&`、`||` 各使用单文件
和跨文件项目，8 个原生进程退出 0，stdout 与逐字节预期一致，
stderr 为空。两宿主两种入口各用 `inout` 条件和嵌套逻辑条件
对照，均通过前端 check，原生准入保留 UnsupportedNode。

拆分前 macOS 通过 `short_circuit` 的 99 项过滤测试，并通过
严格 Clippy。尺寸门禁拒绝 1018 行的原文件。随后移出既有短路
准入模块，未变更逻辑内容，以去除格式空白的模块文本比对核验。
拆分后的编译器源码清单 SHA-256 为
`ea8411906c23a68c94b00021b72d5fe811f80cd12431c36ccd83631cf6cdc78a`。

拆分前编译器另通过 macOS 公开 conformance 门禁及证据复验。
能力为 137/154，综合程序 0/6，23 项已知未实现，零回归、
内部错误和入口差异。该 conformance 结果归拆分前二进制。

最终拆分后，macOS 通过 19 项 Range carrier、7 项短路边界、
14 项依赖防回流测试及严格 Clippy。离线 Linux 通过同范围内
26 项 Range carrier 和短路边界测试。再跑 `&&`、`||` 两宿主
两入口的 8 个原生进程，输出逐字节一致。两组负向输入在两个
宿主的两入口保留上述拒绝。格式、尺寸、文档检查通过。
未对该候选运行新 PR-event CI。

## 347. R0/R4 合入 PR #444 后的基线对账

本地复审分支由 `625f6804` 快进到 `6cc03907`，恢复了原有
未提交改动和新增文件。两处相交的 architecture 文档自动合并，
其余本地文件与合入前备份逐字节一致。§345–346 的原测试结果
仍归各自的编译器清单。当前待验收的源码是 `6cc03907` 加本地
差异，编译器源码清单 SHA-256 为
`87a7c56c5100910009eba5627b2973c082d55d116545d56be65e46099e206a1a`。

重新用 §338 口径数纯基线中的文件数／匹配行数，排除同一行
的函数定义。如下变化为 PR #444 合入后这两个基线的差异。

| 范围 | resolve_names | check_types | check_ownership | complete_file_types | lower_scalar_file_with_entry |
|---|---:|---:|---:|---:|---:|
| frontend tests | 81/153 → 73/133 | 70/107 → 62/88 | 48/73 → 45/70 | 22/30 → 21/25 | 0/0 |
| codegen src | 15/18 → 13/15 | 15/18 → 13/15 | 14/17 → 12/14 | 7/7 → 5/5 | 4/8 → 2/5 |

frontend src 计数沿用 §338，CLI 与 LSP src 对这五项仍为零。
剩余两个旧 Single lowering 直接消费文件为
`lowering_entry_contract_tests.rs`（入口合同）和
`single_readonly_method_tests.rs`（旧只读方法对照）。
仓外 API 和其余 legacy 见证分类仍待验收。

合入后的同一编译器源码清单在 macOS arm64 和离线 Linux x86_64
逐项通过相同的 50 个定向测试，覆盖 carrier、短路边界、可空
写回和旧入口防回流。短路两形态和局部 `String?` 写回通过
两宿主的单文件及跨文件项目入口。12 个原生进程退出 0，stdout
逐字节相同，stderr 为空。短路嵌套和 `inout` 条件的负向入口
仍通过前端 check 并在生成阶段拒绝。macOS 的 `check_cli`
另通过 7 项，格式、文档、尺寸及 diff 检查均通过。
此清单未运行完整 conformance 或新 PR-event CI。

## 348. R0 保留见证与 R5 合入 #444 后的候选复验

沿用 §347 的 `6cc03907` 加本地差异，复验前校对编译器源码
清单。两个仍直接调用旧 Single lowering 的文件已逐项核对。
`lowering_entry_contract_tests.rs` 保留 L0170 及诊断、
分析身份、来源身份的原始拒绝顺序。其 Unit 入口诊断有独立
见证。 `single_readonly_method_tests.rs` 保留旧只读接收者与
结果组合的拒绝边界，同文件的正常入口对照继续走 Unit。
macOS 的这两个模块共 70 项通过。保留范围均属于测试见证。

macOS 复跑完整公开 conformance 门禁及证据复验。能力探针
137/154，综合程序 0/6，23 项已知未实现，零回归、内部错误、
错误输出或入口差异。标准库 Word Frequency 的 40 条公开命令
通过。LSP 的 56 项单测和 1 项真实 stdio 格式化测试通过。

离线 Linux x86_64 在相同编译器源码上完成 154 个探针及 6 个
综合程序，证据复验通过。官方比较器核对不同宿主的输入清单、
两入口状态和成功输出的 SHA-256，完整 SHA 字段
`6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b` 双宿主对账通过。
第一次比较因 macOS 默认写短 SHA、Linux 写完整 SHA 拒绝。
确认二者解析到同一基线后，用完整 SHA 在 macOS 重跑原矩阵，
保留第一次证据。两宿主的 Word Frequency 公开命令各通过 40 条。

综合程序两入口保留的首个失败如下，路线图依赖沿用
`conformance/expectations.json` 的已审条目。后端两项的公开
诊断未定位具体节点，仍需单独缩减。

| 综合程序 | 首个失败 | 已登记依赖 |
|---|---|---|
| `config_parse` | L0080 未解析名称 | N02、N08、L1、L2 |
| `expr_eval` | L0165 错误传播类型未完成 | N02、N05、N07、N08、L7 |
| `linked_stack` | L0085 非空断言操作数不可空 | N01、N03、N07、L7 |
| `raii_state_machine` | 后端 UnsupportedNode | N01、N02 |
| `varint_codec` | 后端 UnsupportedNode | N01、N08、L3b |
| `word_count` | L0165 调用类型未解析 | N02、N03 |

Linux x86_64 的原生 ASan/LSan 门禁退出 0。实际 Koven LLVM
通过属性开关核验、ASan 和 LSan 的 12 个动态开关对照，以及
Map 和 12 个 provider 生命周期形态的 ASan/LSan 运行。

macOS 生成资源门禁按原门限独立复跑退出 0。16 个计数用例、
故障校准、检查器变异和故障缩减按该平台的 partial 验收通过，
没有把 macOS 结果计为 Linux sanitizer 覆盖。此前三个运行的
5 秒超时分别发生在两个新计数产物和一次 clean 缩减重放，
其中第二轮正常用例已经全部通过。三件原超时产物随后各用
原进程执行器和原门限独立重放三次，九次均通过。保留这些
间歇性超时记录，未更改测试门限或确定其根因。

Linux 生成资源套件起初两次因容器 Git 元数据不可用、临时
worktree 目录只读退出。修正隔离工作区后，全套通过 16 个资源
用例、故障校准和检查器红绿对照，仍在故障缩减上用尽原
120 秒预算。随后用同一导出器单独复跑，宿主挂载产物目录与
容器内部产物目录均在原门限返回 `minimization_incomplete`，
完成的候选次数分别为 6 和 3，均未进入最终三次确认。
Linux 完整生成资源门禁仍未通过，保留原门限和失败证据。
这组结果来自离线容器，当前源码尚无新 PR-event CI。

## 349. R0 frontend src 旧分析入口的直接调用归属

在 §347 的源码上排除同一行的函数定义，逐一核对旧
`check_types`、`check_ownership` 的直接调用及实际模块入口。
两列均以文件数／匹配行数计，类型和所有权入口分别覆盖
§347 的 40/50 与 22/23。

| 调用归属 | check_types | check_ownership |
|---|---:|---:|
| 迭代事实测试、重放和私有迭代产物校验 | 28/38 | 12/12 |
| no-carry 循环、两源形状及 Unit 事件对照 | 3/3 | 3/3 |
| Unit 两源候选带相应 feature 的测试 | 1/1 | 1/1 |
| 研究借用消费、frame、循环和零趟辅助测试 | 5/5 | 5/6 |
| 形成产物、嵌套 origin 和借用结果来源测试 | 3/3 | 1/1 |

迭代测试和 `instance_replay` 从 `cfg(test)` 模块进入。
迭代产物校验由 `checked_file.rs` 通过测试专用的
`#[path]` 挂载。研究模块有实验 feature 入口，统计到的
直接调用均在各自 `cfg(test)` 子模块。Unit 两源测试另受
`loan-observation-test-support` 约束。该静态分类说明为何
这些直接调用计入 frontend src 但属于测试。集成测试各自的
迁移或保留理由、间接和仓外调用仍单独追踪。

同口径复查其余两个 frontend 入口。三个
`complete_file_types` 直接调用分别来自形成产物和私有迭代
产物验证测试。 `resolve_names` 有一个生产调用，
`CompilationUnitNames` 的构建器逐文件调用它以取得局部名称
事实，再由 Unit 模型组合 package/import。该共享底层求解
仍有生产职责，清点旧类型和所有权入口时须保留。
其余直接调用来自测试。

macOS 的 Unit 单源恢复模块通过 3 项测试，其中 import-free
原始输入对照直接验证逐文件名称事实、Unit 来源和入口身份，
另两项核对首个真实诊断阶段与缺 return 时的范围。

## 350. R4 RAII 综合程序的两个独立后端边界

沿用 §347 的 `6cc03907` 加本地差异和 §348 对过 SHA-256 的
两个编译器二进制。原程序通过前端，原生构建报 UnsupportedNode。
取出原 `Handle` 与一个只构造它的入口，得到第一组最小输入：

```ko
class Handle(val id: Int) {
    deinit() { println("handle #${this.id} released") }
}
fun main(): Unit { val item = Handle(1) }
```

把字段先读到局部，再插值，结果相同。去掉析构，把字段插值
放到普通入口同样失败。改用纯文本文字的析构对照实际输出
`handle released` 一次。四个输入在 macOS arm64、离线 Linux
x86_64 的单文件和跨文件项目入口都通过 check。三个插值形态
的 12 次原生构建报 UnsupportedNode，纯文本的四次原生运行
输出精确匹配。这与 roadmap L2、现有 Unit 入口插值拒绝见证
以及 refine 字符串文字的 `decode_plain` 门禁一致，归已知
语言增量欠账。本轮未改现有 oracle 或启用 L2。

将综合程序全部插值改成纯文本后，macOS 单文件仍失败。
此时只构造 Handle、只遍历 Command 列表及只调用普通文本的
Command.label 均可运行。只构造 Session 仍失败，但把 Session
析构改为纯文本后可运行。移除其余字段和尾部拼接后，仍有第二组：

```ko
class Session(val name: String) {
    deinit() { println("session " + this.name) }
}
fun main(): Unit { val box = Session("a") }
```

在普通函数用 `"session " + box.name` 也失败。直接打印字段、
先显式 clone 字段再拼接、具名 String 局部拼接三个对照可运行。
五种形态的双宿主、双入口共 20 次前端检查均通过。两种字段
拼接的 8 次原生构建报 UnsupportedNode，三个对照共 12 次
退出 0，两宿主 stdout 逐字节相同，stderr 为空。
静态检查发现 refine 的 String 二元操作数准入能读取具名借用
和判空 proof，普通投影准入要求字段可复制。已有打印和 clone
各有字段借用路径。将它们作为后续修复的参考，核对两操作数的
求值、字段与父对象借用结束、提前退出边和拼接产物的清理；
仅放宽准入无法证明构建与生存期正确。此组合属于已标为交付
的 B07 拼接合同，单独列为 R4 未关闭项。

补核 §348 的 Linux 缩减耗时。容器内部磁盘的回放中，已有
命令计时的导出共约 29.7 秒，三类原生产物的编译和链接共约 88.1 秒，
三类原生进程本身合计约 2 秒。其他回放同样主要耗在重复导出
和链接。这解释原 120 秒预算被消耗在哪里，尚未定位这些命令
各自偏慢的根因。资源门禁结论和预算维持 §348。

## 351. R3/R4 稳定 String 字段的共享二元读取

§350 的 Field String 拼接用例在普通 Unit 与常量 Unit 共用的
跨文件原生测试中先红测。补测还发现左字段完成读取后，前端
误接受右操作数替换同一字段。原前端负例缺少应有 L0135。

前端为有稳定 place 的 String 字段发布二元操作专用的共享借用
来源，右侧求值期间左侧保留实际字段路径。同字段替换被拒绝，
同一根的相邻字段可正常替换。析构规划把字段父对象纳入二元
操作数最后使用和提前退出的合同。String when 继续使用原路径。

Refine 只在同源二元操作消费匹配的共享 place 事实，复用
String.clone 的字段准入和既有投影借用构造。按左到右顺序
求值，正常操作后逆序结束本次新建的字段借用，随后消费
`AfterBinaryOperands` 清理。提前退出走现有 pending loan
清理，Abort 保持不展开。这一片覆盖具名稳定字段，包括析构
中的 this、普通局部、own 参数读取和有证据的字段链。

独立复审核对了短借用、同字段冲突和前后端合同。按复审发现
补齐 own 参数读取。测试曾把 own 参数读取与其字段替换写在
同一函数，后者单独执行也仍处于现有 UnsupportedNode 边界。
拆开的正例验证具名局部的相邻字段替换与 own 参数字段读取，
未改动原字段替换能力。既有只读方法和 nullable 字段正例纳入
同一 macOS 筛选套件。

候选编译器源码清单 SHA-256 为
`67337df7bcc30871b18119cc20ae5257657f6b8d498cd4e36583635e8139d003`。
macOS 通过 15 项 field replace 前端测试和 8 项 codegen 定向测试，
两项新增原生回归分别覆盖普通和常量 Unit。两个公开入口验证
普通拼接、右侧 continue 与 break 的析构，以及右侧 Abort 的
退出码 134 和无析构输出。同字段冲突的两个公开入口均报 L0135。

macOS 另通过 7 项 String 二元相邻清理测试、19 项 String when
测试及 frontend/codegen 库严格 Clippy。重跑 §350 的纯文本
Session 构造对照，已输出 `session a dropped` 一次。原 RAII
综合程序仍报 UnsupportedNode，把所有插值换成纯文本后调用
`Session.apply` 也仍报同一错误，尚未定位该方法内的具体节点。
本切片不将 RAII 综合程序计为通过。

离线 Linux x86_64 的同一清单也通过 15 项 field replace 前端测试。
两宿主两个公开入口的普通、循环和 Abort 用例共 12 个原生进程，
stdout 逐字节一致。普通和循环退出 0，Abort 退出 134，仅打印
退出前缀并报告原错误。四个同字段冲突入口都报同一个 L0135。
相邻字段原生写回、字段相等、own 参数拼接和分组的四个额外
单文件探针在两宿主输出逐字节一致。文档、格式、尺寸及 diff
检查通过。本轮未启动新 PR CI 或复跑全套 conformance。

## 352. R4 RAII 接收者字段交换的缩减

沿用 §351 的源码清单。纯文本版 `Session.apply` 的单操作探针
中，空方法、打印、State 字段读取及判断、命令分支和各项普通
字段写回均能构建。保留 `replace(&this.handle, null)` 即可
重现 UnsupportedNode。进一步移除不相干的类型与语句后，
可空 Handle 的清空、从 null 换入 Handle、非空 Handle 交换、
Int 交换均复现。四种输入共 16 次 macOS/Linux 单文件与项目
检查通过，生成阶段均报 UnsupportedNode。局部普通 class
上同类可空交换的四个构建与运行通过，输出按先 `done` 后旧
资源析构的顺序逐字节一致。

把纯文本综合程序唯一的接收者字段 replace 换成普通赋值，
两宿主的单文件和跨文件项目均运行。其关闭分支先析构旧
Handle，再打印关闭标记。这只用来隔离失败，普通赋值
没有交付旧 owner，不能作为 replace 的修复或验收。

Guide 10 明确包含 `replace(&this.state, ...)`。当前 field
replace producer 和损坏事实校验封闭在自有局部对象的一级
字段路径，前端已有 `this` 无可执行计划的对照。
`HeapFieldExchange` 的 owner 是自有 Value 并要求精确字段 loan，
`HeapFieldReplace` 虽接收独占 receiver loan，但会立即析构旧值。
此处需要能保留原接收者独占权限、交付旧字段 owner 和验证前缀
退出的交换合同。本节记录现有边界，未扩展该 SSA 合同。

## 353. R4 varint 编解码的容器字段长度读取

在 §351 候选上，去掉 `varint_codec` 的字符串插值后仍报
UnsupportedNode。ZigZag 运算、纯文本 bytesText、只构造 Reader
可构建。单独调用 `Reader.atEnd()`、编码或解码各自失败。
Reader 的第一个最小失败为返回 `this.bytes.size` 的只读方法，
同一类型的 `reader.bytes.size` 局部对象对照可构建。
新增跨文件原生测试在方法内字段投影处先红测。

前端已为 `.size` 发布实际接收者的共享 loan。细化层现消费
同轮、同表达式的稳定字段 Place 事实，复用现有字段只读准入
和投影 loan。读取 `ContainerLength` 后逆序结束本次建立的
借用，再按既有 `CallReturn` 与接收者表达式清理。普通
`.size` 与 String 二元读取沿用原合同。独立静态复核通过，
指出临时父对象的字段接收者仍走 Temporary 边界。

当前编译器源码清单 SHA-256 为
`6d57a8b879832e310781ea6b7628b6c4845f04c5f3fb163b7c380a7d4bfaef4e`。
macOS 新增原生测试在基础 Unit 和常量 Unit 分别验证重复
读取、析构中的字段读取及清理输出。macOS arm64 与离线 Linux x86_64
各用单文件和项目入口验证五种访问形态，覆盖 Reader 整体方法、
分组、局部对象对照、Inout 接收者的数组及嵌套 List 字段。
20 次原生运行退出 0，双宿主对应 stdout 逐字节相同，stderr 为空。

编码的最小失败另缩减为对 `inout MutableList` 参数调用
`add`，Int 和 UByte 元素均通过 check、原生生成失败。
同一个 Inout 参数无修改的调用与局部 MutableList 上的
`toUByte()` 后 add 均可运行。这条 Inout 修改路径和完整
解码仍待复审，本次未改变 varint 综合程序的 oracle。


## 354. R4 MutableList 的 Inout 形参追加

§353 的 UByte 最小用例扩展至九次追加，验收扩容后的长度与首尾
元素，并用有输出的参数表达式核对求值次数。跨包 MoveOnly 资源
用例同样通过前端、在原候选生成失败。细化层消费原有 append 类型描述
和元素移交事实，SSA 以独立指令表达同一 Inout 地址上的 header 更新。
LLVM 复用普通 append 的容量增长和 relocation，正常返回后由
调用方取回更新后的 owner。局部值追加继续通过既有结果更新 owner。

初次独立复审发现新指令的来源合同过宽。本地独占 loan 不能替代
Inout 形参的写回协议，已沿用形参来源证明，并验证拒绝本地、
共享及尚有活跃派生 loan 的输入。Provider 派生 place 的直接
读写、借用和继续投影共用活跃性检查。Guide 10 的同根
`list.add(list.size)` 通过短期共享重借用，在 append 前结束。
测试从四个元素起步，检查扩容后的第五个元素仍为求值前的长度 4。

扩大容器回归还发现 SPEC-0350 的旧拒绝见证，在 §336 已完成
泛型 `T?` 布局登记后仍要求 MissingFact。原候选的现有测试可
独立复现此断言失败。新见证保留 per-family 路由，在
Array、List、MutableList 的 Int 实例上核对 canonical 类型、
空容器长度、null 与 present，并保持 arena 不变。实例预算
失败时的输出文件原子性仍由同模块既有负例覆盖。

macOS arm64 的 provider 修订候选（编译器源码清单 SHA-256
`81b41e83f46816ca5b9be3d1ab227edf1ff0c8cdc3698dabbb2842d49be39368`）
通过 12 项容器 SSA 测试、两个新增原生 append 用例及三容器
泛型可空见证。同一源码清单的 macOS arm64 与离线 Linux x86_64
各运行五组单文件与跨文件项目，共 20 次原生执行退出 0，
10 对 stdout 逐字节一致，stderr 全空。矩阵核对 UByte 扩容与
求值、资源逆序单次析构、原 varint 编码、同根长度及元素求值
中的提前 return。macOS 另核对 0、-1、64、Int 最小值和最大值
的实际编码字节，并通过 codegen 测试目标的严格 Clippy。
完整 varint 解码仍未通过。
同一清单在两宿主两入口均可单独读取 Reader 的 pos 和容器
字段长度。局部对象的 `reader.bytes[0]`、方法内的
`this.bytes[0]` 和 `this.bytes[this.pos]` 共 12 次前端
检查均通过，生成均报 UnsupportedNode。源码索引的首个
准入限制是细化层要求具名容器 owner。这是解码的下一个复审范围。

补齐 SSA 文本见证后，新清单 SHA-256 为
`1c43a628dbdc78bf1e8535db7cf4149986f45d9e3b96876b5788b8b365b12b51`。
macOS 上通过容器 SSA 12 项、append 3 项、泛型可空见证
1 项和 String 字段回归 8 项。该清单的五组双宿主、双入口
复跑中，20 次原生运行退出 0，10 对 stdout 逐字节相等，
stderr 为空。格式、文档、尺寸和 diff 检查通过。§353
原候选的扩大容器回归为 167 通过、1 失败，唯一失败为上述
旧泛型 Int? 拒绝见证。最终清单的 Linux Rust 定向复跑通过
append 3 项、派生 place 生命周期 1 项和泛型可空见证 1 项，
macOS 测试目标严格 Clippy 通过。扩大容器回归及新 PR-event
CI 仍需在合批候选收口。


## 355. R3/R4 稳定容器字段的 Copyable 元素索引

以 §354 的冻结源码为对照，新增跨包原生回归在
`reader.bytes[0].toInt()` 首次报 UnsupportedNode。字段索引
现消费原 typed element place 与字段投影描述。先建立同一稳定
字段的共享投影，后按源码顺序求一次索引，再执行既有边界检查
及 Copyable 元素读取。正常路径逆序结束此次建立的投影 loan。

第二个最小失败为 `reader.bytes[readPos(reader)]`。普通无
`deinit` 的 owner 在索引调用返回时被当成最后一次使用，可在
元素读取之前收到析构事实。R3 现将 backing root 保持到整个
索引求值结束。跳转前缀保留该根，`break` 和 `continue`
自身仍按真实目标活性决定清理。直接具名容器
`bytes[advance(&bytes)]` 的原 InvalidModel 随之修复。

提前退出由索引表达式的词法作用域和循环边界决定。真正返回时
先完成 payload，再结束该边上的外层索引 loan。若 payload
先通过内部循环的 `break` 或 `continue` 跳走，保留该循环
之外的索引。独立复审定位过 `when` selector 与共享臂的前缀
状态，以及循环构建顺序的同类问题。最终实现独立记录索引作用域，
避免从其它 CFG 边的 lowering 状态推断当前活跃索引。

在 `inout` receiver 的 `this.bytes[this.pos]` 中，消费
`this.pos` 时使用短期共享重借用。元素 place 的复制读取
须绑定活跃 Shared provider。SSA 只忽略该 provider 具有
唯一来源链的独占祖先，其余冲突及 provider 到期仍照常检查。
独立最终静态复审核对了这些边界，未发现新的阻断。

冻结编译器源码清单 SHA-256
`b727b7212e0428598b8c7a2523c9d26fb975ad7634acd6d4f3ec148463b48360`。
macOS arm64 与离线 Linux x86_64 各通过 13 组单文件及
跨文件项目运行，覆盖原三项红例、索引求值一次、提前返回、
`when` 与 `while`、返回值内走 break、循环跳转及 Inout
receiver 的重复读取。两宿主另以两种入口验证 2 组可观察 varint
解码，包含整数极值、截断和超长。共 60 次正常运行均退出 0，
30 对 stdout 逐字节一致，stderr 全空。字段数组索引为负数及
等于长度各在双宿主、双入口 SIGABRT，共 8 次，均未打印读取
成功标记。macOS 另核验 break 前最后一次读兄弟字段。

双宿主均通过新增 native 回归、11 项 Inout 字段及 45 项前端
容器所有权测试。Linux 扩大 codegen 容器回归 173 项全过；
macOS 另通过 22 项跨文件容器相关所有权测试及严格 Clippy。
格式、文档、尺寸和 diff 检查通过。临时父对象、Rc payload
路径与 index 求值中写回同一个字段接收者仍待分别审查。

原 varint oracle 保持不变，综合程序仍报 UnsupportedNode。
纯文本对照的字节列表、编码循环、往返循环和报告头分别可
运行。剩余失败可以独立复现为 `Result.Err` 分支的
`println("error: " + item.error)`，省略循环也失败。直接
`println(item.error)` 的对照运行并返回正确错误文本；把
字段赋给 owned 局部的对照因 L0132 拒绝，不能替代借用式拼接。
本批尚无新 PR-event CI。


## 356. R4 窄化枚举载荷的字符串二元读取

§355 的公开红例在 Result.Err 臂用 `"error: " + item.error`，
直接 `println(item.error)` 则运行。新的跨包回归用普通
`Outcome` 独立见证，覆盖分组接收者、左右操作数、相等、不等、
正常返回与右操作数提前 return。准入现复用 Borrow 实参的
boxed enum 载荷检查，从原 typed receiver 核对活动 case 与
该 case 的实际字段符号，再用同批二元表达式的 Shared Place
事实取得载荷 loan。仅查找名字绑定时解开 Group，清理仍消费
原接收者与原 operand 身份。

lowering 复用既有 TaggedPayloadPlace、FieldPlace 和共享借用。
完成 String 操作后逆序结束本次创建的借用，再消费
AfterBinaryOperands。提前退出复用原 pending 借用清理。
独立复审覆盖了 case/field 匹配、分组、正常和提前退出、
与原 Borrow 调用的共同准入，未发现阻断。无用例将载荷移出
enum，且同根独占写回仍由 R3 报 L0135。

冻结编译器源码清单 SHA-256
`9ab7c82d498a837bedc0a9aa368aebae4df563d014899c792f78b6b42d8ef54a`。
macOS arm64 与离线 Linux x86_64 均通过普通和常量 Unit 的
定向回归、String 字段 8 项及 SSA 枚举 15 项。公开单文件和
跨文件项目各通过 7 组原生用例，28 次进程退出 0，14 对 stdout
逐字节一致且 stderr 为空。矩阵包含不同 case 同名但不同位置的
字段，以及带 deinit 的资源载荷在正常与提前返回时各析构一次。
纯文本版 varint 的完整编码和解码流程均能运行，原综合程序与
原 oracle 保持不变。macOS 严格 Clippy 及格式、文档、尺寸、
diff 检查通过。

### 字符串插值的待复审合同

原版 varint 在前端检查后仍报 UnsupportedNode。分别用单个
String 和 Int 插值重现，不依赖编解码、容器或 Result。Phase 4
准入与 lowering 只接入纯文本。Phase 2 现遍历 interpolation
表达式，但未完成 Guide 04 的 Display 校验与分段控制流合同。
`${println("side effect")}` 被接受，尽管 Unit 不满足 Display。
`fun result(): Int { println("${return 7}") }` 报 L0088，
因为外层字符串被记为可继续。双宿主的单文件与项目入口均复现。
Phase 3 已发布已完成的 MoveOnly 插值输入清理；后端在接入
之前须有来源明确的格式化合同，并覆核分段求值与退出清理。
本批尚无新 PR-event CI。

## 357. R2 字符串插值的顺序完成性

沿用 §356 的复现。带提前 return 的插值原先把整个字符串记为
可继续，导致外层函数误报 L0088。Unit 复用顺序操作数检查器，
按源码顺序发布 String 的普通完成性与暂定流可达性。即便前缀
离开当前求值，后缀仍用于类型诊断，但不能为循环补出 break
或为暂定流引入写入。循环体内与 while 条件中的普通调用同样
传播实参的暂定终止。条件检查单独保存和恢复这项跟踪状态，
原有循环跳转归属保持不变。

修复前分别实证了后缀插值、调用后续实参和未分派的 inout 写回
会使循环后的非空证明失效。流事实收集按已检查的完成性截断，
并在调用实参全部完成后才登记 inout 效果。循环头尚无 type facts
时，按同一 AST 节点下标记录语法上可证明的跳转，只沿分组、嵌套
字符串与直接具名调用的实参检查。保留跳转表达式自身的求值，
包括 return 值中的可达赋值与 break。安全成员调用和其他无法
证明的复合表达式继续保守处理。用公开入口确认，当前首段为
`${if (flag) { break } else { break }}` 时仍可能把后段赋值算入
循环写入并在后继误报 L0084。只在一臂 break 的对照应保留
L0084。此复合语法边界单独追踪。循环回归使用内存与磁盘 Unit
模式，核对跳过写入保留证明、实际写入清除证明及诊断位置。

此项补齐 §356 的 R2 控制流部分。Guide 04 的 Display 准入依赖
路线图 L1 的预声明协议及实现身份。L2 仍缺借用调用、格式化和
结果清理的后端接线，原版 varint 的综合程序及原 oracle 保留。

冻结编译器源码清单 SHA-256
`8a3ffe038e49e7620b5694ac09591ec2d0c865e37f7bef35d135a48874e231f8`。
独立复核检查顺序写入、return payload 和 while 条件跟踪的
事务边界，并实跑内层条件 break/continue 归属及顶层 L0142。
macOS arm64 与离线 Linux x86_64 分别通过跨文件类型 297 项、
循环流事实 17 项、常量所有权 17 项和 Unit 直接调用借用 358 项。
两宿主各 689 项，无失败、忽略或过滤。macOS 另通过旧入口
type_checking 的 103 项、check_cli 的 7 项以及前端严格 Clippy。格式、文档、Rust 尺寸与 diff 门禁通过。
两宿主各运行 11 组单文件和跨文件 check，对照结果一致。
另外确认 String、Int 的合法插值暂时停在 UnsupportedNode，
Unit 插值仍在 check 被错误接受。三种形态各经两入口实跑，
按 R2/R4 缺口记录。首次脚本漏传项目运行的 entry 参数，修正后
用该冻结清单重新执行。此切片尚无新 PR-event CI。

## 358. R2 循环预扫描的双臂终止与嵌套循环头

§357 的两臂 break 复现在内存 Unit 回归中先失败。若另一臂
可继续，后段赋值仍可能执行，后继 Int 读取必须报 L0084。
修复将既有的语法跳转证明移入私有模块。每次扫描按 AST 类别和
下标缓存，以后序工作栈合成求值序列或双分支的证明。if 在条件
必然跳转时保留条件自身，跳过两臂。否则必须有显式 else 且两臂
都终止，才能跳过外围求值的后缀。顺序控制块保留第一条终止
语句及其前缀，赋值、调用和 return 值在跳转前的效果仍保留。

独立复审追加了嵌套 while 条件和 for 来源的最小复现。旧 CLI
在这两种外层跳转之后仍误报 L0084。预扫描现在只沿进入嵌套
循环体之前求值的头部建立终止证明，保留头部本身的求值。
循环体中的 break 或 continue 不用于证明外层终止。没有 else
的 if 和内层循环自行 break 的可达对照仍清除被写入的事实。
最终独立只读复核核对了上述跳转归属、仍可执行的写入、后序
缓存和冻结文件的身份。

另有一个仍开放的 R2 复现。内层 `while (cond(x, break))`
在检查自身条件之前仍从不可达的循环体 `{ x = null }` 收集
写入，导致条件中先求值的 `x` 误报 L0084。此处与本节修复的
外围扫描目标不同，已用公开 CLI 独立复现。

冻结编译器源码清单 SHA-256
`d20899ec56aed14c9e12da954b86ade08645804bc67650a108bf268c28e5acad`。
macOS arm64 与离线 Linux x86_64 各通过 793 项定向测试，包含
18 项循环流事实、297 项跨文件类型、17 项常量所有权、358 项
Unit 直接调用借用和 103 项旧类型入口。两宿主各运行 19 组公开
单文件及跨文件项目检查，含本节新增 8 组，结果逐项一致。
两宿主也都在上述自身条件缺口的两种入口得到 L0084，正常
进入循环体的写入对照同样报 L0084。§357 的插值格式化与
Display 缺口仍可复现。macOS 的严格 Clippy、格式、文档、
Rust 尺寸和 diff 门禁通过。远端 main 仍为 `6cc03907`，
此批尚未提交及触发新的 PR-event CI。

## 359. R2 while 自身条件的求值前缀

§358 的公开复现转为 Unit 红测。由 `x != null` 证明的 x
在 `while (cond(x, break)) { x = null }` 首个实参位置误报
L0084。Single 和 Unit 的旧预扫描都先把 body 与 condition
的写入并入循环头，未核对首个 condition 是否能正常完成。

两个检查器共用同一个循环预扫描入口。若既有语法证明确认
首个 condition 必跳转，没有回边能将 body 或 condition 本身的
写入带回首个读取。此时跳过循环头预失效，普通类型检查仍沿
求值顺序执行写入失效与跳转记录。作为外层循环的一条 statement
扫描时，嵌套 while 条件内可达的写入仍进入外层回边分析。
有正常后继的条件继续作原有保守扫描。回归独立检查首个读取、
同一条件内真实写入后的第二次读取、循环体可写入回边三项合同。
独立复核核对了新旧入口、外层循环和真实条件前缀的处理。

冻结编译器源码清单 SHA-256
`cdd0673bc5403f21fb3a08795c95165b07444df2496244130074d54597343872`。
macOS arm64 和离线 Linux x86_64 各通过 794 项定向测试，
含 19 项循环、297 项跨文件类型、17 项常量所有权、358 项
Unit 直接调用借用和 103 项类型入口测试。23 组公开 check 的
单文件和跨文件项目结果双宿主逐项一致。macOS 的旧
`check_types` 独立探针也确认死循环体读取得到接受、可达
循环体的回边仍报 L0084。两宿主以两种入口实际运行提前
return，验证非空入参返回 7、空入参返回 0。严格 Clippy、
格式、文档、Rust 尺寸和 diff 门禁通过。当前尚无新 PR CI。

## 360. R4 接收者字段交换的当前候选复现

在 §359 的冻结候选复跑 §352。以方法内
`replace(&this.value, 2)` 和 `replace(&this.value, null)`
分别检查 Int 和可空自有资源字段。两份输入的 macOS/Linux
单文件及跨文件项目 check 均接受，8 次原生生成均报
UnsupportedNode。自有局部对象的相同交换在两宿主双入口
各成功。整数对照确认旧值为 1、新值为 2。资源对照依序输出
`old`、`cleared`、`drop old`，旧资源恰好析构一次。

所有权阶段的一级字段计划仍要求普通类的自有局部对象。
现有 SSA HeapFieldExchange 消费精确字段 loan 并交付旧值，
底座要求自有 owner。接收者已有的 HeapFieldReplace 走独占
loan，但会直接析构旧值。继续保留 Guide 10 的接收者交换
合同及 §352 的综合程序，后续需同时验证字段独占权限、原有
替换值求值的提前退出、交换后的旧值交付和析构次数。

## 361. R3 接收者字段交换的独占凭证

§360 的两个用例本来已经各发布一个绑定原调用和字段实参的
独占 loan，但标准 Unit 所有权没有字段交换计划。新增回归先
验证这个缺口，确认合法 Int 和可空资源的 loan 存在而 commit
计划数为零。

普通非泛型 class 的显式 `inout this.field` 现在发布计划。
producer 核对当前接收者模式、声明、确切可变字段、原投影和新
值交付。沿用同一字段符号作为 place 根。Unit gate 按调用取冻结
凭证，复核 typed 投影、声明与字段所属，核验原 loan 和 Value
交付。接收者声明只在首个此类计划出现时建立索引。旧 owned
local 仍以对象为根加一级字段路径。交换计划不进入 root commit。

回归覆盖分组字段、整数 Copy、可空资源 Temporary、替换值提前
return 仍有字段 loan 但无 commit、只读接收者和共享捕获拒绝。
内部损坏测试以相同字段和类型的另一方法替换接收者声明及投影，
验证都被拒绝。独立复核沿原 loan 授权、闭包捕获与验证查询逐项
核对。此切片交付所有权凭证，SSA 与 LLVM 仍待接入。

冻结编译器源码清单 SHA-256
`6bc07cc9eec361173a8e12f37fd750cbd65314ac4ffd00ef0b99654653fb4f13`。
两宿主复跑 §360 的单文件及跨文件项目矩阵，确认合法接收者两例
仍在原生生成报 UnsupportedNode，自有局部对象的写回、返回值
和资源析构输出保持一致。macOS 另用 constant-enabled Unit 的
rooted 与 MemoryDocument 入口取得相同合法凭证。补核条件式
替换值和不同字段的嵌套交换都通过前端，同字段 RHS 读取仍报
L0135。这三组形态留给后续 SSA 接入验证。

macOS arm64 与离线 Linux x86_64 各通过 43 项定向测试，包括
18 项字段交换、4 项接收者回边、17 项常量所有权、4 项内部验证。
内部测试按 field_replace 过滤，其余三套无忽略或过滤。macOS
严格 Clippy、格式、文档、尺寸及 diff 门禁通过。此批未提交，
尚无新 PR CI。

## 362. R4 接收者字段交换的 SSA 和原生生成

§361 的所有权凭证接入 refine 的同一准入和构建查询。正常提交
消费独占子 loan 及替换值，产生独立旧值，父 receiver 保持活跃。
若替换值确定没有正常出口，准入只消费原 prefix loan。普通
提前 return 结束子 loan，不交换字段。LLVM 复用工作树中已验证
的旧值 load 与新值 store，不执行中间析构。

验证器只允许从确切的当前 Inout receiver 建立独占字段投影。
将其经 CFG 参数传递产生的表示与支配入口视为同一借用身份。
不同字段的共享或独占投影允许共存。活跃的衍生借用、重叠、
过期或来源不明的字段 loan 不能作为交换许可。对以 receiver
为根的共享兄弟字段直接建立投影，避免先借用整个接收者。
交换结束子 loan 时，provider 验证同步退休其派生 place。

扩大验收曾在共享兄弟字段及纯文本 RAII 上得到 InvalidModel。
前者经直接共享投影修复。后者由原 verifier 诊断定位到
接收者在 CFG 中的两种 SSA 表示，确认为同一入口借用后修复。
独立复核检查了同字段、兄弟字段、独立别名、过期衍生 place、
提前返回与父借用保护。派生 Borrow 返回的组合仍在已测
`borrow-return` 形态报 UnsupportedNode，未扩大其 ABI 准入。

冻结编译器源码清单 SHA-256
`2a927dfb9e8bd2458388f5cf2dd7683fc9067d287a4d6abd10a4d0795376c04f`。
最终候选将字段渲染、发射和共同类型合同各自归入对应模块，
三个文件恢复尺寸门禁，未增加例外。macOS 严格 Clippy、
格式、文档、尺寸及 diff 检查通过。

两宿主的公开单文件与项目运行核对整数、可空资源、已交付旧
value、local 对照、兄弟字段嵌套交换、共享兄弟字段和两种
提前 return。另实跑 break 和 continue 前缀，确认取消的交换
没有改变原字段。Abort 前缀退出 134，仅输出进入标记及既有
abort 诊断，未观察到交换或析构。两宿主各自匹配纯文本 RAII
的 28 行预期，包括关闭标记后才析构旧资源。原综合程序未改，
macOS 仍在带插值的版本得到 UnsupportedNode。

最终同一源码在 macOS arm64 与离线 Linux x86_64 各通过
20 项 field_exchange 和 28 项 unit_field_replace_tests。
公开矩阵每个宿主有 20 次正常原生运行、2 次预期 abort，
另有两入口各一条同字段 L0135。普通和 constant-enabled Unit
的 native 回归另按分配指针检查唯一释放。无新 PR-event CI。

## 363. R2/R4 原子置换实参终止与未建立的局部绑定

从 §362 的 Borrow 返回组合移除字段交换后，普通和 inout
方法均复现 UnsupportedNode。现有 Borrow 结果声明准入明确
排除带 receiver 的方法，独立于字段交换。该组合暂未接入，
本节没有放宽方法 ABI。

进一步缩减到已支持的顶层 String entry Borrow 返回。
`replace(&slot, return source)` 原误报 L0088，
`swap(&a[return source], &b[0])` 也有同型红例。Unit 的两个
合法原语现在取已检查实参的普通完成性合取，保留静态 String
或 Unit 结果与描述符。暂定出口继续按原规则约束写回，错误
恢复不改。正常 swap 继续发布描述符，含 L0088 的错误产物
仍隐藏描述符。R2 回归同时检查条件有后继的 replace。

把已终止的 replace 用作普通局部初始化器时，refine 原先仍
查询局部绑定，从而拒绝合法前缀。准入和构建现共用 typed
完成性事实进入只求值路径，构建要求实际终止。绑定从未建立时
不会发布 SSA slot，死后缀不再进入 refine；替换前缀已经建立的
借用照常先结束，再清理已有局部，原借用返回来源保持有效。
独立复核覆盖普通与暂定完成性、描述符、正常 nullable 期望
和缺失事实的原路径。

冻结编译器源码清单 SHA-256
`bcc65ad5fde491c8ba04e54592d050228f2a09383f10a258dee8147d37bd93ed`。
macOS arm64 与离线 Linux x86_64 各有 352 项定向前端测试和
29 项字段交换测试通过。六组合法公开程序各走单文件和跨文件
项目，双宿主共 24 次原生运行均返回原 Borrow，stdout 一致，
stderr 为空。覆盖普通语句、无后继 val、死语句后缀、局部字段
和条件替换。两种入口均接受提前退出的元素 swap 检查。macOS
补测在 main 调用该 probe 的原生生成仍报 UnsupportedNode。
新增普通与 constant-enabled native 回归以分配指针核对原局部
与借用来源各释放一次，提前退出之后的输出不会执行。
macOS 的严格 Clippy、格式、文档、尺寸和 diff 检查通过。

扩大回归使用同一候选的实际 `native_tests::borrow_result_tests`
过滤器，两宿主各 61 项通过。首次按源文件名过滤选中零项，
已用正确模块名复跑。公开日志逐字节对账一致。本批未提交，
尚无新 PR-event CI。

## 364. R4 具名自有顺序容器的元素 swap

§363 的提前退出探针和两个正常交换探针均在原生生成报告 UnsupportedNode。
其中正常探针分别交换同一数组的不同下标和不同容器的元素。同槽探针仍由前端报告 L0135。
新增的 MoveOnly 交换回归在原编译器稳定复现生成失败。

Refine 用同一查询核对 source-qualified 类型描述符和前端发布的独占借用。
具名自有 Array / MutableList 的实参按源码顺序检查下标和边界，再登记元素借用。
LLVM 先加载两个旧值，再写回两个槽。交换过程中不调用析构。提前退出沿既有
调用 loan 清理路径结束已经登记的借用。零大小元素仍检查边界，随后结束两条借用。

独立复审发现两处合法输入被 SSA 误拒。第二下标为 `-((0))` 时，
受检负号结果经 CFG 参数交付，原排异只认直接 Constant。
第二下标为 `return xs[1]` 时，交换不会提交，SSA 原先据此跳过
前缀必需的元素排异。现根据实际独占元素借用启用证明，沿一致的
SSA 入边和值定义识别确切 Int 下标，并在借用建立、读访问和提交时核对。
整容器重排继续检查所有活跃借用。验证器负例覆盖同槽及重复 LoanId。
增量复核核对了两项修复、零大小布局和提前退出的借用清理。

此切片覆盖两个具名自有容器的直接元素。macOS 公开入口另复现
混合元素与局部根的 `swap(&xs[0], &item)` 仍报 UnsupportedNode。
具名借入容器和其他投影组合的准入还需核对。

冻结编译器源码清单 SHA-256
`687f346fe3341a64df2ce8756f1ad09e18b14b6f4c042cbf09a7ae92b4128789`。
macOS arm64 和离线 Linux x86_64 采用同一清单，各通过 35 项字段与交换回归、
12 项容器操作回归和 2 项借用下标回归。普通及常量 Unit 回归逐指针核对释放，
覆盖正常资源交换及两侧提前 return。零大小回归还检查 LLVM 不加载对应存储。

两宿主的公开单文件和跨文件项目入口各运行 12 条正常原生用例和
4 条预期越界 abort，并各拒绝两条同槽 L0135。两端正常输出逐字节
一致，越界诊断在规范化本地路径后相同。补充单文件在两个宿主均确认
continue 和 break 保留原元素，重分配的独占借用由前端报告 L0135。
首次运行因公开探针文件目录与 package 不匹配失败，已更正探针
布局并复跑。macOS 严格 Clippy、格式、文档、尺寸及 diff 检查通过。
本切片尚未提交，未运行新 PR-event CI 或完整 R5 矩阵。

## 365. R4 容器元素与可变局部根的混合 swap

在 §364 候选上，`swap(&xs[0], &item)` 通过公开 check，
原生生成报告 UnsupportedNode。现由同一 refine 查询消费元素类型
描述符、根绑定及两条独占借用的来源凭证。求值仍按实参顺序进行。
正常出口消费原根身份，将元素旧值作为同一局部绑定的新 SSA 值，
并把根旧值写入元素。Copyable 局部先隔离可借用的存储，
已有副本不随写回改变。MoveOnly 资源只在之后的正常清理点析构。

根独占借用沿用 `replace` 和根对根 `swap` 的建立路径，先结束同根
非空证明。右侧下标的提前返回结束已经建立的借用。下标自身可执行
其获准的写回，例如第一个下标里的 `replace(&item, ...)` 先于第二实参的借用。
反向实参顺序由前端按存续借用拒绝。SSA 验证器核对根与借用的确切身份，
原子交换拒绝伪造的同类型根。

独立静态复审核对了正常写回、CFG 根身份、提前退出、LLVM 的两次加载
先于写回，以及旧根原语的行为。初版未纳入借用闭包的存储逃逸规则。
最终候选基于共同借用类型的捕获标记拒绝该类 SSA 交换，覆盖两元素及
混合形态。新增直接 SSA 负例核对交换位置的逃逸诊断。

用于公开运行的编译器源码清单 SHA-256
`9e0a7881c969f92e3629520cb090b5f59888c255c3196d8321230da5fe2ac670`。
后续只修正闭包负例的模拟源码位置。定向测试的最终清单 SHA-256
`3751b7fbab90509c116e9c7ada3ba2ac5e376ee0d1cf9dcc655ab5b6767334c2`。
两份清单只有该测试文件的摘要不同。

macOS arm64 和离线 Linux x86_64 的公开单文件和跨文件项目矩阵
逐字节一致。每宿主运行 8 条正常用例、2 条预期越界中止和 2 条前端
L0135 拒绝。覆盖两种参数顺序、下标先写回局部、资源析构顺序、
条件命中的提前返回、条件未命中的交换提交、判空读出后写回 null、
以及零大小 Unit。补充单文件在两宿主确认 MutableList 的 MoveOnly
写回和析构顺序。经 Inout 参数借入的数组元素交换、堆字段与数组元素混合交换
也都通过 check，并在生成阶段复现 UnsupportedNode。

最终清单在两宿主各通过 84 项定向测试，含 38 项字段和交换、
18 项根交换及 28 项借用闭包逃逸。普通和常量 Unit 的资源回归
逐分配指针核对唯一释放。macOS 严格 Clippy、格式、文档、尺寸及
diff 检查通过。

公开 conformance 在同一生产候选的两宿主各扫过 154 个探针和
6 个综合程序。137/154 能力探针通过，综合程序为 0/6，23 项
保持已知未实现。官方复验与双宿主比较器通过，未见新增回归、
内部错误、错误输出或入口差异。当前源码未重新执行 R5 的资源
生成及 sanitizer 门禁，本批尚未提交或运行 PR-event CI。

## 366. R4 通过 Inout 参数交换容器元素

§365 的公开探针通过前端检查，在原生生成报告 UnsupportedNode。
具名 Inout Array 或 MutableList 现在从原形参借用投影出元素地址。
两个独占元素借用按源码顺序建立，使用既有的同下标冲突判定。
正常提交只结束元素借用，保留父借用供被调函数后续访问。
调用方继续通过原参数槽观察交换结果。提前退出结束已建立的元素
借用，保留容器原内容。

独立复审发现两处 SSA 边界。控制流转发父借用后，原入口 LoanId
与当前块参数曾被误判为彼此冲突。现只将来源唯一且相同的表示
视为同一父借用。另一个问题是扩容或写回会使此前取到的元素
地址失效。现按路径上的同一父借用身份标记旧 place，覆盖扩容、
整个容器换值和普通 Inout 调用。新投影仍可通过活跃的父借用建立。
负例验证这三种写回之后的旧 place 被拒绝，并验证子借用活跃时
禁止结束父借用。正向回归覆盖父借用跨条件分支和循环。
最终增量复核核对了借用身份和地址失效这两项合同。

完整回归的第一轮还揭示一个旧测试假设。该用例把所有独占容器
形参一律作为无效 provider。现区分可写 Array 的合法 Inout 参数、
不可变 List 的独占投影、错误下标类型和已结束的父借用。
已有共享容器读取见证仍保留。

macOS 补充探针确认，堆字段分别与元素、局部根及另一堆字段交换
均通过 check，仍在原生生成报告 UnsupportedNode。三种形态留待
核对堆字段的共同交换准入。

冻结编译器源码清单 SHA-256
`b7b94a9f7a8a41403c81200bb99771b961320e17b98d2a973afc9ad0924d20cd`。
macOS arm64 和离线 Linux x86_64 的公开入口证据逐字节一致。
每宿主有 8 条正常运行、2 条预期越界中止和 4 条 L0135 拒绝。
覆盖跨两个独立 Inout 根的动态下标、同一根的不同下标、
循环的 continue 和 break、提前 return、重分配冲突，以及
MoveOnly 对象的被调函数和调用方析构顺序。

最终清单每宿主通过 84 项定向测试，包括 42 项字段与交换、
12 项容器操作、2 项共享及独占 provider 对照和 28 项闭包
逃逸测试。MoveOnly 原生回归逐指针核对唯一释放。macOS 严格
Clippy、格式、文档、尺寸和 diff 检查通过。当前切片尚未提交，
未重跑完整 R5 或 PR-event CI。

## 367. R4 自有堆字段的原子 swap

§366 的三种合法输入均通过前端检查，在原生生成报告 UnsupportedNode。
现复用现有 Inout 字段 receipt 和直接堆字段借用，接入统一的
`ProjectedSwap`。覆盖自有局部和 Value 形参的直接字段与另一
直接字段、可变局部根、Array 或 MutableList 元素的交换。
使用相同的值类型完成两次加载后再写入。可变局部的新值经原 SSA
绑定交付。字段和容器保持各自的父所有者。MoveOnly 值由后续的
正常清理点析构，提前退出终止当前借用前缀且跳过交换提交。

独立复审发现，堆 owner 来自不同 CFG 入边时，原别名证明会丢失
同一当前对象的字段身份。现保留合流 Value 的身份，并沿逐入边
一致的值转发链识别同一 owner。不同入值或转发环保持保守。
Builder 在实参借用存续时通过确切字段读取或写入自有对象的相邻
字段。普通 MoveOnly 赋值继续先析构旧值再写入。SSA 只对来源为
直接拥有者堆 payload 的字段独占借用开放相应赋值操作。
正负回归覆盖合流与分支组合、此前缀内的读写、同字段重叠、越界
及提前返回。独立增量复核核对了归一证明和析构顺序。

原 replace 负例中的直接字段 swap 属于本轮新增支持，已有正向
原生回归取代该条断言。其余嵌套、泛型及非自有 replace 限制仍由
原测试检查。macOS 另确认经 Inout 类形参和 inout 方法接收者
交换字段均通过前端，仍报告 UnsupportedNode。

冻结编译器源码清单 SHA-256
`7af10e4700b72578f96986b983323137a1191067a72799242725a2c0c3c4832c`。
macOS arm64 与离线 Linux x86_64 各通过 107 项定向测试，
含 49 项字段和交换、18 项根交换、12 项容器操作和 28 项
闭包逃逸。单文件和类型／调用声明实际分置的跨文件项目结果逐字节
一致，每宿主有 12 条正常运行、2 条预期越界中止和 4 条 L0135。
MoveOnly 原生回归逐分配指针核对唯一释放和正常、早退析构顺序。
macOS 严格 Clippy、格式、文档、尺寸和 diff 检查通过。

同一候选运行 Guide 10 和 12 的双宿主 conformance，56/57
个能力探针通过。另一个保持为已登记的未实现项，两端未出现新增
回归、内部错误、错误输出或入口差异。证据复验和双宿主比较器
通过。本切片未执行完整 R5 或 PR-event CI，尚未提交。

## 368. R4 经 Inout 类形参和接收者交换堆字段

§367 的具名 Inout 类形参与 inout 方法接收者探针均通过前端，
在原生生成报 UnsupportedNode。现通过已核对的字段 receipt 和
确切父借用建立独占字段重借用，支持具名形参、显式 this 和隐式
接收者字段。ProjectedSwap 交换两个子借用指定的值并结束子借用。
被调函数可继续使用父借用，正常结果经原参数槽对调用方可见。
同父不同字段可以同时借用，同字段及整父冲突保持拒绝。

独立复审发现，纯堆字段交换原先可能跳过路径生命周期检查。
结束 CFG 转发的父借用后，入口身份仍出现在块状态中。现由
共享或独占堆字段投影启用路径检查，沿实际父借用来源识别已结束
的同一权限。负例在控制流块内结束转发父借用，再尝试通过入口
身份投影，确认 LoanInactive。另验证活跃子借用期间不得结束
父借用，以及两个独占子借用不得指向同一字段。独立增量复核
核对了不同父借用的独立性、同父子借用的清理、接收者专用
HeapReceiverFieldExchange 仍要求当前接收者。

补充 macOS 探针仍复现直接 Inout 根与另一根或字段的 swap，
以及 Inout 根和具名 Inout 参数字段的 replace。上述形态通过
公开 check，原生生成报 UnsupportedNode。保留其初始输入，
后续按可写槽位的共同交接审查。

冻结编译器源码清单 SHA-256
`7d019d13ceb0deee9d69d70e4956d15a54fdab8917ec825c2196cb540b557f11`。
两宿主各通过 96 项定向测试，含字段／交换 53 项、接收者交换
验证 1 项、容器 12 项、provider 对照 2 项及闭包逃逸 28 项。
公开单文件和实际跨文件项目在 macOS arm64 与 Linux x86_64
逐字节一致。每宿主有 8 条正常运行、2 条越界中止、4 条 L0135。
正常回归核对两个独立父对象、循环退出后再次交换、调用方看到
写回和 MoveOnly 析构顺序。普通和常量 Unit 原生测试逐指针
核对唯一释放。macOS 严格 Clippy、格式、文档和 diff 通过。

同一候选的完整公开 conformance 在两宿主各运行 154 个能力探针
及 6 个综合程序，结果为 137/154 和 0/6。23 项保持已登记的
未实现状态。证据复验及双宿主比较器通过，未观察到新回归、
内部错误、输出错误或入口差异。Rust 尺寸门禁通过。本批尚未
提交，当前切片尚未执行资源生成、sanitizer 或 PR-event CI。

## 369. R4 直接 Inout 根与具名参数字段置换

§368 冻结候选中，双 Inout 根交换、Inout 根与 Inout 字段
交换、直接 Inout 根 replace 和具名 Inout 类参数字段 replace
都通过公开 check，原生生成报 UnsupportedNode。细化层现在
消费同轮原语、独占借用和正常交付事实。直接形参在原调用槽
上建立临时独占子借用，具名类字段复用字段重借用。交换或
替换提交后结束子借用，旧值由 replace 的调用方继续持有。
RHS 提前结束时释放已建立的借用，跳过写回和旧值交付。

独立复审发现可空分支终结指令遗漏了整根重借用的挂起检查。
新 SSA 回归在两条边显式运输并清理子借用，修复前 verifier
接受，修复后仅在分支位置报 LoanDependencyActive，普通 CFG
边仍可转发活跃借用。另有负例拒绝结束挂起的父借用和使用交换
前取得的元素地址。扩大检查发现旧字段 verifier 测试仍预期
独占父借用不得建立共享子借用。按 §367 的同根兄弟字段合同，
测试改为对共享与独占父借用均验证子借用存续、父借用提前
结束及字段越界。同字段独占重叠保留独立拒绝见证，变更已独立复核。

冻结编译器源码清单 SHA-256
`e314d699e350a14a97d44e1118d8f016911357b2a17662ef9fa9b160de1c0f8a`。
macOS arm64 与离线 Linux x86_64 各通过 126 项定向测试，
含字段／置换 57 项、资源 verifier 26 项、容器 12 项、
provider 对照 2 项、接收者交换验证 1 项及闭包逃逸 28 项。
基础 Unit 与常量 Unit 的资源回归核对逐指针唯一释放。每个
宿主通过单文件与声明实际分置的跨文件项目共运行 20 次，
stdout 逐字节一致且 stderr 为空，另有 2 次同槽拒绝 L0135。
实跑覆盖调用方写回、两种实参顺序的局部根混合交换、独占
String? 的旧值交付、提前 return 及循环 break/continue 后
再次提交。macOS 的 codegen all-targets 严格 Clippy、格式、
Rust 尺寸、文档及 diff 检查通过。

同一候选的 Guide 10、12 公开 conformance 在两宿主均为
56/57，余 1 项保持已登记的未实现状态。证据复验及双宿主
比较通过，零回归、内部错误、输出错误或入口差异。本节的
验收范围为定向矩阵。合批 PR-event CI、完整 conformance、
综合程序、生成资源与 sanitizer 仍按各自阶段核验。

## 370. R4 自有根见证与 R5 Linux 资源缩减复验

扩大 §369 的原生置换测试范围后，macOS 的旧
`unit_root_primitive` 过滤组得到 21 通过、1 失败。唯一失败
仍期待拒绝直接 Inout 参数的合法 `replace`。此输入现应走借入
槽，旧见证更新为在普通和常量 Unit 均产生一次
`ExclusiveRootReborrow`、一次 `InoutExchange`，以及零次
`RootReplace`。原有 verifier 拒绝 Inout ABI loan 冒充 owned
root 或 owned field 的负例继续保留。本次只改测试。更新版本的
源码清单 SHA-256 为
`136fad0c756e2d0d406cab28353041d6d01b591b58a5e34a5f7379519004bc5d`。
Linux x86_64 根置换组 22 项通过，两个 owned 路由负例均通过。

同时用 §369 的冻结编译器复跑 §348–350 曾超时的 Linux x86_64
生成资源缩减阶段。首次设置因本地 Git 对象替代路径未挂入容器
而中止，补齐独立浅层基线元数据后按原命令和预算复跑。LLVM
21.1.8、Rust 1.96.0，导出器 SHA-256 为
`2d72217a8618d2949362a7d41e6d5420197680ce48686b62dd83036f4fc36d6d`。
原 120 秒、32 候选门限下，用时 69.08 秒。受控
`drop:holder` 缺失见证保持，操作 9→5，源码 717→551 字节，
6 次候选尝试，最终连续 3 次确认，单步最小检查通过。

真实导出共 15.83 秒，三类编译链接累计 51.36 秒，原生执行
累计 1.69 秒。该轮未复现旧超时，尚不能确定两轮耗时差异的根因。
此结果只覆盖完整生成资源门禁中的缩减阶段，保留此前整套超时
历史，整套验收仍须在同一候选上复跑。

## 371. R0 codegen 旧分析消费者的保留用途

沿用 §347 的直接调用口径，在当前工作树复查 codegen src。
`resolve_names`、`check_types`、`check_ownership`、
`complete_file_types`、`lower_scalar_file_with_entry` 分别为
13/15、13/15、12/14、5/5、2/5（文件数／匹配行数）。以下
覆盖 13 个实际消费文件，路径相对 `crates/lang-codegen/src/ssa`。

| 保留用途 | 实际消费者 |
|---|---|
| 旧入口准入和分析身份 | `lowering_entry_contract_tests.rs`、`single_readonly_method_tests.rs`、`lower_frontend_tests.rs`。前两者也是旧具名 entry 唯一直接调用者，见 §348 |
| 借用、退出和事件对照 | `borrow_result_contract_tests.rs`、`lower_frontend_owned_source_continue_tests.rs`、`integer_range_value_tests.rs`、`while_flow_tests/if_cleanup_observation.rs`。保留 Single 的来源、拒绝和清理事件见证 |
| 私有规划和类型合同 | `map_source.rs` 的 Single 规范类型缺失负例、`unit_llvm_tests.rs` 的原模板交付、`lower_frontend/container.rs` 的 Single 容器类型基线、`refine_closure_tests.rs` 的旧运行时意图对照 |
| 旧 gate 与可空 Map 的故障见证 | `unit_lower/nullable_when/source.rs`、`map_provider_exits_pipeline_tests/local_sources/deep/nullable_map_guard_support.rs`。两者使用实际 Single 类型产品验证登记或准备故障检查点 |

沿模块声明核对，这些调用均在 `cfg(test)` 路径，部分还需相应
实验或 CI 测试选择。生产 `unit_lower` 中的直接调用位于内嵌
测试模块。此轮仅补消费者处置账本。旧入口专有的诊断、身份和
LLVM 基线保留至被测实现退役；迁移对照者需先验证相同 oracle。
宏、间接调用、仓外 API 与 frontend 集成测试另行追踪。

## 372. R0 旧最终准入的集成测试消费者

沿用 §347 的调用口径，frontend 集成测试中
`complete_file_types` 的 21 个文件、25 处匹配均已定位。
路径相对 `crates/lang-frontend/tests`。

| 保留用途 | 直接调用文件 |
|---|---|
| 完成阶段的身份、诊断和事实发布 | `final_type_admission/{complete,failed_overload_trials,iteration_recovery,pending_iteration_source,primitive_arity,simple_assignment,static_recovery,unit_statement_map_write}.rs`。覆盖同一分析产品、失败候选、待定迭代、外部同名函数及保留的 Single/Unit 诊断差异 |
| 同一输入的类型与所有权对照 | `control_branch_join.rs`、`enum_case_borrow_categories/support.rs`、`support/closure_safety.rs`。继续核对分支结果、enum 来源和闭包事实拒绝 |
| 迭代、可空 `when` 和容器来源 | `integer_range_iteration_ownership.rs`、`integer_range_values.rs`、`multifile_type_checking/nullable_when.rs`、`ownership_iteration/map_borrowed/{ordinary_borrow_source,pipeline,unacquired}.rs`。已局部迁移的正例继续走 Unit，保留旧来源、类型和借用计划见证 |
| 解析错误传播及残缺解构的阶段门禁 | `multifile_type_checking/destructuring.rs`、`ownership_structural.rs`。保留旧分析下的 Parser/语义诊断次序与禁止发布错误计划的断言 |
| Map 消费身份与清理义务 | `ownership_map/consume.rs`、`ownership_map/consume/field_roots.rs`。保留旧返回交付、字段来源与失败事实对照 |

`lang-frontend` 仍通过公共模块导出 `resolve_names`、
`check_types`、`check_ownership` 和 `complete_file_types`。
manifest 的 `publish = false` 不封闭路径依赖。仓库内 CLI、LSP
和标准库的 src 均无这四个名称的引用。Unit 名称构建依然在
生产逐文件调用 `resolve_names`，该求解器仍有保留职责。
本轮只记录静态消费者处置，未改变公共 API。仓外实际使用、
宏展开和间接调用未由这份清单证明。

## 373. R5 冻结候选的 Linux ASan/LSan 实跑

按 §370 的源码清单
`136fad0c756e2d0d406cab28353041d6d01b591b58a5e34a5f7379519004bc5d`
在离线 Linux x86_64 运行官方 `check_native_sanitizers.py --linux`。
使用固定 LLVM/Clang 21.1.8 和独立 Cargo target，按脚本默认
codegen 测试配置生成 LLVM。三项精确导出测试各运行一次并通过。
门禁退出 0，覆盖 12 个检测器开关对照、一组 Map 原生程序、
12 个 Single/Unit provider 生命周期用例。Map 和每个 provider
用例均分别实跑 ASan、LSan，检测器命中与无报告路径按原 oracle
验证。产物保留运行时档案哈希，UBSan 仍未覆盖。

同一导出器 SHA-256
`2a2d67378749b15e8729f5acc6b2c73bea383cc8f78065820a5c5599ff9d563c`
另实跑 §369 的四个置换探针，对应两种混合实参顺序、循环
break/continue 后重新提交、根及字段的资源归属、条件 RHS
提前 return。只将公开入口名从 `main` 改成导出器使用的
`entry`。四个用例均通过原生分配/释放计数、ASan 和 LSan，
共 12 次执行，stdout 符合 §369 的独立预期且 stderr 为空。
这组资源证据覆盖本轮变更路径。完整生成资源门禁、双宿主
默认 codegen 与 PR-event CI 分别记录。

## 374. R4 Linux 默认 codegen 全套的停滞诊断

§370 的测试更新候选在本地离线 Linux x86_64 容器运行默认
feature、默认线程的 codegen 全套。此进程在 523 条完成记录后
停滞，最后两个超时报备来自 Map local-future 的 Single 与
Unit 运行。间隔 2 秒取两次 `/proc` 样本，发现 libtest
主线程及 10 个工作线程均处于锁等待，CPU 计数未变化。
容器 LLDB 已分离，采到的指令地址不能展开到有效的用户态栈。
当前证据还不能定位锁的持有者或确定根因。

用同一 Linux 测试二进制在独立进程串行隔离该过滤组，
2 项均通过，用时 23.79 秒。全套原始并发的停滞单独保留。
另启动同一二进制的全范围四线程诊断复跑，覆盖项不变。
该轮用来判断本机并发条件，结果与默认线程和远端 CI 分开。
随后四线程复跑越过 523 条完成记录，在原进程已持续停滞后
主动以 SIGTERM 结束原全套，保留失败日志。原进程的停滞线程
执行映射包含 Rosetta，已采样的 futex 系统调用来自该映射。
仍未定位锁的持有者。

## 375. R1 跨行 when 条件中嵌套 if 的 else 归属

继续核对 §316 的复合 selector 以及 #443 复审提出的
`pending_else` 疑点。前一分支用跨行 `produce(1)`，后续
`((true)) && if (outer) if (inner) true else false else true -> 2`。
Guide 06 的内层 else 属于最近 if，外层 else 仍归外层 if。
独立预期为 (true,true) 选该臂、(true,false) 落到 when 的
else、(false,false) 选该臂。

使用 §369 的生产编译器源码清单。在 macOS arm64 和离线
Linux x86_64 的单文件、实际跨文件项目入口各通过 check 与
原生执行，四次退出 0，stdout 均为 `2\n3\n2\n`，
stderr 为空。该验收覆盖有两个待归属 else 的合法组合，保留
既有实现和缺分隔恢复检查。

## 376. R5 当前生产候选的双宿主完整公开语料

使用 §369 的生产编译器源码清单和原 conformance baseline，
离线 Linux x86_64 共运行 154 个能力探针和 6 个综合程序。
CLI SHA-256 为
`c4d45a32dab70f44239ff09612cb951ff9c69b72c207be265b316418c806ea73`。
证据账本的编译器标识同时包含 `6cc03907` 基线与本地源码清单，
复验产物及独立预期均通过。能力为 137/154，综合程序 0/6，
23 项仍为已登记的未实现能力。未观察到新回归、入口差异、
错误输出或内部错误。与 §368 的 Linux 证据逐项比较，160 项
输入哈希、两入口状态标记及 stdout 哈希均一致。综合程序仍为
`config_parse` L0080、`expr_eval` L0165、`linked_stack` L0085、
`raii_state_machine` 与 `varint_codec` UnsupportedNode、
`word_count` L0165。基础可运行门槛仍为 58/74，未达 70。
macOS arm64 同一生产源码的 CLI SHA-256 为
`db8bd3681703cb50dd784bae2bdb12f185391036a1afc5abfbe3520bdc2252f5`。
同一矩阵与证据复验通过，结果也是 137/154、0/6。
原比较器核对输入、两入口分类和成功输出，Darwin/Linux
双宿主对账通过。该轮完整公开语料仍处于 R5 冻结候选之前的
本地源码基线。

## 377. R4 广覆盖 SSA 中的两个过期测试断言

用 §370 的 Linux 二进制扩到 `ssa::` 后，分别隔离实跑两个
失败。Elvis 用例仍以 UnsupportedNode 拒绝，但 §333 已让前面的
`var x: Int? = 7` 具备布局，错误出现在 `left ?: return 0`。
更新诊断片段，保留简单 Elvis 的错误种类和精确位置断言。

字段组的测试名保留旧 Single 拼写，实际使用 `UnitSource`。
其中直接兄弟字段 swap 已由 §367 支持，从负例集合移出。
其余多级、泛型字段和两种 sibling RHS replace 仍要求
UnsupportedNode。另复跑已有的普通／常量 Unit 原生正例，
确认直接字段交换、混合槽写回及 Copyable 快照。

仅更新两个测试文件，编译器生产源与 §376 一致。
测试清单 SHA-256 为
`4c5f15fa604b68765175b73256960537628cd80bdafb94a4c7e15fb172d37122`。
Linux 依次通过 6 项 CFG／所有权、7 项直接字段 replace、
1 项字段交换原生正例，零失败或忽略。扩大套件仍按各自输入记录。

## 378. R0 旧所有权入口的前端集成测试消费者

沿用 §347 的调用口径，frontend 集成测试的 `check_ownership`
为 45 个文件、70 处直接匹配。以下分组无重复、无未分类文件，
路径相对 `crates/lang-frontend/tests`。

| 保留用途 | 文件／调用 | 直接调用文件 |
|---|---:|---|
| 分析身份、准入和原诊断 | 4／11 | `ownership_checking.rs`、`legacy_receiver_admission.rs`、`guide_litmus.rs`、`final_type_admission/simple_assignment.rs` |
| 独占置换、形成、交付和析构 | 7／9 | `ownership_{constants,construction,resource_deinit,primitives,field_replace,nullable_when}.rs`、`enum_case_borrow_categories/support.rs` |
| 调用实参和借用结束 | 4／4 | `ownership_{borrow_continuation,nested_borrow_arguments,two_phase_borrows}.rs`、`unit_direct_call_loan_ends.rs` |
| 闭包来源和失败清除 | 4／5 | `ownership_closures.rs`、`ownership_closures/{callable_contract,callable_provenance}.rs`、`support/closure_safety.rs` |
| 迭代、来源和退出边 | 10／12 | `final_type_admission/pending_iteration_source.rs`、`integer_range_iteration_ownership.rs`、`ownership_iteration.rs`、`ownership_iteration/control_exits.rs`、`ownership_iteration/map_borrowed/{ordinary_borrow_source,pipeline,unacquired}.rs`、`ownership_range_{receiver,carrier,construction}.rs` |
| 借用帧、回边与零次循环见证 | 6／17 | `unit_borrow_{consumer_body,frame_closure,frame_consumers,loop_edge_evidence,zero_body,zero_loop}.rs` |
| no-carry 对照 | 8／10 | `unit_loop_{arm_no_carry,break_arm_no_carry,counter_arm_no_carry,counter_break_arm_no_carry,no_carry,no_carry_boundaries,no_carry_evidence,prefix_counter_break}.rs` |
| Map 消费来源与正常清理 | 2／2 | `ownership_map/consume.rs`、`ownership_map/consume/field_roots.rs` |

旧产品的身份、独有诊断、只在 Single 发布的事实及观测模式对照
保留至其各自的被测合同退役。语义和资源对照的普通正例可以在
Unit 有同一诊断、计划、退出边和析构 oracle 后分批迁移。
计数说明调用位置与保留依据，未对所有消费者另做实跑；
公开可用性边界沿用 §372。

## 379. R5 冻结候选的工具入口与标准库复验

沿用 §376 的 Linux 生产 CLI 二进制和脚本原 byte oracle，
`check_word_frequency.py` 的 40 条公开命令全部通过。本轮另在
§377 的冻结源码上运行 `cargo test -p lang-lsp` 与三个 CLI
集成目标。LSP 56 项单元测试、1 项真实 stdio 格式化测试通过。
CLI 的 `check_cli`、`format_cli`、`project_basic_ownership_cli`
分别通过 7、3、9 项，覆盖来源诊断、格式化和项目入口的所有权
及常量准入顺序。两组均零失败、零忽略。使用离线 Linux x86_64
容器的独立 Cargo target。macOS arm64 的同一生产源码也通过
全部 40 条标准库命令。逐项比较两宿主的退出码、stdout、stderr
和 oracle，40 条完全一致。CLI/LSP 测试计数限 Linux，
双宿主 PR-event CI 另记。

## 380. R4 稳定字段比较的 SSA 见证

§377 使用的旧 Linux `ssa::` 扩大运行共选 2301 项，
结果 2235 通过、3 失败、63 既有忽略。第三项在 macOS 隔离
也复现，原聚合测试仍期待拒绝 `bundle.text == "owned"`。
该表达式按 §351 使用短共享借用。更新测试，要求比较的左
操作数是字段 0 的共享借用，并用 ownership verifier 验证整段
SSA。原泛型聚合的布局断言、临时父对象的 Copyable 字段
和清理断言继续执行。现有
`direct_temporary_move_only_field_read_remains_unsupported` 及
§351 的同字段独占冲突负例保持原样。

更新后的编译器测试源码清单 SHA-256 为
`ae8fc17479b1f438acc94c75ad4175368843d8e8b8328f5b92e448e8cd4fa843`。
Linux `unit_lower_aggregate_tests` 的 13 项全部通过。
§374 的旧四线程全套已得到终态，3404 通过、3 失败、71 忽略、
零过滤，用时 1488.34 秒。失败恰为 §377 的两项和本节的聚合
见证，三条现已各自在更新后的目标组通过。此次生产源码无差异。
同一最新测试快照的 Linux 四线程全套与独立 frontend 全套
仍按各自执行身份核验。

## 381. R5 macOS LSP stdio 的启动超时

在 §380 的前端与工具源码上独立构建 macOS arm64 LSP。
56 项单元测试通过，Cargo 驱动的真实 stdio 格式化用例多次
达到原 10 秒等待门限。临时诊断版定位到首个 initialize 请求。
沙箱外及单个 Cargo job 也复现。该测试子进程 165 个样本
均处于 `_dyld_start`，还未观察到 LSP 处理线程。进程状态
采样为睡眠，未发现可用 SIGCONT 恢复的暂停态，启动等待的
根因尚未确定。

直接执行同一 Cargo 构建的 Rust 测试二进制通过，工作目录
及 Cargo 日志中列出的加载环境复刻后仍通过。独立逐请求探针
在同一 server 二进制上通过单文件及项目初始化、打开、修改、
格式化、关闭文档及 shutdown/exit。Cargo runner 对照出现
有通过也有超时，尚未证明会话或文件描述符是原因。
保留 Cargo 超时记录，测试、原门限及生产 LSP 均保持原样。

## 382. R1–R3 冻结候选的 Linux frontend 全套

编译器测试源码清单为 §380 的
`ae8fc17479b1f438acc94c75ad4175368843d8e8b8328f5b92e448e8cd4fa843`。
在独立的 Linux x86_64 容器 target 上执行
`cargo test --locked --offline -p lang-frontend --no-fail-fast -- --test-threads=4`，
退出 0。243 个目标结果合计 3945 项通过、0 失败、15 忽略、
0 过滤，通过数包含 15 个 doc-test。默认配置下 42 个目标
未注册运行用例；实验能力或忽略组的验收不由这条命令证明。

## 383. R0 旧类型入口和逐文件名称求解器的直接消费者

复查 frontend 集成测试中 `check_types` 的 62 个文件及
88 处直接匹配。45 个文件和 60 处匹配位于 §378 已列的所有权
消费者文件。另 11 个文件和 14 处匹配位于 §372 最终准入的
消费者文件。剩余六个文件和 14 处匹配如下：

| 测试文件 | 保留的 Single 见证 |
|---|---|
| `final_type_admission.rs` | `with_types` 给子模块提供同源码 Single/Unit 原始类型，用于最终准入及分析身份对照 |
| `type_callable.rs` | 有歧义或全失败的 lambda trial 不遗留调用、参数模式或候选符号类型，另有独立 Unit 对照 |
| `type_checking.rs` | 来源／环境身份、重复求解确定性和未完成事实分类 |
| `type_constants.rs` | 常量资格、实例身份和错误分析不暴露部分常量能力 |
| `type_iteration.rs` | 已含名称错误时，旧原始类型检查不级联且不发布迭代计划；其余原八组由双 Unit 承接 |
| `type_range_carrier.rs` | 名称失败后的旧原始类型诊断，与先完成名称准入的两个 Unit 模式分别验证 |

`resolve_names` 在同一集成范围为 73 个文件、133 处直接
匹配；其中 11 个文件、47 处未直接调用 `check_types`。
`name_resolution.rs` 验证逐文件解析规则，
`multifile_name_resolution/forward_enum.rs` 比较枚举成员的
局部与 Unit 归属。其余九个文件是
`ownership_iteration/{capture_owner_identity,loop_carried_choices,nested_instance_replay,origin_fixed_point,phi_edges,phi_layout,phi_source_replay,prior_owned_instances,sibling_source_loans}.rs`，
保留逐文件 SymbolId 与来源重放、汇合、父子借用对照。
Unit 的生产名称构建仍调用同一逐文件求解器，见 §349。

两项清点覆盖当前集成测试的直接调用。身份、恢复顺序和旧观测
见证保留至对应合同退役。迁移普通语义用例前需确认相同 oracle。
间接函数引用及仓外依赖仍在清点范围之外。此次未更改公开接口；
该树的 Linux frontend 动态证据见 §382。

## 384. R5 macOS stdio 超时的主线基线对照

将 `6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b` 导出到独立
目录，使用与 §381 一致的低调试信息 Cargo 配置编译。
`formatting_stdio.rs` 与当前候选逐字节相同。基线同样在 Cargo
启动时达到原 10 秒门限，临时诊断版定位到首次 initialize。
直接运行基线的同一 Rust 试验二进制通过，用时 0.11 秒。
逐请求协议对照也返回全部 14 个响应，单文件和项目分别正常
退出、stderr 为空。两份源码跨度内均可复现这一启动现象，
§381 的超时已有主线基线见证。仍待确定本机启动等待根因，
测试门限及普通验收入口维持原样。当前候选恢复 Cargo 标准
调试信息配置重新构建，也在原门限超时，低调试信息配置
并非该现象的必要条件。

## 385. R4 最新测试快照的 Linux codegen 全套

§380 源码清单在离线 Linux x86_64 实跑
`cargo test --locked --offline -p lang-codegen --lib -- --test-threads=4`。
退出 0，3407 项通过、0 失败、71 忽略、0 过滤，用时
1245.35 秒。先前受三条过期断言影响的完整四线程运行见
§380，默认线程停滞的诊断及原日志见 §374。此次全套选择
及默认 feature 不变，仅固定本机并发为四线程。同一测试清单
在 macOS arm64 的聚合、CFG／所有权、字段 replace 与 Inout
根组分别通过 13、6、7、1 项，共 27 项无失败或忽略。
Linux 同清单的 workspace 全目标严格 Clippy 通过。
Rust 尺寸门禁、文档及 diff 检查通过。现有忽略组、
macOS 同版本全套与 PR-event CI 分别验收。

## 386. R5 冻结候选的 Linux 生成资源全门禁

使用 §380 的源码清单，在离线 Linux x86_64 的 LLVM/Clang
21.1.8 上执行原 `check_generated_owners.py`。checker 变异
需要从 Git HEAD 创建工作树，因此用独立临时仓库保存只用于
测试的同源码快照。fixture 为 `12c20250c17226a7e8298a1fb4c3b73a1f406cac`，
tree 为 `84e4ac35afeb73a440fd7291408f89298c86d7c6`。
开发分支和远端未因该测试变更。导出器 SHA-256 为
`dcfcd9dd3764430c0d6c89f6ae1a41bbdd4697cab86e8b3a7bee83befa937598`。

门禁退出 0，`acceptance.json` 的状态为 `pass` 且
`requirements_met=true`。全部 16 个资源或拒绝用例通过，
检测器校准通过。checker 确认证据可杀死受控 mutant，
恢复源码后再次验证原诊断。G5 在原 120 秒预算内用时
101.80 秒，以 6 个候选尝试把稳定的析构省略故障从
9 步减至 5 步，连续 3 次确认并通过单步最小化。
本轮证据替代当前候选缺失的 Linux 整套结果，§348 的
历史超时仍保留。macOS 平台门禁与 PR-event CI 分别验收。

## 387. R5 macOS 生成资源的首次执行超时

使用 §386 的独立 fixture 与 §385 同清单新编译的 macOS
导出器实跑官方生成资源门禁。case-00 的计数产物在原 5 秒
门限超时，整套退出 1，原证据保留。对同一个已链接产物，
沿原进程执行器、相同门限和字节预期独立复跑三次，均退出 0，
stdout 精确匹配、stderr 为空，各用时 0.013–0.015 秒。
原产物 SHA-256 为
`f57c8321fd6155146afff72326f7617a2b7978b444d39c13c99143ac3fdbb2cb`。
隔离复跑未复现超时，不能据此替代整套验收。第二轮改用较短
的仓外产物路径运行原门禁，保持全部测试、预算和 oracle。
第二轮同样在 case-00 首次计数进程处超时，退出 1。
留存产物再次执行通过；相同字节的新副本及重新链接产物
均在首次执行复现超时。通过不依赖 Koven 或验收执行器的
对照再缩减：使用普通 clang 编译的 `int main(void) { return 0; }`
及相同字节计数副本，分别以普通子进程执行。两个新产物
首次均约 5.00 秒超时，采样的 164 个栈均处于 `_dyld_start`。
对同一文件第二次执行均通过，分别用时 2.60 和 2.79 秒。
已存在的计数产物对照在 0.002 秒通过。当前证据将首次
启动问题定位到本机加载阶段，仍未确定宿主根因。
两轮整套结果保留为失败，门限及验收脚本保持原样。

## 388. R5 macOS 首次加载对照与 LSP 标准配置复跑

沿用 §387 的最小 C 产物，分别通过 fork/exec、显式
posix_spawn 和新会话启动三个相同字节的新文件。本机首次执行
分别用时 7.740、7.496、7.315 秒，均退出 0。同一文件第二次
执行分别用时 0.005、0.028、0.005 秒；通过另一条硬链接路径的
已执行文件用时 0.003 秒。独立探针等待上限为 30 秒，原资源
门禁仍为 5 秒。观测到的差异随文件 inode 复现，三种启动方法均
可复现，宿主根因仍未确定。§387 中两次整套门禁失败仍成立。

§384 的未修改主线另做带计时的 stdio 对照，首次单文件
initialize 约 7.866 秒，项目 initialize 约 2.347 毫秒，后续
请求均不超过 5.35 毫秒。之后在当前候选用原测试、原 10 秒
门限、Cargo 标准调试配置复跑 `lang-lsp`，56 项单元测试和
1 项真实 stdio 格式化测试均通过，0 失败或忽略。stdio 用例
用时 8.73 秒。保留 §381、§384 的既往超时记录。

## 389. R4 生产路由与公开入口的可达性

核对 `refine::route`、Unit `planned_body` 与闭包 thunk 接线。
整函数 subset 拒绝后才进入生产 Compatibility。已选 Refine
后，类型准备或构建错误直接返回。测试专用的强制逐家族路由仅在
subset 接受时生效，退出时恢复线程局部状态。生产 Inout 形参、
Inout 接收者和闭包 thunk 仍受各自的 Refine 准入保护。
§385 的完整 Linux 运行通过八组退役家族边界 deinit、借用与
Rc、置换与接收者、enum 与 List、共享捕获与 View、数值迭代与
Map、String clone 与 Box、根所有权原语；这些见证覆盖相应的
私有保护分支。

用 §369 同一生产候选在 macOS arm64 和离线 Linux x86_64
实跑公开单文件和跨文件项目入口。六种输入组合的 frontend
check 均退出 0，stdout/stderr 为空。直接从入口调用
`change(inout slot: String)` 时，执行输出为 `old\nnew\n`，
退出 0、stderr 为空。加入一个含尚未支持的 Elvis 的未调用函数，
上述结果逐字节一致。改为调用该函数后，两个入口均出现唯一的
`UnsupportedNode`，stdout 为空，原输出目录没有残留产物。
两个宿主结果一致。原生构建的 operational error 沿用入口现状，
单文件退出 2，项目退出 1。此处只记录既有差异，见 Guide 13 的
构建失败约定。仍需按家族核对可达的混合函数及其清理行为。

## 390. R4 macOS 扩大验收的快照区分

较早的 macOS 默认 feature 全套已结束，3400 项通过、4 项
失败、72 项忽略，零过滤，用时 5645.86 秒。失败为 Elvis
诊断位置、兄弟字段置换、Inout 根准入、稳定 String 字段比较
四条过期断言。最新快照的四组定向结果见 §385，其中保留
UnsupportedNode 范围、活跃子借用和临时父对象的负例。

已在 macOS arm64 核验 §380 源码清单全量匹配后启动
`bash scripts/run_codegen_default_tests.sh --offline -- --test-threads=4`。
采用仓库默认 feature codegen 门禁的构建配置，与较早快照
分开记账。待取得终态后更新通过、失败、忽略和过滤数量。

## 391. R5 macOS 生成资源门禁第三次实跑

使用与 §388 相同的最小 C 文件做两轮对照。新文件按原签名、
先运行 `codesign --verify --strict`、重新 ad-hoc 签名三种方式
分别执行两次。六个文件首次执行耗时 4.118–4.635 秒，第二次
0.004–0.008 秒，均退出 0、无输出。校验保留原字节，重新签名
改变字节。两种操作均未消除首次与再次执行的差异。首次耗时较
§388 缩短，变化原因仍未确定。

同一 §386 干净 fixture 及 §387 的同一 macOS exporter 以原门禁
脚本和门限再次执行。16 个资源与拒绝用例均通过。macOS 校准
按脚本记录平台受限，checker 变异验证通过。缩减阶段的原始
clean 正常，受控 fault 如预期遗漏 `drop:holder`，第一个缩减
候选仍复现该差异。第二个候选的首次计数进程在 5 秒门限处超时，
oracle 将两种指纹的差异判定为 flaky，整套退出 1。
在原工作目录、原执行器和原 5 秒门限下直接复跑该次留存的同一
计数文件三次，均退出 0，输出均仍缺少受控的 `drop:holder`，
stderr 为空。该文件 SHA-256 为
`8c84b3a0755d40e2f3e652e81f06f6cb6b9f6923c45ae7ccbda6135e23ce1e07`。
本机首次执行超时仍影响缩减验收。三轮整套失败均保留，脚本、
门限和 oracle 保持原样。

## 392. R4 默认 codegen 忽略项的 CI 选择范围

按 §385 的 Linux 全套日志提取 71 个忽略项的完整测试名，
逐个对照 `ci.yml` 实际调用的 runner 与 manifest。默认配置内
每项均有唯一的显式执行入口。借用故障 manifest 覆盖 60 项，
Unit root-set runner 覆盖 3 项，child cycle runner 覆盖 4 项，
parent receipt 和 scalar owner entry runner 各覆盖 2 项。
借用故障 manifest 的 codegen 清单另有 6 项 feature 专有目标，
不在本次默认日志中。本次是选择范围对账，故障注入的实际执行
结果仍以最终提交的双宿主 Targeted Tests 为准。

同一测试源码在 macOS 最新二进制枚举 3476 个用例，Linux
全套登记 3478 个，共有 3474 个。Linux 独有四个目标，覆盖 ELF
行表、本机目标、非 UTF-8 输出目录和 object 写入失败重试。
macOS 独有两个目标，覆盖 Mach-O 行表与实际 LLDB 断点。
macOS 另有的第 72 个忽略项就是后者，需要 debugserver
task-port 权限。本机沙箱外以最新二进制精确复跑该目标，源码
断点命中并报告帧，1 项通过、0 失败、0 忽略，用时 5.67 秒。
其余 71 个忽略目标与 Linux 清单完全一致。

## 393. R4 循环体内 nullable when 写回后重新借用

同一 §369 生产候选已在 macOS arm64、离线 Linux x86_64
分别复现。循环体在 nullable `when` 的非空臂为局部根写回，
随后用 `if (slot != null) { read(slot) }` 借用新值。完整程序
覆盖 return、continue、break 和有 deinit 的迭代内外资源。
单文件与跨文件项目均通过 frontend check，构建阶段均为
`InvalidModel: frontend lowering failed with MissingFact`。

最小复现只需一个可空类局部、一个带 break 的 while、一次
when 内写回和一次借用。移除 Guard、return、continue 或
末尾按计数 break，完整输入仍为 MissingFact。去掉重新借用、
将 while 改为普通词法块，或把外层 when 改为等效判空 if，
对应对照可以生成产物。直接在循环中反复 move 同一个外部
replacement 的早期探针被前端正确拒绝 L0131，实际复现改用
每次求值的构造或工厂结果。

候选调查聚焦 Compatibility 的 fresh heap If 准入。
当前在存在任一外层 loop context 时提前放弃该入口，随后普通
条件构建无法提供窄化读所需的借用。试验修复与正式回归在隔离
源码上执行。闭合前须核对 proof 已在本次迭代的限定借用后结束，
不得进入回边或其他控制转移，并验证原生输出和析构次序。

隔离源码上新增正式回归，基线红测定位到借用实参 `slot`。
候选保留原 typed/owned Borrow 前缀验证，允许普通循环体内的
fresh heap If 局部申请证明。同一次调用结束子 loan 后，仍由
原表达式末尾结束父 proof；自然回边与显式跳转仍核对活跃证明。
初版红转绿通过 macOS 的 SSA verifier 和 native 输出 oracle。
独立只读复核确认作用域、回边、控制转移和活跃迭代来源守卫，
并指出原用例的 continue 和 break 未从真实借用路径进入。
现扩展为五种运行模式，分别验证真实重新借用后的 return、
continue、break、自然回边，以及写回 null 的对照。扩大套件及
双宿主公开入口的结果待记录。

同一生产补丁在 macOS 初版专组通过 79 项、0 失败、0 忽略。
五模式公开探针用 `own mode: Int` 驱动控制，同一编译器的
单文件和跨文件项目都通过 check 和 run，退出 0，stdout
按预先写定的 45 行 oracle 一致、stderr 为空。`mode: Int`
默认借用会触发本来保留的非 Boolean 环境借用边界，产生
UnsupportedNode。候选未放宽该守卫。两次非空写回产生的
Node 在 stdout 中无独立身份，复核以源码绑定与 SSA 验证补足。
五模式测试源码清单 SHA-256 为
`84237f060b1791ef19ad3dd51e832d12002cd451aa4474fb3f610a76b483ad09`。

同一清单的 macOS 五模式正式回归通过，SSA 验证与本机
运行均成功，1 项通过、0 失败、0 忽略。另以两组公开入口
补跑有 String 标签和 deinit 的载荷。两入口均通过 check、
退出 0，stdout 与独立预期一致。可区分 `old`、`new-1`、
`new-2`，每次写回先析构旧值，再借入当次新值。在 return、
break 和正常循环退出中，当前值的析构也按各自路径核对。

同一清单的 Linux 定向套件通过 79 项，零失败或忽略。较早的
Linux 试跑使用默认借入的 `mode`，78 项通过、该回归在原守卫
拒绝。换为 `own mode` 的最终清单完整复跑。离线镜像内
`llvm-config` 显示 LLVM 21.1.8，需在一次性容器内额外为
llvm-sys 提供同一二进制的 `llvm-config-21` 名称。macOS 的
codegen 全目标严格 Clippy 和格式检查通过。

最终 Linux 单文件与跨文件项目的五模式输出与 macOS 逐字节
一致。带析构标签的补充探针也在双宿主、两个入口各通过，
73 行输出与预定 oracle 一致，退出 0、stderr 为空。
候选的三份变更已纳入本地复审分支，集成树与上述源码清单
逐文件核验一致。GitHub 远端 main 仍为 `6cc03907`。
基于该清单的双宿主完整公开 conformance 已启动，历史全套
及 PR-event 证据仍按实际输入身份记录。

## 394. R5 循环写回候选的完整公开 conformance 与资源复验

采用 §393 的集成源码清单，核对两端编译产物及相同
`6cc03907+84237f06` 输入身份。离线 Linux x86_64 先完成
原完整 conformance 门禁与证据复验。CLI SHA-256 为
`1a67f9d42b5b8b38507b8f2322ead9a4980d7865828d269a51b4ff44eaa7bc03`。
能力探针为 137/154，综合程序 0/6，23 项已知未实现，内部错误、
回归、入口差异及错误输出均为零。与 §376 上一 Linux 候选逐项
核对，全部 160 个输入的源码哈希、分类、两个入口的退出和超时
状态、诊断标记及 stdout 哈希一致。macOS arm64 的同一矩阵
和证据复验也已通过，CLI SHA-256 为
`d4ffdaf7edcc372398d8b62e9809c5091dcf9824d24464d9266384af1c0bd93e`。
官方比较器在相同完整候选身份上确认 Darwin/Linux 一致，能力
均为 137/154，综合程序均为 0/6。

资源门禁需读取真实源码身份，生成资源 checker 还从 Git HEAD
创建受控对照工作树。沿用 §386 的独立 fixture，将本轮三份变更
写入隔离测试快照 `a2db86b4cfe57f547fbd1d3cdcaa59e2631043b0`，
tree 为 `420e34f4a2dc53f73e9fc966a0af39128ff232c6`。
该快照干净且逐文件符合 §393 清单，开发分支没有 fixture 提交。
Linux 原 ASan/LSan 门禁已通过。三组真实 Koven 插桩与
attribute-off 对照、12 种检测开关或故障组合、Map 正控和
12 个 provider 程序全部符合预期。Linux 生成资源门禁亦
退出 0，`status=pass`、`requirements_met=true`。16 个程序、
校准和 checker 变异验证全部通过。缩减用时 70.41 秒，把原
9 步的重复故障减至 5 步，连续确认三次且通过单步最小化。
Linux exporter SHA-256 为
`e29376704295416887c27043afac3903c56d431fcbd83cd0340763bdd5dca3c2`。
同一 fixture 的 macOS 生成资源门禁使用 §393 正式回归的
exporter，沿用原脚本和门限，退出 0。16 个程序、checker 和
缩减均通过。缩减用时 102.04 秒，连续确认三次并通过单步
最小化。校准与整套报告保留脚本定义的 `status=partial`、
`requirements_met=false`，原因是 macOS ASan 不支持、泄漏
仅用计数器验证。Linux ASan/LSan 另有本节完整证据。macOS
exporter SHA-256 为
`c901f68ad17a8b99878aa65666604724bd4e1148d16424f47fede9d0aa87dd35`。
§387、§391 的历史首次执行失败记录继续有效。上一份 macOS
codegen 全套仍按实际运行身份跟进。Rust 物理行数门禁通过，
2624 个手写文件，三处已有例外。

## 395. R0 旧入口的显式别名与间接引用清点

复用仓库 lowering 依赖门禁的 Rust token 扫描器，在 §393
集成树的 2624 个手写 Rust 文件查四个准确标识符。忽略注释、
字符串和字符字面量，逐条区分定义、use 声明、调用形状和其余
引用。统计为名称 token 次数。同一文件多次调用分别计入。

| 名称 | 定义 | 导入或重导出 | 直接调用形状 | 显式别名或裸函数引用 |
|---|---:|---:|---:|---:|
| `resolve_names` | 1 | 80 | 201 | 0 |
| `check_types` | 1 | 75 | 153 | 0 |
| `check_ownership` | 1 | 64 | 107 | 0 |
| `lower_scalar_file_with_entry` | 1 | 2 | 5 | 0 |

旧 lowerer 的重导出为 `cfg(test)` 下的 `pub(crate)`。
另外三个函数所在模块是公开的，两个 crate 都继承工作区的
`publish=false`。Cargo manifest 中真实仓内下游为 CLI、LSP
和 codegen，依赖位置与 §349、§371–372、§378、§383 一致。
仓外通过 Git 或本地路径引用仍无法从本仓核销。本扫描不展开
宏、生成文件或动态构造的引用，也不把词法计数当作 rustc
调用图。当前未看到需要追加处置的显式别名或函数指针入口。

## 396. R1 完整文件中的跨行 when 恢复对账

沿 §316 和 §375，以 §393 的前端库调用真实 lex 与
`parse_file`。为 break、continue 与 LF、CRLF 分别验两条
合法路径及四条损坏路径。合法路径覆盖独立 selector，以及
去掉箭头后仍能形成的合法前一臂续行。损坏输入分别缺 selector
的右括号、缺内层 if 条件右括号、缺箭头右侧表达式、插入
意外的 `@`。每个文件有非 ASCII 注释与两个独立后继声明。

两个宿主各 24 个输入。每个输入独立解析两次，完整 Parser
产物稳定。所有输入都保留 `else -> 3` 和两条后继声明的
精确范围。四组合法独立 selector 保持前一臂为跨行
`produce(1)`，四组合法续行按真实箭头边界分成两臂。
16 个损坏输入产生诊断，输出的前臂范围和诊断码在双宿主
逐字节一致。缺内层条件右括号和意外 token 会先吸收到前一臂
以恢复，未吞掉后续 arm 或声明。本次为定向审查，解析器和
正式语料无需改动。

## 397. R2 subjectless when 首个已知匹配后的非空事实

用 §393 的同一生产源码在 macOS 和离线 Linux 核对 17 个输入，
单文件与实际跨文件项目共 34 个检查结果，两宿主逐字节一致。
最小复现在 `x != null` 已成立后使用
`when { true, clear(&x) -> { return x }; else -> { return 0 } }`，
其中 `clear` 接收 `inout Int?` 并写入 null。两个公开入口都
在 arm 内的 `x` 报唯一 L0084，实际未到达写回。改成等价
`if (true || clear(&x))` 通过。把第一个条件换成 false 时，
后续写回真实可达，L0084 符合预期。另确认首个 true arm 后
另一 arm 的写回会污染 when 后继。旧逐文件 `check_types` 的
最小 true、false 输入亦均报 L0084。

隔离回归复用 `while_flow_facts` 的类型和位置 oracle。基线
红测把已知匹配臂的 `x` 记录为 Error，预期 Int。Unit 已有
布尔 pending 分边，但准备 arm 类型环境时按覆盖域复制
可能已被后续写回清掉的 flow facts。旧 Single 在相似位置
也沿覆盖域合并，而 pending 可达性另算。后续修复需用已
证明的成功边与失败边约束真实事实合并，保留对不可达源码的
诊断和原穷尽性规则。复合谓词 `flag || (clear(&x) && false)`
在 if 和 when 内同样保守拒绝，应与已知常量首个匹配的复现
一并核对其应有的事实边界。本次记录红测，尚无生产补丁。

## 398. R2 隔离候选的复审发现

§397 初版隔离候选用现有语法布尔结果排除 subjectless when
的已知不可能匹配或未匹配边。macOS 的 `type_checking` 103 项、
`while_flow_facts` 20 项均通过，正负回归分别保留 Int 与 L0084。
本机 CLI 的同臂与后续臂探针均验证该局部修复。另从已构建的
本机二进制实际运行四个集成目标，最终准入 63、所有权迭代 221、
闭包 75、整数区间迭代 2 项均通过。整套调试构建因磁盘余量
停止在编译阶段，回收本次生成的临时产物 2.36 GB，未计为通过。

独立复审提出的两个边界已实际确认：

- `flag && (when { true, clear(&x) -> true; else -> false })`
  作为外层逻辑右操作数时，共享写入扫描会重计不可达的
  `clear`。扩展红测确认后继 `x` 仍为 Error。原子 when
  内的事实修复不能闭合此组合。
- 对返回 Int 的函数，唯一正常路径在 `when { true -> return 1 }`
  中提前返回。初版候选让前端通过，但原生生成仍为
  InvalidModel/MissingFact。加入空 else 仍复现。Refine
  普通 selector 生成了假边，未和前端的静态可达性对齐。

另实跑旧 Single 版本见证。外层以可空 Boolean 为 subject，
内层 `when { true, clear(&x) -> true; else -> false }`
作为 selector 时，新 Unit 通过，旧 Single 仍杀掉外层的
`x` 窄化并报 L0084。死备选更新了旧检查器的版本，匹配边
只保存了事实。将首个 true 换成 false 仍应保留该错误。

后继工作需让共享写入扫描尊重 selector 的有序求值，按
实达边合并事实及版本，并协调返回完整性与 native CFG。
保留覆盖和穷尽性规则的边界。候选、正负及组合回归仍在
隔离目录，开发分支仅更新本账本。

## 399. R2 已知 selector 的写入、版本和原生出口对齐

在 §398 的隔离复审后补齐三处接线。共享写入扫描按顺序只
扫描可求值的 selector。确定未命中的 selector 本身仍求值，
只跳过不可达的臂；确定命中后不再扫描后面的备选或臂。旧
Single 在每条匹配边保存 facts 与版本，只把正常出口合并。
Refine 的 subjectless selector 启用字面量折叠，保留之前的
nullable 视图准入。取反交换真假目标，逻辑组合复用既有短路
与清理。独立复核先发现视图参数无法打开折叠，纠正后复审未
发现本片阻断。

macOS 四个前端定向目标共 484 项通过。新增真实写回负控
单独重跑，20 项通过。相同前端库的旧 Single 嵌套 Boolean
版本探针验证 true 路径无诊断，false 路径仍为 L0084。
两个 §398 的原生 return 用例在单文件和项目出口都输出
`ok`。额外原生探针在单文件和跨文件项目中确认已跳过的清除不输出
`clear`，已执行但确定未命中的 selector 仍输出
`clear\ncleared\nok\n`。全部 14 个 check 与 10 个
native 运行符合独立预期。各入口证据按实际尝试保留。

隔离候选源码清单 SHA-256 为
`f145dd7e909541c595aeb8698870fb3a22522c6ef4c6b95fef68a9b486895717`。
最终两宿主各通过 484 项前端定向与 6 项 codegen 定向。
macOS 的 Refine 控制流扩大 31 项与前端、codegen 全目标
严格 Clippy 通过。双宿主的 14 个公开 check 和 10 次 native
运行逐字节对账一致。七份变更已纳入本地复审分支，3287 个
源码清单条目逐文件符合冻结候选。新一轮双宿主 conformance
已启动。未知结果的复合
谓词仍需把真实写入按布尔成功／失败边传递。当前实跑的
`if (flag || (clear(&x) && false))` 和同形单 selector
`when` 仍保守报 L0084，未将本片当作全部组合已闭合。

## 400. R2 运行时复合谓词两边的写入边界

§399 集成候选上的失败回归复用 `while_flow_facts` 的实际
类型和位置 oracle，将真假两条路径分别钉住。`flag ||
(clear(&x) && false)` 的命中臂只可能来自无需执行 clear 的
左操作数，false 路径会执行 clear。若把右操作数的 false
改为 true，命中臂重新包含实际写回。对偶的 if 形态
`flag && (clear(&x) || true)` 在 false 边跳过写回，改为
`flag && (clear(&x) && false)` 后 false 边可含写回。

红测精确定位到匹配臂读取 `x` 时收到 Error、预期 Int。
后续须复用共享的布尔边写入判定及真实调用事实，恢复只在
相反结果边上失效的旧窄化，同时禁止从漏扫或未完成的写入
信息恢复事实。本节未更改开发分支的复合谓词行为。

隔离探针直接调用现有 `collect_normal_expression_mutation_targets`
及 `writes_on_boolean_edge`。六组真实 Parser AST 用例覆盖对偶
短路、取反、常量结果及左侧另一根的写入。真假边的源级目标分别
符合预期，包括“假边写 x、两边都写 y”。因此可复用现有判定，
仍需验证类型检查中活跃调用、写回版本和 trial 回滚的连接。
该探针未修改集成源码。

§399 集成候选的 Linux 完整公开 conformance 与证据复验
已通过，137/154 探针、0/6 综合程序，23 项已知未实现，
零回归、入口差异、错误输出或内部错误。macOS 同一身份的
完整门禁及证据复验也通过，官方比较器确认 Darwin/Linux 一致。
GitHub main 已复核，仍为 `6cc03907`。

## 401. R2 复合谓词的分边事实与右操作数

在隔离候选中复用现有扫描器按目标身份取所有可能的写入，
仅恢复当前结果边上所有同根写入均已排除的旧 facts 和普通
版本。两套 checker 都接入 if 真假边及 subjectless when
命中／未命中边。原 pending 集合仍按各自布尔边合并，没有
新增跨表达式缓存。扫描前确认普通版本确有变化，逐键要求
存在版本增量，并保留永久失效标记。

首个候选的四个前端目标在两宿主各通过 485 项。macOS
单文件和真实跨文件项目的 when 正控输出
`hit\nmiss\nclear\nok\n`，对偶 if 输出
`hit\nclear\nmiss\nok\n`，各退出 0、stderr 为空。
独立复审确认原分支接线，另发现逻辑 RHS 自己仍会读到
左侧合流事实。新增回归在该入口确认 NullableInt，预期 Int。
接线后首轮 macOS 扩大至 486 项均通过；旧 Single 的逻辑
合流保留两条路径版本的最大值。

加固后的生产源码完成双宿主前端定向，各 486 项通过。
三项原生程序的单文件和真实跨文件项目共 6 次 check、6 次
run，两宿主的 12 行结果逐字节一致。RHS 探针在左边为真时
保留非空读取，为假时执行 clear，输出
`hit\nmiss\nclear\nok\n`。旧逐文件检查器直接验三项正控
都无诊断，同根写反例仍为 L0084。独立复审确认普通版本合流、
pending 和 trial 回滚，指出父测试文件超过 1000 行，已将新
复合谓词用例放到现有子目录并补 `||` 和 RHS 真实写的对照。

生产源码与最初加固清单一致，测试布局拆分后的最终源码清单为
`7bd5f5631bbd23280937290bdedb839642ec9d907c14c187ce8b13723f3b782f`。
两宿主最终流事实 suite 各 22 项通过，macOS 前端全目标
严格 Clippy 和格式检查通过。11 份变更已纳入本地复审分支，
3289 个源码清单条目逐文件与验收输入一致。双宿主完整公开
conformance 已按该清单启动。复合 if／when 作为不透明布尔
操作数仍保守处理，长左结合链的新增版本快照和重复扫描未做
单独性能测量。

## 402. R2 集成候选的一致性、尺寸和短路扫描计数

§401 源码清单对应的 macOS arm64 与 Linux x86_64 公开 conformance
经比较器确认一致。能力探针 137/154，综合程序 0/6，已知未实现
23 项，内部错误、回归、入口差异、错误输出均为零。Linux 与前一
候选的 160 个输入逐项核对，分类、超时和退出状态、诊断标记、
源码及 stdout 哈希一致。

Rust 尺寸门禁发现 codegen 的 `control_flow_tests.rs` 从 991
增长至 1007 行。将新增的已知命中 selector 测试移到测试子模块后，
父文件为 994 行，门禁通过。独立核对拆分后的清单
`2f0108009139dd865dc7533c641c28b79be7128216d171cd62ddf528b989cdb3`
共 3290 项，生产源码维持上述 conformance 的输入。
拆分后的 SSA 测试随后在 macOS 以新模块注册路径定向复跑通过。

在隔离候选的 unit 类型入口测量 `bypassed_result_writes`。
追加 8、16、32、64、128 个短路项，普通版本未变时扫描和访问
计数均为 0。链首一次写回时扫描次数分别为
9、17、33、65、129，节点访问为 456、1340、4452、16052、60756。
默认 Rust 测试线程在 128 项处栈溢出，上述完整计数来自仅用于
诊断的 16 MiB 栈。重复扫描与宿主栈分别跟踪。

## 403. R3 长短路左链的析构规划器栈溢出

由变量 `flag` 和 130 个 `||` 写成的合法 if 条件在旧候选的
macOS arm64、Linux x86_64 公开 `check` 上触发 SIGABRT，
单文件和项目入口均复现。macOS LLDB 回溯反复经过
`DropPlanner::expression_inner`、`evaluate_owner_expression`、
`expression`、`short_circuit`。新增公开进程回归在旧二进制上失败。

析构规划器改用显式栈保存直接左链的节点。左操作数完成后逐节点
执行原有 RHS 和合流，同时完成每节点的投影作用域与 owner
observation。独立静态复审核对普通和常量分支、作用域恢复、
不续行与错误传播，并确认提前识别逻辑节点与由调用、成员访问
或枚举构造发布的 typed descriptor 没有有效节点重叠。

macOS arm64 和 Linux x86_64 的 `check_cli` 各通过 8 项，
析构规划器 lib 筛选各通过 482 项，2 项既有忽略。130 个 `||`
的单文件和项目原生运行均退出 0，输出 `false-ok\ntrue-ok\n`。
macOS 前端 lib 严格 Clippy 通过，Rust 尺寸及文档门禁通过。
双宿主单文件和项目的长链资源探针均通过预期输出逐字节对照。
左侧为真时跳过 RHS 和临时构造。左侧为假时 RHS 执行一次，
其临时资源在进入 if 之前析构一次，外层资源在函数退出时析构一次。

macOS 下 256 和 512 个 `||` 的合法单文件
`check` 仍触发 SIGABRT。256 项的 LLDB 回溯定位到
`Checker::check_short_circuit` 与 `check_expression_with_escape`
之间的递归。同一公开入口回归改用 256 项在隔离测试中确认失败，
Linux 的 256 项单文件探针亦以 134 退出。
保留 R3 阻断状态，继续检查所有权流、liveness 和
下游阶段在扩大链长时的行为。

## 404. R3 短路左链的所有权流检查

以 256 个 `||` 的同一公开入口回归锁定 §403 剩余中止。
Unit 流检查改用显式栈完成左操作数，随后按内到外顺序应用
原有 RHS 策略和 Flows 合并。常量计划仍在访问左侧前登记，
每节点原有 callable provenance 和借用交付上下文按原顺序恢复。
独立静态复审检查了 typed descriptor、常量优先、正常与提前退出、
错误回传和作用域恢复。复审发现的过时 AST 分派说明已更新。

macOS arm64 与 Linux x86_64 的 `check_cli` 各 8 项通过，
所有权 lib 筛选各 798 项通过、14 项既有忽略。macOS 前端 lib
严格 Clippy 通过。两宿主 256 个 `||` 的
单文件和项目原生运行均退出 0，输出 `false-ok\ntrue-ok\n`。

macOS 的 512 和 1024 项变量短路链仍触发 SIGABRT。
512 项的 LLDB 回溯定位到 R2 Unit 类型检查的
`check_logical_binary` 与 `check_expression_with_expectation`。
同一 CLI 回归在隔离测试中改为 512 项后验证失败。已建立 R2
隔离候选，继续核对左链 facts、pending 退出和活跃性边界。

## 405. R2/R3 的 512 项公开检查与 R4 原生边界

Unit 类型检查沿未缓存的直接逻辑左链收集节点。下钻期间没有
流状态变更，所有需恢复写回的节点共用入口快照。各节点求值后按
原顺序发布 nullable、类型与完成性，根节点仍走外层 expected
处理。独立复审确认缓存边界、版本、RHS 和未完成路径语义。
`condition_facts` 仍递归读取左前缀，§402 的重复扫描尚待处理。

类型候选编译后，512 项 CLI 用例仍失败，LLDB 定位到
`liveness::Builder::expression`。活跃性改为在保留每节点
`expression_after` 的同时按策略处理 RHS，再迭代左侧。
独立复审核对 index roots、常量提前结束、RHS 三种可达策略、
普通和诊断性描述符及未改的退出处理。

macOS arm64 与 Linux x86_64 的 512 项单文件、项目 `check`
回归通过，`check_cli` 各 8 项。两宿主 1024 项单文件
`check` 退出 0。两宿主五个定向 suite 共各 535 项通过，
覆盖 `while_flow_facts`、`multifile_type_checking`、基础 Unit
及 multifile 普通和常量所有权。macOS 前端 lib 严格 Clippy
通过。聚焦清单
`a0067013e7105a7ebbc0f9358b68053cbbb81dd177ad58ccd207803e50cc66e3`
在纳入本地分支后逐文件核对。

同一候选的 512 项原生 `run` 在两个宿主的单文件、项目入口
均以 134 退出。macOS LLDB 回溯定位到 R4 兼容路径
`UnitExpressionLowerer::lower_short_circuit` 与 `lower_expression`
之间的递归。保留跨阶段阻断项。下一候选须核对 SSA 合流、
每节点 `AfterExpression` 和 `BranchExit` 清理、常量计划与
两种公开入口的原生输出。

## 406. R4 兼容 lowering 的 512 项原生回归

在现有原生 CLI 套件加入 512 个 `||` 的真实 build/run。
旧候选在单文件 build 以 SIGABRT 终止，回归首先失败。新候选在
原 guard、常量与描述符分派后登记逻辑节点，显式栈按内到外
恢复 SSA 生成。每个左操作数经过既有 `AfterExpression` 清理与
非空证明退役后才交给父节点。原有 raw `lower_expression`
入口的根清理边界、常量计划的操作数/分支身份检查、RHS 提前退出
及 `BranchExit` 编号保持原顺序。独立静态复审批阅了上述路径。

macOS arm64 与 Linux x86_64 的原生 CLI `while_flow` 各
9 项通过，codegen `short_circuit` 筛选各 99 项通过。
512 项单文件和项目都输出 `false-ok\ntrue-ok\n`。两宿主
的 1024 项纯布尔单文件原生探针也逐字节匹配预期。

512 项资源探针加入共享实参上的临时 Guard。双宿主单文件
原生入口仍触发 SIGABRT，macOS LLDB 定位到 Refine 的
`Builder::logical` 与 `condition_internal`。该路径已建立
隔离修复和独立原生回归，跨阶段阻断项继续跟踪。

## 407. R4 Refine 动态左链的资源清理

Refine 动态短路先按外到内建立 middle/skip 块，完成左操作数后
按内到外恢复原分支 span、proof、临时资源和清理合流。
独立静态复审确认块顺序、左右入口、提前退出及 drop/loan 交付。
512 项带 Guard 的真实 build/run 在 macOS arm64、Linux x86_64
的单文件和项目入口均通过。短路侧不创建 RHS Guard，执行侧在
进入 if 前析构 RHS Guard 一次，外层 Guard 在函数退出时析构一次。
Mac 同时通过纯布尔回归，Linux 原生 CLI `while_flow` 10 项通过。
上述候选的两份新增差异逐文件核对后纳入本地分支，R4 四文件清单为
`87f69b5da3fbca8416e0e4037245141c1b3364ec9dbff4dc1ea5ce8078ee6164`。

本次 Mac 筛选耗时 225.75 秒，Linux 九个邻接用例与资源用例共
290.69 秒。采样显示 Mac 项目构建停留在 SSA
`verify_dominance/compute_dominators`，记录为后续性能审查证据。
另一个 512 项资源用例将链首改为 `true`，在同一候选的两个
宿主上仍以 134 退出。Mac LLDB 确认
`prepare_logical → required(left) → condition_value` 的
递归。此形状保留为 R4 阻断项，后续核对 bool 值构建、
`pending_views`、清理与原生退出。

## 408. R4 Refine 已知短路左链与 R3 括号边界

将 §407 的 `true` 起始变体加入公开原生回归。旧候选在单文件 build
复现 SIGABRT。新候选为需要左值的短路节点保存原值合流和剩余动作，
从内到外恢复 `pending_views`、贷款和每个表达式的清理。普通固定边、
Const Always/Never/Conditional 和无正常出口都使用原计划和目标。
独立静态复审批阅了各分派、发散和恢复步骤。

512 项带 Guard 的已知短路用例在 macOS arm64 与 Linux x86_64
单文件和项目入口均通过。两条运行路径都跳过 RHS，外层 Guard
各析构一次。macOS 另实跑 512 项必经和必跳过的 `&&` 对照，分别
确认右侧只执行一次和完全跳过，输出与析构顺序匹配。

扩大形状时，两宿主的 64 层带括号左链可以通过公开 `check`，
128 层带括号左链和 64 层带括号右链均因栈溢出中止。Mac 的
64 层左链原生输出匹配，两个失败形状的 LLDB 回溯都在 R3
所有权流检查，分别涉及 Group 分派和短路 RHS。合法源码复现与
受控故障注入分别记录，R3 仍保留跨层阻断。

## 409. R4 SSA 支配集合的逆后序迭代

§407 的资源回归采样将编译耗时定位到 `compute_dominators`。
以可达块的确定性逆后序替换按块 ID 扫描。入口、不可达块、
可达前驱交集和诊断报告顺序沿用原合同。独立静态复审确认
自环、回边、重复目标、不可归约环与原转移收敛到同一支配集合。
独立四节点图模型遍历 65,536 份有向图，与逐一移除候选支配节点
后做可达性检查的定义一致。模型证据与编译器实跑分别记录。

两版各在两个宿主运行同一组 11 项原生 CLI `while_flow`，
全部通过。测试阶段实测 macOS 207.94 秒降至 21.27 秒，
Linux 289.31 秒降至 15.92 秒，均不含 Rust 构建时间。
这些是本轮整组耗时。512 项布尔及资源用例仍逐一断言
单文件、项目输出和析构顺序。纳入本地分支后核对四项文件清单
`5baa3843a06e42da311b617cb28b5d78763c2f55149c46b5e920848f7b9a1c3d`。

## 410. R3 括号短路链的恢复与双宿主原生验收

两条公开回归在旧二进制上分别复现 128 层括号左链、64 层括号
右链中止。Unit 所有权流检查保存 Group、短路左操作数和右操作数
的续行状态，不再从这些节点递归进入同一检查入口。左侧完成后，
必经 RHS 取走正常后继，条件 RHS 克隆并保留旁路，已跳过 RHS
不进入检查。Group 保留原 typed 分派、用途和 escape 参数。
独立复审核对了优先级、正常与跳转合流、错误传播和上下文恢复。

隔离候选的 macOS arm64 两条公开回归和 Linux x86_64 的
`check_cli` 10 项通过。随后两宿主分别运行 128 层左链、
64 层右链的单文件和项目 Guard 程序。八次 `check`、八次
`run` 均退出 0，stderr 为空，原生 stdout 全部为
`19908089bed0a0c34f7f4be30828487c064f265556b06cfbd0d943ebf0b73b7b`。
跳过边不构造 RHS Guard，执行边先输出 RHS，再析构该临时一次，
随后进入 if，外层 Guard 在每次调用退出时析构一次。

故障注入继续由 CI 承担。当前交付基线转向 #445 的
`eb0eaa30`，以上旧基线验收保留，合并树按新身份重新验证。

## 411. #445 基线与 192 层括号右链

基于 #445 合并提交 `eb0eaa30` 应用本轮候选，Git 三方应用
没有文件冲突。独立复审核对九个重叠文件，覆盖 callee 求值顺序、
callable join 的表示、临时容器和普通路由边界。首次候选
`dbf7077063e32d29062927716658a7d6ec6d694bc6dc2e87eb6cbf6a3fa0b011`
两宿主定向前端各 543 项、`check_cli` 各 10 项、原生
`while_flow` 各 12 项，公开 conformance 比较器确认
137/154 能力探针、0/6 综合程序、23 个已知未实现，零回归。

扩大到 128、192 层括号右链时，旧候选在 macOS 的公开
`check` 再次中止，LLDB 定位到 R3 析构规划器处理 RHS。
析构规划器保存 Group 与 RHS 的求值状态，保留普通分支在
传播错误前的原 ValueState，并沿原顺序完成析构及 Rc 投影合流。
独立复审核对各策略、错误恢复、Group 所有者和观察回调。

R3 修复后 192 层的公开检查通过，macOS 原生入口仍在 Refine
的 RHS 条件生成中中止。Refine 按待处理条件与分支状态迭代，
Group 和 Not 仍传递原 span、目标与 witness；独立复审核对
已知计划、普通分支、提前退出和资源/借用状态。两宿主在新候选
各通过 10 项 `check_cli` 和 12 项原生 `while_flow`。
其中 128 层左链、192 层右链都在单文件和项目入口逐字节
核对 stdout、空 stderr、退出状态及 Guard 析构顺序。
macOS frontend/codegen lib 严格 Clippy 通过。最终代码与
验收工作树逐文件一致，非文档清单共 3621 项，SHA-256 为
`d74fa1fb1090c81dc51c57dc7236dafa7274e3648eef182a879ae0f70002321f`。

## 412. PR #448 的 body-only Boolean 测试输入

首个 PR-event head `4cbaf35f` 的 Ubuntu Targeted Tests 在
bounded composition 中报告 3495 项通过、1 项失败、77 项忽略。
失败项原用字面量 `true` 作 selector，同时断言有一个
`Conditional`。已知 selector 按本批控制流事实折叠，实际为零。
[首轮 Ubuntu 作业](https://github.com/Halckon/Koven/actions/runs/44455505167/job/135329751111)

将 selector 改为运行时 Int 参数的比较。Boolean 仍只来自函数体，
保留跨文件输入重排、完整 SSA 类型表和恰一个动态分支的断言。
修改限于测试输入及账本。macOS 定向复跑通过，原生
conformance 对应的生产源码保持一致，最终门禁交给新 head 的
PR-event 双宿主 CI。

## 413. `9d9bc3d8` 对齐后的 R0/R1 增量核验

以 #479 合并提交 `9d9bc3d8` 为本轮基线。#448 的最终
PR-event head `757aca35`、base `eb0eaa30` 对应合并验证树
`bc051c65`。GitHub 记录 macOS、Ubuntu Targeted Tests、
Code CI Evidence 和 CI Passed 全部成功，实际合并提交为
`15955ffb`。[PR #448](https://github.com/Halckon/Koven/pull/448)
· [验收 CI](https://github.com/Halckon/Koven/actions/runs/44457419954)

R0 对比 #448 合并提交与新基线，在 `crates` 各 crate 的 `src` 树中
按源码调用形状检索 `resolve_names`、`check_types`、
`check_ownership`、`lower_scalar_file_with_entry`。命中文件
从 60 降至 50，移除的 10 份都在 codegen，无新增命中文件。
提交 `d7d841d8`、`0c23d232` 和 `e97b6876` 分别记录
私有 Single 入口、引擎和旧 mapper 适配器的退役。旧 lowering
入口名称目前仅见于禁止依赖的测试匹配。Unit 的 Compatibility
路由仍由 `planned_body.rs` 分派。

四个 codegen 测试文件仍直接消费旧前端产物：

| 文件或模块 | 保留用途 |
|---|---|
| `lower_frontend_owned_source_continue_tests.rs` | 旧前端所有权诊断与事实见证 |
| `map_source.rs::canonical_callable_tests` | 调用目标与具体签名身份 |
| `unit_llvm_tests.rs` | Single callable planner 的来源对照 |
| `while_flow_tests::proof_cases::if_cleanup_observation` | Single/Unit 的存储形态和清理事实比较 |

CLI 与 LSP 生产源码未发现这四个名字的直接调用形状。以上是
源码搜索与模块归属核验。宏展开、生成代码及仓外依赖仍开放。

R1 复用 [#477 F5 CI](https://github.com/Halckon/Koven/actions/runs/45797758882/job/139832703686)
和 [#477 F6 CI](https://github.com/Halckon/Koven/actions/runs/45797758634)
的已验收证据。F5 是语法变更文档的增量校验。F6 的 Lean 64 条
定理和 Kani 24 个有界 harness 对应十片局部合同，涵盖分组所有者、
硬分隔符传播、Pratt 优先级、试探和诊断锚点等。#477 合入后的
Lexer、Parser、formal/parser 与 F5/F6 工作流到本基线无内容差异。
局部证明的输入边界按原账本保留。

在 `9d9bc3d8` 的 macOS arm64 工作树运行生产 Parser 的四组
集成测试。`parser_block_line_continuation`、`parser_control_flow`、
`parser_error_propagation`、`parser_statement_separators` 共 59 项
通过，零失败或忽略。覆盖块所有者的真实分隔符、零宽 L0169、
后继元素与声明保留、跨行 selector 和 break/continue 的归属。
本次未重跑 Lean/Kani，也未在本地执行故障注入。

## 414. R2 短路左前缀的结果边写入摘要

在 `9d9bc3d8` 复现 §402。以一个 `inout` 写回开头，追加
8、16、32、64、128 个变量短路操作数。临时诊断计数累加两个 Unit
来源的保守 AST 收集和结果边筛选工作项。写回仅能发生在相反结果边时，
基线访问数为 918、2686、8910、32110、121518。可能在选中边写回时，
访问数为 972、2788、9108、32500、122292。无写回对照始终为 0。

Unit 检查器在流版本变化时构建直接短路左前缀的写入摘要。外层
节点沿既有布尔语法可达性，把本层右操作数纳入摘要。每个结果边仍只
恢复该边无法写入的稳定 binding。摘要仅存于当前左链遍历，脱离
适用条件时丢弃，保留原试探与完成性边界。独立静态复审核对了结果边
转移、已知结果、跳转、稳定身份和试探边界，并要求把长链回归的读取
移到最后一个右操作数，确保直接检验摘要给 RHS 恢复的事实。

临时计数的候选中，两组写回复现的访问数分别为
218、346、602、1114、2138 和 230、358、614、1126、2150。
无写回仍为 0。计数只覆盖保守收集和结果边遍历，使用 16 MiB
测试线程栈以便与 §402 的诊断方式对齐。计数插桩已撤除。
当时的 `condition_facts` 仍递归重算左前缀，后续见 §415。
不同写入根数随链增长时，结果集合复制的成本仍会增长。

macOS arm64 默认测试线程栈下，`while_flow_facts` 23 项、
`multifile_type_checking` 298 项通过，均无忽略。新增 128 项尾部
读取见证在两个 Unit 来源覆盖 `&&` 真边和 `||` 假边的接受与拒绝，
并核对拒绝时的 L0084 及源码范围。另复跑 `control_branch_join` 9 项、
`final_type_admission` 63 项、`lambda_completion` 4 项、
`type_callable` 47 项，全部通过且无忽略。前端 lib 与两组定向集成
测试的严格 Clippy、Rust 尺寸、F5 文档及文档结构门禁通过。
Linux 和完整原生入口矩阵待复验。本轮未在本地执行故障注入。

## 415. R2 条件事实的迭代遍历和左前缀复用

对齐 GitHub `main` 的 `28076229`，包含 #480 的 parser formal
归档，保留 §414 的本地修复。对比 §414 候选与本节改动，两者均在
同一基线验证。条件事实的遍历改用显式栈，保留分组、取反和逻辑
节点的访问顺序。直接左链在本层右操作数检查结束、节点类型与完成性
发布后累积条件事实，供下一节点使用。逻辑合流沿用原有右侧写回
失效和两条结果边合并。独立只读复审核对了类型与写回事实读取时机、
试探与循环回放边界，并补验完整根条件向 if 正文传播的见证。

在 macOS arm64 上对 128 项链分别验证 `&&` 真边、
`||` 假边的窄化与写回失效。两种 Unit 来源的四组正反例中，
`condition_facts` AST 访问合计由 133128 降到 1036，类型结果
与 L0084 范围一致。临时计数已移除。另验带分组和取反的长条件
向分支正文发布非空事实。此计数只覆盖条件事实访问。复杂右子树
的写入收集和事实集合合并仍有独立成本。

macOS 六组前端集成测试共 446 项通过、零忽略；严格 Clippy
及 Rust 尺寸门禁通过。LLVM 21.1.8 下定向 `check_cli` 3 项、
`native_cli` 4 项全部通过，分别过滤 7、59 项，覆盖长链、带括号
左右链、双入口输出与临时资源析构顺序。另以 128 项带 nullable
`inout` 写回的合法源码实际构建并执行，单文件与项目入口均退出
0、stderr 为空，stdout 均为 `null\nseven\ntaken\nclear\n`。
本轮未运行 Linux x86_64 和完整原生矩阵，故障注入留给 CI。

## 416. R2 短路左链的操作次数门禁

在普通 frontend lib 测试中用真实 Unit 类型检查入口处理合法源码。
分别观测 32 项和 128 项直接左链。两个操作次数门禁都覆盖
`&&` 真边和 `||` 假边，链尾窄化后的实参必须通过类型检查。
条件事实探针以判空比较建立非空事实。写入探针以 `inout` 改写
可空变量，验证选中结果边能绕开写回。两项计数都要求长链实际增加
访问，且四倍输入增长时的访问量最多为短链的五倍。

计数只在测试构建中启用，作用域为触发测试线程的一次同步 Unit
类型检查。独立复审确认探针覆盖非空条件事实、两个运算方向和实际
RHS 消费。双宿主 Targeted Tests 的现有 `check_integration.sh`
经 `check_core.sh` 运行 frontend lib，因此无需另设 CI 接线。
本项复审不涉及另线推进的 D10、E2、E3 开发；故障注入由 CI
既有步骤执行。

macOS arm64 实跑，同一门禁在未修复的 `28076229` 上两项均按
增长阈值失败。首个 `&&` 用例的条件访问从 1024 增至 16384，
写入工作项从 5035 增至 69259。基线构建仅加入观测计数与测试，
两项均在增长断言处失败。候选的 `&&` 和 `||` 均通过，
条件访问为 32 → 128，写入工作项为 137 → 425。两项计数只约束
此单写回根的直接左链，保留 §415 的右子树扫描与事实集合成本边界。
相关 `while_flow_facts` 套件复跑 25 项通过，无忽略。

## 417. R5 当前候选的 macOS 公开入口与 LSP 证据

基于 `280762299af21d7c6425b2250891a96aa1333e44` 和 §413–416
未提交候选重新构建 `kovenc`，macOS arm64、Rust 1.96.0、
LLVM 21.1.8。构建及验证前后对照 3829 个编译器、语料与脚本输入。
输入清单 SHA-256 为
`6aeec27a9cd72e8ce389f2f524491f310ca127165cf6601ede99cada453bfd1b`，
验证二进制 SHA-256 为
`e4a408e094e0b624e8ccdacfdf1feb8319329f328a503a2565ecbc4286da0a37`。
证据标识同时包含父提交和工作树清单摘要。原始文件保留在
`/private/tmp/koven-spec0392-public-candidate-nq_6up7s`。

以 `baseline-aef3f4f8.tsv` 和现行 `expectations.json` 执行完整
公开语料。154 个探针与 6 个综合程序经过两种入口共执行 320 次。
探针能力通过 137 项，含正常运行 121 项、预期诊断 13 项和语言层
预期 abort 3 项。17 个探针及 6 个综合程序仍匹配已知未实现合同。
内部错误、超时、错误输出、模式差异及回归均为零。回归门禁通过，
原始证据经 `--verify-evidence` 独立重算通过。基础能力 P2 的
runnable group A 为 58/74，离目标 70 项仍差 12 项。

同一工作树在 LSP 复跑 8 项格式化、1 项源集分阶段诊断及 1 项
真实 stdio 格式化，全通过。另在真实 stdio 的单文档和项目模式
依次打开 128 项条件链、加入条件写回、撤销写回。版本 1 和 3
诊断为空，版本 2 只在最后的 `accept(x)` 实参 `x` 上报告
`L0084`，两次会话均正常关闭并退出 0。

此节为 macOS 单宿主增量证据。冻结候选的 Linux x86_64、双宿主
证据比较、sanitizer 和资源矩阵仍待验收。LSP 本地仅执行上述普通
选集，故障注入留给 CI。

## 418. R4 跨文件 Compatibility 与 Refine 写回

在 `28076229` 及 §417 候选上检查跨文件 callable。普通 Unit
SSA 回归在固定 SourceId 下交换输入文档顺序。入口块各发出一次
临时字符串数组循环和资源字段 `inout` 写回。入口和写回函数均
由 Refine 成功构建，循环函数走 Compatibility。两种顺序的
路由清单与渲染 SSA 一致，均通过 ownership/SSA verifier。
独立复审确认 route 记录来自成功的 Refine 构建且没有强制路由，
并核对实际调用目标和 Unit 文档交换范围。

使用 §417 已绑定的 macOS 原生编译器执行单字段对照。单文件及将
声明与入口拆成不同包、不同文件的项目入口均通过 check/build/run。
stdout 均为 `loop:a\nloop:b\ndrop:old\nafter\ndrop:new\n`。
双字段扩展均先析构旧左、旧右再继续执行，退出时依次析构新右、
新左，每个资源恰好一次。同样的两种布局改为重复借入左字段后，
前端均以 `L0135` 拒绝。主诊断指向第二个独占实参，并标出首个
借用和提供者中的第二个 `inout` 形参。两型原始输入保留于
`/private/tmp/koven-spec0392-r4-crossfile-*`。

两种支持目标的 LLVM 64 位指针布局、本机 object 单一公开入口、
闭合复合类型布局预检及 Unit 容器布局四项定向测试通过。
x86_64 布局在 macOS 上由 LLVM backend 计算，Linux 原生行为待
双宿主 CI。故障注入留给 CI。

## 419. R0 工作区生成目标与源码 include 清点

在 `28076229` 上用 Cargo metadata 逐项核对五个 workspace package。
五个 package 均未声明自定义构建或 proc-macro target，源码未使用
`OUT_DIR` 读取生成的 Rust。五个 crate 的 `src` 树共有三个
`include!`，分别拼入 codegen 的 Array/只读方法 Unit 回归，以及
frontend 的测试环境 helper。三处均位于测试模块，包含文件没有
直接引用 §395 记录的四个旧入口标识符。

CLI 与 LSP 生产源码仍未检出四个旧入口标识符。LSP 两处
`macro_rules!` 分别生成 server 通知错误和 Unit session 错误的
`From` 转换。此为 workspace target、include 和生产入口的源码
清点。外部 proc-macro 展开、间接函数身份及仓外消费者仍开放。

## 420. 同一源码清单的 macOS 与模拟 Linux x86_64 复跑

候选父提交为 `280762299af21d7c6425b2250891a96aa1333e44`。
两端验证同一份 3831 项输入清单，其 SHA-256 为
`6d65d159d6fa8e41af4b78764e3b96bbd4e8da9b312895267a1bd761b568aab8`。
其中包含 §418 新测试与其构建所需的 Guide 和教程。本轮证据目录为
`/private/tmp/koven-spec0392-linux-amd64-b2f1oss_/out`。

Linux 使用仓库原 CI 镜像摘要
`b5ce8a4479579ecc57194e385f13109a52022114f0e19cab93fc426bdbb9500e`。
Docker daemon 为 Linux arm64，容器报告 x86_64/Ubuntu 26.04，
Rust 1.96.0、LLVM 21.1.8 和 CI 工具链校验通过。
容器以无网络及只读源码启动。离线缓存只包含 Cargo.lock 对应的
crates.io 归档与索引，归档哈希逐一对照 lockfile。
首次运行通过 R2 测试后停在 llvm-sys 探测：模拟环境执行缺失的
`/usr/lib/llvm-21/bin/llvm-config-21` 返回 127，阻止探测器
继续搜索。第二个一次性容器新增该名称到镜像已安装
`llvm-config` 的链接。源码、锁定依赖与工具二进制使用原始字节。

模拟 Linux 的操作次数门禁 2 项、`while_flow_facts` 25 项、
定向 `check_cli` 3 项和 `native_cli` 4 项均通过。短路原生
回归覆盖单文件及项目输出、右侧是否被求值与内外资源清理。
macOS 已在同一清单重构 CLI，二进制与 §417 的 SHA-256 一致，
完整公开语料门禁与原始证据重算均通过。

§418 的跨文件路线测试在模拟 Linux 通过。两字段源程序的
单文件和拆包项目输出逐字节匹配 macOS，分别验证旧左右资源写回
时析构、新右左资源退出时析构。重复借入在两种 Linux 布局
均由前端以 `L0135` 拒绝，诊断与两个附带范围匹配 macOS。
初次公开语料调用在校验输入时发现临时快照未带全 Guide 章节。
用与 macOS 相同的 16 份 Guide 文件补齐，
补充清单 SHA-256 为
`33db6ce5514c84972ff33977b1270239d8c8dcebb9bcc82462ec6310341a0a9c`，
复用同一 ELF 编译器执行。

补齐后，模拟 Linux 的完整回归门禁和证据重算均通过。macOS
与模拟 Linux 的共 640 次公开入口调用（每端 320 次）经
`--compare-evidence` 逐项比较通过。两端一致报告 137/154
探针能力通过、17 个探针及 6 个程序已知未实现、零内部错误、
超时、回归、入口差异或错误输出。Linux 的编译器与项目可执行文件
经文件头确认均为 x86-64 GNU/Linux ELF。Linux 证据 JSON SHA-256
为 `1a4a03e7b47d0398e8aae208a9b1a0294460363f453a14bd2d3fdb9b1ca04dc6`，
macOS 证据 JSON SHA-256 为
`7a925e2f118ac14b5e0fdd9d106a5bf7b2bf8588b258a0bce94bf866f93b6c90`。
上述 Linux 证据来自 ARM 宿主中的 x86_64 模拟环境。冻结提交的双宿主
PR-event CI、物理 x86_64 宿主、完整 sanitizer 与资源矩阵仍待验收。
本轮仅运行普通测试，故障注入留给 CI。

## 421. R3/R4 短路条件后的组合退出清理

在 §420 的生产候选上，新增普通源码的原生回归。函数持有一个
外层资源和每轮两个同层资源，短路条件右侧借入临时 Guard。
同一程序依次走以下路径：

- `continue`。首轮结束条件调用后析构其临时实参，再按 second、
  first 顺序析构本轮资源。第二轮执行正常循环体，循环结束后
  外层 owner 仍可读取。
- `break`。条件实参完成清理后按逆序退出本轮，继续执行循环后
  的外层 owner 读取。
- `return`。条件实参先清理，返回表达式求值后清理其临时实参，
  随后依次析构 second、first 和外层 owner，调用方收到 7。

独立复审从 Guide 10 推导上述预期。32 行 stdout 逐字节对照，
用独立标签区分读取和析构。覆盖短路跳过、目标循环、正常回边、
临时借参清理、词法清理与实际返回值。原生输出证明可观察行为，
内部 `EndBorrow` 事件的所有权和 SSA 见证见 §422。

macOS arm64 的新 CLI 回归通过，另外把 Guard 和辅助函数移至
独立包的第二个文件。该项目及单文件均 check、build、run
通过，退出 0、stderr 为空、32 行输出一致。相同循环改为
`borrow val alias = first.label` 的对照在两种入口均报
`L0164`，与 §306 登记的循环体内具名子借用边界一致。

macOS 的完整 `native_cli::while_flow` 选集通过 13 项，严格
Clippy、Rustfmt 和文档检查通过。模拟 Linux x86_64 使用 §420
镜像和生产输入，仅覆盖本节新增的测试文件。定向 CLI 回归通过，
跨文件正反例置于容器本地文件系统时也全部通过。两边的规范化结果
JSON 相同，SHA-256 为
`e0f06e245a0fa5c4c27e6d6165e684e40a8866f73cd0b2201473b3c73cf94588`。
原始日志与输入位于
`/private/tmp/koven-spec0392-linux-amd64-b2f1oss_/out/r3-exits`。

首次跨文件项目在宿主共享挂载上构建时，CLI 已发布 ELF，随后
报告 private staging 目录身份校验失败并保留目录。使用同一 ELF
和源码字节在容器本地目录复跑通过，未定位共享挂载上身份差异的
具体成因。故障注入留给 CI。

## 422. R3/R4 组合退出的 Unit loan-end 与 SSA 见证

§421 的 CLI 原生回归和新增的普通 Unit SSA 回归共用
`short-circuit-loop-exits.ko`。前端见证按源码范围重新定位每个
临时构造和控制转移。真实 Unit ownership 产物中，三个
`observed(Guard(...))` 和一个 `result(Guard(...))` 都为同一
实参发布 Shared loan 的 `CallReturn` 结束事件，结束前的 drop
数量为零，同点随后析构本次的 temporary。continue、break 的
`ControlTransfer` 仅依次析构 second、first，return 再析构 outer。

SSA 回归从对应的真实 DirectCall 实参取 loan ID，经
`BorrowBegin` 和 `RootPlace` 找到 owner，并要求 owner 来源区间
对应同一个 Guard 构造。该调用的同块正常后继中必须先结束这一
loan，再析构这一 owner。独立复审复核了源码、Unit 和 SSA 的
身份链。此断言针对该 fixture 当前的直接调用布局；跨入口动态
输出仍由 §421 验证。

macOS arm64 的所有权及 SSA 定向回归通过。独立的前端产物读取
也复核四个调用点与词法退出事实。

## 423. 重启后的候选恢复与证据复跑

宿主重启清除了原 `/private/tmp` 复审目录、编译缓存和原始证据。
原模拟 Linux 容器及执行句柄均已消失，中断时的 §422 Linux 终态
未知。使用 `28076229` 和会话中的编辑记录恢复跟踪改动与三个
新增文件。按重启前口径重新计算 3833 项输入清单，SHA-256
为 `b6e03b9603b0ae791d03250e0c409f3fac1b6f3bef2adb991798917523c300fd`，
与中断前一致。恢复后的本地补丁、文件归档和重跑日志放在
持久工作区的 `.local-artifacts/0392-recovery`。

恢复后的 macOS arm64 定向复跑通过 30 项、无失败或忽略。
其中操作计数 2 项、流事实 25 项、组合退出所有权/SSA 1 项、
跨文件路由 1 项，以及共享 fixture 的公开原生双入口 1 项。
GitHub `main` 已前进到 `48268b43`。差异为 loan-kernel 提案和
原型脚本，复跑证据仍绑定上述冻结的恢复输入。

模拟 Linux x86_64 从同一恢复清单离线重跑 §422 的普通
所有权/SSA 和公开原生双入口测试，各通过 1 项、无失败或忽略。
镜像及临时 LLVM 工具名链接沿用 §420。新的原始日志为
`.local-artifacts/0392-recovery/macos.log` 和 `linux.log`。
本轮只重建上述定向证据，重启前的完整 conformance 原始证据
目录已丢失。本地未执行故障注入。

## 424. 对齐 `48268b43` 并恢复持久化公开入口证据

从 `28076229` 快进到 `48268b43854e7209b49f0192ece1eb0e754f1b3f`。
远端 11 个改动文件与本地 16 个改动文件无路径交集，快进前后
本地 tracked patch 一致。逐文件对照 §423 输入，crate 源码、
测试及 Cargo/Rust 工具链输入的字节一致。macOS 和模拟 Linux
从同一份 3849 项源码、Guide 与语料清单执行，清单 SHA-256 为
`012d77d517d0f3f108a98a2c19d708319ba749d67096bdd8b577a022c6f59734`。
原始证据保存在持久工作区的 `.local-artifacts/0392-main-48268b43`。

模拟 Linux 另执行普通 Map 的干净样本。复用 §423 已构建的
原样 IR 导出测试，核对 plain 程序 stdout，再由现行
`check_map` 运行 ASan 和 LSan。ASan 为 11 个 Koven 定义添加
插桩，执行输出符合预期且没有 sanitizer 报告。LSan 输出符合
预期且未报告泄漏。两项的命令、IR 和运行结果均已保存。

两端各完成 320 次公开入口调用，原始证据各自通过重算，并由
`--compare-evidence` 验证同一清单上的退出状态、诊断记号和
stdout 一致。两端均为 137/154 个探针能力通过，17 个探针及
6 个综合程序符合已知未实现合同，零超时、内部错误、回归、
入口差异和错误输出。macOS 的证据 JSON SHA-256 为
`395dcd1d1fae7a6b32c22bc43feeb6cd2cb4c2f76e65b210dad8e17f33af6b61`，
模拟 Linux 为
`76115b03fe83bdd01cae59c0e021bfdcc16abc4e79ff887d2738fa963c373a6a`。
冻结提交的双宿主 PR-event CI、完整 sanitizer 和资源矩阵仍待验收。
故障注入留给 CI。

## 425. R5 生成资源原始用例的跨平台验收

在 §424 的 3849 项清单上，复用 SPEC-0269 的
`model.cases`、`prepare_case` 和 `Execution.case`。
这次执行入口调用原始用例循环，校准、checker 变异、故障注入
和缩减留给 CI。原始证据位于
`.local-artifacts/0392-main-48268b43/generated-{macos,linux}`。

macOS arm64 和模拟 Linux x86_64 各 16 项通过。逐项对照源码
哈希、预期诊断和正常 stdout，24 次分配和释放均由 pointer
counter 验证。单独见证 `v2-0-11` 按 0、2、3、1 顺序释放，
替换后的字段先于 Holder 分配释放。8 个预期拒绝分别为
L0131 和 L0133 各 4 个，两端诊断和范围一致，均未生成 LLVM。
模拟 Linux 的 8 个正常用例另外全部通过 ASan 和 LSan，
输出匹配独立 oracle，stderr 均为空。

普通运行与诊断矩阵已复验。冻结提交的双宿主 PR-event CI、
完整 sanitizer 与校准/缩减验收仍待进行。

## 426. R2 右结合短路链的无写入扫描

通过 GitHub 核对本轮 `main`，仍为 `48268b43`。从普通 Unit
类型检查入口构造有非空分支事实的分组右链。`&&`、`||` 各取
8、16、32 个右侧条件项。修复前两种运算符的条件节点访问次数
均为 33、65、129，写入扫描访问次数均为 288、1088、4224。

在每棵语法树上惰性建立可能写入的上界。名称、`this`、字面量、
可递归证明无写的分组、二元表达式和无 `inout` 调用可以跳过
正常路径的写入扫描。赋值、`inout`、错误节点及未分类表达式
继续由既有扫描判断。索引计数为 36、60、108，扫描访问为零，
条件节点访问次数与修复前一致。计数覆盖本例的索引构建和扫描，
含真实写入的分组右链仍可能反复扫描，保留为后续 R2 检查项。

新增 `forward(clear(&x))` 回归，要求嵌套调用写回后拒绝读取
旧的非空类型。在 MemoryDocument 和 RootedUnit 两个输入模式
核对事实和诊断。macOS arm64 与模拟 Linux x86_64 各通过类型检查
单测 38 项、`while_flow_facts` 26 项，操作计数一致。独立只读复审
核对了提前返回的语法范围、调用实参及求值路径、解析恢复回退，
未发现阻断项。故障注入矩阵留给 CI。

§422 的共享短路退出用例在 macOS arm64 与模拟 Linux x86_64
各通过一次原生回归。每次均走单文件和项目公开入口并逐字节核对
`continue`、`break`、`return` 的输出和析构顺序。本轮前端 Clippy、
改动文件格式检查、文档门禁和差异检查通过。

## 427. R2 含写入右链的条件汇合扫描

以 §426 的 Unit 入口和分组形状为基线，把链末调用换成
`clear(&x)`，使 `Int?` 局部变量发生一次 `inout` 写回。
`&&`、`||` 各用 8、16、32 个右侧条件项，语料
均通过类型检查。修复前两种运算符的写入扫描访问分别为
392、1288、4616 次，证明含写入链仍存在重复扫描。

条件汇合只需要从到达右侧的左边事实中剔除被写入的根。该边
没有事实时跳过扫描。两个运算符都从根部的非空比较携带事实，
嵌套的布尔名字没有新事实。门禁要求每份语料恰好一次汇合写入
查询。修复后写入扫描访问为 273、793、2601 次，条件访问仍是
33、65、129 次，写入索引是 40、64、112 次。

求值检查仍会在恢复入口快照后扫描每个需要处理的右侧，用来
更新流版本和丢弃旧事实。当前基线仍有超线性增长。后续复用
必须证明正常完成、待定退出、调用有效性和循环跳转的可达性
在摘要生产、读取与 trial 回滚间一致。不能仅用前后版本相等
替代按正常路径收集写入。

macOS arm64 和模拟 Linux x86_64 各通过 4 项操作计数测试、
26 项 `while_flow_facts`，计数逐项一致。两端再次通过 §422
单文件与项目入口的原生退出和析构回归。独立只读复审核对了
两个结果边的合流公式及写入收集器的状态依赖，未发现阻断项。
前端 Clippy、改动文件格式检查、文档门禁和差异检查通过。
本轮未执行本地故障注入，冻结提交的双宿主 PR-event CI 尚待验收。

## 428. R2 含写入右链的求值摘要复用

在 §427 的含写入语料中，先加入输入翻倍时工作量最多增长
2.5 倍的门禁。旧实现从 8 项的 313 次增长至 16 项的 857 次，
其中包含写入索引的构建访问，门禁按预期失败。

Unit 在检查右侧操作数时，可以沿逻辑表达式和透明分组带回
正常路径写入集合。逻辑节点左侧可由语法证明无写入时，
其独立扫描的写入集合取自正常完成的右侧。使用者在外层
正常完成性检查和可达性一致的前提下复用该集合。带写左侧、
一般表达式、缓存类型和可达性不一致的场景沿用原扫描。
摘要仅存在于当次 RHS 检查调用中，未进入 typed facts 或 trial。

两种运算符在 8、16、32 项的写入扫描均为 126、238、462 次，
索引访问仍为 40、64、112 次。该结果覆盖每条链只有一次写入
且左侧无写的形状。新增普通源码用例在两个 Unit 输入模式核对
能到达的写入、短路旁路、内层左侧写入、`return` 前写入和内层
循环 `break` 前写入的流事实及诊断。

macOS arm64 和模拟 Linux x86_64 各通过 4 项操作计数检查和
27 项 `while_flow_facts`。两个平台再次通过 §422 单文件和
项目入口的原生输出及析构回归。独立只读复审核对了正常扫描
等价性、可达性回退、分组、类型缓存和 trial 的状态边界，未发现
阻断项。前端 Clippy、格式、文档和差异检查通过。故障注入留给 CI。

## 429. R2 求值摘要的退出与重放见证

普通 Unit 源码分别触发两条回退路径。`keep(clear(&x), return 0)`
保留调用的静态 `Boolean` 类型，所在分组右侧无法正常完成，
消费端拒用摘要一次。先在外层左侧返回的用例仍检查分组右侧，
其中内层循环在 `break` 前调用 `clear(&x)`。摘要生成位于临时
不可达路径，消费者恢复外层可达性后拒用摘要并调用原收集器
一次。两个用例分别断言对应计数，且直接复用次数均为零。

`resolve` 的 `String` 和 `Int` lambda 重载都检查同一段分组
右链，最后的 `intResult(value)` 使 `String` 候选失配。`Int`
候选通过，调用结果可按 `Int` 返回。记录到两次有效摘要复用，
用于核查正常重载 trial 还原后会重新分析同一节点。

独立只读复核追踪了 lambda 预检查、两个候选各自的基线恢复、
赢家快照恢复和无待定写回的前提，确认本例没有单候选重试或
循环回放产生的额外计数。台账中的提前返回用例只锁定分支选择，
传到外层的写入集合已经为空。重载计数不单独标记候选身份。

macOS arm64 和模拟 Linux x86_64 各通过 6 项计数测试、
27 项 `while_flow_facts`、47 项 `type_callable`，三条新见证
计数一致。前端 Clippy、格式、文档和差异检查通过。
GitHub `main` 仍为 `48268b43`。本轮未执行本地故障注入。

## 430. R5 固定候选的公开入口与 LSP 跨平台一致性

以 `48268b43854e7209b49f0192ece1eb0e754f1b3f` 和 §426–429
工作树重建固定候选。3859 项编译、Guide、教程与语料输入的
清单 SHA-256 为
`508aa5e9d0aaa25dbcf72ab5b4ad553f512501c808d18307bcad535d56a282b0`。
macOS arm64 和模拟 Linux x86_64 均在运行前验证清单，分别
从同一份源码以 `--locked --offline` 构建。原始证据保存在
`.local-artifacts/0392-429-conformance`，包含源码快照、编译器、
原始输出、判定矩阵与两端日志。

每端通过两种公开入口运行 154 个探针和 6 个综合程序，共
320 次调用。两端能力通过 137/154（正常运行 121、预期诊断
13、预期 abort 3）。17 个探针及 6 个综合程序匹配已知未实现
合同。各端无超时、内部错误、回归、错误输出或入口差异。
原始结果分别通过重算，含历史基线的 `--compare-evidence`
跨平台比对通过，两端 `matrix.tsv` 与各自的 §424 结果相同。
P2 基础 A 组仍为 58/74，未达到 70 项阈值。Oracle 文档对账需
依赖 Spec 与路线图元数据，已在完整工作树执行并通过。

同一候选的两端 LSP 各通过 8 项格式化、1 项源集分阶段诊断
和 1 项真实 stdio 格式化。独立运行真实 stdio 的单文档与
项目会话，打开 32 项右结合短路链、加入 `clear(&x)`、撤销
写回。两端各版本的诊断 JSON 完全一致。版本 1 与 3 无诊断，
版本 2 只在最后 `return x` 的 `x` 报 `L0084`，四次会话均
正常关闭并退出 0。

同一清单另复跑 §425 的普通生成资源矩阵。两端各通过 16 项，
其中 8 项正常、8 项预期拒绝。正常用例各核对 24 次分配与
释放，`v2-0-11` 验证释放顺序 `0,2,3,1`。两个平台的拒绝都为
4 项 `L0131` 和 4 项 `L0133`，诊断范围一致且未生成 LLVM。
模拟 Linux 的 8 项正常用例分别通过 ASan 与 LSan，输出符合
oracle，stderr 为空。运行结束后，源码快照和工作树的 3859 项
清单再次通过字节核对。本地未执行故障注入，物理双宿主的
PR-event 门禁与发布候选验收仍待完成。

## 431. R4 跨文件混合路由与原生双字段写回复验

复用 §430 的固定候选和已构建二进制。两端各执行一次 §418
的定向 SSA 回归。固定 SourceId、互换 Unit 文档输入后，入口、
资源字段写回与临时数组循环按同一 SSA 路由和渲染结果通过
ownership/SSA verifier。入口和写回走 Refine，循环走
Compatibility；入口块对两个目标各保留一次直接调用。

重新构造 §418 的普通源码见证并逐字节绑定两端输入。单文件
与跨包跨文件项目各运行单字段、双字段和重叠字段三个场景。
正常场景分别通过 `check`、`build`、执行独立产物与 `run`。
单字段顺序为循环输出 a、b，析构 old，输出 after，析构 new；
双字段先依次析构 old-left、old-right，输出 after 后依次析构
new-right、new-left。每个析构各一次，正常退出 0，stderr 为空。
重复独占左字段时，`check` 与 `build` 同报 `L0135`，主范围为
第二个 `&`，次范围分别指向第一个 `&` 和第二个 `inout` 形参。
失败构建没有生成产物。两端诊断和输出总账 SHA-256 同为
`56fbc041f551807d5c57f36cd0a622d77542f45ad3abfb610643c2b5c402f524`。

模拟 Linux 的成功样本在容器本地 `overlayfs` 上执行。首次用
宿主共享挂载的 `fuseblk` 执行时，构建已发布 executable，随后
私有暂存目录身份检查返回退出码 2 并保留暂存目录。对当前候选
交换路径顺序后，相对和绝对路径各出现一次，12 次共享挂载
对照共 2 次失败，12 次本地对照全部正常。§424 的旧编译器
在相同源码和环境上也复现了 1/6，共享挂载之外的 6 次对照均
通过。这些已发布的产物单独运行仍输出正确。最小的 Rust
目录创建、文件写入和硬链接探针保持目录身份稳定，无法单独
复现完整构建的间歇现象。这一平台边界延续 §421，尚不能归因
给当前写回或短路修复。原始证据保存在
`.local-artifacts/0392-429-conformance/r4-*` 和对应的暂存探针目录。

在隔离软件副本中仅扩充错误文本，保留发布、身份判定与清理路径。
共享挂载上的一次失败记录暂存目录为目录，设备号始终为 23，
初次保留的 inode 是 109731，清理时同一路径返回 109734。
最初报错时已发布的产物单独运行，退出 0，stdout 与 macOS
一致且 stderr 为空。此见证定位到身份检查的实际输入差异，
其来源仍需进一步定位。本批验收的生产二进制与工作树代码未使用
隔离副本的日志改动。本地未执行故障注入。

## 432. R4 目标布局、最终产物与调试断点

复用 §430 固定候选。macOS arm64 和模拟 Linux x86_64 的代码
生成器各通过 9 项定向测试，覆盖两种目标的 64 位指针、闭合
聚合的大小与偏移、Unit 容器、本机 object 的唯一公开入口、
普通与 abort 的链接执行、多源码调试 IR 和各自目标的行表。

另保存两个平台的单文件与项目最终产物。macOS 为 arm64
Mach-O 和 UUID 匹配的 dSYM，Linux 为 x86_64 ELF。每个
产物只有一个平台入口符号，`llvm-dwarfdump --verify` 报告
无错误且行表包含 `main.ko` 第 2 行。从另一工作目录直接运行
均退出 0、stdout 为 `debug witness\n`、stderr 为空。正常
构建完成后未残留 `.kovenc-stage-*`。

macOS 的 `native_cli::final_debug_artifacts` 通过 4 项，其中
3 项实际命中断点，覆盖单文件、项目、跨根及 Unicode 源码；
仅因 debugserver 权限默认忽略的独立 Codegen LLDB 用例也以
精确目标复跑通过。模拟 Linux 的 5 项 CLI 用例为 2 通过、
3 失败。三个源码断点均解析出地址，但程序正常执行完毕，未
停在断点。同容器的 `cc -g -O0` C 对照用 PIE 和非 PIE 两种
形式均重现这一现象。记录为当前模拟环境的动态断点限制；
真实 x86_64 上的命中仍待验收。日志和实际产物位于
`.local-artifacts/0392-429-conformance/r4-debug-*`。本地未执行故障注入。

## 433. R0/R2 默认生产入口的编译产物对账

以 §430 的同一清单，在 macOS arm64 和模拟 Linux x86_64 用
Rust 1.96.0 从默认功能构建导出 `lang-frontend`、`lang-codegen`、
`kovenc`、`lang-lsp` 的 MIR。对照六个准确标识符 `resolve_names`、
`check_types`、`check_ownership`、`complete_file_types`、
`lower_scalar_file_with_entry` 和 `emit_native_object` 及其所在
MIR 函数。两端匹配的调用方与目标一致。在 CLI、LSP、Codegen
三个默认生产目标的 MIR 中，上述标识符均未出现。Frontend 的
18 处匹配只有一条来自目标自身定义及其内部闭包之外，即
Unit 名称构建 `CompilationUnitNames::new` 的闭包调用
`resolve_names`；旧 Single 类型、所有权及最终准入的定义仍在库中。

正对照定位到 CLI 单文件和项目的 `check_unit_frontend`、Unit
最终准入与 object 生成，LSP 的 Unit 名称与恢复分析，Codegen
的 owned Unit 入口及 `lower_unit_from_facts`。以 `llvm-nm`
复核两端真正链接的 CLI 和 LSP，六个准确标识符中只保留了
逐文件名称求解器及其闭包符号。Cargo workspace 内连接 Frontend
的包为 CLI、LSP 和 Codegen。Frontend 库的三个 Single API 仍公开。

源码对照继续匹配 §413 的四个 Codegen 旧事实见证。两端各通过
4 项普通源码验证，覆盖迭代借用诊断、调用签名身份、可空泛型
callable 规划及析构事实。两端的生产 lowering 依赖方向检查
也通过，确认退役的原生门面和 Single lowering 入口未被恢复。

这批证据绑定默认功能下已编译的代码，补充 §395、§413 与 §419
的源码检索。未审计其他功能组合、仓外 path/git 依赖或所有可能
的间接运行时调用。完整产物、匹配位置和日志保存在
`.local-artifacts/0392-429-conformance/r0-mir*`。未运行故障注入。

## 434. R1 UTF-8、换行与公开诊断位置的复验

复用 §430 固定候选。macOS arm64 和模拟 Linux x86_64 各通过
64 项 SourceMap、Lexer、Parser 长 UTF-8 恢复、跨行分隔及 LSP
位置转换测试，零失败、零忽略。普通源码另加入同一行的
中文、补充平面字符与组合字符，在 LF 和 CRLF 上分别走单文件
与项目入口。每端 28 次 CLI 检查、28 次真实 LSP 版本发布，
诊断与定义 JSON 总账 SHA-256 一致：
`3229247b846b48b8d3584f0007b7bce5492a75a2e52d0c4605fd6bf0b6576928`。

用例包括 `β` 的 `L0001`、保留字 `async` 的 `L0002`、缺右括号
及两种 EOF 的 `L0010`。独立按原文核对 CLI 的半开字节范围、
从 1 起算的标量行列，及 LSP 的零起点 UTF-16 范围。CRLF 下 `β`
占字节 `67..69`，CLI 主位置第 3 行第 23 列，LSP 为第 2 行
零起点第 23–24 列。缺右括号在下一行 `}` 前给零宽范围并标出
实际的 `(`。同一行 EOF 与有末尾换行的 EOF 分别定位于行末
和下一行起点，均保留开花括号的相关位置。未截去后继声明的
词法及语法错误样本仍能通过真实 LSP 查询定位 `target` 的
声明。恢复源码后，诊断清空，四种 LSP 会话均正常退出。

原始证据、用例驱动与日志保存在
`.local-artifacts/0392-429-conformance/r1-*`。本轮未修改生产代码，
未执行故障注入。复验范围为本批具体源码与定向套件。

## 435. R5 P2 基础 A 组缺口的阶段核对

从 §430 的固定候选证据独立抽出 A 组全部 16 个不可运行项。
在 macOS arm64 与模拟 Linux x86_64 各执行 32 次 `check`，
保持原探针字节及项目转换摘要。实际阶段、诊断与已保存的
`run` 结果对应。两端的观测 JSON SHA-256 同为
`7f25694bf5fd91bf4e40a5e05513a688410100614bd8e4a388fac15df68c88b9`。

| 阶段和实际诊断 | 探针 | 路线图合同 |
|---|---|---|
| `check` 通过，`UnsupportedNode` | `p02_interp`、`p05_double`、`p06_char`、`p15_elvis` | L1/L2、N05、N05、N03 |
| `check` 通过，`UnsupportedNode` | `p25b_linked_recursive`、`p40_nested_generic_class`、`q08_enum_string_payload` | N03、N01、N02/L7 |
| `check` 通过，`UnsupportedSource` | `p28_companion` | N06 |
| `L0165` | `p11_interface_bound`、`p16_result_q`、`p21_box_enum`、`q03_fn_ref`、`s03_to_pair` | N06、N08、N07/L7、N04、S6 |
| `L0080` | `p27_thread`、`s02_pair_builtin` | T1/N04、S6 |
| 预期借用拒绝 `L0164` | `q17_generic_class` | N09 |

其中 15 项为已登记未实现，`q17_generic_class` 的借用拒绝通过
诊断 oracle，计入 A 组总数但不计入可运行数。8 项后端错误的
原生标记一致，退出码符合现有单文件 2、项目 1 的 operational
合同。P2 仍为 58/74，距门槛 12 项；B 组诊断为 7/7。

另外在两端各运行 20 次缩减源码对照。显式判空替代 Elvis、
字符串拼接替代插值、具体 `Dog` 参数替代泛型接口上界均能
原生运行。单独构造空 `MutableList<Int>` 和具体类持有非空列表
均通过；泛型类持有列表在仅构造时仍报 `UnsupportedNode`。
只保留 `Char` 字面量也会失败。递归类可构造一个带空尾节点的
实例，去除递归调用但保留双节点构造与字段读取仍失败。这些
对照定位到已有能力边界，没有改动原语料、oracle 或准入。
两端另通过 2 项边界测试，分别核对 Elvis 的准确拒绝位置和
SPEC-0372 已交付的泛型标量字段与枚举能力。`q03_fn_ref` 与
Guide 07 的当前暂缓范围相符。原始证据在
`.local-artifacts/0392-429-conformance/r5-p2-*`。本轮未执行故障注入。

## 436. R5 综合程序的首个失败与相邻能力

按 §430 固定候选，从相同原始源码及项目转换各复跑 12 次
公开 `check`。两个进入原生生成的程序另用隔离观察副本打印
已有 `NativeObjectError` 的源 Span。观察副本只增加日志，
其退出码和失败分类与原候选记录一致。macOS arm64 与模拟
Linux x86_64 的诊断、跨度和对照输出总账 SHA-256 同为
`26431752c7338c50273416b9e1c387741c14554eb8d3492cd84302fa4f4f21dd`。

| 程序 | 单文件首个失败，项目对应同一源码片段 |
|---|---|
| `config_parse` | 名称 `L0080`，第 5 行的 `Display` |
| `expr_eval` | `L0165`，第 24 行的 `this.parseSum()?`，共 10 项 |
| `linked_stack` | `L0085`，第 29 行的 `rest!!` 中 `!!`，`rest` 已由 while 条件窄化 |
| `raii_state_machine` | `check` 通过，原生 `UnsupportedNode` 首指第 7 行 Handle `deinit` 里的字符串插值 |
| `varint_codec` | `check` 通过，原生 `UnsupportedNode` 首指第 44 行的 `"${bytes[i]}"` |
| `word_count` | `L0165`，第 43 和 44 行的两个 Map `in` 查询 |

七组对照在每个宿主上各走两个入口。去掉 `rest!!` 的对照
推进至两处 `Box.value` 类型未完成。Map 最小对照保留同一
容器、插入和值，只切换查询拼写：Guide 12 声明的 `"fox" in m`
报 `L0165`，`m.contains("fox")` 运行并打印 `hit`。同样替换
原 `word_count` 两个运算符后，完整前端检查通过，下一阶段
仍报 `UnsupportedNode`。把 Map 的运算符合同缺口单独登记，
修复需核对操作数求值、借用事实及既有 `MapContains` 消费边界。

RAII 和 varint 的纯文本输出对照均能原生结束。RAII 保留
`replace(&this.handle, null)` 时，关闭消息先于换出的 handle
析构；普通赋值对照顺序相反，两者均恰好三次释放。说明该
接收者字段的交换在当前对照可执行，先前 §352 的缩减边界
在该场景已变化。varint 的纯文本对照经过截断与超长分支。
原六个综合程序及其 stdout oracle 保持原样，成绩仍为 0/6。
证据保存在 `.local-artifacts/0392-429-conformance/r5-programs-*`。
工作树生产代码未改，本地未执行故障注入。

## 437. R4 Map 查询接收者提前返回

在 §430 的原编译器上，`table(return 9).contains(key())` 通过
前端，单文件构建在整个查询 Span 处报 `MissingFact`。定向原生
回归在修复前复现。Compatibility 的 Map owner helper 把接收者
明确的 `Diverged` 转成错误。现在 `lower_map_contains` 直接从
同一 operand helper 传播 `Diverged`，在登记 pending owner
之前退出。正常完成和真实错误仍沿用原路径。独立复审核对了
`ControlTransfer` 清理、查询未发生以及原正常路径，未发现阻断。

新回归在单文件和项目入口检查只输出 `nine`，接收者内 `return`
的结果为 9，`table` 与 `key` 的可见动作均未执行。修复候选
清单 SHA-256 为
`45848c416e90205593f631f0a4ea78f2711f860ac0f1e6a7b2cb3056e47b1e42`。
原冻结二进制及观察二进制分开保留。修复复验使用独立 Cargo
target，避免隔离日志副本的构建缓存混入。原始证据保存在
`.local-artifacts/0392-437-map-receiver`。

未插日志的原二进制在两入口均通过 `check`，单文件与项目
`build` 分别退出 2、1，且均报 `MissingFact`、未发布产物。
修复后 macOS arm64 与模拟 Linux x86_64 的原生 `while_flow`
选集各通过 14 项。另带顶层常量的对照在两端两个入口各输出
`nine`。macOS 默认并发运行时有 6 项在原 10 秒执行门限超时；
保持二进制、源码和门限，串行复跑 14 项全部通过。严格 Clippy、
格式、文档和差异检查通过。

## 438. R2 Map 中缀包含关系的求值边界

§430 编译器在两宿主的单文件和项目模式分别复跑六份普通源码。
`key() in limits()`、`key() !in limits()` 先输出 `key`、再输出
`range`，命中结果分别为 true 和 false。`table().contains(key())`
先输出 `map`、再输出 `key`，查询成功。相同函数用于 Map 的
`in`、`!in` 或不匹配的键时，现行入口均报 `L0165`，未执行
程序。两端观测 SHA-256 同为
`5f450d7a99d753dbb96e29ca5697802e05f9b9c40dd0ae1b07653d88efdde9e4`。
两端另各通过 3 项普通 Range 和 Map 方法提前返回对照。

Guide 12 声明 `key in map` 和 `map.contains(key)` 的 Boolean
结果。Guide 04 明确整数 Range 的源码求值顺序；Map 中缀的
操作数顺序尚无明确条款。两种顺序会改变副作用以及提前退出时
已求值对象的清理，已提交用户选择。独立复审确认现有
`UnitMapContainsDescriptor` 可承接查询事实，Phase 3 的 Map
调用合同当前只接 `Expression::Call`。中缀实现还需选定键的
上下文定型、两操作数的借用及退出清理，并让已有 native 查询
按同一规则消费。subjectful `when` 的 Map selector 属于独立
入口，需另取证。本轮未改变中缀准入，也未执行故障注入。

## 439. R4 Map 写入接收者提前返回

当前分支快进至 main `e4f5b11bad27ea72b9193021611e83353ad1176a`。
主干增量只涉及三份提案文档，快进前后的 19 个本地改动文件逐一核对，
内容一致。按 §437 原有范围固定 3859 项验证输入，清单 SHA-256 为
`b46d00af128ff4361548edd4b1bef75b9599d53806fd223d37e7cc14ee0028c8`。
该范围内相对 §437 只有 Map lowering 与原生测试两份文件变化。

`table(return 9).put(key(), value())` 在修复前通过前端，但单文件
原生回归报 `MissingFact`。Compatibility 现在传播 receiver 的
`Diverged`，在登记 pending owner 和求值键、值之前结束。
独立复审确认正常路径仍核对 source unit 和 SSA owner 值，保留
写回和清理。新回归精确输出 `nine`，没有执行 table、key 或 value。

macOS arm64 与本机模拟 Linux x86_64 的 `while_flow` 原生选集
各通过 15 项。两端各通过三组单文件和项目对照：带顶层常量的提前
返回输出 `nine`，正常临时 Map 写入仍报 `L0134`，命名根写回
依次输出 `map`、`key`、`value`、`nine`。macOS 严格 Clippy
通过。原始证据保存在 `.local-artifacts/0392-439-map-put`。
真实 PR 双宿主和故障注入留待 CI。

## 440. R4 Map 读取接收者提前返回

在 §439 候选上，普通临时 Map 查询和键表达式内提前返回均可
构建运行；`table(return 9).get(key())` 的单文件与项目构建
分别退出 2 和 1，均报 `UnsupportedNode`。原 Refine 准入要求
接收者具备普通 Map 布局，提前返回的查询因此回落到缺少
`MapGet` 支持的兼容路径。

现在先核对查询与两个操作数的来源身份。前端明确发布接收者
不落地事实时，Refine 只准入实际求值的接收者。已有 builder
传播 `Diverged`，跳过键求值和查询操作；正常完成路径仍核对
布局、类型和键。独立复审核对了准入、builder 的正常与退出路径，
以及临时工厂和键均未执行、外层资源恰好析构一次的断言，未发现
阻断问题。

修复候选固定 3859 项输入，清单 SHA-256 为
`2298a35707ee95a383aec11ab54be4ee9860291d7aa950a4bfb63f1cd7f1607c`。
相对 §439 只有 Refine 准入与原生测试两份文件变化。原始证据保存
于 `.local-artifacts/0392-440-map-get`；相邻失败的前态探针保存
于 §439 的 `get-macos`。

macOS arm64 与本机模拟 Linux x86_64 的 `while_flow` 原生选集
各通过 16 项。三组单文件和项目对照的输出在两端逐字节一致：
下标接收者提前返回时输出 `drop:outer`、`nine`；键提前返回
时先输出 `map`，随后析构外层资源；正常查询输出 `map`、
`key`、`drop:outer`、`nine`，命中为 7、未命中为 null。
严格 Clippy、格式、文档和差异检查通过；本地未执行故障注入。

## 441. R4 Map 删除和借用回调的接收者提前返回

在 §440 候选上，`table(return 9).remove(key())` 和
`table(return 9).withValue(key(), action)` 均在单文件与项目
入口报 `UnsupportedNode`。正常命名根上的删除和借用回调通过；
临时 Map 正常完成后执行删除仍报 `L0134`。新增定向原生回归
在修复前复现单文件构建失败。

两个操作共用 Refine 的接收者退出见证，核对描述符与操作数
来源，只消费前端明确发布的接收者不落地事实。准入仍递归
核对接收者可否 lowering，builder 实际生成 `Diverged` 才能
结束。该返回发生在取得 Map 权限、求值键和回调之前。
独立复审核对了 Phase 3 正常边上的权限获取、两侧见证和
正常调用路径，未发现阻断。回归断言输出
`drop:remove`、`nine`、`drop:withValue`、`seven`，两个外层
资源各析构一次，工厂、键和回调体均未执行。

修复候选固定 3859 项输入，清单 SHA-256 为
`e0ba625334e2ea9425d7dd0871cf008641e4ffcc368aee575e65e2d2bc60681d`。
相对 §440 的四份变化为 Refine 见证、准入和 builder，以及 CLI
回归。原始证据保存在 `.local-artifacts/0392-441-map-operations`。

macOS arm64 和本机模拟 Linux x86_64 的 `while_flow` 原生选集
各通过 17 项。删除接收者退出、删除键退出、正常删除、临时
Map 删除拒绝、回调接收者退出、正常回调六组用例，均经两端
单文件和项目入口验证，退出码、stdout 和 stderr 一致。
严格 Clippy、格式、文档及差异检查通过。本地未执行故障注入，
真实 PR 双宿主验收仍待 CI。

macOS 的双入口附加探针使用会打印的回调工厂。接收者提前返回时，
工厂和回调体均未执行；命名根正常调用时，键、工厂和回调体
依次执行。相邻 `consume` 探针在两个入口均显示：命名根转换
通过，正常临时接收者由前端以 `L0130` 拒绝，提前返回的接收者
进入原生构建后报 `UnsupportedNode`。这三组原始证据保存于
`next-macos-corrected`，留待下轮核对消费式转换的准入。

## 442. R4 Map 消费式移交接收者提前返回

§441 的 `table(return 9).consume()` 在单文件和项目入口报
`UnsupportedNode`。新增定向原生回归使用持有 `deinit` 条目的
Map 工厂，修复前单文件构建复现同一错误。

Refine 将该描述符接入 §441 的共享退出见证。只有描述符没有
消费来源、接收者身份匹配且前端明确发布不落地事实时才提前
退出。Builder 生成 `Diverged` 后结束，未进入 owner 移交。
独立复审核对了 Phase 3 退出边、正常本地根的 Move 合同和
原 `remove`／`withValue` 检查，未发现阻断。新回归只输出
`drop:outer`、`nine`，工厂不执行，条目不构造，外层资源恰好
析构一次。

固定候选的基线为 `e4f5b11bad27ea72b9193021611e83353ad1176a`，
3859 项输入清单 SHA-256 为
`704be06141bd1d1eea4319751dd5ed45e090a23ab4d1ec7b37d4d1023de46559`。
该范围内相对 §441 仅共享见证和 CLI 回归两份文件变化。原始
证据保存在 `.local-artifacts/0392-442-map-consume`。

macOS arm64 和本机模拟 Linux x86_64 的 `while_flow` 原生选集
各通过 18 项。命名根移交、临时接收者拒绝、再次消费拒绝和
括号包裹接收者提前退出四组用例，在两个平台的单文件与项目
入口上退出码、stdout、stderr 逐字节一致。正常移交后保持只读
查询，条目先于外层资源析构且各一次；临时接收者和已移交根
分别报 `L0130` 与 `L0131`。macOS Clippy、格式和差异检查通过。

后续范围已取证。macOS 单文件和项目入口在命名 Map 已完成、
`withValue` 的键表达式提前返回时均报 `UnsupportedNode`。
此时需核对已建立 Map 借用的退出清理。现有 String 键
`requireValue` 借用对照在两入口均输出 `value`、`key`；
只把其中键替换为 `return` 时，两入口均报 `UnsupportedNode`。
另一个借用绑定探针在接收者提前返回时均报 `MissingFact`。
这两类借用结果退出待分别核对，原始探针见 `adjacent-string-macos`
和 `require-fixture-macos`。

## 443. R5 Map 退出修复后的公开语料复验

以 §442 的同一清单和编译器身份复跑 conformance，沿用原语料、
oracle 与历史 TSV 基线。macOS arm64 和本机模拟 Linux x86_64
各执行 154 个探针和 6 个综合程序的单文件、项目入口，共 640
次调用。两端均为 137/154 探针通过，包含 121 项正常运行、
13 项预期诊断和 3 项预期 abort；17 项探针与 6 个综合程序仍
匹配既有未实现合同。没有回归、错误输出、内部错误、超时或
入口不一致。两端证据束通过原始输出重算和跨平台比对，
各自的 `matrix.tsv` 与 §430 逐字节一致。Oracle 静态对账通过。

R5 发布门槛仍开放。P2 基础 A 组为 58/74，距离 70 项阈值还差
12 项；B 组诊断为 7/7，综合程序为 0/6。本次验证范围为公开
语料与 §442 原生回归，PR 双宿主、故障契约和发布候选完整
矩阵待 CI 或发布冻结后验收。本地未执行故障注入。
候选二进制、原始输出和重算日志保存在
`.local-artifacts/0392-442-map-consume`。

## 444. R4 Map 借用回调的参数退出与暂存键清理

在 §442 候选上，`withValue` 的接收者完成后，键或回调参数内的
提前退出会在两入口报 `UnsupportedNode`。新原生回归先复现；
按键分支、回调参数和循环分别隔离后确认，直接放开参数准入
仍会遗漏已借出的临时 `String` 键。

Refine 逐个检查实际可达的操作数，键不落地时跳过回调；回调参数
不落地时 builder 必须实际得到 `Diverged`。已经建立的键和 Map
临时借用按原退出规则逆序结束，再消费前端发布的 ControlTransfer
清理。暂存键准入核对同源描述符、借用的准确键、临时目标以及
处于回调参数内的真实退出点。键有正常分支时，仍执行命中回调。

独立复审发现 Borrow 与 carrier 返回路径可能保留传出的 loan。
本轮对此类函数保留原有控制转移准入限制，复审确认该发现已关闭。
同一借用返回检查迁入相应的 range/borrow 模块，行为保持原样，
避免 field 子模块超出物理行门槛。

最终固定候选的 3859 项输入清单 SHA-256 为
`5fdc72d1f51d17a72fdadcdb20c78723fdadc572e0c11a58987f34b579c51964`。
证据保存在 `.local-artifacts/0392-444-map-with-value`。

macOS arm64 与本机模拟 Linux x86_64 的 `while_flow` 原生选集
各通过 19 项。新用例逐字节核对键分支的返回与正常命中、
回调参数返回前已生成的 String 键、键内 break/continue 后的
Map 重用与精确析构。两端补跑的单文件和项目对照一致，覆盖
已持有临时 String 键后的 action break/continue、callback 内
局部 return、缺失键，以及 Borrow 返回原有拒绝边界。严格
Clippy、格式、Rust 物理行、文档和差异检查通过。本地未执行
故障注入，真实 PR 双宿主验收仍待 CI。R5 完整语料结果仍以
§443 的上一固定候选为准。

## 445. R4 Map 确定槽位借用的提前退出和预检准入

修复前，`requireValue` 的接收者或键提前返回时，两入口出现
`MissingFact` 或 `UnsupportedNode`。正常查询结束后接普通
`return 7` 的缩减对照也会被拒绝。隔离日志确认，后者以及
混合键内的返回在选择 lowering 路由前就被旧全函数控制流
预检拒绝。日志副本只记录失败跨度和路由，正式验证使用
未经插桩的独立二进制。

当同源 Map 描述符、Phase 2 不落地事实和 Phase 3 不派发事实
均匹配时，Refine 只 lowering 已到达的操作数。接收者提前
返回时不建立查询借用；键提前返回时，仅把接收者新建的借用
纳入退出清理。Builder 要求实际得到 `Diverged`，不交付槽位
结果也不建立借用绑定。

会到达查询的绑定仍核对来源 loan、布局和键。旧预检对
同源、对应准确 `requireValue` 来源 loan 的普通根绑定使用
已有专用路径，实际借用的形成、结束与 verifier 继续由所达
路由负责。独立复审核对了调用、借用和清理的来源，确认普通
非 Map 借用及 Compatibility 的绑定拒绝维持原合同。
共享预检也可能被实验入口调用；本次没有改动 D10、E2 或 E3
的授权、计划或发射逻辑。

冻结候选共 3860 项源码及验收输入，清单 SHA-256 为
`3d6d4e04f3120536b47b05bcfc90f3a5ce3cba49e33f09b61163bbfd87956328`。
新增输入为集中维护退出见证的 `refine/map_exit.rs`。
原始证据保存在 `.local-artifacts/0392-445-map-require-value`。

macOS arm64 和本机模拟 Linux x86_64 的 `while_flow` 原生选集
各通过 20 项。新回归覆盖接收者退出、键退出、同一混合键的
返回及命中路径，检查借用结果只在命中路径建立，Map 可在
结果作用域结束后再次读取。两端的单文件、项目附加对照输出
一致，覆盖正常读取、查询后返回、临时来源 `L0162` 和活跃
借用下重叠写入 `L0135`。macOS 仍通过已有的普通／兼容预检
矩阵，严格 Clippy、格式、物理行数、文档和差异检查通过。
本地未执行故障注入。

同一候选另复跑 Guide 12 的 48 个探针，两端各经单文件和项目
入口执行 96 次，均通过 47 项运行和 1 项预期诊断。原始证据
通过重算与跨平台比对；此次集合语料验证不替代 §443 的完整
语料基线或真实双宿主 PR CI。

## 446. 对齐 `e339b881` 并复验最新 R5 候选

当前分支从 `e4f5b11b` 快进到
`e339b881f76d98d08365add4d56f3d1b98e1d244`。主干增量仅含
SPEC-0369 归档及配套路标文档，快进前后 29 个本地改动文件
逐字节一致。§445 的 3860 项编译、Guide 与语料输入也逐一
保持一致，清单 SHA-256 为
`3d6d4e04f3120536b47b05bcfc90f3a5ce3cba49e33f09b61163bbfd87956328`。
本次以新基线和这份清单记录候选身份，原始证据保存在
`.local-artifacts/0392-446-main-conformance`。

两端各从冻结源码重新构建 `kovenc`，通过单文件与项目入口
复跑 154 个探针和 6 个综合程序，共 640 次调用。两端都是
137/154 个探针通过，分别为 121 项运行、13 项预期诊断和
3 项预期 abort，另有 17 个探针、6 个综合程序匹配已知未实现。
零回归、超时、内部错误、入口差异和错误输出。原始证据通过
重算及跨平台比对，两端各自的 `matrix.tsv` 与 §443 逐字节一致。
P2 A 组为 58/74，B 组为 7/7，距 A 组阈值还差 12 项；
正式综合程序为 0/6。

同一候选的 macOS arm64 与本机模拟 Linux x86_64 分别构建
LSP，各通过 8 项格式化、1 项 source-set 分阶段诊断与 1 项
真实 stdio 格式化。32 项右结合短路链的单文档、项目会话
均经历初始打开、插入 `clear(&x)` 和撤销；只有第二版发布
返回处 `x` 的 `L0084`。两端 JSON 逐字节一致，也与 §430
对应诊断证据相同。文档、物理行数和 oracle 静态对账通过。

同一候选在模拟 Linux 通过普通 Map 样本的原样 LLVM 导出及
原生执行。ASan 对 11 个 Koven 定义插桩，ASan 和 LSan 均
退出 0，stdout 与 oracle 一致，stderr 为空。这是普通样本的
动态证据；完整 sanitizer、故障契约和物理双宿主 PR CI 仍待验收。
本地未执行故障注入。

词频综合程序的缩减对照在两端分别走单文件和项目入口，
四组结果一致。独立复审按 Guide 12 重算原始词序和计数，
重建的 16 行输出与正式 stdout oracle 一致。原源码保留
`L0165`；仅用 `.contains` 替换两处 Map 中缀查询的对照
推进到原生 `UnsupportedNode`。去掉插值输出的逻辑对照通过
11 个不同词、16 次总计、最高频 `the` 4 次、重复词
`the fox dog` 及成员查找，逐键输出顺序与首次出现一致。
只读计数对照另验 `fox`、`dog` 各 2 次及 `cat` 缺失。
在同一逻辑对照上恢复普通的重复词拼接输出，两端仍通过；
仅恢复逐键频次的插值语句即重现 `UnsupportedNode`。
原综合程序、中缀求值合同和完整输出 oracle 保持原义；
缩减通过不计入发布成绩。对照保存在 `word-count-macos`
与 `word-count-linux`，补充格式对照见 `word-count-format-*`。

## 447. R0 旧前端公开入口的 crate 外消费验证

在 macOS arm64 上建立普通的独立 Cargo package，通过 path
依赖消费 §446 冻结候选的 `lang-frontend`，关闭默认 Cargo
feature。独立程序使用公开的 `resolve_names`、`check_types`
与 `check_ownership`，从实际源码生成和消费各阶段产物。
合法的 Int 返回无诊断；同一个 String 绑定被移动后再次读取，
词法、语法、名称与类型阶段通过，所有权阶段恰好产生
`L0131`。证据在 `external-frontend` 和 `external-frontend.log`。

该结果验证这些旧函数在候选上仍是 crate 外可调用的公开 API，
能形成可观察合同。真实仓外消费者的清单、其版本使用量和其它
feature 组合仍需独立证据；本次没有迁移或删除这些入口。

## 448. R1 当 `when` 缺闭合符时保留外层恢复边界

在 §446 编译器上，最后一臂完整但缺少 `}` 时，同行 EOF
和调用者的 `)` 附近同时报 `L0010` 与 `L0065`。只给 EOF
前加入换行即去掉 `L0065`。Guide 06 将该码用于同行下一臂
缺分隔，EOF 与外层闭合符都不构成下一臂；普通源码的
定向 Parser 回归在修复前复现了额外诊断。

修复后用一致的硬停止条件结束 `when` 的臂循环和分隔检查，
外层闭合符由原调用者处理，缺少 `}` 仍在边界前报零宽
`L0010`。独立复审核对了 EOF、外层 `)`／`]`、现有未终止
词法 owner 的诊断抑制以及真实同一行下一臂的分隔检查。
新增回归断言完整调用和后继声明范围，并保留同行缺分隔时
`L0065` 的真实 selector 位置。

冻结候选基于 `e339b881`。在 §446 的 3860 项输入范围内只改
Parser 与定向测试两份文件，清单 SHA-256 为
`67540794833ba64c5f5e76132b6faa0c1b0678e3df6ca1850c35bb24fcd27080`。
原始证据保存在 `.local-artifacts/0392-448-when-recovery`。

macOS arm64 与本机模拟 Linux x86_64 各通过 67 项 Parser
测试，覆盖控制流、错误传播、分隔、换行及诊断范围。两端各
复跑 28 次公开 CLI 检查、28 次真实 LSP 版本发布及 20 次
后继定义查询。LF 与 CRLF 的零宽缺闭合符范围、真实分隔
诊断及后继定义逐字节一致，JSON SHA-256 为
`0a4f411b3eb2f2cbc0d468dd9316d51893fdf4943113f442d22e920c32748962`。
严格 Clippy、格式、物理行数、文档及差异检查通过。本地未
执行故障注入；完整 R5 语料仍以 §446 的上一候选为准。

## 449. R1 保留未闭合调用之后的 `when` 分支

GitHub 核对远端 `main` 与当前基线均为 `e339b881`。
§448 候选复现 `false -> take(1` 后接换行 `else -> 2` 时，
调用恢复吞掉下一臂并报 `L0034` 与 `L0010`。同类下标探针保留
后继，修复范围限定在调用。无词法配对的调用遇到明确的分号
或可识别的后继 selector 时，以零宽 `L0010` 归还边界；已有
配对的嵌套 `when`、lambda 和跨行实参维持原有 token 归属。

独立复审发现，多组换行实参会让探针反复扫描长后缀。修订
复用同一游标和顶层逗号后等价的选择器状态；若逗号前仍有
待匹配的 `else`，恢复探针结束该试探。复核确认配对调用、
旧成组探针和诊断发布的边界未变化，未发现新的阻断项。
工作量回归覆盖 4/16 层未闭调用，以及 64/128 个普通实参和
缺 `else` 实参。

冻结候选保留 §448 的 3860 项输入范围，变更为 Parser 和测试
共四个文件。清单 SHA-256 为
`ae1dec1663f5f8e652119b50e6567eabf26884a1a3c9d79c66ff37e05db6f94b`，
证据位于 `.local-artifacts/0392-449-when-arm-recovery`。

macOS arm64 与本机模拟 Linux x86_64 各通过 69 项 Parser
集成测试和 3 项工作量测试。两端分别通过 24 次公开 CLI
检查、24 次 LSP 诊断版本发布及 24 次后继定义查询，覆盖
单文件与项目入口、LF 与 CRLF、缺闭合符及恢复后的正常输入。
两端观测 JSON 逐字节一致，SHA-256 为
`b3feb26878d650252d8d86f7ae520047468c21d748a86a6ad741ed00eeb72049`。
严格 Clippy、格式、Rust 文件行数、文档与差异检查通过。
本地未执行故障注入。本条为 R1 定向验收，完整 R5 证据仍见 §446。

## 450. R1 损坏实参恢复保留外层 `when` 分支

§449 冻结二进制复现未闭调用 `take(1 :` 后换行接
`flag -> 2` 时，错误扫描吞掉后续分支，`L0010` 落在
`when` 右花括号。新增 Parser 回归先在 §449 实现上失败。
修正使缺少配对闭合符的调用只在退出局部词法 owner 和
delimiter 后检查下一臂，保留真实损坏 token 的 `L0033` 或
`L0034`，在下一臂或外层分号处发布零宽 `L0010`。
回归覆盖错误实参和错误分隔符，以及局部括号内的箭头。

独立复审发现，逐 token 重同步会从长跨行中缀链的各起点
反复试探。修订在待解析操作数处复用后缀结果，并按待匹配
`else` 数量限制可以传播的结论。复核确认配对调用、旧成组
探针、独立 `else` 和分号，以及局部 owner 的 token 归属
保留原行为。工作量回归比较 64/128 项跨行操作数，覆盖
普通链、嵌套 `if`、以及末尾有箭头的嵌套 `if` 链。

冻结候选基于 `e339b881`，相对 §449 变更三个 Parser 文件与
一份测试，沿用 3860 项输入范围，清单 SHA-256 为
`51765e7aca55d027bf2847864de8abd0baee4b6220fb4da7ebaf48ac9d7a750b`。
证据位于 `.local-artifacts/0392-450-when-call-panic`。

macOS arm64 与本机模拟 Linux x86_64 各通过 86 项 Parser
集成测试和 4 项工作量测试。两端各执行 20 次 CLI 检查、
20 次真实 LSP 诊断发布与 20 次后继声明定义查询，覆盖
单文件、项目、LF 和 CRLF。诊断精确位置、开括号标签及
后继定义结果逐字节一致，JSON SHA-256 为
`31869c7a7cbc2ad57bda29cf28635dfe9e285a5fe5be4c807a8baa3644c08a94`。
严格 Clippy、格式、物理行数、文档及差异检查通过。
本地未执行故障注入。此项为 R1 定向验收，完整 R5 证据仍见 §446。

## 451. R4/R5 在语法修复后的冻结候选重跑全量语料

沿用 §450 候选与 3860 项输入清单，逐项重算确认冻结目录与
工作树一致。macOS arm64 与本机模拟 Linux x86_64 各用同一
候选的 CLI 从公开入口执行 154 个探针和 6 个综合程序，每项
走单文件和项目，共 640 次调用。复算原始输出并通过跨宿主
证据比较；各宿主的 `matrix.tsv` 与 §446 逐字节一致。

两端均为 137/154 个探针通过，含 121 项预期运行、13 项预期
诊断和 3 项语言规定的预期 abort。回归、入口差异、超时、
内部错误及错误输出均为零。另有 17 个探针与 6 个综合程序
落在既有未实现范围。兼容性回归门禁通过，P2 A 组仍为
58/74（门槛 70），B 组为 7/7，正式综合程序仍为 0/6。

同一 CLI 又实跑 `short-circuit-loop-exits.ko`。两端的
单文件、根包项目及具名包项目各通过 check、build 和独立
进程执行。continue、break、return 三条路径的临时实参及
迭代局部对象按已有 oracle 析构，外层资源按对应控制流退出
时机清理。六次原生运行均退出 0、stderr 为空，32 行
stdout 与 oracle 一致。入口观测 JSON 的 SHA-256 为
`6c1a305c3cc8a67b43bc30dcd69d742128330285b3554f9b82cbc8bfda79dd88`。
初版探针将 `package app` 放在项目根包路径，按合同被
`L0146` 拒绝；更正路径后才计入验收。

Oracle 文档静态对账使用冻结脚本和语料，另从 `e339b881`
补齐引用的 Specs 与路线图元数据，未更动冻结编译输入。
静态摘要与 §446 一致。原始证据与独立只读复核结果见
`.local-artifacts/0392-451-conformance` 及本轮账本。

同一冻结源码在模拟 Linux 上通过普通 Map 的原样 LLVM
导出与原生运行。ASan 标记 11 个 Koven 函数定义；ASan
与 LSan 进程均退出 0，stdout 精确为 `done\n`，stderr
为空。独立复审核对了源码身份、语料门禁口径、资源 oracle
和干净样本的测试选择。本地未执行故障注入；完整 sanitizer、
物理双宿主 PR CI 与 v1 发布候选门槛仍待验收。

## 452. R1 未闭合调用缺操作数时保留下一臂

§450 候选在 `false -> take(1 +` 后换行接
`flag -> 2` 时，错误地把 `flag` 作为加法右操作数，随后把
箭头当成非法分隔符。Parser 回归先在原实现上复现
`L0034` 与 `L0010`。闭合调用的合法跨行前缀与中缀正控通过。

仅当调用没有词法配对闭合符时，实参解析启用 `when` 臂边界
恢复。缺操作数产生止于上一已消费 token 的空错误节点，
诊断 `L0009` 与缺调用闭合符的 `L0010` 定位在下一臂。
已确认边界向外层 Pratt、postfix 与控制体传递，避免把有符号
selector 当作中缀续行，或把外层 `else` 归给内层 `if`。
值上下文确实缺 `else` 时仍发布 `L0057`。

独立复审另发现，嵌套 `if` 条件缺 `)` 时，控制体入口可能
再次消费已提交的 selector。修复后以普通及花括号 selector
核对下一臂及前一臂的 AST 范围。复核确认配对结构清除了
实参恢复标志，下一臂由 `when` 消费，未发现新阻断项。

冻结候选基于 `e339b881`，含 3861 项输入，清单 SHA-256 为
`068797d5e0ce835cf1c1ec988abc5bf19e3f80ce1a1a624a8f4aacca444c13b0`。
原始证据位于 `.local-artifacts/0392-452-when-operand`。

macOS arm64 与本机模拟 Linux x86_64 各通过 118 项 Parser
集成测试、4 项工作量回归及 1 项停止标志穷举测试。两端各
通过 28 次 CLI 检查、28 次真实 LSP 诊断发布和 28 次后继
声明定义查询，覆盖 LF、CRLF、单文件和项目入口。两个缺
`else` 的输入在独立单文档 LSP 还会发布恢复后的 `L0084`，
项目 LSP 与 CLI 在此只发布 Parser 诊断。两端完整观测 JSON
逐字节一致，SHA-256 为
`519d4f8b0ac15341f9d6f2825a5fddaa34243a9703e4c3e26a17ee680dce942b`。
严格 Clippy、格式、文件行数、文档与差异检查通过。

另在同一 macOS CLI 确认 `take(1 as` 后换行的相邻形状仍会
把 selector 读入独立 TypeRef 路径，发布 `L0080`、`L0034`
并把 `L0010` 移至下一臂之后。该 TypeRef 恢复缺口继续记账。
本轮未执行故障注入，完整 R5 语料证据仍见 §451。

## 453. R1 未闭合调用缺类型操作数时保留下一臂

§452 的遗留输入 `false -> take(value as` 后换行接
`flag -> 2`，原实现把 `flag` 解析为类型；Parser 回归
先复现 `L0034` 与错位的 `L0010`。修订把 `as`、`as?`、
`is`、`!is` 以及非结合运算符链的恢复接到同一个类型
操作数入口。无配对调用闭合符且确认了外层 selector 时，
在上一运算符末尾构造零宽 `TypeRef::Error`。缺类型的
`L0014` 和缺调用右括号的 `L0010` 均位于下一臂的起点。

若现有严格类型索引在当前位置能识别完整函数类型，
`(Int) -> Int` 或 `move (Int) -> Int` 的箭头仍由类型
持有。正控覆盖未闭调用内的完整函数类型和已闭调用内的
跨行命名类型。独立复审核对严格类型证据、非结合链、
零宽范围及下一臂的进展，未发现新阻断项。

冻结候选沿用 §452 的 3861 项输入，变更一份 Parser 文件
与一份测试文件，基线 `e339b881`，清单 SHA-256 为
`4c152f8ab9d05b138e7b3534b7e06b9b213ea334be819ce36c14c258420e050f`。
原始证据位于 `.local-artifacts/0392-453-when-type-operand`。

macOS arm64 与本机模拟 Linux x86_64 各通过 140 项 Parser
集成测试及 4 项工作量回归。两端各通过 24 次 CLI 检查、
24 次 LSP 诊断发布和 24 次后继声明定义查询。单文件和
项目入口在 LF 与 CRLF 上的完整观测 JSON 逐字节一致，
SHA-256 为
`eb55a46512a92d9378aa7495c1b88a23ab6ad5d3707bae9febd04a3ef4029f8c`。
严格 Clippy、格式、物理行数、文档与差异检查通过。

macOS CLI 另确认 `value as List<` 后换行的相邻输入仍会
在缺少 `>` 时把下一臂读为类型实参，证据见
`type-arg-neighbor.stderr`。该路径发生在已开始的 TypeRef
内部，继续列为 R1 恢复缺口。本轮未执行故障注入，完整 R5
语料证据仍见 §451。

## 454. R1 缺少泛型闭合符时保留下一臂

§453 的 `value as List<` 后换行接 `flag -> 2`，原
TypeRef 把 `flag` 读成类型实参，调用恢复又消费箭头。
定向 Parser 回归先复现错误的 `L0034`。修订只将未闭合
`when` 臂调用的类型操作数恢复上下文传给泛型参数列表。
确认下一臂后，若缺类型实参就在上一实符号末尾建立空
`TypeRef::Error`，然后由各层泛型和调用在下一臂前报零宽
闭合符诊断。已有实参而仅缺闭合符时保留该实参。

独立复审发现 `flag > limit ->` 可与 `List<flag>` 共享
严格类型前缀。修订在 `>` 后接同行裸操作数时继续检查
selector 归属。复核确认既有配对调用、完整跨行泛型、
完整函数类型与已提交的下一臂都保留其归属。遇到跨行
比较右操作数或软中缀名称的歧义仍优先保留严格类型。
本轮在 4/16 层未闭泛型上检查选择器探针的访问增长。

冻结候选基于 `e339b881`，沿用 3861 项输入，清单 SHA-256
为 `684b82f211b6c68e2145912ed41560c8ce091aaede82d2355720ee92015d02f3`。
原始证据位于 `.local-artifacts/0392-454-when-type-arguments`。

macOS arm64 和本机模拟 Linux x86_64 各通过 142 项 Parser
集成测试和 5 项工作量回归。两端各通过 28 次 CLI 检查、
28 次 LSP 诊断发布、28 次后继声明定义查询。单文件和项目
入口在 LF 与 CRLF 上的诊断位置、闭合符标签和后继定义
逐字节一致，JSON SHA-256 为
`7f7761b68ef07f71ece11640d542b0e078642e9509cb6dae2c46eabd6c8a73c3`。
严格 Clippy、格式、物理行数、文档和差异检查通过。

macOS CLI 的 `rhs-next-line.stderr` 和 `soft-rhs.stderr`
保留上述两类歧义的失败证据，当前仍会把 selector 的首个
标识符读入类型。本轮未执行故障注入，完整 R5 语料证据仍见
§451。

## 455. R1 软中缀名称与比较选择器归属及 R5 复验

§454 在 `take(value as List<` 之后换行接
`flag > and -> 2`，误用 `and` 的软中缀身份屏蔽下一臂，
由定向 Parser 回归先复现 `L0009`、`L0034` 和错位的
`L0010`。现在严格类型后的同行软中缀还需有能开始右操作数
的有效 token。`and ->` 与 `and.member ->` 均已通过保留
外层 selector 的回归；`List<Int> and limit` 继续使用已有
中缀解析。独立复核确认新增前瞻不推进游标、不重复扫描表达式
后缀，未发现新阻断项。

`List<` 后换行接 `Int >`，再换行接 `limit ->` 时，末行
可以独立构成一个臂，保留完整类型及末行独立臂。该正控锁定
与比较右操作数跨行同形时的确定性恢复顺序。

冻结候选基于 `e339b881`，沿用 §454 的 3861 项输入，
清单 SHA-256 为
`88fb256647ac14902983a1d0b734dba0f619d9715c4e5529925c880b1d90e572`。
证据位于 `.local-artifacts/0392-455-when-soft-selector`。

验证期间远端 main 合入 [PR #484](https://github.com/Halckon/Koven/pull/484)。
本地分支快进至 `4c8fc369` 前，15 个上游文件与 40 个本地
变更文件无重叠；快进后逐项核对，本地内容逐字节一致。
原冻结候选保留用于完整语料证据，另外冻结快进后的 3862 项输入。
编译输入仅变更六份 LSP 文件，清单 SHA-256 为
`bb7d56e0ee57930e797d731d88c43dbd5025494c70a01374ee1fc8eb37ce4975`。

macOS arm64 和本机模拟 Linux x86_64 各通过 143 项 Parser
集成测试和 5 项工作量回归。快进后各通过 60 项 LSP 单元测试
及 1 项真实 stdio 格式化测试。快进前后各宿主均完成 20 次
公开 CLI、20 次真实 LSP 诊断及 20 次后继定义查询，覆盖
单文件、项目、LF 和 CRLF。四组观测 JSON 逐字节一致，
SHA-256 为
`bef36b3640d63a2340c5339cecee3346bb69e2b8f4f2e64937bcd467679cf91e`。
快进后的两端另启用两个 LSP opt-in 参数实跑项目 manifest。
跨文件定义准确指向 `src/app/math.ko`，未解析诊断附带
`missingSymbol` 源片段，归一化输出逐字节一致。

完整语料以原冻结二进制执行 640 次公开 CLI 调用，两端
矩阵均与 §451 逐字节一致。快进后重建的 CLI 在各自宿主
与原冻结二进制逐字节一致，语料 manifest 也已核对，因此
原始运行证据覆盖当前相同 CLI 产物。能力为 137/154，
P2 A 组 58/74、B 组 7/7，综合程序 0/6。零回归、入口差异、
超时、内部错误及错误输出。严格 Clippy、格式、物理行数、
文档、静态 oracle 和差异检查通过。本地未执行故障注入，
物理双宿主 PR CI 与正式发布门槛仍待验收。

## 456. R2 循环预扫描保留 `when` 的实际求值前缀

§455 冻结 CLI 中，先以 `x == null` 的提前返回建立窄化，
随后 `while (cond(x, flag))` 的循环体只含
`when (break) { else -> { x = null } }`，合法的循环头读取
仍误报 `L0084`。分组主体、已退出主体后的尾随写入，以及
`when { keep(quiet(), break) -> ... }` 的可检查 Boolean
选择器均可复现同类误失效。公开 CLI 红证据单独保留。

语法预扫描和已检查写入收集共用 `when` 求值前缀。先检查
主体，再按序访问仍可到达的选择器。确定退出时计入导致
退出的操作数，裁掉后续不可达臂；先前可能命中的臂和跳转前
的写入保留。语法证明仅在所有可匹配臂都不正常完成且没有
落空路径时裁掉语句尾部，仍区分嵌套循环体中的 `break`
和进入嵌套循环前求值的 header。

新增双 Unit 回归核对头部及退出后已有的 `Int` 窄化，
覆盖已知缺失、潜在和确定的较早命中、跳转前写入、嵌套
header、嵌套 body、主体 `continue` 与正常尾部。
独立复审核对已检查写入摘要、分支合并及跳转归属，未发现
新阻断项。

冻结候选基于 `4c8fc369`，共 3863 项输入，清单 SHA-256 为
`c3b1a5e385da800f6be77e18ce00402d4782edc571761f7b5bfcbd5fb8253f38`。
证据位于 `.local-artifacts/0392-456-when-loop-prefix`。

macOS arm64 和本机模拟 Linux x86_64 各通过 540 项类型
及所有权集成测试、6 项工作量回归。两端各完成 32 次 CLI
检查、32 次 LSP 诊断发布及 32 次后继定义查询，观测
JSON 逐字节一致，SHA-256 为
`13cf797777b0786556c0ddfc0df06deb39f08ad7749ccfd1cff71eef69b43666`。
严格 Clippy、格式、物理行数、文档和差异检查通过。

两端的单文件与项目入口分别实跑直接 `break` 对照、
`when` 主体退出和 `Int` 实参前缀后的选择器退出。12 次
原生运行均输出预期行序，主体和选择器路径各析构一次，
`Int` 实参的动作先于析构。`Unit` 实参前缀的选择器在
四个入口均通过前端检查，原生构建仍报 `MissingFact`，
单独列入 R4。macOS 的无资源缩减另表明 `Int` 实参
在 `if` 和 `when` 均可执行，`String` 在该 `if` 形状可执行，
在该 `when` 形状报 `UnsupportedNode`。本轮未执行故障注入；
完整 R5 证据仍见 §455。

## 457. R4 保留选择器提前退出前的 Unit 实参借用

远端 `main` 与当前基线同为 `4c8fc369`。按 §456 原案
复现 `when { keep(quiet(), break) -> ... }`。旧候选的有资源
原案报 `MissingFact`，无资源命名实参缩减报 `UnsupportedNode`。
所有权产物认定外层调用确定不派发，并记录已完成的
`quiet()` 实参对临时 `Unit` 的共享借用。

Refine 的准入和构建共用这一条件：源计划存在且无派发目标，
调用是声明调用，所有权事实确认调用不会派发、实参已完成，
实参和 Borrow 形参的具体类型都为 `Unit`。构建按源码顺序
执行实参一次，必要时物化本地 `Unit`，沿现有路径建立共享
借用。后续实参跳转时先结束待处理借用，再执行控制转移的
清理。继续使用现有的普通派发路径与调用约定。

新增公开入口回归覆盖乱序命名实参、后续实参及派发跳过，
以及 `own Guard` 实参放弃时的析构。独立只读复审核对了
准入、求值顺序、借用和清理，并确认退出路径复用现有逻辑，
未发现阻断。

冻结候选含 3863 项输入，清单 SHA-256 为
`0acf3b21f97eed864670dc91af196ed9783ea58308a2a86b1fa30e3c08b97441`。
证据位于 `.local-artifacts/0392-457-unit-call-prefix`。

macOS arm64 和离线容器内的 Linux x86_64 各通过 21 项
循环流原生测试。§456 的直接跳转、主体跳转、`Int` 与
`Unit` 实参前缀四组程序在两端分别以单文件及项目入口实跑，
16 次的前端检查、原生构建和运行输出均符合预期，双端对应
输出逐字节一致。macOS 另通过既有的 SSA 借用结束与
资源析构顺序回归。严格 Clippy、格式、物理行数、文档和
差异检查通过。本轮未执行故障注入。

macOS 上，普通 `if` 中的对应 `Unit` 前缀缩减仍报
`MissingFact`。`when` 中借入临时 `String` 或默认借入
临时 `Guard` 后跳转，仍报 `UnsupportedNode`。可到达的
普通 `Unit` 参数调用缩减也沿用原有 `UnsupportedNode`
边界。上述缩减在旧基线和本候选中均通过公开前端检查，
分别留在 R4，不能以本轮析构通过关闭这些条目。

## 458. R4 普通 if 使用已验证的 Unit 实参前缀

§457 候选的 `if (keep(quiet(), break))` 通过前端检查，
单文件原生构建仍报 `MissingFact`。同一调用通过 Refine 的
局部准入，但普通 `if` 未触发整函数选择 Refine；旧 lowering
无法消费该 `Unit` 临时借用。新增公开入口回归先复现此错误。

通过 §457 已有借用前缀条件的调用记录其 Refine 需求，
仅在调用准入通过时纳入整函数路由。整函数仍须通过现有
签名、可达函数体、循环和析构检查。共享借用物化、实参
求值与跳转清理继续由上一节的构建逻辑负责。独立复审核对
调用身份、路由门禁和既有正常派发条件，未发现阻断。

冻结候选以 `4c8fc369` 为基线，含 3863 项输入，清单
SHA-256 为
`abea41e86a26dc3f9aa9659a41c6fa09c6b3403539030045e316abeb54d51d4d`。
原生及质量门禁证据位于 `.local-artifacts/0392-458-if-unit-prefix`。

macOS arm64 和离线容器内的 Linux x86_64 各通过 22 项
循环流原生测试。两端分别用单文件和项目入口核对 `break`、
`continue`、`return`，12 次前端检查、原生构建和执行
均符合预期，对应输出逐字节一致。原生回归中 `quiet()`
恰好执行一次，跳转后的实参、派发和分支均未执行。严格
Clippy、格式、物理行数、文档和差异检查通过。故障注入
留给 CI。

macOS 复核临时资源对照时，普通 `if` 的临时 `String`
借用仍可运行。`when` 的 `String` 字面量、插值和函数返回值，
以及作为默认 Borrow 实参的临时 `Guard`，均通过前端检查，
在原生构建报 `UnsupportedNode`。临时所有者的借用和退出
清理继续列入 R4。

## 459. R4 选择器提前退出时的直接临时实参借用

§458 的 `when` 选择器在临时 `Guard` 或 `String` 已借入、
后续实参跳转时仍报 `UnsupportedNode`。所有权产物已发布
临时构造、字符串字面量和插值的直接共享借用结束及析构；
普通具名调用的自有 `String` 返回值已有同一退出析构，
对应的真实借用结束尚未发布。新增公开回归先复现错误。

所有权阶段从既有控制退出动作发布该自有 `String` 实参的
直接 loan end，沿用同一 source、call、argument 和 owner
身份。正常返回时的发布范围保持原合同。Refine 只按确定
不派发的源码计划放行，逐项核对已完成借用、准确的控制
退出点、无先行析构和无条件临时 owner；现有构建器在
退出边结束借用，再按原事实析构。普通函数按退出点、调用
及实参身份惰性索引事件，命中仍核对完整 loan。独立复审
核对发布边界、清理顺序及索引身份，未发现阻断。

冻结候选基于 `4c8fc369`，含 3863 项输入，清单 SHA-256 为
`6d0d804b1360ff9f560c604dbebe3b8fe746078318feb0639af721bd7205d44d`。
证据位于 `.local-artifacts/0392-459-temporary-borrow-exits`。

最终冻结版本在 macOS arm64 和离线 Linux x86_64 容器
各通过 359 项前端直接借用结束测试和 23 项循环原生测试。
六组选择器程序分别经两端的单文件与项目入口执行，24 次
前端检查、构建和输出均符合预期，对应输出逐字节一致。
混合 `break`、`continue`、`return` 实测确认两只临时
`Guard` 按实参逆序各析构一次。macOS 的正常派发对照
也维持派发、实参析构与后续分支的顺序。严格 Clippy、
格式、尺寸、文档和差异检查通过。故障注入留给 CI。

插值仍通过前端检查，在两种公开入口的原生构建中报
`UnsupportedNode`。Refine 的字符串表达式准入和构建当前
都只接收无插值字面量，既有静态字符串测试也规定动态
插值拒绝。已发布的借用结束事实不能解除该 lowering 边界，
单独保留该复现。

将混合回归的终止实参改为
`stop = if (skip) { break } else { true }` 后，前端仍通过，
macOS 单文件原生构建报 `UnsupportedNode`。该调用有可派发
边，需要独立核对正常返回和提前退出各自的借用结束及
析构；本节的确定不派发准入不覆盖该复现。

## 460. R4 有正常派发和提前跳转两条边的临时实参借用

§459 的混合用例把后续实参改为
`stop = if (skip) { break } else { true }` 后，前端接受，
单文件原生构建报 `UnsupportedNode`。公开入口回归先复现。
临时借用已存在真实控制退出事件；旧准入仅接受确定不派发
的调用，因此漏掉仍有正常派发边的合法用例。

准入现要求 Phase 3 的已知派发状态与同一源码调用计划
是否存在派发目标一致。已建立的借用、临时 owner、来源、
退出点、零先行析构及无委派条件沿用 §459。提前退出使用
对应的 loan end 与析构；正常边保留前缀并派发，返回后
原构建流程先结束借用，再消费 `CallReturn` 析构。前端
回归核对同一 `String` 临时 owner 在退出和正常边的事实。
独立复审核对两条边及准入身份，未发现阻断。

冻结候选基于 `4c8fc369`，含 3863 项输入，清单 SHA-256 为
`577964c58a4ac5f6102e8960960c8fa18198eefec219a72efb5050422934e23c`。
证据位于 `.local-artifacts/0392-460-two-call-exits`。

macOS arm64 和离线 Linux x86_64 容器各通过 359 项前端
直接借用结束测试和 24 项循环流原生测试。三个双出口用例
在两端的单文件及项目入口完成 12 次原生运行，输出逐
字节一致。提前 `break`、`continue`、`return` 均跳过后续
实参和派发。正常边按源码顺序完成后续实参及派发，随后
两只临时 `Guard` 逆序各析构一次，再执行命中的分支。
严格 Clippy、格式、尺寸及差异检查通过。故障注入留给 CI。

同一候选完成 R5 公开全量语料复跑。154 个探针及 6 个
综合程序均经两端的单文件和项目入口运行，共 640 次调用。
原始证据包通过双宿主校验，两份矩阵与 §455 逐字节一致。
探针通过 137/154，综合程序通过 0/6；无回归、入口差异、
超时、内部错误或错误输出。矩阵 SHA-256 为
`c774542544b88775c12e95b9e8437362cbaa72d450257785e368303cb27579e6`。
P2 A 组仍为 58/74，B 组为 7/7。正式发布门槛未关闭。

普通可到达的 `Unit` 参数调用继续保留为 R4 独立条目。
macOS 单文件缩减中，默认与显式 `borrow` 均通过前端，
原生构建报 `UnsupportedNode`；`own Unit` 通过前端，
原生构建报 `InvalidModel`。前者已有参数声明拒绝边界。
后者的内部 callable ABI 也有拒绝 `Value(Unit)` 实参槽的
适配器回归，需要核对调用方、被调用方和 ABI 表示。动态
插值边界仍见 §459。

## 461. R4 可到达的直接 Unit 参数调用

§460 留下的默认 Borrow、显式 Borrow 和 `own Unit`
调用均通过前端，原生构建失败。新增公开回归先复现
`UnsupportedNode`。默认与显式 Borrow 被参数声明拒绝，
向下追踪又确认 SSA 直接调用契约拒绝 Unit 参数。
普通 Value 参数还缺少 LLVM 私有直接函数的零大小表示。

普通直接调用保留实参与形参的 SSA 类型及模式。非 receiver
的 Shared Unit 使用现有零大小本地存储地址，Value Unit
在 LLVM 私有直接函数中使用空结构体参数。Unit 源码返回仍
使用 `void`；通用 callable 类型、显式 SSA Unit 返回和
Inout 的原准入维持既有边界。实参按源码顺序只求值一次，
继续消费前端所有权事实。两条 lowering 保留 Unit 局部
的可借用存储及重赋值；Refine 的每个局部使用独立可写槽。
独立复审批出了重赋值丢失存储及两个绑定复用槽的问题，
修正后逐项复核通过。

冻结候选基于 `4c8fc369`，含 3863 项输入，清单 SHA-256 为
`4198071e3b119a5bf136307630c5a12843f483c5fffe0975b032671a958476f2`。
证据位于 `.local-artifacts/0392-461-direct-unit-arguments`。

该候选在 macOS arm64 与离线 Linux x86_64 各通过 9 项
LLVM Unit 存储测试和 25 项循环流原生测试，另实跑跨文件
同包项目，输出逐字节一致。R5 两端完成 640 次公开 CLI
调用，证据包通过校验；矩阵与 §460 一致，探针 137/154、
综合程序 0/6，无回归、入口差异、超时、内部错误或错误输出。

严格 Clippy 要求去掉共享类型表的一次多余取引用，最终清单
SHA-256 为
`53f9750da052aa75f8851d92819e4b4c4e773fd8b2bdb59d60a9245319833347`。
逐字节核对了两个源码清单间的唯一改动。最终候选在两端
重跑七组单文件与项目用例，共 28 次原生运行，含三种参数
形态、分组表达式体返回、重赋值、Refine 选择器和泛型
`T = Unit`。输出与前一候选逐字节一致。最终源码通过严格
Clippy、格式、尺寸、文档与差异检查。故障注入留给 CI。

动态字符串插值的原生 lowering 待验收，见 §459。按
Guide 03/04 复测 §356/357 的前置 Display 合同：普通
函数值、Unit 与未实现 Display 的 class 插值，在 macOS 的
单文件及项目 check 中均错误通过；Int 对照按预期通过。
显式实现预声明 `Display` 的单文件对照仍报 L0080。
下一步同时需要 L1 协议身份及 L2 格式化调用产物，不能仅
在后端为插值拼接字符串。

## 462. R2 字符串插值的 Display 身份与类型准入

GitHub 的 `main`、本地 `origin/main` 和工作分支基线均为
`4c8fc369`。保留此前复审的工作树改动。本片按 Guide 02/03/04
修复 §461 记录的前端拒绝和漏判，范围截止于类型准入。

CLI 与 LSP 使用受信任的 `koven.core.Display` 声明，按
SourceMap 内的源码身份定位协议。普通名称仍遵守词法遮蔽；
用户自定义同名接口或同名方法不会赋予协议资格。内建类型
按 Guide 的支持表准入，普通类型及泛型上界使用已有名义
接口匹配。对可正常产生值但缺少协议的插值操作数报 L0085，
定位到该操作数；不完成求值的 `Nothing` 路径无需转换。

独立复审发现三处 LSP 交接问题，均已修复并复核。内存文档
保留单用户源语义，只允许编译器登记的内嵌支持源随附；
内嵌声明的引用不生成无效跳转，指向该源的相关诊断保留为
文本注记；同包重名在项目根两种排序下均落到用户声明。
公开协议回归覆盖单文档及项目，另保留原有定义和位置差分。

冻结候选含 3866 项输入，清单 SHA-256 为
`b0b9b3cb9513c3a2ad8b9e3baabfb8385bf793ec49529cfcaa9739a768c04787`。
证据位于 `.local-artifacts/0392-462-display-admission`。
macOS arm64 与离线 Linux x86_64 各通过前端 685 项、
CLI check 11 项、标准源登记 15 项、LSP 63 项及 1 项
stdio。macOS 另过 45 项编译单元索引、内存文档与名称解析
回归。严格 Clippy、格式、尺寸、文档及差异检查通过。
macOS 的第一轮编译后仅改过一次 Rust 格式。

两端完整 conformance 的 640 次公开 CLI 调用均有证据，
探针 137/154、综合程序 0/6，无回归、入口差异、超时、
内部错误或错误输出。Linux 快照不含 Git 元数据，以冻结
清单代替 commit 字段作为候选依据，分别验签证据文件并
核对全部 160 项判定、成功运行的 stdout SHA-256 和汇总。

`config_parse` 在两种入口均越过原未解析的 `Display`，
接着命中 L0165。用 §461 二进制加显式接口声明复跑，
归一化文件头后，14 条诊断的代码、位置、消息及顺序与本片
完全一致。更新该程序唯一允许的记号并重生成审计摘要；
保留 `known_unimplemented`、源码及输出 oracle。

下一片 R4 验证前端发布的格式化来源如何驱动原生生成。
`p02_interp` 及 §459 的实参插值仍需覆盖，特别是操作数
求值顺序、提前退出及字符串所有权。故障注入继续留给 CI。

## 463. R4 插值的类型计划与临时值清理交接

在 §462 基线上用 `p02_interp` 复测。macOS 单文件与项目
的 `check` 均通过，原生入口在 SSA lowering 报
`UnsupportedNode`。另建带显式 `Display.toString` 和
`deinit` 的 Source 级用例，同时覆盖两次转换和中间
`return`，两个入口复现相同阶段的拒绝。

类型产物现在以 source-qualified 字符串表达式为键发布有序
步骤。文本保留 Span；内建转换保留类型，名义类型与受约束
泛型保留标准协议成员标识。确定退出的操作数结束执行计划；
原有类型诊断仍遍历后缀。类型试探使用同一事务产物，常量
重检同步丢弃旧计划。未确定的操作数不获得转换目标。

所有权 DropPlanner 读取已解析计划，继续执行真正的退出
操作数及其控制转移清理。测试按表达式身份验证普通完成时两
个临时接收者逆序析构，以及提前 `return` 时只清理已形成的
临时值。独立复核检查了协议身份、回滚和两条清理路径。

SSA 的转换调用、拼接及中间 owned String 的退出清理仍待
实现。实际格式化借用必须发生在接收者的 `AfterExpression`
析构之前。独立复核另定位到已有的 Elvis 所有权交接缺口：
`${x ?: return 7}` 的非空路径应能继续，同一 drop walker
的通用二元表达式路径却静态可见会访问右侧。该项待独立复现，
并在 Phase 2/3 分支事实交接处收口。

冻结基线仍为 `4c8fc369`，输入清单含 3867 项，SHA-256 为
`35691ae49b67cfb894c4201cb0a3e85404f0211ffba66adc4b3b17bf912ed11e`。
复现、原始日志与候选见 `.local-artifacts/0392-463-interpolation-plan`。
macOS arm64 和离线 Linux x86_64 各通过 300 项多文件类型、
359 项调用借用结束、28 项循环流事实测试。macOS 另通过
11 项 CLI check 和 frontend 严格 Clippy。用最终 macOS
二进制重跑两例两入口，前端均通过，原生失败边界相同。
格式、尺寸、文档与差异检查通过。故障注入留给 CI。

## 464. R4 原生 String 插值与转移边清理

沿用 §463 的有序计划。先加公开入口回归，复现
`p02_interp` 及多段插值的 `UnsupportedNode`。Refine
支持内建 String 转换及确定退出步骤，沿用现有文字解码、
SSA StringConcat 和 String ABI。转换以只读方式读取接收者，
先形成独立结果，再完成接收者原有的清理。当前拼接前缀按
所在循环深度入栈，函数返回清理全部前缀，循环跳转只清理
被退出的层级；Abort 保留现行不展开析构语义。

多段插值的部分 return 暴露旧准入缺口：已求值的源临时
实参被外层调用的借用范围掩盖。改为只采纳成功准入的准确
插值操作数，并交叉核对 Phase 3 已发布的转移点。独立复审
又确认常量组的临时值以真实物化根作为事实目标，已复现
并支持该身份；复审同时核对了拼接中间所有者的作用域。

macOS arm64 与离线 Linux x86_64 各通过两项原生回归，
每项均执行单文件与项目入口。覆盖组、借入 String、多段
只读求值、部分 return、内外层 break 界限、continue 与
常量物化。macOS 另通过 8 项 codegen 定向测试、常量原生
对象的确定性与失败保留测试，以及严格 Clippy。UTF-8、嵌入 NUL 和两段读取之间修改源变量的
附加用例在两宿主的单文件和项目入口输出逐字节一致。
格式、尺寸、文档和差异检查通过。

普通名义类的直接 `toString()` 可以原生调用。带 deinit
的 Display 实现仍在原生拒绝；去掉 interface 的同形对照
通过。已有具体布局路径仍排除带 interface 的析构 owner，
见 `unit_plan/concrete_types.rs::schedule_layout`。
后续先审清该历史准入，再接入插值的标准成员分派。其余
内建转换与 §463 的 Elvis 分支问题仍待处理。

证据位于 `.local-artifacts/0392-464-native-interpolation`。
Linux 快照含 3868 项，SHA-256 为
`2a3df54b33c1cda1923cad70ab265033ca28dda519db0b2e31fff49d0dcbc3bc`。
macOS 最终清单含 3869 项，SHA-256 为
`fdd7615bd61ea0dc714aa05432f8f06036170364f07cb919a77ab9aa99bb8b2d`。
两份快照的差异限于 codegen 单元测试的断言修正及分文件，
Linux 执行的 CLI 回归与矩阵均不编译这些文件。

完整 conformance 在双宿主共执行 640 次，逐项判定与成功
stdout 一致。探针从 137/154 到 138/154，唯一改变的是
`p02_interp`，两入口均输出 `hello k\n`；六个综合程序
仍属已知未实现。证据包已校验并通过跨宿主比对。
故障注入留给 CI。

## 465. R4 静态接口资源的布局与析构可达性

重现带普通类型 `deinit` 的 `Display` 实现直接调用
`toString()` 时抛出 `UnsupportedNode`。具体资源布局
原依据接口列表提前拒绝。去掉该条件后，继续核对现有的
具体类型及字段、析构描述符的 owner/receiver 身份，并
按声明键登记隐藏析构实例。静态委托仍使用前端已核验的
实体字段，析构遍历沿原字段次序和对象身份。

独立只读复核核对了布局、委托和析构队列。两项原生
回归分别验证同作用域资源逆序析构，以及委托包装者的
析构先于资源字段。旧 marker 负例改为析构体内资源可达的
正例，泛型资源和闭包限制保留。macOS 通过 8 项 SSA
deinit 测试及严格 Clippy。

用带可观测格式化与析构的 §463 用例复测。显式调用现在
通过，名义插值仍在原生准入报 `UnsupportedNode`。现有
`UnitInterpolationPart` 给出标准协议及顺序，但常规
实例规划只枚举已进入的显式 call，接收者借用事实亦绑定
该显式调用。后续要为隐式转换发布可到达位置及接收者
借用来源与结束事实，按具体接收者复用现有 override /
delegation 路由。转换完成后结束该借用；原临时接收者
仍在整个字符串完成或所属控制转移点清理。

另复测了 `T : Display` 的显式 `value.toString()`，确认为
既有 L0165，与 `p11_interface_bound` 的通用 member
解析边界相同。名义插值的首例使用已通过类型分析的具体
类，泛型场景分别核对静态类型与实例分派。

证据位于 `.local-artifacts/0392-465-display-resource`。
冻结输入 3869 项，清单 SHA-256 为
`33e16e0c93b7df48cce3c23de65727feb9eb1b54165c706183fc665ca6f2f527`。
macOS arm64 与离线 Linux x86_64 的两项原生回归均通过，
每项覆盖单文件与项目入口。另在两宿主两入口验证了同类
`Rc` payload 的共享计数退出后只析构一次。macOS 的
普通默认成员对照也保持输出及析构顺序。格式、尺寸和文档
检查通过。

完整 conformance 在两宿主执行 640 次，保持 138/154，
六个综合程序均在已登记的未实现边界。全部 160 项判定与
§464 相同，双宿主成功输出逐字节一致。故障注入留给 CI。

## 466. R4 名义 Display 插值的调用与清理

以 §465 的资源类复现名义插值原生拒绝。所有权阶段现在
按源码操作数记录实际进入及能否正常完成，同一操作数重访
按可能正常完成合并。实例规划只为有正常后继的名义转换
建立调用点，用具体接收者复用静态 override 与委托路由。
内建 String 实例复用原拼接路线。

Refine 每个操作数求值一次，按共享借用把接收者交付
同步调用，并在拼接前逆序结束接收者与委托字段的新借用。
转换结果由拼接消费其文本后独立释放；源临时值按前端
既有事实在整个插值完成或实际转移点清理。正常路径测试
覆盖两只析构资源逆序，提前 return 只清理已形成的前缀。

复审复现了两处准入与构建不一致。一处是判空后的活动证明
及字段读取。准入只采纳可证实的 Field 借用，保留普通值
投影路线。另一处是命名常量条件证明首个插值必然退出，
类型计划仍保留后缀。两端现在只在所有权明确证明该操作数
不完成时停止扫描，并且构建器核对实际退出。测试使误规划
不可达后缀会触发原生拒绝，正常只输出 `stopped`。

另外复现 `when` 将枚举收窄为具体 case 后，隐式调用找不到
枚举声明的 Display 实现。类型登记、布局需求、静态分派与
析构可达性在具体化前使用枚举根类型；构建器复用具名绑定的
槽位或借用，保留 case 的静态流事实。独立复核核对了根类型
替换次序及调用后的借用结束。新增用例在两个公开入口分别
核对具体枚举与借入的 `GenericLabel<Int>`。

原生回归覆盖 String 对照、普通名义实现、判空与字段借用、
委托、同一泛型的名义和 String 实例、单层 lambda、提前
return、不可达后缀与枚举收窄。另实跑 value class 及 enum
的对照。
名义实例通过插值静态分派；§465 中泛型普通成员调用
`value.toString()` 的前端 L0165 仍是独立问题。Boolean、
Char、整数与浮点等内建 Display 转换也仍待原生接入。
下一项的 Boolean 缩减用例在单文件和项目入口通过
`check`，两端 `build` 均以 `UnsupportedNode` 拒绝。

证据位于 `.local-artifacts/0392-466-nominal-interpolation`。
冻结输入 3871 项，SHA-256 为
`6b492c495d08a47f15ee75c34a2c3af5112735f2954a72ba96015cac99316539`。
macOS arm64 与离线 Linux x86_64 各通过 7 项原生回归，
每项均核对两个公开入口的 stdout、stderr 和退出码。macOS
并行批次有 6 项在执行产物时触及 10 秒上限；同一二进制
串行复跑 7 项全部通过。完整矩阵收尾时以默认并发再次
复跑，7 项在 15.33 秒内通过，未修改上限。Linux 还通过
687 项前端测试；对应前端输入与 macOS 已通过的 687 项
一致。macOS 通过 8 项插值相关 codegen 测试和前端、
codegen、CLI 严格 Clippy。

完整 conformance 在双宿主执行 640 次，通过证据校验及
跨宿主比对。探针维持 138/154，六个综合程序仍是已登记的
未实现项。160 项判定与 §465 一致，无新增回归或执行超时。
故障注入留给 CI。

## 467. R4 内建 Boolean 的原生插值

基于 §466 的双入口拒绝，先补动态 Boolean 插值回归并
复现 `UnsupportedNode`。Refine 根据类型阶段发布的转换
目标和实例化后的类型选择现有布尔值读取路径，操作数只
求值一次。两条 SSA 边分别形成 owned `"true"` 和
`"false"`，在拼接前合流；拼接后释放转换结果。普通
Boolean 与 `T : Display` 实例化到 Boolean 共用转换，
跨文件常量也经过该路径。现有整数插值拒绝和 Span
检查保留为相邻边界。

双宿主资源对照分别实跑单文件与项目入口。前缀资源在
提前 return 和正常路径各析构一次；提前退出时没有
输出插值字符串，正常路径输出 `[prefix|false]`。
跨文件常量与借入对象的 Boolean 字段在项目入口实跑；
两宿主分别输出 `[true|false]`、`<false>`、`<true>`。
Linux 首次把探针制品写到共享挂载时，在制品发布后报
私有暂存目录身份变化。改用容器内临时目录复跑通过，
三组 stdout 与 macOS 逐字节相同。

证据位于 `.local-artifacts/0392-467-boolean-display`。
冻结输入 3871 项，清单 SHA-256 为
`56e1f245a62d10bf3dbb1b06b4e120e465692ae4d1b45fdb44f475397d1e348f`。
独立复核确认两条格式化边分别创建并移交一个 owner，
且普通和泛型路径均使用同一具体类型判断。双宿主各
通过 8 项原生插值回归，macOS 另通过 9 项 codegen
插值测试与 codegen、CLI 严格 Clippy。
完整 conformance 双宿主共执行 640 次，两份证据分别
校验并通过跨宿主比对。138/154 个探针通过，六个综合
程序保持已登记的未实现状态，160 项判定与 §466 一致。

下一项 Char 缩减用例覆盖 ASCII、三字节和四字节 Unicode
标量。两个入口通过 `check`，原生 `build` 均报
`UnsupportedNode`。
故障注入留给 CI。

## 468. R4 Char 插值和字面量的 Unicode 标量

先用公开入口回归复现 Char 插值的 `UnsupportedNode`。
直接内建转换和 `T : Display` 实例均通过正常值路径
求值一次。新增的 typed SSA 操作只接受 Char 值，返回
独立 String owner。LLVM 根据标量选出 UTF-8 长度和
字节前缀，使用已有 String 分配与析构 ABI。保留实际
长度，避免把固定容量内的尾字节计入输出。

加入参数和返回值场景后定位出兼容 lowering 拒绝 Char
字面量。Refine、兼容入口与常量求值共用词法已验标量
的解码。回归逐字节核对 NUL、四种编码长度、代理区两侧
与 `U+10FFFF`，同时覆盖泛型实例和默认 Borrow 实参。
`p06_char` 的有序比较仍单独在原生入口拒绝。

独立复核核对了 SSA 操作数和结果类型、String 分配与
释放，以及两条字面量路径。将 LLVM 位运算等价复算的
1,112,064 个合法 Unicode 标量与 Python UTF-8 逐字节
比对全部一致。原生测试另核验实际产物的代表性边界值。

证据位于 `.local-artifacts/0392-468-char-display`。
冻结输入 3871 项，清单 SHA-256 为
`54d68f16020d52836d9b09fc788975079872770991e592f18ac08b2a5018070a`。
macOS arm64 和离线 Linux x86_64 各通过 9 项原生
插值回归与 300 项多文件类型测试。跨文件 Char 常量与
借入对象字段的项目输出也逐字节一致。macOS 另通过
3 项常量值测试、9 项插值 codegen 测试及前端、codegen、
CLI 严格 Clippy。格式、文档与 Rust 尺寸检查通过。
完整 conformance 双宿主共执行 640 次，通过证据校验和
跨宿主比对。138/154 个探针通过，六个综合程序仍属已登记
的未实现边界，160 项判定与 §467 一致。

下一项 Long 十进制插值已用最小值、零和最大值复现，
两个入口通过 `check`，原生 `build` 均报 `UnsupportedNode`。
故障注入留给 CI。

## 469. R4 八种整数的十进制插值

先补普通参数与 `T : Display` 的双入口回归，复现后端
拒绝。Refine 用具体内建类型核对前端转换目标，沿正常
值路径只求值一次。与 Char 共用的 SSA 转换只接受 Char
及 8、16、32、64 位整数，结果须为 String owner。

LLVM 把原 Abort 整数转十进制的位运算提取为共用实现。
SSA 整数决定符号扩展，负数以无符号幅值分解，保留
`Long` 最小值与 `ULong` 最大值。每个转换点在函数入口
预留固定栈缓冲，按实际字节数分配和复制 String 内容，
拼接后释放转换结果。Abort 沿用原有输出路径。

已有拒绝用例使用仍未接入的 Float/Double 插值，保留
原 Span、操作错误和回滚断言。独立复核核对了整数
极值、栈缓冲、String 释放和 Abort 路由。

macOS 和 Linux 额外实跑两入口的循环插值，五次重复
转换的 stdout 逐字节一致。索引 `-2147483648` 访问
长度为 3 的数组触发正常语言 Abort，两宿主两入口均
输出完整 `abort: index -2147483648 out of bounds for size 3`
及源码位置，stdout 为空。

证据位于 `.local-artifacts/0392-469-integer-display`。
冻结输入 3872 项，清单 SHA-256 为
`0fd09f7145cbcfa12d848676a6cd510a6ccf633e60d22ca8715f4d9d502e72d1`。
双宿主各通过 10 项原生插值回归和项目制品错误回归。
macOS 另通过 9 项插值 codegen 测试、普通 println/
error 准入、容器越界、回滚与 lineage 边界测试，以及
codegen、CLI 严格 Clippy。

完整 conformance 在两宿主执行 640 次。两份原始证据
均将 `raii_state_machine` 和 `varint_codec` 报为
`unexpected_pass`，各在双入口退出 0、stderr 为空，
stdout 分别与原 `expected.txt` 的 369 和 375 字节一致。
复核后将两项转为 `run_ok`，保留原始证据，用同一批
stdout、stderr、退出码重判。154 项 probe 与其余四个
程序维持原判；新证据通过跨宿主比对，结果为 138/154
探针及 2/6 综合程序。更新后只变更门禁与文档，3872 项
清单 SHA-256 为
`27eff4e4a72b1a47bd7c9b42298acd95cb18b5b43ccc25259fe3c6a66d868178`。
以新门禁在两个宿主定向复跑 `p02_interp` 与这两个程序，
两入口退出和输出再次通过比对。
修正了 §464 插值探针转正后遗留的静态审计断言；18 项
conformance 工具测试通过。程序转正的证据覆盖现行
原生验收，双宿主 sanitizer 仍待 CI 验证。
故障注入留给 CI。

## 470. R4 Char 的 Unicode 标量比较

以 `p06_char` 和双入口边界回归复现原生
`UnsupportedNode`。Refine 与兼容路径均接收同型
Char 的四种有序比较，Refine 同步接收已有的等值
比较。typed SSA 要求操作数同型并产生 Boolean，LLVM
对 Char 使用无符号 i32 的标量顺序。整数继续使用
原有符号规则，算术和位运算仍走整数契约。

新原生用例覆盖 NUL、两字节、三字节、补充平面字符，
同时核对相等边界、反向顺序及比较后插值。独立复核
核对了两条 lowering 路线、SSA 类型隔离及 LLVM
六种比较谓词。双宿主对照确认有符号和无符号极值
仍正确排序，混合 Char / UInt 比较报 L0085。

旧门禁下，`p06_char` 在 macOS 两入口均输出
`char\n`，记录为 `unexpected_pass`。当前门禁
用原有输出转为 `run_ok`，保留历史 baseline。泛型
`<T : Comparable>` 的独立输入在前端报 L0080，
该前端缺口另行跟踪。

证据位于 `.local-artifacts/0392-470-char-order`。
冻结输入 3872 项，清单 SHA-256 为
`3a330523288438f29b6658d86313a784a9650cec6e0086f5abf7399d1128b8b6`。
macOS arm64 与离线 Linux x86_64 各通过 38 项
控制流原生回归。macOS 另通过 Char SSA 正向回归与
codegen、CLI 严格 Clippy。18 项 conformance 工具
测试、格式、文档与尺寸检查通过。
完整 conformance 在双宿主执行 640 次，证据通过
跨宿主校验。探针达到 139/154，综合程序维持 2/6。
160 项判定与 §469 对比，唯一变化为 `p06_char`。

N05 更新为部分完成并重新生成路线图视图。冻结后仅
`conformance/ORACLE_AUDIT.md` 因路线图同步更新，
编译器输入与可执行 oracle 保持同一份。`p05_double`
仍通过前端 `check` 并在原生 lowering 报
`UnsupportedNode`，Double/Float 项保持未验收。
故障注入留给 CI。

## 471. R4 原生 Float / Double 运算

以 `p05_double` 和双入口运行时回归复现
`UnsupportedNode`。SSA 新增独立的 32、64 位浮点
类型和等宽位模式常量。字面量沿用前端的解码路径，
在目标位宽直接舍入；LLVM 用等宽 bitcast 构造常量，
保持符号零。Refine 与兼容路径接入一元正负号、
加减乘除和取余，整数继续使用 checked 控制流。

SSA 验证浮点结果和操作数同宽，验证一元和二元的
操作数个数。LLVM 使用无 fast-math 的操作与 IEEE
谓词。NaN 的等值及四种有序比较为假，不等为真。
实际用例核对 Float 舍入、动态 NaN、负零和除以零。

独立复核发现原取余用例使用字面量会被折叠。改为
两种位宽的运行时参数后，Linux 确认缺少 `fmod` 与
`fmodf`，公共链接驱动接入 libm。同时收紧 Float
进入旧 Map 查询结果比较及 Map 键的准入，保留
浮点容器尚未接入的边界。浮点 Display 插值另行
记录，现有拒绝用例继续有效。

证据位于 `.local-artifacts/0392-471-native-float`。
首次双宿主验证输入 3874 项，清单 SHA-256 为
`fbd01de2e847cbe11a47abcf32b0ed61c338350f465e493cc8b3d9287fe076bf`。
复核后将标量渲染移入子模块，回收浮点可空包装的
SSA 准入，补上浮点宽度、参数数、结果数与 Map 查询
结果比较的静态合同测试。最终输入 3875 项，清单 SHA-256 为
`973cc4a1137a77f709b7d90a878a74af4373cbe35779b0fa53c23a756ad58887`。
最终候选的动态取余及 IEEE 回归在 macOS、离线 Linux
双入口通过；macOS 另通过 SSA 合同测试和原有
`List<Double>` 拒绝测试。首次完整矩阵双宿主一致，探针
140/154、综合程序 2/6。`p05_double` 转为 `run_ok`，
保留原探针源码、预期输出和历史基线。
最终完整矩阵在双宿主执行 640 次并通过证据比对。
与 §470 相比，唯一转正项为 `p05_double`，探针从
139/154 升至 140/154，综合程序维持 2/6。最终
格式、文档、尺寸与 codegen/CLI 严格 Clippy 通过。
故障注入留给 CI。

## 472. R4 Float? / Double? 的原生存在标记与提取

在同一工作树以 `Float?` / `Double?` 原生回归复现
`UnsupportedNode`。Refine 与兼容 lowering 接入
ADR-0033 显式存在标记及原位宽载荷，复用已验证的
判空和非空提取。双入口回归区分 `null`、正零和负零，
核对 Float 的 32 位舍入、`when` 非空臂读取及 `!!`
成功边。该用例未覆盖 `!!` 的空值终止。

独立复审发现顺序容器可借 nullable 包装绕过既有浮点
准入限制，旧候选在正常 CLI 入口成功运行了
`List<Double?>`。修复在容器准入处同时检查直接类型和
nullable 载荷，并为未包装及已包装元素保留拒绝回归。
复审确认普通 `Float?` / `Double?` 和现有 Map 键边界。

共有 LLVM `NullableBranch` 会先暂存 inline 载荷再读标记，
与 ADR-0033 对空值载荷的读取约束存在差距。当前空值构造
将包装清零，本轮未观察到输出错误，该共享路径待单独复审。
浮点 `Display` 的有限值、负零及特殊值文本未找到精确约定，
本片未修改浮点插值或 conformance 预期。
另一个独立单文件探针 `a == b` 的两个参数均为 `Double?`，
通过前端 `check`，原生生成仍报 `UnsupportedNode`。该用例
登记为后续 R4 可空值等值比较缺口。

最终证据位于 `.local-artifacts/0392-472-nullable-float`。
远端 main 与工作树 HEAD 一致，为 `4c8fc369`。冻结输入
3875 项，清单 SHA-256 为
`05f0db6c308eded35d0e0b1af162eb38f0c40e7e1e7dda16901ed937f18dfe13`。
macOS arm64 与离线 Linux x86_64 各通过两项双入口原生回归、
容器准入回归及 CLI build。macOS 也从正常 CLI 入口确认
`List<Double?>` 恢复明确拒绝。两宿主执行完整 conformance
640 次，证据校验及跨宿主比对通过。探针 140/154、综合程序
2/6，160 项逐项判定与 §471 一致。macOS codegen/CLI 严格
Clippy、格式、文档、尺寸与差异检查通过。故障注入留给 CI。

## 473. R4 可空浮点的等值与判空

在 §472 的基础上，双入口原生测试复现同位宽可空浮点等值
比较的 `UnsupportedNode`。Refine 与兼容 lowering 复用原
`Compare`，保留从左到右的单次求值和操作数提前退出；裸
浮点与同宽可空浮点比较时物化存在包装。SSA verifier 只接受
同一可空浮点类型的 `==` / `!=`，测试区分位宽、裸值、排序
及 Map 查询结果。

LLVM 先取存在标记；至少一侧为空时直接让两个标记决定等值。
两侧存在才读取载荷并执行 IEEE 等值，`!=` 对结果取反。
原生输出覆盖空值组合、非空相等/不等、NaN、正负零、两种
混合操作数顺序、求值次序、右侧提前返回及兼容循环中的比较。
路由测试分别证实实际 Refine 和兼容入口，并核对 LLVM
存在分支之前没有读取载荷的指令。

独立复审发现旧兼容路径按析构事实准入临时值，导致合法
的可空浮点调用结果无法与字面量 `null` 比较。扩展原生
回归先复现失败，再让 Copyable 浮点比较走其已定型的表示，
保留其他临时值的清理合同。复核确认两种字面量位置、命名
描述符优先级及原 Map/MoveOnly 路由。

另实跑两个 macOS 探针：与求值结果为 `Nothing?` 的 `if`
表达式比较、与返回 `Nothing?` 的调用比较。两者均通过前端
`check`，原生生成仍报 `UnsupportedNode`。前者需补带求值
效果的上下文物化，后者还涉独立 `Nothing?` ABI。§472 中共有
`NullableBranch` 的提前载荷读取问题仍另行跟踪。

最终证据位于 `.local-artifacts/0392-473-nullable-float-equality`。
基线 `4c8fc369`，冻结输入 3875 项，清单 SHA-256 为
`7caeba4310c4de14929983d2baa102cadd8b21866edd536cc9b5a464ad56bbb9`。
macOS arm64 与离线 Linux x86_64 各通过路由与 SSA 合同测试、
三项双入口浮点原生回归和 CLI build。完整 conformance 在
双宿主执行 640 次，证据与宿主间比对通过。160 项判定与 §472
一致，探针 140/154、综合程序 2/6。macOS codegen/CLI 严格
Clippy、格式、文档、尺寸与差异检查通过。故障注入留给 CI。

## 474. R4 带标记 NullableBranch 只在非空边读取载荷

以动态 tagged 可空值复现 LLVM 在执行判空分支前提取载荷并写入
借用槽，违反 ADR-0033 的空载荷读取约束。带标记的 inline
nullable 和已注册 presence wrapper 现在只在非空边提取和暂存
载荷。借用槽仍在函数入口分配，每次到达其非空边重新填充。
所有非空显式参数和隐式借用视图的 PHI 使用实际 LLVM 前驱，
空边直接以原判空块为前驱。

独立复审核对了动态边、支配、多个前驱、回边与调试位置，并确认
句柄、指针 niche 和借入判空的原有 ABI 路径。新增动态标量
测试覆盖 tagged inline 和 Map 结果，验证判空块的读取位置、
非空借用槽的初始化与 PHI，并实际执行两臂。带标记的 String
结果测试核对非空借用输出以及一次分配、一次释放。新 fixture
未单独构造多起点共享 proof、回边或可空 Map owner，复核按
相关控制流与布局合同静态覆盖。

基线为 `4c8fc369`，最终证据位于
`.local-artifacts/0392-474-nullable-present-edge`。冻结输入
3875 项，清单 SHA-256 为
`c41c7dcebb9b0f727382b409ee0cd099aee308a35547ebf58444b584193794d5`。
macOS arm64 与离线 Linux x86_64 各通过 27 项定向测试、
CLI build 和完整 conformance。定向测试覆盖 Map 取出、资源
回边、Rc 句柄及浮点和文本各自的单文件／项目入口。640 次
conformance 的跨宿主比对通过，探针 140/154、综合程序 2/6，
160 项判定与 §473 一致。macOS 严格 Clippy、格式、文档、
尺寸与差异检查通过。故障注入留给 CI。

## 475. R4 Nothing? 的原生存储与可空浮点比较

复跑 §473 的控制表达式和函数返回探针。新增双入口回归在修复前
的原生 lowering 报 `UnsupportedNode`。Guide 03 与 Guide 11
定义 `Nothing?` 只有 `null` 一个有效值，且是 Copyable。
复用 ADR-0033 的 tagged inline nullable 框架，不可实例化的
载荷只占零大小布局。Value、Borrow 和返回都使用同一带标记表示。
`NullableNull` 物化空值，判空恒为真。verifier 禁止构造载荷，
拒绝载荷类型作为 SSA 实体或函数结果，也拒绝生成非空证明。

有副作用的 `if`、直接调用和借用实参仍先完成原求值，Refine
控制结果与比较操作数使用各自已验证的类型。转换到 `Float?` 或
`Double?` 时产生该目标类型的普通空值；比较复用已验证的
nullable 等值。独立 `Nothing?` 判空与两个同类型值的等值
同样可用。测试覆盖左右两个操作数位置、函数返回/参数、局部
存储、提前退出、资源析构和兼容循环。

独立复审复现兼容 Borrow 临时值适配缺事实，以及 Refine 的外层
nullable 预期污染比较内部，修复后分别保留已验证的 loan 事实
及比较操作数的真实类型。另复现 Refine 把 Copyable `Nothing?`
提早变成 MoveOnly 实参，导致后续实参 return 时留下未清理的
包装。现在按原顺序完成源求值，仅在真实调用边生成拥有型空值。
回归覆盖调用前退出和正常交付，独立复核确认双方的顺序、终止
和所有权账本。

独立探针确认 `String?` 与非字面量 `Nothing?` 的直接比较仍报
`UnsupportedNode`，`List<Nothing?>` 仍由前端 L0125 拒绝。
兼容按值适配本片限 Copyable 目标，Refine 才接入上述 MoveOnly
实参延后物化。`Nothing?` 调用结果的 `!!` 通过前端 `check`，
原生 lowering 仍报 `UnsupportedNode`，应核对其求值后终止路径。

基线 `4c8fc369`。最终证据位于
`.local-artifacts/0392-475-nothing-nullable`。冻结输入 3875 项，
清单 SHA-256 为
`48ea3af024bfdc89cd2f811c00ffe43cd7d7ad05fdee6199e6aad092dac42374`。
macOS arm64 与离线 Linux x86_64 各通过 24 项定向检查和
CLI build。补跑单文件／项目探针在两个宿主都由被调用方读取
MoveOnly 实参，确认正常路径确实收到空值，提前返回只执行
源表达式的副作用。完整 conformance 共 640 次，跨宿主校验
通过，160 项判定与 §474 一致。探针 140/154，综合程序 2/6。
macOS 严格 Clippy、格式、文档、尺寸与差异检查通过。
故障注入留给 CI。

## 476. R4 Nothing? 非空断言终止路径

以双入口普通源码回归复现 `Nothing?!!` 的 `UnsupportedNode`。
Unit 类型检查保留原 operand 与 assertion 描述符，并发布断言无
正常后继。所有权检查先读取操作数，保留其已有退出边。若求值
完成，断言进入 Abort，drop planner 不再规划正常退出的清理。

Refine 和兼容 lowering 核对同源 typed／owned 描述符，复用既有
的 compiler-bound Abort 原因、源码起点与运行时。操作数正常
完成时直接终止，无非空证明或取载荷操作。操作数自身提前
return、break、continue 时沿其原路径结束，未完成的调用留在
不可达处。普通 nullable 的已验证非空提取路径继续存在。

前端回归核对操作数与断言各自的完成性，以及 Abort 没有作用域
drop；SSA 回归分别证实真实 Refine 和兼容路由，核对一次调用、
无载荷取用与准确 Abort 原因。CLI 两入口回归检查正常执行的 operand
析构、调用者不展开、stdout、stderr 路径与行列以及 134 退出码。

首次独立复审复现两项遗漏。裸 `when` 操作数带临时资源时，
跳过操作数的外层清理会使 Refine 产生 `InvalidModel`，加括号
却能先析构再终止。泛型的拥有参数具体化为 `Nothing?` 时，
所有权计划仍可以是 `Consume`，仅接受 `Copy` 会报
`MissingFact`。两条 lowering 路径现都保留操作数的完整正常
清理，对于匹配的 `Copy` 或 `Consume` 计划都按具体类型
终止，并且不生成载荷路径。两处经独立静态复核关闭，新增双
入口回归覆盖裸和加括号的 `when` 及泛型实例。

基线为 GitHub `main` 的 `4c8fc369`。最终证据位于
`.local-artifacts/0392-476-null-only-assertion`。冻结输入 3877 项，
清单 SHA-256 为
`3e17365ad0a2500109ba597278224b18a5705c931acdf3de3f0cb2c2d705bea2`。
macOS arm64 与离线 Linux x86_64 各通过 58 项定向检查及 CLI
build。两宿主均验证 640 次 conformance 调用的证据，跨宿主
一致，且各 160 项判定与 §475 一致。探针 140/154、综合程序
2/6，零新增回归。macOS 严格 Clippy、格式、文档、文件尺寸和
差异检查通过。故障注入留给 CI。

下一项 R4 已用本轮修订后的 CLI 确认。普通 `String?` 与
非字面量 `Nothing?` 的 `==`／`!=` 表达式通过前端，但
单文件和项目入口均报 `UnsupportedNode`。复现及对照输出见
`.local-artifacts/0392-477-nullable-text-presence`。后续须核对
空字符串与 `null` 的区分、两种操作数顺序及临时值清理。

## 477. R4 非字面量 Nothing? 与 String? 混合比较

同一源码的单文件和项目入口均复现原 `UnsupportedNode`。
普通 Unit ownership 将混合比较纳入共享读和临时值清理。
泛型模板先记录可能的 owner，具体 `String` 实例履行析构，
具体 Copyable 实例沿已验证的值类型执行。前端继续拒绝右侧
写入重叠字段，诊断 L0135。

Refine 和兼容 lowering 按源码顺序执行两个操作数。检查只读
`String?` 的存在标记，空字符串算存在。为支持合法的不相交
字段访问，字段在其操作数处读取标记并结束短借用，父对象仍按
前端事实在比较后或正常提前退出时清理。独立复审复现了兼容
单级字段、Refine 不相交写入和泛型临时清理三个缺口，均有
对应的双入口回归。真实路由测试核对两条路径均走 presence
检查，并验证 SSA 和两个调用的顺序。

为满足尺寸门禁，字段事实查询移到 `refine/field_operand.rs`，
可空字符串操作数准入移到既有的可空等值检查旁。字段身份、
借用和清理继续按同一组前端事实验证。

基线 `4c8fc369`，冻结源码 3880 项，清单 SHA-256
`70104d55d17707394b187a2beb27460fbd8ef9bd45a2c709e0d46fe9bda8d95a`。
证据位于 `.local-artifacts/0392-477-nullable-text-presence/final`。
macOS arm64 和离线 Linux x86_64 各通过 51 项定向测试及 CLI
build。每宿主的 640 次 conformance 调用通过证据校验，跨宿主
一致，160 项判定均与 §476 一致。探针 140/154，综合程序 2/6。
macOS 严格 Clippy、格式、文档、尺寸和差异检查通过。
故障注入留给 CI。

多级临时字段链 `make().inner.text == missing()` 在 Refine
实跑正常。置于数组 `for` 并使用兼容路线时，前端通过，
两个原生入口仍报 `UnsupportedNode`。前端的临时借用目标是
`make()`，而兼容路径当前要求字段的直接接收者与该目标相同，
因此拒绝 `make().inner`。后续验证逐段消费 typed 投影事实，
并核对读取后结束子借用、父对象只析构一次的顺序。该项继续
作为 R4 缺口，复现及预期输出见
`.local-artifacts/0392-477-nullable-text-presence/nested-compat`。

## 478. R4 兼容 lowering 的多级可空字段

在数组 `for` 中复现 `make().inner.text == missing()` 的
`UnsupportedNode`，与同程序去掉循环后的 Refine 输出对照。
兼容路径现从字段逐段检查 typed 投影、AST 接收者与来源，再
与所有权发布的临时根或具名字段路径核对。只求值一次根；从
该根建立各级共享字段借用，复制末端的存在标记并逆序结束
子借用。临时值和具名值仍在原前端清理点析构。

双入口回归覆盖三段字段链、临时根、拥有根、借入形参、等值
两种操作数顺序、空串和提前 `return`、`break`、`continue`。
SSA 回归确认走兼容路线，验证字段借用父子关系与清理顺序。
独立复审追加了多级 `inout` 根的不相交字段写入，并复测已有
单级字段路径。

基线 `4c8fc369`，冻结源码 3880 项，清单 SHA-256
`bc7ab01c64a5427ae92e92d0253ba58a0fafe2356a5d32b7d27f184d717770ec`。
macOS arm64 与离线 Linux x86_64 各通过 53 项定向测试及 CLI
build。每宿主的 640 次 conformance 调用通过证据校验，跨宿主
一致，160 项判定均与 §477 一致。探针 140/154，综合程序 2/6。
严格 Clippy、格式、文档、尺寸和差异检查通过。原生及 SSA
证据位于 `.local-artifacts/0392-478-nested-field-comparison`。
故障注入留给 CI。

词法 `borrow val alias = view(owner)` 的字段链与非字面量
`Nothing?` 比较仍通过前端并在普通原生入口报
`UnsupportedNode`。字面量 `null` 比较同样失败；
`borrow val alias = owner`、将 `view(owner)` 的结果作为
普通借入实参、具名借入形参访问同一字段链三组原生对照均通过。
复现记录位于 `.local-artifacts/0392-478-nested-field-comparison/borrow-alias`。

## 479. R4 词法借用返回的多级字段判空

SSA 回归确认拒绝出现在选路之前。旧的整函数借用准入可识别
可空借用结果自身的观察，但漏掉以借入对象为根的字段链。
本轮按同一 source unit 核对名字、typed 字段投影和比较类型。
非字面量比较要求比较及操作数位置的唯一共享借用事实与词法
绑定来源一致。Boolean 包装关系按源缓存，避免每个绑定重扫
整份 AST。

实际观察同一来源、同一类型子绑定的比较时，准入可沿该词法
链传递。callee 返回字段时，父绑定的存储类型可与来源根不同。
验证使用父绑定的类型和已发布的同一借用来源。独立复审曾
复现普通字段子借用错误跳过原准入，修正后只在能追到对应
Scoped 调用结果和实际字段比较时启用专用路径。该负例保留。

原生验证覆盖在 `if` 与 `while` 中读取多级字段、左右操作数、
字面量判空、短路、比较后提前返回、callee 返回实投影和词法
子借用。跨文件项目和单文件输出逐字节一致。对同一来源申请
重叠独占借用仍报告 L0135。

基线 `4c8fc369`，冻结源码 3880 项，清单 SHA-256
`e8308f7aa301ab0ed9a029d951788ba2706d5d3055ad5022f0c24db4020d9f99`。
最终门禁在 `.local-artifacts/0392-479-borrow-result-fields/reviewed`。
macOS arm64 与离线 Linux x86_64 各通过 58 项定向测试及 CLI
build。每宿主校验 640 次 conformance 调用，跨宿主一致，
160 项判定与 §478 一致。探针 140/154，综合程序 2/6。
严格 Clippy、格式、文档、尺寸及差异检查通过。
故障注入留给 CI。

右操作数内嵌的 `if` 提前返回，以及借用结果跨数组 `for`，
仍有通过前端而原生报 `UnsupportedNode` 的复现，继续纳入 R4。
证据位于 `.local-artifacts/0392-479-borrow-result-fields`。

## 480. R4 字段判空操作数中的提前返回

旧 SSA 回归在选路之前拒绝字段混合比较中内嵌的 `if`。原
普通源码在前端通过，原生报 `UnsupportedNode`。本轮只从
已经验证的混合比较进入另一操作数，沿显式控制尾部查找
提前返回。逐词法绑定核对 source-qualified `ControlTransfer`
结束事实后，才准入该返回及其所属控制。前置的无关 `if`
仍由原准入处理，SSA 回归保留对应的拒绝用例。

原生双入口回归核对左右操作数、裸返回、带值返回、嵌套
`else if` 和继续路径的输出及清理。左侧读取字段标记时
结束短字段借用，提前退出时由原前端事实结束词法借用，
再析构所有者与其字段。独立复审复测了经 callee 投影的
同源子借用，单文件与跨文件项目 stdout 逐字节一致。

基线 `4c8fc369`，冻结源码 3880 项，清单 SHA-256
`94bdfa8c41d3dde46f84f3cad127fec43a0a17c6dc88729ab82b6ba81d340bc1`。
macOS arm64 和离线 Linux x86_64 各通过 60 项定向测试及 CLI
build。每宿主校验 640 次 conformance 调用，跨宿主一致，
160 项判定与 §479 一致。探针 140/154，综合程序 2/6。
严格 Clippy、格式、文档、尺寸与差异检查通过。
故障注入留给 CI。

仍待处理词法借用调用结果跨数组 `for`。已保存两次迭代的
双入口复现、具名借入参数的对照输出，以及将数组先赋给
具名变量的对照。证据位于
`.local-artifacts/0392-480-borrow-field-operand-return/array-for`。

## 481. R4 词法借用结果跨数组 for

具名数组和临时数组原在整单元准入报 `UnsupportedNode`，
此时尚未选择 lowering 路径。本轮按 source unit 核对 typed
Array 描述符、循环源码身份及观察所在的循环体；只给实际被
观察的词法绑定登记它进入的数组循环。区间父链覆盖嵌套循环，
兄弟循环、数组源表达式里的比较及其他绑定不共用该准入。
原有 ownership iteration plan 和 Refine verifier 继续验证
实际提供者及退出清理。未新增 lowering 路由。

双入口回归覆盖临时数组、具名数组、借用结果的子绑定及
`continue`、耗尽、`break`、`return`。数组元素的析构输出
证实具名数组留到词法退出，临时数组在返回时逆序清理，
借用源各析构一次。SSA 回归比较具名、临时、嵌套三种正例并
保留 sibling loop 负例。独立只读复审核对了字段身份和区间
父链；跨文件项目与单文件 stdout 逐字节相同。

本轮重新 fetch 的 `main`、`origin/main` 和工作树 HEAD 均为
`4c8fc369`。最终验证使用同一份 3880 项冻结输入，清单 SHA-256 为
`6bccdb125f9069c48fe3f6ddfeeaeef19d1a98fc0e079c85395a49cae34ef299`。
macOS arm64 与离线 Linux x86_64 各通过 62 项定向测试和 CLI build。
两份 conformance 证据包和跨宿主校验均通过，探针 140/154，综合程序
2/6，双宿主共 640 次入口执行，160 项判定与 §480 一致。macOS 严格
Clippy、格式、尺寸、文档和差异检查通过。故障注入留给 CI。

## 482. R2／R4 内建 Display 显式调用与浮点插值复现

复核 §468 的浮点插值见证。在 §481 已验证的两个编译器上，
13 个样例各执行单文件／项目 `check` 和 `run`，双宿主共 104 次。
归一化输入路径后，状态、成功输出和首条诊断一致。
直接插值 `Double`、`Float` 以及 `T : Display` 实例化到 `Float`
均通过前端，原生生成报 `UnsupportedNode`。同一泛型插值实例化
到 `Int` 输出 `3`，具体名义类型显式调用输出 `label`，直接插值
`Double?` 仍由前端报 L0085。

显式 `toString()` 单独在成员准入失败。`T : Display` 与
`Float`、`Double`、`Boolean`、`Char` 报 L0165，`Int`、`String`
报 L0080。隐式插值已有 `UnitDisplayTarget`，其中浮点内建见证
未进入 Refine 的标量转换准入，SSA `StringFromScalar` 和 LLVM
转换也尚无浮点实现。分别保留 R2 与 R4 缺口。

Guide 03／04 已要求浮点支持 `Display`。沿 §472 再核对，浮点
文本仍缺少精度、指数表示、负零及非有限值拼写的精确定义，
原生浮点转换待该输出规则确认。样例和双宿主结果见
`.local-artifacts/0392-481-borrow-field-array-for/float-display`。

## 483. R2／R4 接口上界成员选择与具体实现分派

本轮固定 `4c8fc369`。`p11_interface_bound` 的泛型调用原报
L0165。成员选择现在使用接口上界的签名和继承替换，调用
仍记录真实 `T`、Borrow 模式与所选 requirement。回归核对
`Child<Int>` 继承 `Base<Int>` 时发布的 `Int` 实参与正确接收者。

独立复审发现仅修前端会让默认体覆盖错跑。原生复现的覆盖／
默认／显式 `super` 输出为 `1 1 1`，应为 `2 1 1`。另有经
委托及祖先上界的调用忽略替换默认体。具体分派复用前端覆盖事实，
区分实际 `StaticSelf` 及显式 `super<I>`。委托祖先通过已认可
的同合同有效成员发布，别名只在现有有效下一跳均无匹配时参与。
回归验证本地覆盖、继承默认体、委托、显式 `super`、默认体内部
普通 `this` 调用及逆序析构。跨文件项目另核对两个不同实例和
具体 `Display.toString()` 重复调用。

独立复审又发现泛型覆盖体独占 `Extra<Y>` 时没有发布
`Extra<Int>`。直接调用或插入显式 `Extra<Int>` 都能绕过原失败。
直接上界调用现在通过已验证覆盖的 owner 全量替换与 callable
实参入队，复用现有有界实例工作队列及存储检查。两项存储对照
验证 `List<Nothing>` 只在实际选中的覆盖或默认体被实例化时触发
对应检查。本轮验证了覆盖非法和被覆盖默认体非法这两个方向。

最终运行冻结输入含 3880 项，清单 SHA-256 为
`8990e260a67991a5648063ec1e2c632356781984676a296a66a01089ffc49571`。
macOS arm64 与离线 Linux x86_64 各通过 302 项类型、86 项
SSA 规划和 4 项双入口原生测试。4 项标准预算压力测试按现有
CI-only 标记保留。两宿主的具名资源重复读取和析构 stdout
与跨文件项目字节一致。macOS 严格 Clippy、格式、文档和尺寸
门禁通过。冻结后的审计计数断言由 5 项定向 Python 测试另行验证，
编译和 conformance 输入与该清单一致。

两宿主共完成 640 次 conformance 入口执行并校验证据包。
`p11_interface_bound` 两入口均输出 `dog\n`，这是对 §481 的
唯一判定变化。探针升至 141/154，综合程序保持 2/6。

后续阻断已缩到 `I.through()` 的非泛型默认体内部再经 `StaticSelf`
调用泛型覆盖时，具体覆盖体仍可能缺少 `Extra<Int>`。当前最小
复现通过 `check`，原生报 `MissingFact`。默认体具体 `Self` 需随
实例需求传播到前端类型发布，不能用额外声明强行补齐。显式
内建 `Display` 和浮点插值仍在 §482 的边界。原始样例见
`.local-artifacts/0392-483-interface-bound-member`，故障注入留给 CI。

## 484. R2／R4 默认体的具体 Self 与泛型覆盖实例发布

在 `4c8fc369` 的 §483 累积工作树上先将 `this` 转调固化为原生
回归。旧版 `check` 接受，而执行在 `MissingFact` 停止。显式源码
调用的实例队列现在以目标、类型实参和具体 `StaticSelf` 区分默认体。
嵌套默认调用继续传播该类型，选出的覆盖体按既有预算与存储合同实例化。
显式 `super<I>` 保留指定默认体。多段委托沿已验证的下一跳转发，
逐段重映射 owner 实参与 callable 实参，默认体使用实际委托端点。

原生回归覆盖两个接收者通过同一默认体链得到不同结果、显式
`super`、两跳委托和析构。类型回归分别检查默认体内所选泛型覆盖
的非法存储，以及同一默认体分派时被覆盖的非法实现不进入实例队列。
普通无替换成员仍不进入模板索引。独立复审核对了实例键、委托实参、
显式 `super`、回滚和预算。

冻结输入 3880 项，清单 SHA-256 为
`d1a6bacaa55bf58b47a8b24bbd2ceb52774136ba6c22aea0ca6197160b8ab493`。
macOS arm64 与离线 Linux x86_64 各通过 303 项多文件类型、8 项
泛型体、86 项 SSA 规划和 6 项原生测试。4 项预算压力测试按现有
CI-only 标记保留。跨文件项目及单文件均输出 `2 3 2`，随后依次
析构 `second` 和 `first`。泛型 owner 与 callable 实参对照的
两个直接／委托调用均输出 `2`。macOS 严格 Clippy、格式、文档、
尺寸和差异检查通过。故障注入留给 CI。

全量 conformance 两宿主共执行 640 次，证据包与跨宿主比对通过。
探针 141/154、综合程序 2/6，160 项判定均与 §483 一致。

同一次复审在隐式 `Display` 插值发现额外入口。仅从插值进入继承
默认体时，显式源码调用表没有该需求，原生仍报 `MissingFact`。
试验将已定型的插值计划接入队列后，直接和泛型插值能输出 `2`，
已明确退出的嵌套插值也可排除。但命名常量导致前缀退出时，普通
类型计划仍含后缀，试验会误实例化 `List<Nothing>` 并报 L0125。
该试验与两个正反复现保存在 `.local-artifacts/0392-484-contextual-body`，
最终候选在两宿主仍输出 `stopped`。继续复审时需以能证明转换完成的
事实发起隐式需求，避免从所有 `Format` 粗略反推。

## 485. R2／R4 以所有权确认的路径补齐隐式 Display 实例

在 §484 冻结候选复现。继承默认体的隐式 Display 转换通过 `check`，
原生生成缺少覆盖体独有的 `Extra<Int>` 并报 `MissingFact`。
普通明确退出和命名常量前缀退出仍输出 `stopped`。

共同入口先建立原有类型和所有权产物，再从同轮产物取出已完成的
插值转换与实际进入且可分派的普通调用。类型补齐复用原有已验证
覆盖／委托路由及有界实例队列。显式调用的既有类型检查保留，
额外 Display 需求从已确认的根沿完整调用路径传播。泛型体中独立
检查过的插值不能绕过其未执行的调用点成为根。

补齐后的类型重新通过准入并重新生成配对的所有权产物。CLI 单文件
和项目入口均消费新产物；LSP 单文档恢复与工作区在原基础所有权
能力内复用同一补齐入口。含已有所有权诊断的产物保留原诊断，
常量程序的 CLI 需求只采用真实常量控制能力的所有权结果。

独立复审重现了仅给插值边标可达性时会误报的两条路径：被前缀
跳过的普通 `show` 调用，以及已开始格式化的默认体内部被跳过的
`this.later()`。当前原生用例覆盖两条普通边、直接及泛型插值、
嵌套退出和析构。可达的非法泛型存储以 L0125 进入用户诊断。
早期试验和复现留在 `.local-artifacts/0392-485-display-demands`。
故障注入留给 CI。

在 GitHub 比较远端 `main` 与基线 `4c8fc369`，结果为 identical。
冻结清单包含 3881 项，SHA-256 为
`3d0d7819ddb69aa50565977afde426607b21f90ac8c5d04ca46fedd70d4303e9`。
macOS arm64 通过 8 项实例模板、5 项单文档恢复、11 项 LSP、
11 项 CLI `check` 和 7 项原生回归，以及严格 Clippy。原生测试
初跑有两项现有用例超过 10 秒，保留原日志；串行复跑 7 项全部通过。
跨文件项目与等价单文件均输出 `2\n2\nouter\nstopped\ndrop actual\n`，
stderr 为空。LSP 同时验证已有 L0131 或 deferred 结果时保留原诊断。
macOS 的公开 conformance 探针为 141/154，综合程序为 2/6，
无新增回归；与 §484 逐项对账的 160 条判定一致。

Linux x86_64 离线验证首次在 LSP 构建停滞，保存原日志后关闭该
容器。单任务 Cargo 重跑通过 303 项多文件类型、8 项模板、5 项
恢复、86 项 SSA 规划、11 项 CLI `check` 与 7 项原生回归。
4 项 CI-only 压力测试按原标记跳过。两入口的 stdout 与 macOS
及预期文件逐字节一致，stderr 为空。

macOS 补验同一冻结输入的 303 项多文件类型与 86 项 SSA 规划。
Linux 在同次复跑末尾通过 11 项 LSP 测试。
双宿主公开 conformance 的 640 次入口执行已通过证据包对账。
两宿主各有 141/154 个探针与 2/6 个综合程序通过，逐项与 §484
一致。此处记录的构建、原生和语料结果均绑定 `4c8fc369` 基线，
尚无本批的 PR-event CI。

## 486. R2／R4 复查显式内建 Display 的分层缺口

用 §485 的冻结编译器复跑 §482 的 13 个见证。macOS 单文件和
项目各执行 `check` 与 `run`，共 52 次。`T : Display` 实例化到
`Int` 的显式 `value.toString()` 现已通过前端；两入口在原生计划
阶段仍报 `UnsupportedNode`。同类型的隐式插值输出 `3`，名义类
显式调用输出 `label`。直接对 `Int`、`String` 调用仍报 L0080，
`Boolean`、`Char`、浮点仍报 L0165。浮点插值仍是独立的 R4 缺口，
`Double?` 则按现行 Guide 由前端报 L0085。

直接调用的成员选择目前只为内建 `String` 查标准扩展，其余内建
类型走非名义类型的扩展查询。泛型调用经过正式 requirement 选择，
但在 Unit 计划的具体派发中没有内建 witness。两层应使用同一
`Display` requirement 身份。现有标量插值已提供整数、布尔、字符
与字符串的转换路径；显式调用还要分别核验 `String` 的 Borrow
接收者与自有返回，可复用 `StringClone`，并核验求值提前退出。
浮点输出规则仍依 §482 跟踪。

另在 macOS 双入口增加两个原生见证：带输出副作用的整数接收者，
以及同一词法借用连续两次转为自有 `String` 后再次读取原借用。
两个见证均通过 `check`，`run` 均在同一计划阶段失败。修复后预期
分别输出 `receiver\n3\n` 与 `label\nlabel\nlabel\n`。
复现源码和日志在 §485 证据目录的 `reviewed/explicit-probes`。

## 487. 对齐 main 的词法 owner 事件验证

远端 `main` 在 §485 验证期间合入 #483，从 `4c8fc369` 快进到
`d44c8b9a`。19 个上游文件与本地改动只有 `parser/engine/recovery.rs`
重叠。以原基线三方合并，保留上游的 owner 事件验证与现有 `when`
分支缓存及恢复停止条件。对照 §485 清单，原有输入仅 `lexer/mod.rs`、
`parser/engine/recovery.rs` 和 `source.rs` 发生变化；上游另新增三个
Parser owner 事件 Rust 文件。所有其余冻结输入均与合入前一致。

macOS 默认测试线程复现 §256 的 64 层未闭合下标栈溢出。
将 `when` 边界判断移出 prefix 帧的试验仍溢出，已撤回。最终只
拆开下标键解析与下标收尾，递归解析键时无需保留括号、诊断和
恢复分支的临时变量。独立复审逐项对照游标、poison、诊断数、
分隔符、恢复顺序和 AST span，未发现 P1/P2。未调整默认栈大小
或语法深度预算。

修订冻结清单为 3884 项，SHA-256 为
`ab9444b71f777435e54423c50ffe7ec42010d413b99748ce8a042c59ff196bb0`。
macOS 调试测试二进制的 `parse_index` 函数序言分配从 8512 字节
降至 1744 字节，含寄存器保存区。两个宿主默认栈各通过 73 项
Parser 单元测试（含原 64 层回归）、20 项 Lexer 与 10 项 Source/Span。
macOS 另通过 23 项下标恢复集成用例、前端严格 Clippy，并在实际 CLI
两入口对 64 层未闭合源码各返回同序的 65 条正常诊断和退出码 1。
初跑与首个试验的 abort 留在
`.local-artifacts/0392-487-main-0510/reviewed`。

macOS 和 Linux 均通过 §485 的公共 `check` 与原生定向回归。
两宿主单文件与跨文件项目的输出及析构顺序均与 §485 预期
逐字节一致，stderr 为空。§485 的完整 conformance 仍归原冻结
基线，本节为新基线的定向复验。最终核对远端 `main` 仍为
`d44c8b9a`，故障注入留给 CI。

## 488. R2／R4 显式内建 Display 的选择、转换及接收者清理

沿 §486 区分前端选择与原生转换。普通内建调用和 `T : Display`
依次使用同一标准库授权的 requirement。已选调用记录真实接收者
类型、Borrow 模式与自有 String 结果；同名用户方法保持普通分派。
浮点成员选择也遵守该合同，原生转换仍按 §482 保留明确未支持边界。

原生计划对整数、布尔、字符和 String 做内建转换。整数、布尔和
字符复用标量转换，String 经实际共享借用复制。Compatibility
也消费这一合同；其普通字段复用已验证的堆字段链，标量字段读取
按 Copyable 值处理。所有权阶段把临时字段、容器和 Rc 载荷的
背后 owner 保留到共享读取完成。命名 Rc 载荷的析构交给现有
`CallReturn` 流程。括号匹配使用投影身份，临时清理继续使用已
发布的 owner 身份。

首版 Refine 样例通过，但独立复审在循环 Compatibility 路由及
投影接收者找到了遗漏。实跑确认本地 Rc 的 `.clone()` 对照通过，
`.toString()` 会漏消费提前登记的析构；临时 List 还会在前端报
`InvalidUnitArgumentPlace`。各次红例与候选记录保存在
`.local-artifacts/0392-488-explicit-display`。验收覆盖普通与泛型调用、
接收者的单次求值、临时析构、控制退出和两入口原生输出。

另确认既有组合缺口。紧接同一函数内字符串拼接建立词法借用时，
§487 编译器的 `.clone()` 与本节候选转换均报 `UnsupportedNode`。
独立的生产者返回拼接结果后再借用可正常运行，作为本轮的堆分配
String 对照。原样例单列跟踪，故障注入仍留给 CI。

复核冻结输入含 3884 项，清单 SHA-256 为
`e3051d0cbb909b18c7bf214a8bda8bc3bba1e3133a297a9249d33d9a632da074`。
独立复审核对 Rc 根句柄、分组宿主与当前 receiver 字段的最终差异，
未发现新的 P1/P2。macOS 通过 304 项多文件类型、11 项 CLI
检查、9 项双入口原生回归。跨文件项目验证泛型分派、仍可读取
原借用，以及先析构临时宿主再使用已复制的 String。八种整数的
有符号最小值及无符号最大值均与预期十进制文本一致。

离线 Linux x86_64 的同一快照也通过 304 项类型、11 项 CLI、
9 项双入口原生回归。跨文件项目及八种整数对照的输出与 macOS
逐字节一致。两宿主的显式 Double 样例均通过 `check`，原生阶段
给出 `UnsupportedNode`。macOS 严格 Clippy、格式、文档检查和
尺寸门禁均通过。未触发本批的 PR-event CI。

旧快照上的五个拼接对照进一步收窄了后续范围：单独拼接、字面量
建立借用和跨函数返回拼接结果均输出 `label`；同函数拼接后建立
借用，无论直接读取还是比较后读取，都通过 `check` 却在 native
返回 `UnsupportedNode`。见 `0392-488-explicit-display/next-concat`。

## 489. R4 词法借用前后的 String 拼接初始化

§488 的冻结编译器对最小样例 `check` 成功，原生 `UnsupportedNode`
指向借用建立前的拼接表达式。带路线观测的隔离副本确认在 callable
路线选择前已拒绝。单独拼接、字面量借用和跨函数返回拼接结果
三个对照均通过。

复用常规 String 借用结果的词法遍历。只在绑定建立前、已证明
结束后或互斥语句区域，登记类型事实确认的普通 `val`／`var`
拼接初始化。分组和嵌套拼接沿表达式树逐节点核验左、右和结果
均为内建 String。活动借用区、其他运算、循环及控制转移仍按原
准入判定。本批只改变准入，SSA 消费原有借用和析构事实。

独立复审未发现 P1/P2，核对了类型身份、活动区与互斥臂的边界。
冻结输入 3884 项，清单 SHA-256 为
`9418c47fd9c9c2dc2f1c5680ead2c8b2e1f01394087463424b54af972f7d2e24`。
macOS 与离线 Linux x86_64 各通过 9 项原生测试、11 项公共
`check` 测试及构建。跨文件项目与等价单文件的四份输出均逐字节匹配预期，
覆盖左右求值顺序、UTF-8、内嵌 NUL、借用中比较和克隆、离开
借用后再次读取及拼接。Linux 用独立 LeakSanitizer 运行正常
项目程序，以禁用栈和寄存器根扫描的配置核验退出时无报漏，
不替代全套 ASan 与故障注入。macOS `lang-codegen --lib` 严格
Clippy 通过。活动区直接加入拼接的对照仍通过 `check`、原生
返回 `UnsupportedNode`，包括拼接两个独立值，以及借用作为左或
右操作数；最后一次读取借用之后再拼接原 owner 可以运行。
未触发本批 PR-event CI，故障注入留给 CI。
原始记录在 `.local-artifacts/0392-489-concat-borrow`。

## 490. R4 活动 String 借用中的拼接与实参提前返回

§489 冻结候选能检查借用活跃时的拼接，原生构建返回
`UnsupportedNode`。借用值分别作左右操作数、活动区拼接独立值、
直接打印拼接结果以及右操作数提前返回均已复现。普通调用内的
拼接还会触发 Refine 的临时析构准入。共享调用借入的是整个实参，
右侧的显式 `return` 发生时，已完成的左侧临时值仍由内层拼接持有。

词法准入仅沿已核验的内建 String 拼接和可正常返回的同步调用，
沿用活动借用的共享读取。对显式 `return`，查同一借用绑定的
退出事实。若整个拼接实参尚待形成，Refine 只接受精确
`ControlTransfer` drop 所指的已完成左侧临时值。左侧剥除透明
括号与前端事实身份一致，继续使用原 SSA 的交付、结束借用与析构
顺序。具名 `Nothing` 调用、内建 Abort、函数值调用、`inout`
接收者及 `inout`／跨线程实参不走本批的普通调用准入。

独立复审找出返回值中的拼接、调用模式和括号身份遗漏，修复后
在冻结候选上复核，未发现新增 P1/P2。冻结编译输入 3884 项，
清单 SHA-256 为
`fa738a8cdeff8d24d0dbd65c4f3420d5dd68920b8408f8dd106538b318728a03`。
以 GitHub 主线 `d44c8b9a` 为基线，macOS 与离线 Linux x86_64 各通过
12 项定向原生测试、11 项公共 `check` 测试及 CLI 构建。四份单文件
与跨文件项目的原生输出逐字节一致。带副作用的样例在提前返回时依次
输出 `left`、`return-value`、`drop` 和调用方打印的 `stop`；
正常路径打印拼接结果，随后仍可读取借用，最后析构一次 Marker。
两宿主均以 L0135 拒绝同一个活动借用冲突写回样例。正常进程退出的
Linux 独立 LeakSanitizer 未报泄漏，关闭了栈和寄存器根扫描。
macOS `lang-codegen --lib` 严格 Clippy 通过。本切片未触发 PR-event
CI，故障注入留给 CI。
原始记录在 `.local-artifacts/0392-490-active-concat`。

## 491. R4 后续实参提前返回时的共享临时资源

§490 的冻结候选正确处理先求值的 String 实参和重排命名实参。
把前置实参换成具名函数返回的普通类临时值后，`check` 通过，
原生构建报 `UnsupportedNode`。去掉提前返回、改借具名资源或移除
词法 String 借用的三个对照可运行。路线追踪显示 Refine 通过
signature 和 body 准入，临时析构检查缺少前置实参的 loan end，
于是落入无法生成该词法借用的 Compatibility。

初版将普通工厂返回纳入原有直接调用观察，macOS 和 Linux 各有两项
旧契约断言失败。保留这两项断言和旧表范围，从同一次实际 cleanup
顺序独立投影已完成的自有普通类结果在提前退出时的共享借用结束。
采用完整 loan、调用、实参和源码退出点身份，供 Refine 原有
准入核验借用先于 drop；临时值仍由原 builder 清理。新事实随分析
失败或 deferred 撤销，不依赖可关闭的动作观测，也不进入实验提供者。

独立复核核对旧契约、事实撤销、实例化类型和索引身份，未发现
新增 P1/P2。临时方法接收者的相邻组合仍复现 `UnsupportedNode`。
移除活动借用后仍失败，改用具名接收者或去掉提前返回后通过。
Refine 的临时接收者准入当前拒绝调用内的控制转移，另行跟踪。

冻结编译输入 3884 项，清单 SHA-256 为
`ef7df04c9f00017289a184c018a143c612213dbe4fd5631d5c90656fca3ddb3f`。
macOS 与离线 Linux x86_64 各通过 359 项直接调用与借用结束事实
测试、13 项定向原生测试和 11 项公共 `check` 测试，CLI 构建通过。
四份单文件和跨文件项目的原生输出逐字节一致。提前返回时先求值
返回值，再一次析构实参临时资源，随后析构外层局部资源；正常调用
完成后两份资源同样各析构一次。两宿主仍以 L0135 拒绝活动借用和
写回冲突。Linux 独立 LeakSanitizer 在共享挂载的构建收尾曾报 staging
身份变化，改在同一镜像的容器私有临时盘构建后复跑通过，无报漏。
macOS 的 frontend/codegen 严格 Clippy 通过。最终工作树单独执行 Rust
尺寸门禁，冻结清单之后只更新门禁配置，为编排接线登记精确上限
1003 行。未触发 PR-event CI，故障注入留给 CI。冻结输入及验收日志在
`.local-artifacts/0392-491-concat-later-argument`。

## 492. R4 临时共享接收者的实参提前返回

§491 冻结编译器对 `sink().accept(... return ...)` 通过公开
`check`，原生构建报 `UnsupportedNode`。原有 Refine 临时 receiver
准入在检查到调用范围内的控制转移时拒绝，已有 builder 则能在实参前
借用接收者，并在普通 return 先结束 pending loan 再消费 drop。

只对普通类、直接非委托同步调用的共享临时接收者扩大准入。显式
`return` 必须属于某个源码实参，接收者已正常求值且具有同源、
同表达式和同退出点的无条件临时 drop。分组以已检查的完整接收者
身份消费事实。Unit 按退出点和临时 owner 构建索引，实参身份按
源码跨度二分后确认。保留 break、continue、Abort、非共享接收者
以及借用或载体返回的既有边界。缺少匹配的 return drop 仍会拒绝
Refine，亦未扩张已有拼接表达式的词法控制准入。

独立复核覆盖 receiver 身份、动作顺序、分组以及源码实参跨度，
未发现新增 P1/P2。冻结输入清单 3884 项，SHA-256 为
`e30998accebd38947f6842d5ab96f7f3556028532b35e4d8e5398e1c1bb52964`。
macOS 和离线 Linux x86_64 各通过 359 项调用借用结束事实测试与
14 项定向原生测试、11 项公共 `check` 及 CLI 构建。相同单文件和
跨文件项目在两宿主的四份输出逐字节一致。提前返回路径先输出
返回值求值标记，再分别析构临时接收者和外层资源一次，跳过方法；
正常路径执行方法、析构接收者，再读原有词法借用。两平台继续
以 L0135 拒绝冲突写回。Linux 独立 LeakSanitizer 未报泄漏，macOS
`lang-codegen --lib` 严格 Clippy 通过。文档、尺寸与格式门禁通过。
未触发 PR-event CI，故障注入留给 CI。原始记录在
`.local-artifacts/0392-492-temporary-receiver`。

另一个可复现缺口是活动 String 词法借用参与拼接，右操作数的
两个分支分别有提前返回，而其中一支仍可继续产出 String。
即使用具名方法接收者，`check` 仍通过而原生构建报
`UnsupportedNode`；同一表达式改为读取自有 String 后可运行。
该对照将后续检查指向词法借用的复合控制准入。

## 493. R4 String 拼接值分支内的第二条提前返回

复现 §492 末尾对照：临时或具名接收者的普通调用实参、以及
直接 `val` 拼接初始化均可通过 `check` 却报原生 `UnsupportedNode`。
原有准入核对值 If 的两臂尾表达式，未遍历臂内尾表达式之前的语句，
因此遗漏了继续产生 String 的一臂内的条件 return。

沿原 String 拼接与值 If 尾表达式审核进入两臂，前置语句继续使用
原有迭代式词法控制遍历。每个 return 必须匹配同一借用绑定的
`ControlTransfer`，正常边保留借用。该遍历不改变 CFG 或析构事实。
普通 If 只增加局部遍历；若前置语句包含带类型化不可达截断的
语句 When，则沿用既有的表达式扫描。

独立复核未发现新增 P1/P2。冻结输入 3884 项，清单 SHA-256 为
`d0a04bdcdd692a833ac8f926f839d1058f3cca2d13cade2c7b7fddc5fa5d624f`。
macOS 和离线 Linux x86_64 各通过 359 项调用借用结束事实测试、
15 项定向原生测试、11 项公共 `check` 与 CLI 构建。单文件与跨
文件项目的四份输出逐字节匹配。在两条提前返回边上，左操作数
和对应的返回值先完成求值，再析构外层资源一次；正常边读取同一
借用并交付拼接结果。共享借用与独占写回冲突继续报 L0135。
Linux 独立 LeakSanitizer 无报漏，macOS `lang-codegen --lib`
严格 Clippy 通过。本切片未触发 PR-event CI，故障注入留给 CI。
若两臂尾部均继续产生 String，且提前返回都只位于前置语句，当前
词法借用例仍复现失败，自有 String 对照可运行，留作下一组退出审核。
原始记录在 `.local-artifacts/0392-493-concat-return-paths`。

## 494. R4 String 拼接值 If 两臂的前置返回

§493 候选对两个尾部均可继续产生 String 的分支通过 `check`，
原生构建报 `UnsupportedNode`。等价自有 String 对照可运行。
只处理臂尾 return 的旧准入无法进入这两个分支的前置语句。

String 拼接操作数的值 If 按臂分别验证尾表达式。它必须可继续
并具内建 String 类型，或是匹配当前借用绑定精确退出事实的
`return`。至少一臂提供 String 值。两臂前置语句交回已有迭代
遍历，任何提前返回仍按同一规则核对；空臂、直接 break/continue
及无退出证据的 return 均不获得准入。普通 If 的四条边分别测试，
另有零提前返回的纯 String 双值臂原生对照。macOS 临时探针中，
同一拼接作为共享调用或带括号的临时接收者方法实参时，均按预期
区分两条退出与两条正常调用边。

独立静态复核未发现新增 P1/P2。冻结输入 3884 项，清单 SHA-256 为
`1d7c413dec82c9ea7d8f35cbd024496c3f9158d57a4ec93737b9b04afd766cc4`。
macOS 与离线 Linux x86_64 各通过 359 项调用借用结束事实测试、
16 项定向原生测试、11 项公共 `check` 和 CLI 构建。单文件与跨文件
项目的四份输出逐字节一致。每条提前返回边先求值再析构，两个正常边
分别交付各自拼接值并继续读取活动借用。Linux 独立 LeakSanitizer
无报漏，macOS `lang-codegen --lib` 严格 Clippy 通过，文档、尺寸与
格式门禁通过。本批未触发 PR-event CI，故障注入留给 CI。

把同一两臂改写为布尔 selector
的值 When 后，词法借用例仍通过 `check` 而报原生
`UnsupportedNode`，自有 String 对照可运行。值 When 另行跟踪。
原始记录在 `.local-artifacts/0392-494-concat-arm-prefix`。

## 495. R4 活动 String 借用与穷尽值 When 拼接

§494 冻结编译器对内建 String 拼接中的穷尽布尔值 When 通过
`check`，却在活动词法借用下报原生 `UnsupportedNode`；改成
读取自有 String 的对照可运行。

值 When 复用原选择器审核和类型化穷尽边界。截止前每臂均须
以可继续的内建 String 尾值或同一借用绑定已有退出事实的
`return` 收尾，且至少一臂能正常给出值。前置语句使用原控制
遍历，截止后的选择器与臂沿用已有不执行事实。一次词法遍历聚合
截止范围，仅按单个原范围的完整包含条件允许语法表达式。
按起点排序并移除被包含区间，再逐表达式二分查找；交叠区间
不合并，以免扩大允许范围。

独立只读复核未发现新增 P1/P2。原生回归覆盖布尔选择器恰好
求值一次、两条早退边、两条正常边，以及穷尽后的 `else`
不执行。macOS 临时探针还覆盖同臂截止后的选择器、臂尾直接
return、无主体条件、整数参数及一次拼接内的多个截止范围。
若将整数参数主体换成返回 Int 的值 If，活动借用下仍报
`UnsupportedNode`，自有 String 对照可运行。复合 selector 另行跟踪。

冻结最终输入 3884 项，清单 SHA-256 为
`18655d2be83b22557288b1626538cb946d7211d7790830158347afe9b32fa7b3`。
首轮 macOS 严格 Clippy 要求将穷尽 guard 改用等价的
`is_none_or`；复核确认三个语法分支的真值不变。
最终 macOS 和离线 Linux x86_64 各通过 359 项调用借用结束事实
测试、17 项定向原生测试、11 项公共 `check` 和 CLI 构建。四份
单文件和跨文件项目输出逐字节一致，冲突写回仍报 L0135，Linux
独立 LeakSanitizer 无报漏。macOS `lang-codegen --lib` 严格 Clippy
通过；覆盖已存在的 CLI 副本后首次 smoke 被 SIGKILL，使用同字节
的新文件复跑通过。文档、尺寸与格式门禁通过。未触发 PR-event CI，
故障注入留给 CI。
原始记录在 `.local-artifacts/0392-495-string-value-when`。

## 496. R4 活动字符串借用与复合整数 When 主体

§495 对整数参数主体的值 When 可运行；改用产生相同 Int 的值 If
后，前端 `check` 成功但原生构建报 `UnsupportedNode`。等价自有
String 可运行。原有主体遍历到值 If 为止，不能将该节点送入
已有值臂与退出事实审核；再次包入整数算术也复现同一问题。

普通字符串词法借用策略将已核对的整数 When 主体送入现有迭代
控制遍历。工作项沿整数运算、前缀与受支持普通调用保留各子表达式
的实际类型身份。值臂正常退出时提供所需整数值，提前返回则核对
当前绑定和源码退出点。语句 When 复用此路径，专用可空借用入口仍
沿原准入。原 CFG 继续先求主体再求选择器，然后才进入选择的臂。

永久回归覆盖算术包裹的复合主体、主体提前返回且跳过选择器、
两条臂内返回及两条正常续行。macOS 补充探针覆盖语句 When、
整数前缀和普通调用中的复合主体，普通策略下借入具名 `String?`
的语句 When 对照也可运行。独立静态复核未发现新增 P1/P2。
冻结输入 3884 项，清单 SHA-256 为
`4e60ecc8e75426d07c2f16518246ae05b4ccd1e8f871c9ea3a2270319a79a7ee`。
macOS 与离线 Linux x86_64 各通过 359 项调用借用结束事实测试、
18 项定向原生测试、11 项公共 `check` 和 CLI 构建。单文件及跨
文件项目四份输出逐字节一致。冲突写回仍报 L0135，Linux 独立
LeakSanitizer 无报漏。macOS `lang-codegen --lib` 严格 Clippy 与
文档、尺寸、格式门禁均通过。本切片未触发 PR-event CI，故障注入
留给 CI。

同一对照矩阵还复现了 Boolean、String 和无载荷 enum 的复合
When 主体缺口。三种活动借用输入均通过 `check`，原生均报
`UnsupportedNode`，对应自有 String 输入均可运行。这些类型一并
留作后续复合主体复审。
原始记录在 `.local-artifacts/0392-496-compound-when-subject`。

## 497. R4 复合 Boolean、String 和 Copyable enum 的 When 主体

§496 的三组活动 String 借用例均通过前端 `check`，原生构建报
`UnsupportedNode`；对应自有 String 例可以运行。三组主体分别由
值 If 产生 Boolean、String 和无载荷枚举。布尔主体再经普通
直接调用或以整数值 If 作比较操作数时，仍能复现。

同一普通字符串词法借用策略按实际型别审核上述复合主体，
包含语句 When 和拼接操作数的值 When。同根 Copyable enum 的 case
尾值沿用原值适配；其它继续值核对精确类型及继续求值事实。
Boolean/整数子表达式沿标量上下文及已有直接调用门禁传播，
提前返回仍核对当前借用绑定与实际退出边。主体及选择器的求值、
匹配及未匹配的临时值清理由现有 builder 按原事实完成。

独立复核未发现新增 P1/P2。永久回归覆盖三种主体各五条路径，
含主体返回、两臂返回与两条正常续行。动态 String 主体覆盖
先失配再匹配及全部失配。最终 macOS 的额外运行确认布尔主体经
整数比较和普通调用包裹时输出一致。冻结输入 3884 项，清单 SHA-256 为
`914a21fa62818d49ee1637cc3bc5cefb6e5a0fc150cee6badc294aaab3c2371e`。
macOS 与离线 Linux x86_64 各通过 359 项调用借用结束事实测试、
19 项定向原生测试、11 项公共 `check` 和 CLI 构建。三种主体合并的
单文件与跨文件项目四份输出逐字节一致。冲突写回仍报 L0135，Linux
独立 LeakSanitizer 无报漏。macOS `lang-codegen --lib` 严格 Clippy
及文档、尺寸、格式门禁通过。本切片未触发 PR-event CI，
故障注入留给 CI。

带 String 载荷的 MoveOnly enum 探针走 case 类型测试时，借用和
自有 String 对照均通过 `check` 并报原生 `UnsupportedNode`。
自有 String 对照即使使用直接构造的临时枚举主体也失败，将同类
枚举主体绑定为具名局部值后可运行。该组合单独跟踪临时主体的
准入和清理。最小复现不需要拼接，现有 Refine MoveOnly enum
主体准入要求去掉括号后为具名引用。

macOS 与离线 Linux x86_64 各通过 359 项调用借用结束事实测试、
19 项定向原生测试、11 项公共 `check` 和 CLI 构建。组合三种
主体的单文件及跨文件项目四份输出逐字节一致，Linux 独立
LeakSanitizer 未报漏，冲突写回仍报 L0135。macOS
`lang-codegen --lib` 严格 Clippy 与文档、尺寸、格式门禁通过。
本切片未触发 PR-event CI，故障注入留给 CI。
原始记录在 `.local-artifacts/0392-497-compound-when-matrix`。

## 498. R4 临时 MoveOnly 枚举主体与分支清理

直接构造的 MoveOnly 枚举作为 When 主体时，公开 `check` 通过，
原生生成报 `UnsupportedNode`，预先绑定同一枚举的对照可运行。
前端现在将已完成的临时枚举登记为待清理值。事实回归核对同一
Unit、来源和临时身份，在完整 When 结束及臂内 return 的实际退出
各发布一次对应清理。Refine 按实际枚举根及类型化构造参数审核值来源；
具名主体和 nullable 选择器仍用各自的路径。

独立复核复现了臂内全部提前退出、内层循环跳转时丢失外层临时槽
的 `MissingFact`，并追查了 Boolean When 快速路径和 For 出口。
构建器现在为匹配、未匹配及循环继续路径保存实际到达边的临时槽，
正常合流只保留共有槽。各实际出口依旧消费前端的清理事实。
复核了各分支的事实顺序、Boolean 快速路径和三类 For，末轮
未发现新增 P1/P2。

原生回归覆盖直接构造、带活动 String 借用的条件构造、类型匹配、
臂内 return、臂正常完成，以及 While 和数值 For 的
跳转和退出。普通 Loop、具名 List 及工厂产生的 Map 另作探针。
枚举臂直接以 `listOf(...)` 作迭代源的探针仍报 `UnsupportedNode`。
§497 的编译器用具名枚举主体也复现。迭代源换成普通函数返回的
List，或让循环自然耗尽，仍能复现。当前候选的临时 Array 与
MutableList 探针同样失败。具名 List、Boolean 臂和脱离枚举臂的
直接迭代对照可运行，另留作容器循环准入复审。

远端 `main` 和本地基线均为 `d44c8b9a`。候选冻结输入共 3885 项，
清单 SHA-256 为
`766e5a276066b0e01324d753701b77a8fbc7445964fc498a81974caecc27a4ba`。

macOS AArch64 与离线 Linux x86_64 各通过 359 项调用借用结束事实、
27 项基础所有权、20 项定向原生和 11 项公开 `check` 测试，CLI
构建均成功。两宿主单文件与跨文件项目的四份输出与 62 行预期
逐字节一致，覆盖每条枚举载荷析构的顺序和次数。冲突写回仍报 L0135。
Linux 项目另跑 LeakSanitizer，无报漏。macOS `lang-codegen --lib`
严格 Clippy 及文档、尺寸、格式门禁通过。此候选尚未触发 PR-event CI，
故障注入留给 CI。

原始记录在 `.local-artifacts/0392-498-temporary-move-enum`。

## 499. R4 When 臂内的临时顺序容器迭代

§498 留下的临时 Array、List、MutableList 迭代均通过前端检查，
与枚举 When 组合时原生报 `UnsupportedNode`。具名迭代源可运行；
加入已有专属 Refine 调用的对照也可运行，证明此处缺口在准入。
首次故障的 SSA 路由回归在未修复源码上实际失败。

临时顺序迭代的门禁现在接受同一可调用体内实际访问、通过准入
且普通 lowering 无法处理的 When。普通 Boolean 的双臂判定与
原路径共用，带 nullable 描述符时复用普通路径限定的 owner
来源及 heap 布局条件。含 While 时，候选可重新遍历函数体；遇到
真正支持的 When 才确认路由。独立审查初轮发现三态 Boolean?、
带描述符的 nullable 标量以及不可达节点误给资格的问题，相关
回归覆盖两种 nullable 形状、前后语句换序、死代码与嵌套闭包隔离。
后续复审另复现了返回函数值的无主体 When 漏标，每臂和合流
SSA 类型通过后才登记同一资格。独立复核未发现剩余 P1/P2。

现有 Provider 仍从前端事实取得一次 source 求值、共享 loan 和
各退出动作。原生回归以两个有析构的元素区分各轮及元素逆序，
覆盖耗尽、continue、break、return 与未进入枚举臂。其预期顺序为
本轮局部值先清理，迭代源按高索引到低索引析构元素，最后清理
外围枚举载荷。原始记录在 `.local-artifacts/0392-499-temporary-iteration`。

复审期间将分支快进至 `c4a25908`（#487）。主干这一段只改动
`docs/proposals`，与候选源文件无交集。冻结清单含 3885 项，
SHA-256 为
`045541f210decdbfc2bf6486d2a43a497fc3bbe71799d6210948425781baf763`。

macOS AArch64 与离线 Linux x86_64 各通过 28 项定向路由及 SSA
测试、两项 CLI 原生回归和 CLI 构建。三类容器在两宿主分别通过
单文件和跨文件项目运行，12 份输出与 42 行预期逐字节一致。
三份 Linux 项目另经 LeakSanitizer 运行，未报泄漏。两个 nullable
用例和函数值 When 的公开入口探针两宿主也与预期一致。macOS
严格 Clippy、仓库文档、Rust 尺寸及格式门禁通过。本轮未提交或
推送，PR-event CI 与故障注入待后续执行。

另确认一个仍开放的普通 lowering 缺口。以 If 形成具名 `Resource?`，
在其 null 臂直接迭代临时 List，§498 及本候选均通过 `check`，
原生生成报 `MissingFact`。移除循环、先绑定 List、将 nullable
主体改为 owning 参数，以及加入本就需要 Refine 的操作，四个对照
均原生输出 `loop\ndrop\ndone\n`。在 If 的非空臂先以
显式 `Resource?` 局部交付也能得到相同输出。LLDB 将失败定位到
普通 lowering 的 `merge_unit_exits` 检查 If 的两个继续值时，
SSA 实体类型不一致。移去 When，只在 If 初始化之后保留临时
容器循环，仍可复现同样失败。下一步核对语义期允许的 nullable
交付如何进入现有 If 分支适配。
当前候选的同形探针中，String? 可运行，工厂返回的 Box?、Rc?
均报 `MissingFact`，Int?、Boolean? 报 `UnsupportedNode`。
后四项尚待按各自的布局和准入定位，不复用上面的错误归因。

## 500. R4 普通 lowering 的可空值合流

§499 的 Class、Box 和 Rc 复现均通过 `check`，普通原生路径以
`MissingFact` 拒绝。LLDB 定位到两臂的 SSA 返回实体类型不同。
同形 Int? 另在 `control_result` 的布局准入处报 `UnsupportedNode`。
新增资源和标量原生回归分别在修改前真实失败。

控制结果使用类型规划中现有的可空句柄、String niche 及整数/布尔
inline layout，校对载荷类型与 SSA slot。每个继续臂先按已发布类型
求值并完成原有表达式清理，再用原有 owner 交付适配完成 wrap，或把
已求值的 `Nothing?` 转为目标空值。控制身份和跨臂 verifier 保持
原合同。嵌套 nullable-subject When 的 MoveOnly 结果仅允许每个
继续臂可证明只产生 null，保留非空 owner 提取的旧限制。此处复用
既有 When 选择及原 lowering；表达式副作用不参与 null 值证明。

独立复审复现了调用、局部读及 nullable-subject When 的空值组合，
修复后实跑复核，并额外核对早退、同名遮蔽和旧 String 路径。三个
非空 owner 对照仍被旧 guard 拒绝。尚开放的边界为保守检查嵌套 If
的静态死臂、`return` 后不可达语法尾值，及 const 路径的独立
nullable When 委派。单一继续臂把非空 If 结果在外层交付为 nullable
的既有 `MissingFact` 亦单独保留复现。LLDB 指向局部交付的
`adapt_owned_value_to_expected` 查不到已声明 `Resource?` 的 SSA
类型 ID。此处先查 formed local 的规划输入和 owner 布局登记。
MutableMap? 的同形探针仍失败，其已规划聚合布局另行审查。

基线 `c4a25908`。冻结源清单含 3885 项，SHA-256 为
`cb4c2ce9173ed710f6a29775c02d118b02ea11160e502e4b7862e0915845165d`。
macOS AArch64 与离线 Linux x86_64 各通过 12 项定向原生和 11 项
公开 `check` 测试，CLI 构建成功。五组用例在两宿主分别实跑单文件
和跨文件项目，20 份输出逐字节符合预期。Linux 的五份项目执行另经
LeakSanitizer 检查，未报泄漏。macOS 的严格 Clippy 与 codegen
测试目标编译通过；文档、Rust 尺寸和格式门禁通过。未提交或推送，
PR-event CI 与本地不执行的故障注入仍待 CI。

原始记录在 `.local-artifacts/0392-500-nullable-if-joins`。

## 501. R4 正常形成的局部可空布局与 F6 主干对齐

复现以临时顺序循环保持普通 Unit 路由。If 一臂 return，继续臂
返回非空值，前端把 If 表达式发布为非空类型，局部声明仍为相应
可空类型。表达式类型规划因此不包含局部的目标布局，正常边在
`adapt_owned_value_to_expected` 取得目标 SSA 类型 ID 时报告
`MissingFact`。独立复审用可达的空函数签名预先登记同一可空
类型，原用例即可运行。红测在修复前实际重现。

通过已校验的来源、符号和最近可调用作用域找到正常形成的局部，
复用现有 mapper 登记声明的 nullable heap 和 inline 布局。局部
类型参数读当前实例的替换。Map 保留原登记与 K/V 约束，lambda
和 deinit 的专用路径保持既有准入。原生回归检查 Class、Box、Rc、
String、Float、Double、Copyable value class 和 Resource 泛型实例。
覆盖早退前后的资源析构、工厂是否执行及继续路径输出。MoveOnly value class 和 Char? 布局仍单独跟踪。
本节冻结候选的可空 MutableMap 双臂报 `MissingFact`，LLDB 确认出在 If SSA
合流的结果类型一致性检查。其单一继续臂及显式可空局部作非空
臂结果的原生对照均通过，修复见 §502。

主干在复审中更新至 `d405ed2b`（#486）。唯一文件重叠为
`when_boundary.rs`。此候选将本地损坏实参边界恢复、缓存和债务
剪枝接入主干的 selector 状态机，保留原证明核、Kani harness、
Lean 文件及 workflow。独立复审逐分支检查两种扫描入口、词元
推进、换行及缓存，未发现合成回退。Lean 与 Kani 的原证明范围
仍是状态转换，词元适配、恢复缓存与诊断由普通解析回归核验。
本地未运行形式化证明，Lean 与 Kani 产物仍按 CI 候选身份记录。

冻结源清单含 3886 项，SHA-256 为
`17c79d3c57f1055763f7adb808492e6e434cff619795a8f1e5ccc41fd0a01644`。
八组用例在 macOS AArch64 与离线 Linux x86_64 实跑单文件和
跨文件项目，两宿主共 32 份输出逐字节符合预期。Linux 的八组
项目执行均通过 LeakSanitizer，未报泄漏。两宿主各通过 7 项
When 状态/访问计数、34 项 parser control-flow、12 项定向原生
及 11 项公开 `check`。macOS 严格 Clippy、文档、格式与尺寸
检查通过。

原始记录在 `.local-artifacts/0392-501-formed-nullable-layout`。

## 502. R4 可空 Map 的双臂合流

§501 的两臂继续探针表明规划已有可空 Map 聚合，但普通 If 合流
没有选择它。LLDB 将 `MissingFact` 定位到两臂的 SSA 结果类型
不一致。非空臂显式返回可空局部的对照可执行，永久原生回归先以
`MissingFact` 实际失败。

控制结果仅接纳由现有 nullable Map 表登记且 payload ID 一致的
聚合。非空交付复用 `NullableWrap`、空臂按真实 `Nothing?` 求值后
生成空聚合。Map 查询 result 表上的其它聚合仍由原路径处理，原
K/V 和非空 owner 提取门禁保持。独立复审验证了嵌套 If/Boolean
When、借用使用后的 owner、临时判空和局部析构；不支持的可空
Map 值类型对照仍由原门禁拒绝。

冻结源清单 3886 项，SHA-256 为
`d206c7b8b50a9760850d3c424b4c40238a2f1a9490a03e5456e537023deacda9`。
macOS AArch64 与离线 Linux x86_64 各通过 6 项定向原生回归
和 CLI 构建。Map 与 MutableMap 使用单文件和跨文件项目入口，
共 8 份 stdout 与预期逐字节一致。Linux 两份项目另经
LeakSanitizer，未报泄漏。macOS 严格 Clippy 通过。解析器源与
§501 冻结输入的对应文件逐字节相同。再次确认嵌套 If 静态死臂
及先 return 后出现的不可达非空尾值仍报告 UnsupportedNode。

本轮未提交或推送。原始记录在
`.local-artifacts/0392-502-nullable-map-joins`。

## 503. R4 可空 When 只校验实际可交付的结果

可空主体的普通 When 要交付自有可空值时，检查会把静态死臂和
提前 return 后的尾值也算成可能的非空 owner。§502 观察到的
`UnsupportedNode` 在永久回归上实际复现。复用 Unit 已计算的
布尔语法结果，为 If 单独发布来源限定的结果边，再与普通完成性
取交集。只检查能交付的分支及块尾。块内检查局部初始化器、
表达式语句和循环的正常后继，for 在检查循环体前先检查 source。
实际条件求值、控制流生成与析构继续使用既有路径。Const 路由及
可达非空 owner 的原门禁保持。

独立复审先指出仅用普通布尔结果会漏掉静态死臂，以及 for 来源
提前退出应与循环体退出区分。修正后以最终 macOS CLI 实跑
13 份源码，覆盖动态短路效果、嵌套循环的 break/continue、终止
前缀、可达非空 owner 和 Const 原门禁，未发现阻断。

冻结源清单 3886 项，SHA-256 为
`b0c15c067410786eac7dc62f524f00be0920717be42c604232618e8255362a24`。
同一冻结输入在 macOS AArch64 和离线 Linux x86_64 实跑单文件
与跨文件项目。四份 stdout 与预期及彼此逐字节一致，包含条件
副作用和析构顺序，stderr 为空。Linux 项目经 LeakSanitizer
未报告泄漏。两宿主的可达 owner 负控均通过 `check`，原生运行
均报告 `UnsupportedNode`（单文件退出码 2，项目退出码 1）。
两宿主各通过 28 项流事实回归、2 项 nullable When 及 4 项 If
合流原生回归。macOS 的 frontend/codegen 严格 Clippy 通过。
本地验证范围为普通源码及项目泄漏检查，故障注入保留给 CI。

原始记录在 `.local-artifacts/0392-503-nullable-when-reachability`。

## 504. R4 可空 Char 的合流和临时判空复审

本地旧输入丢失后，从主干 `d405ed2b` 与历史变更记录恢复的候选先过
默认 workspace 编译检查和原生 CLI 构建。恢复候选的源文件清单
`b404cf873edd6167acda547bbf21fb8f45d918ce935741bf98925d070724af35`
与 §503 原清单的身份分开记录。§503 合法 When 的单文件与跨文件项目
在两宿主复跑，stdout 与原 oracle 一致，stderr 为空。

`Char?` 使用已实现的显式 tag 和 Unicode scalar 载荷。直接与字面量
null 比较时，已规划的 inline Char 存储读取 tag。非 inline 布局
仍走已有判定，MoveOnly 临时值仍要求原始析构事实。回归覆盖等于与
不等于、null 在左右两侧、NUL 及 Unicode 字符的存在性。最终修复
的清单 SHA-256 为
`f8158f1451592cbc23efb4cda6f877666234e222fda4ffaeea68ce3cd3fc34b1`。
macOS AArch64 与离线 Linux x86_64 的单文件和跨文件项目原生输出
逐字节一致，退出 0 且 stderr 为空。macOS 另通过 1 项新回归和
2 项 §503 nullable When 定向回归。独立复审核对了布局准入与
MoveOnly 门禁，并实跑单次求值、提前返回和析构的普通对照。

完整合流用例仍保留失败，兼容 lowering 的非空载荷提取和插值待修。
对恢复版运行官方 conformance 选择器，四项已有探针
`p02_interp`、`p05_double`、`p06_char`、`p11_interface_bound`
在两个宿主均通过。§469 已确认的 `raii_state_machine` 和
`varint_codec` 在恢复候选的两种入口均失败，归类为
`BE:InvalidModel`。再生账本保留两项程序的 `run_ok` 合同，待查
恢复差异。故障注入保留给 CI。

恢复 §469 中遗漏的 SSA 验证规则后，8/16/32/64 位整型转字符串
与既有生成指令重新一致。上述定向 conformance 在同一清单
`46e3abc1e60a818547bf37a4ed928ead959c702faf55a01b8cd5143b63e0d6c6`
的两宿主分别通过 4/4 探针与 2/2 综合程序，两个入口无分歧。独立
复审逐字节核对了两程序的八份原始输出，并核查有符号和无符号位宽。
随后补回 §488 中 Boolean 的对应规则，显式 `true.toString()`
在双宿主、双入口原生通过。macOS 定向 Display 回归共 2 项，通过
1 项，组合借用接收者回归仍因 `UnsupportedNode` 失败。

对组合 Display 失败做 macOS 缩减，`Char.toString()` 与词法
`borrow val` 单独均通过，在同一函数内前后两种顺序均报
`UnsupportedNode`。Integer 和 Boolean 的同形输入通过。LLDB
观察到普通调用解析及兼容借入绑定的拒绝，组合路由待修。

Boolean 规则补回后的冻结源清单 SHA-256 为
`3b29d1eefce15c89e63ce1deaff366d325eebd8e3f6654274f89611bcc71d4d2`。
macOS AArch64 与离线 Linux x86_64 完整执行 154 项能力探针及
6 项综合程序的单文件和项目入口。各有 141 项能力探针、2 项综合
程序通过，17 项符合账本中已知未实现的预期。无入口分歧、输出误差、
回归或内部错误。基础能力的可运行组为 62/74，离 70 项门槛仍差
8 项。两份原始账本分别记录了短、长主干 SHA。确认它们解析为
同一主干提交后用仓库核验器逐项比对原始证据、成功输出摘要及汇总，
结果一致。冻结输入身份由清单记录。当前两项原生回归仍未关闭，
这份证据只覆盖公开语料门禁。workspace 编译、格式和文档检查通过。

## 505. R4 Char 字面量和可空标量的 Refine 准入

恢复 §470 的 Char 字面量准入分支，前端具体类型必须为内建
`Char`。生成端继续用共享的字面量解码器构造 Unicode 标量。
修复前，同一函数中的 `Char.toString()` 会在存在词法借入时让
函数落入不支持该借入的兼容路径。恢复后，原组合回归覆盖直接
及泛型转换，转换后仍可读取借用值。普通有界兼容路径的 Display
回归同时通过。

Refine 的可空标量判断补入 `Char`。`Char?` 沿既有显式 tag
ABI，只在非空边提取 Copyable 载荷。新增原生回归覆盖 NUL 载荷、
空臂以及提前返回，核对真实 NUL 字节和每条路径一次析构。
原含临时容器 for 的完整合流用例保留原输入和 oracle。在两宿主
的单文件和跨文件项目仍报 `UnsupportedNode`。缩减后，受限兼容
路径可以判空，窄化后比较报 `InvalidModel`，插值报
`UnsupportedNode`。下一步核对兼容分支的 Copyable 非空载荷
交付与 SSA 边身份，原普通临时容器 for 的路由门禁保持。

最终冻结源清单 5032 项，SHA-256 为
`307dba568b9f7e59b1cef501964b0527b2015b2f456a62e3b1e229f99f14c908`。
macOS AArch64 与离线 Linux x86_64 的单文件和跨文件项目，对
组合 Display、临时判空及非空载荷三组正例共 12 份 stdout
逐字节符合预期，stderr 均为空。macOS 的两项 Display 测试与
新增窄化测试通过。独立复审核对了前端类型、Unicode 常量、
非空边及原有 loan 和 drop 的交付。故障注入留给 CI。

## 506. R4 兼容路径的可空 Char 载荷与插值

原 §504 合流用例包含普通临时容器 for，进入兼容 lowering。
存储里已有显式标记的 `Char?`，但旧名称读取把整个包装值传给
需要 Char 的比较。旧 String 入口还会拒绝已有 typed plan 的插值。

兼容路径以同源、同绑定的前端非空事实和精确 inline 布局准入
Char 载荷读取。对当前 SSA 包装值建立局部 `NullableBranch`，
通过非空边生成 proof，再读取对应的 Copyable 载荷。违反已经
建立的非空事实时存在一个防御性 abort 边。所有仍活跃的绑定、
loan 和 pending 值沿两边按既有 CFG 合同转运。插值仅接收确认
可完成的 Char 名称读取及其透明括号，按 typed plan 拼接，前缀
跨载荷分支重绑定，拼接后分别释放两个输入。普通循环路由、
原失败回归及 oracle 保持。

初版遗漏了跨分支的线性资源转运，独立审查指出阻断，原生测试
也被 SSA verifier 拒绝。修正后独立复审通过。macOS 的三项
nullable Char 集成测试通过。冻结源清单 5033 项，SHA-256
`54305eb665d1391cd16989de1f67e9c0c230aec5556af89f488c42d2bba5ba55`，
在 macOS AArch64 与离线 Linux x86_64 实跑原完整用例、临时
判空、Refine 窄化与组合 Display。单文件和跨文件项目 16 份
stdout 逐字节匹配，stderr 为空。独立 macOS 对照另验证
短路窄化、重复插值、真实 NUL、活动前缀及两个 Char 比较。
故障注入留给 CI。

完整 macOS `native_cli` 首跑为 68/131。失败包括 50 项 10 秒
超时和 3 项 LLDB 启动错误。限制为两条测试线程并允许 LLDB
启动后为 118/131，三个断点及全部本轮 Char 回归通过。对照已
重建的上一冻结源码，8 项 lowering 失败和两项默认 Display 的
错误输出在前后版本均复现。另有三项只读方法同属默认体分派，
见 §507。合流、插值、默认派发以外的扩大检查缺口保持可见。

## 507. R4 恢复具体 Self 的默认体覆盖分派

§506 扩大验收发现，继承的默认 Display 体在调用泛型成员时输出
`1`，原 oracle 为 `2`。上一检查点也会调用被覆盖的默认实现。
类型阶段把 `StaticSelf` 与类型参数同等处理，SSA 调用路由却
只把类型参数标成具体分派。恢复 `StaticSelf` 的分类后，调用
计划与 runtime recipe 预检沿同一描述取路由，具体接收者选择
覆盖。限定 `super` 优先选指定默认体，委托端点由已有事实确定。

隔离候选在 macOS 通过 11 项只读方法回归。两项原错误用例在
macOS AArch64、离线 Linux x86_64 的单文件与跨文件项目均恢复
预期输出。独立复审核对了规划与 recipe 两处入口，并以真实源码
实跑泛型继承覆盖、两跳委托及显式 `super` 的六份对照输出。

合并两项修复后的冻结源清单 5033 项，SHA-256
`a4b2969fe4082312e5e50f0e7e2cadc89dc3600f7da66208c5e8e34cfe0b626f`。
macOS 定向复跑 3 项 nullable Char 和 11 项只读方法全部通过。
原含 for 的 Char 用例及两项默认 Display 用例在双宿主、双入口
共 12 份 stdout 逐字节符合预期，stderr 为空。此前全套报告的
其余 8 项 lowering 失败在合并候选实跑仍与上一检查点一致。
范围包括可空浮点、借用字段判空、String 与名义插值及提前退出
selector。保留原预期，完整候选仍待扩大复审与 PR-event CI。

## 508. R4 恢复浮点表达式准入与插值退出清理

两项可空浮点原生回归在 §507 候选报 `UnsupportedNode`。
Refine 补齐 `Float`／`Double` 的字面量、算术和比较准入，
负号生成已有的一元浮点 Negate，避免按整数减法处理负零。
沿用宽度验证和 IEEE 比较，整数位运算与浮点容器限制保持。
原借用字段判空复现含 `generic<Float>`，随浮点准入恢复通过。
独立复核还确认 Guide 03 的 NaN 条目与另一处文字冲突。现有
比较继续遵循 §471，NaN 不等比较为真，等值和有序比较为假。

普通与名义插值的后续片段提前退出时，Refine 已记录完成格式化
的临时值，但旧准入未使用集合，将源值误认为仍受外层调用借用。
恢复精确表达式身份检查，按既有 drop fact 清理原输入。
独立复审核对了 Group、常量、嵌套插值和当前未完成片段，并
实跑三种控制退出，确认已创建的资源按作用域各析构一次。

## 509. R4 恢复直接 Unit 实参与独立绑定身份

原三个 Unit 选择器回归覆盖中途退出的 `if`、`when` 及
可达派发。恢复 §461 的 Refine 直接 Unit 实参准入，仅消费
与已规划直接调用、具体类型和所有权完成事实一致的操作数。
已存在的 Unit 借用继续 reborrow，局部借用使用其当前绑定。
前缀提前退出时省略未抵达的实参，保留源码顺序。
Inout Unit 和函数值 ABI 的既有限制保持。

三项原生回归先通过，独立复审发现 Unit 绑定复用 RHS SSA
根。声明和重赋值改为在右侧正常完成后各自发出独立的
Unit 定义。新增源到 SSA 回归校验两个借用根不同，以及重赋值
仅改变目标绑定的根。另将 §500 的旧条件计划测试迁到现有
`ControlResult` 接口，保留其错配控制身份必须在生成 CFG
之前失败的断言。

独立根修正的候选在 macOS AArch64 和离线 Linux x86_64
实跑 14 组源码。单文件和跨文件项目共 56 份 stdout 逐字节
符合预期，stderr 均为空。范围包含浮点、借用判空、插值退出、
Unit 选择器的派发及非派发、完整 Char 合流与默认 Display
分派。绝大多数路径使用原有测试的源码及 oracle。

单元测试首次受旧控制结果接口阻塞，迁移后旧根实现的首次
命令实际选中 0 项。已把根身份回归移到实际构建的
`while_flow_tests` 模块。根身份单元测试的红绿、完整 macOS
native_cli 与同一候选 conformance 仍待完成。故障注入留给 CI。

## 510. 冻结候选验收与最新 main 对齐

基于 `d405ed2b` 的 §509 候选通过 macOS 完整原生 CLI
131/131，无跳过。Unit 局部根回归先在 Alias 版精确失败，
两个参数实际引用同一个 ValueId；独立定义版的原测试通过，
迁移后的错配控制计划测试也通过。两平台的公开 conformance
用仓库证据验证器完成 640 次入口执行的对账，结果为 141/154
能力探针和 2/6 综合程序，无入口分歧。原候选及证据分别冻结。

随后据 GitHub `main` 的 `36012f867f154fb96d0894db52737e7ba3a14a2e`
在隔离副本对齐，主干前进 12 个提交。六处文本冲突均在语料
目录和测试。保留主干的 164 项探针与 10 项验收程序、逐字节
不变的 154 行历史基线。四项探针和两个程序沿用先前已验证的
原始 oracle 转为 `run_ok`；配置解析程序的失败记号仍仅接受
`FE:L0165`。同一次对齐修订了报告中的目录计数、失败判定和
历史 marker 展示。静态目录审计为 148/164、2/10，只记录期望，
对齐后的实际验证另行记账。四项目录相关 Python 测试与文档
校验通过。独立复审核对了目录身份、原始流校验与失败门禁。

应用 #448 后续修复之前，`36012f86` 上的对齐候选完成
macOS arm64 和离线 Linux x86_64 的 696 次公开 conformance
入口评估。两个宿主各通过 148/164 项能力探针、2/10 个综合程序，
24 项匹配已知未实现合同。证据比较器判定一致，无内部错误、
入口差异、回归和错误输出。P2 基础 A 组为 62/74，距 70 项门槛
差 8 项。14 组定向程序的单文件和跨文件项目共 56 份输出逐字节
符合预期，stderr 为空。macOS workspace check、576 项前端
测试、65 项 LSP 测试及 11 项公开 check 测试通过。
上述记录绑定本节的对齐候选。

## 511. PR #448 独立复审的三项跟进

在 `d405ed2b` 的独立工作树复现三项发现，保留红绿日志及
单独补丁。在 `36012f86` 对齐候选合回两处正确性修复。
一元 `!` 继承操作数的必然跳出属性，使 `!cond(x, break)`
保留条件入口及循环后的非空证明。普通前端回归覆盖两个 Unit
输入模式，并确认 `!cond(x, flag)` 仍因可能的回边写入报告
L0084。两种公开入口在修复前的 break 正例各报告两条 L0084，
分别对应条件内与循环后的读取。

`HeapReceiverFieldExchange` 按当前指令处的替换值内容证明
检查借用闭包逃逸，与普通堆字段交换使用同一判断。SSA 负例使用
合法的 Inout 接收者及直接独占字段投影，断言唯一拒绝的指令及
来源。公开源码是否可产生本例 SSA 未经运行验证。

直接短路左链的真假写入摘要已随 §414–416 恢复。独立复审发现
对齐候选遗漏后续普通 lib 门禁的计数定义与实际遍历接线，首次
复跑因此未能编译。恢复 §416、§426–429 的六项原有见证和
`cfg(test)` 接线，分别观察条件 AST、写入及结果边工作项、
条件汇合、RHS 完成性、可达性和重载重放。独立复核确认两处
正确性修复与计数范围，故障注入留给 CI。

macOS arm64 定向测试通过 28 项流事实、6 项操作计数、29 项
闭包逃逸和 2 项接收者交换，其中两个 SSA 筛选重合 1 项，均无
忽略。32 项到 128 项，条件访问为 32 → 128；链首一次写回时，
`&&` 与 `||` 的写入及结果边访问均为 62 → 158。该计数针对
本用例的 AST 工作项，不外推多写入键的集合成本。公开单文件及
项目的修复后 break 正例均退出 0，普通回边对照均仅报一条 L0084。
以 `7` 和 `null` 调用正例，两个入口的 macOS 原生运行均退出
0，stdout 为 `seven\n`，stderr 为空。前端定向严格 Clippy 和
文档结构验证通过。

Linux x86_64 的 28 项流事实及六项操作计数通过，各计数与
macOS 一致。完整候选的尺寸门禁发现 `refine.rs` 达 1004 行。
把共享的 Display 接收者、布局和内建类型查询移到同层
`refine/display.rs`，主体为 959 行。核对三项方法体逐字节
一致，访问范围仍限于 Refine。未增加尺寸例外。

Display 模块提取前，Linux 定向候选通过 29 项闭包逃逸与
2 项接收者交换，其中 1 项重合。两种公开 check 入口均接受
break 正例，并在可达回边的反例只报一条 L0084。两个 Linux
原生入口的 stdout 与 macOS 逐字节一致，stderr 为空。Display
模块提取后的 macOS 原生定向测试 7 项通过，包含默认分派、
插值退出及资源析构。Rust 尺寸复验通过，保留既有三处例外。

Display 模块提取后的 macOS 完整原生 CLI 通过 131 项，零
忽略，包含 LLDB 断点用例。§510 的完整双宿主 conformance
绑定修复前的对齐候选；修复后的完整双宿主 conformance、
PR-event CI、资源和 sanitizer 门禁仍待冻结候选验收。

## 512. 修复后冻结候选的普通跨平台动态验收

以 `36012f867f154fb96d0894db52737e7ba3a14a2e` 为父提交，
冻结 §511 修复和 Display 模块提取后的 5062 份输入。源码
清单 SHA-256 为
`519986806ceba4ac91adb69281c4b0fca417b97b95c1ddc1265310fe1e37394a`。
本节新增前保存可应用的候选补丁，验证前后两端重算清单一致。
GitHub `main` 开始、结束均为该父提交。Linux 为离线镜像中的
x86_64 模拟环境，macOS 为本机 arm64。

两宿主使用同一标识 `36012f86…+worktree-519986806ceba4ac`
构建编译器。164 项探针和 10 个验收程序合计 696 次单文件与
项目入口评估。仓库证据比较器验收通过，两个宿主均为
148/164 能力探针、2/10 综合程序，24 项匹配已知未实现合同。
记录的内部错误、退化、入口差异及错误输出均为零。基础 A 组
为 62/74，离 70 项目标差 8 项。macOS 的全 workspace、全
target 严格 Clippy 通过；两宿主各通过 65 项 LSP 测试
及 11 项公开 check 测试，零忽略。macOS 完整原生 CLI 的
131 项结果见 §511。

普通资源循环调用原有 `model.cases`、`prepare_case`
与 `Execution.case`。两端各通过全部 16 项，源程序、预期
结果、实际诊断与 stdout 逐项对账。8 个正常程序通过指针
计数器核对 24 次分配和释放，`v2-0-11` 按 0、2、3、1
次序释放。8 个拒绝样本中 L0131、L0133 各四项，范围一致
且未生成 LLVM。Linux 的 8 个正常样本及普通 Map 另外通过
ASan 和 LSan，输出符合预期，stderr 为空。Map 的 ASan
输入覆盖 11 个 Koven 定义。首次资源复验仍用构建阶段的
`/usr` 前缀，容器的 `/usr/bin/clang` 版本命令退出 127。
使用实际 LLVM 21 安装前缀和原导出器复跑通过，编译器源码
清单保持一致。

故障注入、校准、检查器变异及完整 sanitizer 合同留给
PR-event CI。上述本地候选无提交身份，物理 Linux 宿主与
最终 PR CI 仍待验收。

## 513. R0 默认生产目标宏展开复核

在 §512 冻结清单上，用本机已安装的
`nightly-2026-08-21` 执行 Cargo `-Zunpretty=expanded`。
五个默认生产目标分别为 frontend、codegen、std 三个库以及
`kovenc`、`lang-lsp` 两个二进制，共 353384 行展开文本，
清单在展开前后保持一致。采用宿主默认目标和默认 feature，
动态验收继续使用仓库固定 Rust 1.96.0。

在五份展开产物中，`resolve_names` 的定义、导入和唯一
调用均在 frontend；`UnitResolver::new` 为 Unit 中每份
源文件执行该局部名称解析。`check_types` 和
`check_ownership` 只出现公开定义，`complete_file_types`
另有重导出。`lower_scalar_file_with_entry` 和
`emit_native_object` 均不出现。独立复审追踪 CLI 的两种
check 及 build/run 路径到 Unit，追踪 LSP 独立文档、清单外
文档和 source set 更新到相应 Unit 分析。此项收口当前
macOS 默认生产配置的宏展开清点；其他宿主 cfg、非默认
feature、测试与仓外调用者仍按原范围跟踪。

## 514. R4 公开语料的实际分流与析构对照

基于 §512 的编译器输入核对 ADR-0032 的普通 Unit 路由。
普通计划体由整个函数的 subset 判定决定；Refine 型的类型
准备和构建均在成功返回之前用 `?` 向外传播。闭包 thunk
单独判定，拒绝时直接返回 `UnsupportedNode`，没有
Compatibility thunk。其余 Inout、borrow/carrier return
仍由普通计划体的能力保护约束。此处只复审交接，不开展
D10、E2、E3 的另线迁移。

在隔离副本仅给 `planned_body.rs` 和 `orchestrate.rs`
增加路由观察。观察清单 SHA-256 为
`97573a73a7f551549bc24ee199fccb768ea940c0fb48d8c1cdeb0f655a2d207d`。
记录计划体或 thunk 的预检决定及函数名，写入仓外文件，不使用
测试专用的逐家族强制路由。记录发生在构建前，失败样本的部分
记录不计为成功构建。观察写入需成功才能保持原失败传播；
本轮以未插桩的 §512 二进制和原始结果验证这项边界。
conformance 的编译器入口是观察 wrapper，其记录散列对应
wrapper 文件。观测版实际二进制 SHA-256 分别为
`26229585b69e7a83d3020c638fbee48795b5287651e1b0c6a24539094ed0226f`
（macOS）和
`a4d8d52ccce63b225cafeceabc92318ad778649d7ebcd5105df9bad262af0880`
（Linux）。

macOS arm64 和离线模拟 Linux x86_64 各重跑 348 次公开入口，
观测版与同端未插桩编译器的退出状态和 stdout/stderr 逐字节
一致；两个宿主记录的函数名、决策及顺序相同。单文件与项目
逐项路由计数一致。每端 258 次正常运行中，46 次包含
Compatibility，记录 348 次计划体 Refine、60 次计划体
Compatibility、12 次 thunk Refine。加上 6 次预期语言
Abort 后，计划体 Refine 为 350 次、Compatibility 为 64 次。
四项既有未实现探针的双入口另记录 8 次 Compatibility，
分别为 Elvis、递归链表、value class 等值及带 String
载荷的 enum；这些记录未计入通过。36 次预期诊断均未进入
路由。语料没有触发未资格化计划体或拒绝 thunk。

同一嵌套资源函数的直接体走 Refine，带 Array 循环的体走
Compatibility。两种写法在每个宿主分别运行单文件和实际
跨文件项目，项目带一个无关声明；干净版与观察版共 16 次。
输出逐字节一致，比较正常返回和提前 return 都按
Outer、Middle、Inner 顺序各析构一次。正常比较中，右操作数
输出先于清理，资源清理先于条件分支打印。每端另用
未插桩的 SSA 测试二进制通过 9 项普通路由和 2 项已迁移
家族用例，包括 Map 操作、Rc 构造和顺序容器 size。

独立复审核对两处记录点、资源 oracle 和双宿主原始流。
本轮证据覆盖语料实际可达实例及定向合同。§389 的八组
强制逐家族边界使用历史 CI 证据，留待冻结提交的 CI 复验。
本地未执行故障注入。

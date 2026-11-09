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

## 6. 非目标与兼容边界

不更换手写 Lexer＋递归下降＋Pratt，不追求全语言 LL(1)，不新增通用 HIR/优化框架或依赖，
不扩展 Guide、不补齐全部 v1 功能。DFA/最左推导属于算法或理论，不是额外发布能力。
修复只改变违反现行合同的接受结果/事实/诊断；公开接口与跨 crate 类型变化须单列验收。

## 7. 当前阶段结果与未覆盖

| 阶段 | 当前证据 | 未闭合边界 |
|---|---|---|
| R0 | 公共流水线和阶段准入已定位。§93、§105 区分旧测试入口与 Unit 生产回退，§151 固定 `369fff4c` 的直接调用清单并确认共享 `resolve_names` 的生产依赖 | 逐消费者核完保留或迁移理由，仓外 API 和间接调用另记边界 |
| R1 | 独立抽查 UTF-8/身份、模式栈、Pratt、strict trial、lambda DFA，F4–F6 已修。文件级类型参数换行恢复见 §114，L0088 闭合身份见 §139，委托 lambda 恢复及预算见 §143–144，普通分组、下标与错误后缀归属见 §164–165、§167–168 | 未闭合普通分组和缺少可靠归属证据的错误输入仍需按公共入口复审 |
| R2 | F1/F2/F8–F11 的已列复现有修复证据，F11 验收见 §16–18。失败试探和流事实见 §22–25、§98、§104，常量失效与独立诊断见 §141–142，短路写回见 §170–171 | 逐消费者处置、剩余流事实组合及 Unit/Single 的完整差异仍待验收 |
| R3 | basic/const 交接身份、临时父 owner、两阶段借用已独审。F3 已修，F7 已加旧入口能力门。§145–147 核验观测索引身份，§151–153 核验 nullable 清理及构造 loan 结束 | 继续分流旧事件见证，复审循环、部分构造、各退出边和动态资源矩阵 |
| R4 | 已独立核对 subset/build、部分路由、SSA/LLVM 验证及资源用例。§148–150 的借用初始化交接经 #394 双宿主验收并合入，§153 有构造实参跨包本机运行证据 | 生产 Unit 仍有兼容准入，需对账剩余家族、非法 SSA、资源故障与双宿主动态证据。第一轮冻结 SHA 的纵向复审尚未完成。目标机仍为 `OptimizationLevel::None` |
| R5 | 已明确两轮触发门槛 | v1 尚未冻结，第二轮发布验收待启动 |

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

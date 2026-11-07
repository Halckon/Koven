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
| R0 | 基线、工具链、公共流水线与独立审查已建立；原始 docs checker 通过 | 全部真实消费者/逐合同清单仍需对账 |
| R1 | 独立抽查 UTF-8/身份、模式栈、Pratt、strict trial、lambda DFA；F4/F5/F6 修复，恢复矩阵、分句预算与全部 frontend 实跑 | class-family 全部 malformed owner 未穷尽。局部解构的初始化式及分号 owner 已修，见 §60。借用解构内层右括号及外层停止范围已修，见 §63 |
| R2 | 独立检查 trial/final admission/provenance；F1/F2/F8 和 F9/F10 已列复现有修复证据，既有 types/callable/final-admission 实跑 | 逐消费者处置与流事实组合仍开放。候选与继承 owner 回滚见 §22–24，循环和 Elvis 流事实见 §25。局部 value class 解构已按字段名匹配。私有字段访问和解构可见性已修，见 §62。Unit 待定迭代源已修，旧单文件入口仍待复审，见 §63。自动 component 已退役，见 §64 |
| R3 | basic/const factory 身份链与临时父 owner 关系独审；F3 已修复复核；unit ownership/deinit/两阶段借用与能力隔离实跑 | F7 已以能力门关闭漏报；旧消费者迁移、循环/析构全矩阵与动态资源验收开放；独占捕获前端进展见 [§49](#49-r3-copyable-独占捕获借用) 与 [§50](#50-r3-moveonly-捕获根替换) |
| R4 | 独立核 routes、函数/thunk 无错误回退、drop 核验、SSA/LLVM 双门及 sibling 原子发布；CLI 本机 build 通过 | 全家族 subset/build、emit/verifier 与 debug/fault/sanitizer、Linux 实跑待验；目标机仍为 OptimizationLevel::None。局部解构与临时顺序源的路由已修，`while` 的交叉边界仍开放，见 §65 |
| R5 | 已明确双轮触发门槛 | v1 尚未冻结，不执行发布验收或声称完成 |

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

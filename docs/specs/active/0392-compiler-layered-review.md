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
| F5 | Guide 06：同行 block elements 必须有分号；`{ val x = 1 val y = 2 }` | parse_block 无诊断，带分号正控同样无诊断 | 本机修复并独立复核；三 owner 共用 L0169，非法正例及下游 fixtures 已迁移；双宿主 CI 待验，见 §10 |
| F6 | 恢复不得吞外层分隔符；`f(a.async., sentinel)` / `{ a.async.; sentinel }` | 新失败回归分别证实吞逗号、分号并产生额外 L0034/L0013 | 已修复：error-postfix 保留 caller stops 和分号；逐实参/逐 statement 核对后继 identity |
| F7 | 旧单文件 receiver 与 unit 的所有权合同未等价；同 F3 输入 | 修复后仓外探针确认旧 `check_ownership` 仍 diagnostics/deferred 均空，unit 已报 L0131 | 本机以 L0170 明确拒绝旧入口不能建模的 own receiver，撤销成功事实，旧 lowering 负控通过；不声称实现旧 receiver 语义或完成消费者迁移，见 §10 |
| F8 | Guide 12：顺序容器显式签名的元素必须可存储 | Unit 迁移漏 L0125×2，旧 CLI 接受单独 `List<Nothing>`。独审另确认 capability、名义参数和静态 bound 内的接口旁路 | 后置签名实参校验与 runtime type-ref 检查，保留重载形状，见 §12 |
| F9 | Guide 12：具体单态化后的顺序容器必须仍满足存储合同 | 独审实跑 `fun <T> fail(): List<T> = error("never")` 和 `fail<Nothing>()`，CLI exit 0 | OPEN，核对候选 trial 中替换后的具体类型、诊断归属与回滚 |
| F10 | Rc 签名载荷与 body 构造资格存在差异 | 独审实跑 `fun reject(a: List<Rc<Copyable>>): Unit {}`，CLI exit 0 | OPEN，逐项核对 Rc 签名、构造与嵌套验证的 producer 和准入责任 |

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
| R1 | 独立抽查 UTF-8/身份、模式栈、Pratt、strict trial、lambda DFA；F4/F5/F6 修复，恢复矩阵、分句预算与全部 frontend 实跑 | class-family 全部 malformed owner 未穷尽 |
| R2 | 独立检查 trial/final admission/provenance；F1/F2/F8 已修复复核；既有 types/callable/final-admission 实跑 | 泛型替换 F9、Rc 载荷 F10、逐消费者处置及全部流事实组合尚未闭合 |
| R3 | basic/const factory 身份链与临时父 owner 关系独审；F3 已修复复核；unit ownership/deinit/两阶段借用与能力隔离实跑 | F7 已以能力门关闭漏报，旧消费者迁移仍待完成；循环/析构全矩阵与动态资源验收未闭合 |
| R4 | 独立核 routes、函数/thunk 无错误回退、drop 核验、SSA/LLVM 双门及 sibling 原子发布；CLI 本机 build 通过 | 全家族 subset/build、emit/verifier 与 debug/fault/sanitizer、Linux 实跑待验；目标机仍为 OptimizationLevel::None |
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

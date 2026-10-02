# SPEC-0236: String.clone 显式深拷贝端到端

> **性质**：变更合同 · **状态**：done · **读取时机**：实施或验收 String.clone 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P234-0236` |
| 所属 Phase | Phase 2 / 3 / 4 |
| 语言规范 | [Koven v0.39](../../guide/README.md)，[String 封闭操作](../../guide/13-program-runtime-standard-library.md#封闭的最小操作) |
| 批准依据 | 2026-10-01，用户对“先落地 String.clone()，暂缓 Str 和 toString() 的规范切换”明确回复“认可，开始实施” |
| 前置 Spec | [SPEC-0192](0192-general-string-runtime.md)（done） |
| 前置 ADR | [ADR-0018](../../adr/accepted/0018-string-owner-runtime-abi.md)（accepted）、[ADR-0027](../../adr/accepted/0027-explicit-string-clone-abi.md)（accepted） |
| 关联 ADR | ADR-0027 |
| 阻塞项 | 无；新增断言已双平台验收，归档提交最终CI按第13节完成交付门禁 |
| 影响范围 | `lang-frontend`、`lang-codegen`、`lang-cli` 回归测试、文档与结构门禁 |
| 语言语义变更 | 是；仅启用 v0.39 的 String.clone 增量及 String literal / Str 冲突消解 |

## 1. Goal

单文件与 compilation-unit 编译链均能将 builtin `String.clone()` 编译为 shared Borrow 源、
返回独立新 owner 的显式深拷贝；源与结果分别遵守普通 move、loan 和 ASAP drop 规则。

## 2. 背景

一般 String runtime 已有唯一 owner、literal、concat、比较与输出，但缺少将借用值物化为
独立 owned String 的封闭操作。本次从实际 main v0.38 基线增加该能力；完整语义以
[v0.39 String](../../guide/13-program-runtime-standard-library.md#string) 为准。
不把任何尚未合并的 SPEC-0235 分支规则带入本次 guide；0236 先集成，0235 之后重基到后续
v0.40，不能与本次 v0.39 并列成为 current。

## 3. 范围与需求

- Phase 2 在单文件与 compilation-unit 路径发布稳定的 String clone intrinsic identity、
  receiver 类型、shared Borrow effect 和 owned String 结果；绑定 builtin identity，不以
  用户声明的 `String` / `clone` 名称冒充 intrinsic。
- Phase 3 消费 typed facts，建立调用期 shared loan，保留源 owner 可用性；结果形成独立
  owner obligation。覆盖局部值、Borrow 参数、临时值、以及已支持的 owned / Borrow
  顺序容器的 String 元素 place，不改变元素的不可移动规则。
- Phase 4 通过专用 `StringClone` SSA operation 运输该契约；verifier 要求有效 shared loan、
  String operand/result 与独立 owner。LLVM 复用目标布局、集中 malloc/free/abort adapter，
  遵循 ADR-0027 的非空精确长度分配与空串 canonical storage。
- 沿用既有诊断类别与 span；错误参数/类型实参、错误 receiver、move 后读取等不得绕过
  frontend 检查，也不得靠 backend 成员名称分支补语义。
- `String?` 沿用一般 nullable 规则，不添加 nullable clone 特例；既有 inline-nullable
  String native ABI 限制保留。

## 4. 非目标

- `Str`、Str→String、`toString()`、interpolation 转换协议；字面量仍是普通 String owner。
- `Copyable` / `Cloneable` 能力扩展、ARC/GC、共享缓冲区、SSO 或容器整体深拷贝。
- 一般 extension/member 机制、通用 clone 协议、其他 builtin clone 或常量求值 clone。
- SPEC-0235 的其他语义内核规则、inline-nullable String ABI 或新增 workspace crate。

## 5. 验收标准

- [x] 单/多文件 typed 合同可见；仅 builtin String 的合法零参零类型实参调用绑定 intrinsic
- [x] source 在 clone 后仍可读，结果可独立 move / 返回 / 捕获；Borrow 参数和临时源合法
- [x] owned 与 Borrow 容器元素 clone 保留元素 owner；直接 owned 读取仍按既有规则拒绝
- [x] moved source、错误参数/类型实参、错误 receiver 的诊断与 span 回归覆盖
- [x] SSA 正向 verifier 与错误类型、缺失/失效/错误模式 loan 等负向用例覆盖
- [x] native 覆盖 heap/static/empty、Unicode、内嵌 NUL、单/多文件及源/结果独立生命周期
- [x] 非空结果 malloc 精确 length 并完整复制；空串不 malloc；drop 不重释放或释放静态存储
- [x] 受影响 Rust 定向门禁与下游检查已执行；clone 相关门禁通过，既有5项失败单列，记录实际命中数
- [x] Architecture 更新为经代码与测试确认的事实；Guide / ADR / proposal / inventory / DAG 一致
- [ ] PR 对应最终提交的必需 CI 全绿且无未决状态后，迁移 Spec 到 archive 并同步索引

## 6. 技术方案与边界

类型检查、ownership 和 SSA 只消费上一阶段发布的事实；单/多文件入口对外语义相同，
可共享窄领域 helper，但不得用单文件循环替代 compilation-unit 分析。
长期布局与分配决定见 [ADR-0027](../../adr/accepted/0027-explicit-string-clone-abi.md)，
原始布局和 drop provenance 继续由 ADR-0018 规定。frontend 不包含 pointer/capacity/LLVM 事实。

## 7. 实施计划

1. [x] 完成 typed intrinsic 和 ownership 事实运输 → 定向 frontend 正反例
2. [x] 完成专用 SSA/verifier 与 native helper → 定向 codegen/SSA/native 检查
3. [x] 同步实现事实和文档治理 → Python 文档门禁及迁移校验
4. [ ] 填写实际验收、创建 PR、等待 CI 后归档 → 最终提交与 CI 证据

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | String.clone 全链路、测试与 v0.39 文档 | `feat(string): add explicit owned clone (SPEC-0236)` |
| 2 | 必需 CI 验收与 Spec 归档 | `docs: close String clone acceptance (SPEC-0236)` |

所有工作在 `feature/spec-0236-string-clone` 范围封闭交付；本合同的 in-progress 状态不宣称本地测试或 CI 已完成。

## 9. 未决问题

无语义未决项。本地实现与所列定向验证不等于远程 CI 完成；当前未获新分支发布授权，
只做本地阶段提交，保持 active / in-progress。

保留的 native 边界：Borrow `Rc<String>` 参数的 `.value.clone()` 仍确定性返回
UnsupportedNode（单/多文件负测），不把 handle-slot pointer 当 control block；本轮不扩大
旧 Rc / nullable ABI。owned Rc payload、String 参数与 owned/Borrow 容器元素不受此限制。
`String?` inline ABI、safe-call 以及既有未封闭 receiver 形状仍为原边界。

## 10. 验证记录

| 验收项 / 命令（目标与过滤器） | 结果（实际测试数） | 未运行原因 / 复用证据 |
|---|---|---|
| 工具链 | Rust 1.96.0 / LLVM 21.1.8 / Clang 21.1.8 | x86_64 Linux + glibc；统一 target 复用，未运行 macOS |
| Red：`cargo test -p lang-frontend --test string_clone` | 0 passed / 4 failed | 未实现时 MemberReceiver deferred / 元素移动诊断；同选择修复后 4 passed，后续扩至 14 |
| Red：native `string_clone` | 首轮 2 failed | 其中一个 fixture 的数字 Unicode escape 未定义，先改为合法 `\0`；容器 receiver L0136 为真实功能缺失 |
| 审查新增投影 Red：`cargo test --locked -p lang-codegen --lib string_clone` | 8 passed / 2 failed | 临时容器元素与 unit Rc payload InvalidSsa；修复后同范围全绿，后续扩展字段与边界负测 |
| `cargo test --locked -p lang-frontend --test string_clone --test ownership_checking --test ownership_containers --test ownership_construction --test ownership_closures --test ownership_rc --no-fail-fast` | 83 passed，0 failed / ignored / filtered | 14 clone、30 ownership、12 containers、8 construction、15 closures、4 Rc |
| `cargo test -p lang-frontend --test string_clone --test ownership_rc --test type_checking --test type_callable --test multifile_type_checking --test multifile_ownership_checking` | 170 passed / 5 failed，退出101 | 实际先运行multifile ownership 71/71，再运行multifile types 99/104；未使用no-fail-fast，失败后其余选择未执行 |
| `cargo test -p lang-frontend --test string_clone --test ownership_rc --test type_checking --test type_callable` | 118 passed，退出0 | 补跑前批未执行项：当时clone13、Rc4、type_checking82、type_callable19；追加unit drop断言后clone单独14/14，最终83项批次再次覆盖 |
| 5项 multifile types 基线 | 未修复 | 与 SPEC-0228 已登记名称/失败一致：companion_constant_initializers_publish_stable_ordinary_typed_facts、top_level_initializers_publish_stable_cross_file_symbol_and_expression_types、deferred_explicit_constructor_type_arguments_publish_no_construction_fact、cross_file_when_diagnostics_cover_shape_order_coverage_and_branch_join、unit_lambda_diagnostics_stop_jumps_and_returns_at_callable_boundary |
| `cargo test --locked -p lang-frontend --lib ownership_checking::checker::drop_planner:: -- --nocapture` | 96 passed，81 filtered，0 failed / ignored | drop planner共享路径、循环/快照/实例回放 |
| `cargo build --locked -p lang-cli` | passed，退出0 | 实际Linux CLI构建 |
| `cargo check --locked --workspace --all-targets` | passed，退出0 | 最终 Rust 全target编译 |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | passed，退出0 | 无新增 lint allow；新负测初版 expect_err 缺 Program:Debug 已修正后复验 |
| `cargo fmt --all -- --check` | passed，退出0 | 最终格式门禁，新增capture/负测已包括 |
| `cargo test --locked -p lang-codegen -p lang-cli --no-fail-fast` | 563 passed，0 failed / ignored / filtered | codegen493、CLI48单元+3format+9native+6project、4codegen doc-tests；包含clone结果move capture与提前return |
| native allocation/drop counters | passed | 非空 dynamic/clone/static-clone 4次malloc精确匹配4次free，empty clone不分配；拒绝静态free/重复free/泄漏；allocation failure先abort |
| frontend全量 / macOS / 远程CI | 未运行 | 遵循定向门禁；当前宿主仅Linux；本地阶段尚未发布。此前3项call-argument基线失败未重跑，也未声称修复 |
| `python3 -m unittest discover -s scripts/tests -v` | 通过，26 tests | 2026-10-01；含 current 版本唯一性、页元数据与 proposal 候选边界回归 |
| `python3 scripts/check_docs.py` | 通过，405 Markdown files | 文档结构检查不替代语义等价证明 |
| `python3 scripts/gen_spec_dag.py` | 已生成 | 当前 3 live + 213 archive；全图 216 Specs |
| v0.38 归档与 SHA-256 账本核对 | 16/16 通过 | 对 main d3e64a4 原页仅机械重写跨目录相对链接；其余 14 个领域页仅 v0.38→v0.39 |
| PR 必需 CI | 未完成 | 不以本地定向测试替代最终提交 CI |

## 11. 最终交付与关闭依据（2026-10-02）

本节记录最终实现与远端交付；第5节第4项仍有下述证据缺口，不能完成生命周期关闭。
第9–10节“尚未获发布授权”、Linux本地结果、未运行 macOS/CI 与五项基线失败均保留
为当时事实；本轮未重跑 Cargo。

- 实现提交：`345f0309f16bac61c9bfe252c4bd82e52c30fa53`。
- 最终整合：[PR #7](https://github.com/Halckon/Koven/pull/7)，head
  `11051e200441a21cdf6dee6a6d153d2e9ffe26c6`；合入
  `e22e11b736aab1231209e3403bd0c931b9ddb940`，包含于复核 main
  `34189046319a8b727285d471596647d5de56996e`。
- 精确 head 的 [CI run 36877486546](https://github.com/Halckon/Koven/actions/runs/36877486546)：
  8/8 jobs success，macOS/Ubuntu 的 check、Clippy、core、stage、Guide 实际成功。
  stage含 `string_clone`，core运行完整 codegen/CLI；不是frontend全量。

| 第5节原验收 | 最终代码、测试与运行证据 |
|---|---|
| 1：两入口 intrinsic 身份 | `crates/lang-frontend/tests/string_clone.rs` 的 `check_both`、非法实参/类型实参、普通同名member、成功/失败overload trial，14项直接suite；builtin身份与回滚均独立核对 |
| 2–3：独立owner、源可用、容器element | 同suite的 Borrow/owned/临时receiver、result move、field/Rc owner与unit call-return drop事实；单/unit `string_clone.rs` lowering消费typed/ownership合同 |
| 4：错误receiver、moved source、诊断Span | 诊断类别已有 `clone_does_not_make_string_copyable`、错误类型/参数与active exclusive loan负例；但 `check_both`只比较code，未比较primary Span/source slice。本轮未找到clone专项诊断Span回归，此项不能按原勾选直接判定满足 |
| 5：SSA与loan负例 | `crates/lang-codegen/src/ssa/string_operation_tests.rs` 的 `string_clone_requires_active_shared_loan_and_creates_independent_owner` 与 `string_clone_verifier_rejects_ended_exclusive_or_nonstring_loans_and_wrong_result`；源/副本两种drop顺序及精确拒绝类别 |
| 6：真实native值与生命周期 | `native_string_clone_tests.rs` 与 `native/unit_string_clone_tests.rs`：UTF-8、NUL、empty、Borrow/container、临时源一次求值、返回/capture；第10节与最终PR7 core实际执行 |
| 7：精确分配、复制、释放和OOM | `llvm/string.rs`按源length分配并memcpy、空串canonical路径；`string_clone_heap_owners_free_once_and_static_empty_clone_has_exact_allocation_cost`断言4次malloc/4次free且无静态free/重释放；OOM用例在发布结果前abort |
| 8–9：定向、共享/下游、文档 | 第10节83项frontend、96项drop planner、563项codegen/CLI及文档原始记录；[SPEC-0237](0237-local-integration.md)交叉typed与numeric+clone+Box native补充组合证据；Guide/ADR/Architecture保持各自权威 |
| 10：最终发布/CI | 上述精确PR7 head/run与合并节点满足远端交付条件 |

实现与最终CI已经交付，但本批保留 `active/in-progress`，不把原第5节第4项的勾选
当作完整验收证据。下一步需在单文件/unit入口为moved source、错误参数/类型实参及
错误receiver补精确诊断Span/source slice oracle并实际运行，或提供已有等价证据；
`type_checking/checker/string.rs`与unit对应实现静态可见使用name_span，不能替代该回归要求。
此缺口不证明生产实现错误，本次docs-only也不新增或运行测试。

Borrow `Rc<String>.value.clone()`、inline-nullable String ABI及safe-call原限制仍保留；
Str/toString、通用clone、容器整体深拷贝均非目标。第10节失败后未执行的targets、fixture
修正和原五项multifile失败不重写；后者由SPEC-0247单独验收，不能倒称clone阶段全量通过。


## 12. 诊断 Span 验收补强（2026-10-02，本地）

第11节所列代码/Span oracle 缺口已在独立 `fix/spec-0236-clone-span` 分支补强；
基线为 `main 4383509`，测试提交为 `bf690f8c9b6f746ff9e17611001298dd98001d6d`。
本节只追加新证据，不改写第9–11节的历史结论；本批最新提交的远端 CI 尚未运行，
因此保持 `active/in-progress`，第5节最后一项与第7节最后一步继续不勾选。

原 `string_clone` 的14个测试名称与顺序保持，没有为计数新增重复案例。文件内私有
`assert_diagnostics` 以手写的唯一上下文及高亮片段独立定位期望位置，逐项比较诊断总数、
原code顺序、包含source identity与起止byte offset的完整 `Span`，以及实际source slice。
所有原负例加入不改变语义的中文/emoji注释，并断言诊断之前的byte offset大于Unicode
scalar count；相同 `clone` / receiver 拼写的不同occurrence不能只凭切片相等通过。
生产实现、诊断code/message、共享Source/Span或测试harness均未修改。

| 原验收 / 既有测试 | 本轮补充的精确 oracle（单文件与unit入口均执行） |
|---|---|
| 第5节第4项：`clone_rejects_arguments_and_type_arguments` | L0121只指向多余实参 `1`；L0091只指向对应泛型调用的 `clone` 成员名 |
| 第5节第4项：`clone_does_not_make_string_copyable` | L0131指向move后clone调用中的 `source` 读取，不能误指声明或此前move |
| 第5节第4项：`clone_rejects_non_string_intrinsics_and_nullable_receiver` | Int/List/Array/Rc/Box/nullable六个L0080逐一指向所属调用的 `clone`，不能交换相同片段的来源 |
| 相关既有负例：`clone_cannot_read_through_an_active_exclusive_loan` | L0135指向后发生的 `text` 冲突读取，保留现行Guide所有权诊断合同 |
| 相关既有负例：`failed_clone_overload_trials_publish_no_intrinsic_fact` | L0124指向歧义调用的 `choose`；原trial失败不发布intrinsic事实断言保留 |
| 原正例与非目标 | 原Borrow/owned/临时/元素/source可用/result独立move及loan/drop断言全部保留；普通同名member与safe-call deferred边界不变 |

| 本轮验收命令 / 检查 | 实际结果 | 范围与限制 |
|---|---|---|
| `cargo test --locked --offline -p lang-frontend --test string_clone` | 14 passed，0 failed/ignored/filtered | Rust 1.96.0，x86_64 Linux；5个既有负例中的11条诊断各在双入口断言，合计22条精确code/Span/slice比较 |
| oracle主动突变：同目标加 `clone_rejects_arguments_and_type_arguments -- --exact` | 0 passed / 1 failed / 13 filtered，预期退出101 | 人工将第二个 `clone` 的期望错指第一个同名token，Span比较拒绝 `114..119` 与 `64..69`；不是生产缺陷，突变不提交，恢复后上述完整14项再次通过 |
| `cargo clippy --locked --offline -p lang-frontend --all-targets -- -D warnings` | passed，退出0 | 受影响crate全部targets严格lint；不是执行frontend全量测试 |
| `cargo fmt --all -- --check` | passed，退出0 | 最终Rust格式检查 |
| 测试身份与diff检查 | 14/14名称及顺序保持，`git diff --check` passed | 生产Rust零diff；已核主动突变完全恢复 |
| `python3 scripts/check_docs.py` 与历史正文保全核对 | passed，460 Markdown；原Spec正文逐字前缀保留 | 不改inventory/DAG；脚本未变，Python policy suite本轮未运行 |
| 原全链路最终CI复核 | PR7 exact head `11051e200441a21cdf6dee6a6d153d2e9ffe26c6` 的[run 36877486546](https://github.com/Halckon/Koven/actions/runs/36877486546)仍为8/8 jobs success | 2026-10-02重新读取GitHub run/jobs/steps；复用第11节原交付证据，不冒充本批新断言远端结果 |
| native、完整frontend、workspace check、macOS、本批远端CI | 本轮未运行 | 本片只改局部测试oracle，没有生产/共享Span/harness/API变化；原native与双宿主证据见第10–11节，新增断言须经本批PR CI确认后独立归档 |

第5节第4项已具备本地直接回归证据。下一步先对本批精确head运行现行PR CI；确认必需
检查全绿且无未决状态后，以独立文档提交完成archive迁移、索引、inventory与DAG更新。
Borrow `Rc<String>.value.clone()`、inline-nullable String ABI、safe-call及其余第4节非目标
保持原范围；不以本轮断言补强证明任何这些边界已扩大，也不涉及SPEC-0182。


## 13. 新增 Span 断言的双宿主验收与归档（2026-10-02）

[PR #17](https://github.com/Halckon/Koven/pull/17) 首轮精确head
`4673ebf64181719c0534965ece12bccbc9565e93` 的
[CI run 36994788404](https://github.com/Halckon/Koven/actions/runs/36994788404)
已完成，8/8 jobs success，无job跳过或未决状态。两宿主的workspace check、严格Clippy、
core/native、stage与Guide步骤均实际成功；分别读取stage日志确认本轮新增断言确实执行。

| 宿主与测试job | `string_clone` 实际结果 | 直接核验 |
|---|---|---|
| Ubuntu 24.04 x86_64，job `110799597541` | 14 passed，0 failed/ignored/filtered，0.01s | 原14个测试名称全部逐一命中；5个负例中的双入口code/Span/source slice oracle实际通过 |
| macOS 14 AArch64，job `110799597649` | 14 passed，0 failed/ignored/filtered，0.03s | 同14个测试身份与22条精确诊断比较；未因宿主条件过滤或只编译而跳过 |

本地测试提交 `bf690f8c9b6f746ff9e17611001298dd98001d6d` 对应远端
`9a8fee912f653989408765a4cf63a4f7c5c3079e`，tree同为
`3f89a50f63e2be905276abe0ff2335f9023b47e9`；本地验收文档提交 `20c1b88` 对应上述
首轮head，tree同为 `e262b0993fa1d6b50d841fad133fa2ba1e2e6992`。connector以已核验
Halckon账户创建远端提交；metadata不同造成SHA不同，fetch后的完整diff为空。

第11节的原验收映射继续成立；唯一未闭合的第5节第4项现在由第12节精确oracle及上述
双宿主实际运行补足，第5节第10项由原PR7交付与本轮PR17精确head的必需CI共同证明。
原有界Goal记为 `done`，迁移到archive并同步索引、冻结inventory与DAG。
第5/7节旧未勾项及第9–12节的待发布/缺Span/待CI措辞保留为各次验收的历史快照；
本节记录后继关闭依据，不把后来运行结果倒填进早期记录。

本轮归档提交仅改文档、生命周期inventory及生成图，不再改Rust或测试语义；本地执行
`python3 scripts/check_docs.py`（460 Markdown通过）、Python policy suite（45 tests通过）、
生成物确定性复验与 `git diff --check`（均通过）。
归档提交的最终head仍须跑现行PR CI，其结果更新同一PR，不为重复记录状态持续追加提交。
PR保持Draft，不自动转Ready或合并；第4节非目标及Borrow Rc payload、inline-nullable、
safe-call边界保持，第10节原失败/未运行记录完整保留。frontend全量仍未执行。

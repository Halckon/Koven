# SPEC-0319: 最终类型消费准入与源级诊断

> **性质**：有界修复合同 · **状态**：in-progress · **读取时机**：准备严格 frontend 消费准入与原始 probe 红测时 · **唯一真源**：本合同；语言规则仍由现行 Guide 定义

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P2-0319` |
| 所属 Phase | Phase 2 最终类型资格；Phase 3 前置准入；Phase 6 CLI 诊断 |
| 语言规范 | [Guide v0.43](../../guide/README.md)、[类型阶段边界](../../guide/03-types-generics.md#基础类型检查与局部推导)、[Result](../../guide/09-nullability-errors.md)、[迭代](../../guide/12-collections-destructuring.md#371-compiler-bound-provider-与执行顺序) |
| 批准依据 | 2026-10-14 用户确认有限收尾后按 A诊断阶段封闭→B enum实参分类→C List MoveOnly转移追踪推进；有限收尾main CI已通过，按已审合同推进tests-only红测；真实helper独审与开测许可分别确认 |
| 前置 Spec | [SPEC-0252](../../archive/specs/0252-basic-unit-ownership-driver.md)、[SPEC-0253](../../archive/specs/0253-single-file-analysis-facade.md)；保留其 raw/recovery 合同 |
| 基线 | official fetch main `188654603edde5ece3edab114458116c61510371`，tree `ad1e85b00e64aabd832b00218cb17385e1c8ac29`；独立 `fix/spec-0319` |
| 影响范围 | frontend 最终消费资格与诊断；普通 single/project CLI；仅必要的事实与回归测试 |
| 语言语义变更 | 否；修复现行 Guide 已要求的阶段隔离，不启用新语言能力 |
| 阻塞项 | 合同已独审；真实红测及helper名单尚待固定后开测；当前tests-only，不先改生产 |

## 1. 单一 Goal 与验收边界

普通 single/project CLI 在把源码类型产物交给 ownership 之前，必须完成一次明确的最终
消费准入：已闭合的非法使用给既有源级诊断，仍未完成的必需类型给准确的实现边界诊断，
不让本片冻结根因泄漏为 ownership 内部错误、BlockingDeferred、MissingFact 或 InvalidSsa。
这不是实现所有 Deferred 所代表的语言功能，也不是宣布所有 frontend 错误均已穷尽。

一个 Spec 分三组有界验收：冻结四个根因与 CLI 红测；共享最终资格与不误杀对照；两入口
诊断/退出闭环。B 的 enum构造实参类别和 C 的 List MoveOnly交付不得夹入本 Goal。

## 2. 真实输入与基线

附件四份普通源码逐字审过，原字节与作者矩阵留仓外，完整stderr尚未核验；测试落地时放单一 fixture 真源，
两入口消费同字节。下表 byte Span 均相对原 UTF-8 文件，不含额外 package/const 前缀。
输入取最新95-probe双矩阵包（tar SHA256 `a2eed0228ac3b43aa41eceb17f71f96d173d29560762fcc2867f3031e5df1bd3`）。
其中原matrix.tsv记614a7a7f/rust1.97，增量另附matrix-b43a475b.tsv。下表引用后者的作者报告，
不是18865460的本轮执行证据，不合并两个基线的计数。

| 原 probe | 本片根因分类 | 计划唯一主诊断与 Span | 作者 single / project 结果 |
|---|---|---|---|
| `r01_bogus_member.ko` | 已知 Int 的不存在成员调用；须先核真实选择事实及成员规则 | L0080，`fooBar` `[40,46)` | BlockingDeferred / MissingFact |
| `r02_bogus_member_str.ko` | String 未发布 `length`；不能从 Kotlin 补能力 | L0080，`length` `[44,50)` | BlockingDeferred / UnsupportedNode |
| `p34_for_range.ko` | `0..<5` 类型仍为 Deferred(Call)，provider 前置未完成 | 候选 L0165，完整 `0..<5` `[40,45)`；保留 range 根因，不追加 L0159 | MissingFact / ownership 内部错误 |
| `s04_question_on_nullable.ko` | 已知 Int? operand 不符合 Result-only `?` 规则 | L0085，真实 question_span `?` `[32,33)` | UnsupportedNode / ownership 内部错误 |

p34 不被改写为受支持的 range provider，也不宣称整个 for 在现行首轮合同中合法；
先报告尚未完成的 range 表达式根因。s04 的 `f` 即使未被 main 调用也要检查，不能只审选中entry。
四份源码 SHA256：

- r01：`6c054514b3c6712b4f6b8664f1d2f8bcbc95b26d47f38d837e250f2e32cc6aa2`
- r02：`c4820919e33e78b9dd0f9a1afe1dd9d2f0b932c9c6ba1829750fd553e93c1a9d`
- p34：`0b965cd543ad7fbb628f14caaaf898392e3e53c5094fee9532619c63cf45d6bf`
- s04：`9bc6a61a89737d245f17949b52345e8979f7fb501dfe697c966d59f66b1734b7`

## 3. 诊断与最终资格合同

1. `check_types`、`check_compilation_unit_types` 的中间推断与 recovery 继续可保留
   Error/Deferred、独立正确 facts 及原始原因；不得用全 TypeArena 扫描改变它们的含义。
2. 最终资格依据真实 AST 消费角色和已选 call/construction/projection/intrinsic facts 建立
   必需的类型 roots，再递归检查类型图；合法 TypeParameter 不是未完成类型。两入口使用
   同一分类规则与各自身份适配器，不按名称字符串构造 Result 或其他 compiler-bound 身份。
   覆盖所选source-set中ownership会检查的全部声明/body/initializer；未选SourceMap文件、
   trial候选残留和未被这些roots引用的类型表条目不纳入。
3. 静态 callee/classifier/构造目标不等于运行时值；仅在已选事实证明其角色时免除对应值义务。
   真实 receiver、实参、function-value callee、initializer、返回值及类型组成仍递归检查。
   不跳过整个 callee 子树，不将所有 statement 或 assignment 一律豁免；已完成效果由事实证明。
   单文件assignment无结果消费角色仍须检查target/value，不把既有Deferred(Assignment)效果当缺值。
4. Error 只抑制已有根因的派生错误；同根 local/call/loop body 不重复报类型或 L0159级联。
   独立错误按 source key、Span、code 稳定保留；不能因一个源错误跳过其他文件的正常检查。
5. L0080 仅用于已知 receiver 身份且查找范围已闭合的不存在成员。必须先排除已有 call/field/
   intrinsic descriptor，以及 `clone`/`inv` 等仅 call 语境成员；Deferred(MemberAccess)本身
   不是“不存在”的证据。保留现有容器 L0130 等更精确诊断，不做通用成员解析重写。
6. L0085 只承接本片已确定非 Result operand 的非法 `?`，primary取AST question_span。
   不顺带封闭全部 Result 返回/E匹配规则；无法证明身份时不得据拼写放行或臆造该诊断。
7. 拟新增 L0165（合同已审，尚未注册；当前注册表到L0164）：**最终类型分析未完成**。
   表达实现边界而非语言非法，消息保留具体构造/DeferredReason责任及最小根因Span。
   不复用L0083（不能用于后续阶段deferred）、L0015（parser）或L0164（borrow flow）。
   仅真实DeferredReason或已登记未实现责任可用此码；成功descriptor缺必需事实、分析身份
   不匹配等内部invariant错误仍保持内部分类，不能包装成“合法未实现”。
   未分类的未完成值仍不得获得最终资格；不能用同一含糊“unsupported”掩盖丢失的根因。
8. unit basic和constant在同一最终资格门下，不可在basic拒绝后借constant fallback穿透。
   single严格消费入口在ownership前拒绝；成功资格绑定原sources/inputs/names/typed分析身份，
   不能拿另一次typed检查的成功结果放行当前产物；不改变原facts、ownership规则或常量路径。
   LSP及显式raw调用者仍保留原recovery行为，不悄改其聚合诊断或定义索引合同。

## 4. 最终接缝与必须保留的对照

候选接缝是frontend私有共享 admission分类器，加single/unit身份适配与显式final completion
诊断/资格；严格CLI先通过该资格，再调用既有ownership链。不得在CLI重新推导语言语义。
保留`analysis/single_file.rs`原façade及`analysis/unit_basic_ownership.rs`的raw/base合同：
single CLI在typed_observer内调用frontend最终检查，unit CLI在basic/const分流前通过同门。
旧validate/validate_constants继续只表达原基础/常量capability，新final complete资格与之明确区分。
失败必须携带非空、真实根因诊断；未完成源码不能退化为空FrontendDiagnostics。

以下真实测试不是“错误测试待删”，均须保留其仍有效的合同：

- `type_checking::later_phase_nodes_keep_distinct_deferred_reasons`：raw无诊断且原因区分。
- `multifile_type_checking::expression_tails::remaining_expression_tails_publish_stable_types_and_traverse_cross_file_children`
  与 `initializers::cross_file_unannotated_forward_use_preserves_the_single_file_deferred_boundary`：
  raw facts、输入逆序及旧validate成功断言保留；它们只证明旧基础能力，新增同源码final拒绝断言，
  不伪称旧入口原来已经阻断。
- `source_construction::deferred_explicit_constructor_type_arguments_publish_no_construction_fact`：
  未绑定external Opaque保留recovery，不伪造construction；不能把它误报为未知源码名字。
- `type_checking::non_null_assertion_error_and_deferred_operands_do_not_publish_extraction`：
  已有根因不级联；`body_recovery::body_error_recovers_and_preserves_independent_source_qualified_facts`保留独立正确事实。
- `guide_litmus`的已记录 Shape/Resource/Node/BitMasks type-target Error/None、listOf静态callee
  Deferred(Call)、single assignment效果；必须按角色证明，不复制测试字符串白名单作生产豁免。
- 成功角色对照须包含println/error、Array/List/MutableList构造及已有intrinsic、String.clone/整数inv、
  普通call与nominal构造、assignment、generic TypeParameter；合法Any约束不等于未实现的Any值表示。
  至少一例成功外层call包含非法member或nullable ?，证明子树没有被整片豁免。
- `single_file_analysis::full_recovery_facts_and_original_source_match_frozen_manual_chain`与
  `each_gate_and_observer_stop_before_every_following_callback`的raw顺序、身份与callback合同不变。
  后者含主动停止callback，仅静态保留合同，不自动纳入云执行白名单。

## 5. 精确测试与安全 helper

先固定 tests-only 输入与仓外完整manifest，再由独立review核精确helper链，按下列目标逐名
`--exact`执行；不使用全suite/discover。下列7目标已落地；首批真实结果见§7，不将导入或fixture错误当行为红测。

| 测试target / 计划精确函数名 | 普通源码与断言 | helper链/运行边界 |
|---|---|---|
| `native_cli` / `final_type_admission::single_probe_roots_reject_before_codegen` | 原四probe × build/run × human/JSON；single保持既有exit2，stdout空，根因code/Span准确；无内部错误字样、无新产物 | 复用`native_cli.rs::run`→原版`CARGO_BIN_EXE_kovenc`；不直接launch输出程序 |
| `project_cli` / `final_type_admission::project_probe_roots_reject_before_codegen` | 同四probe同字节放project × build/run × human/JSON；project保持既有exit1，schema v1/位置完整，零额外诊断 | `TestProject`/`project_build`/`run`→原版CLI；标准loader仍只ranges |
| `project_cli` / `final_type_admission::project_const_fallback_keeps_type_admission` | 独立const前缀版本，重新核Span；typed未完成不得穿透constant fallback | 同一原版helper，编译拒例；不改facts/IR或runtime |
| `final_type_admission` / `frozen_probe_roots_preserve_raw_deferred_recovery` | 四probe × 无/有const：raw精确Deferred原因，旧basic/constant资格保持 | 普通SourceMap→lex→parse→names→types；仅读事实与既有validate |
| 同target / `resolved_roles_and_symbolic_types_keep_raw_value_facts` | constructor/callee、String.clone、inv、List.size、Function值、TypeParameter/Any约束和assignment | 两入口真实描述符及值类型；没有final资格API时不声称最终通过 |
| 同target / `existing_root_errors_do_not_cascade_through_calls_or_iteration` | 既有L0085穿过外层call/iteration不级联；UTF-8精确Span及独立Long事实 | 两入口原类型分析；不进入ownership |
| 同target / `selected_source_order_and_forward_types_keep_basic_qualification` | 未选文件隔离、双文件正逆序、ForwardValueType与旧validate成功保持 | 只对真实selected source-set重新分析，无facts注入 |

target名不是`--exact`过滤器的一部分。首个可编译行为红测使用现有公开CLI；4个frontend目标保护raw/base事实，不冒充final准入验收。
新增final资格接口后再补同源码拒绝/正例接受、跨文件稳定诊断及有效外层call中非法member/nullable ?的递归检查；
strict失败不得进入ownership的证明随真实接线补齐，不用forced observer制造停机。
保留既有`diagnostic_model::production_catalog_contains_exactly_the_published_frontend_codes`
与上述raw/recovery目标，注册新码时只更新真实目录预期。LSP现有recovery相关精确目标另核后列入共享回归。
合法正常源码的native对照仅在后续名单审定后运行原版LLVM/native，不继承旧批次测试数。
不运行云故障、counted-allocation、sanitizer、calibration、aggregate或CompositionContracts。

## 6. 非目标与停止条件

不实现Pair/Result自动加载、Result?完整传播、range/Map provider、通用unknown-member解析、
新数值/字符串能力、error文本打印或新的CLI命令；不改变Guide、B/C分类/交付、SSA/LLVM/ownership。
不以删除single、合并全部入口或增加通用MIR为前置。若真实角色/facts不足以区分合法占位与
必需值，先固定该普通源码和缺口，收窄验收或报告延期，不能按同型、拼写或测试名单放行。
需要改变语言语义或已发布诊断含义时停止；新增实现边界码先审含义，不静默变为语言禁止。

## 7. 实施与验证记录

- [x] 官方fetch新main、核全局0319空闲；原probe静读及Guide/旧recovery合同审查。
- [x] 合同独审；有限收尾main CI通过，批准推进本片tests-only红测。
- [x] tests-only实际测试/helper独审后获准逐名执行；接口新增与生产实现保持后继小提交。
- [x] tests-only提交与真实失败基线：4个raw/base保护通过、3个CLI行为红；无编译接线失败。
- [ ] 最小frontend准入及CLI接线；准确诊断、不级联、raw对照和成功能力保护。
- [ ] 影响面共享回归、Architecture事实、双宿主CI与有界归档；不引用旧CI当本片成功。

| 本轮检查 | 实际状态 |
|---|---|
| 源码/Guide/附件静态审查 | 原附件作者矩阵仍单独记账；本轮独立执行以如下固定输入为准 |
| 首批输入/工具链 | `305ae6049d5a19c56ce4c990d45e7ddab6fb7963` / tree `d9b426ac0eb4fbacdea269c1fcb63e953c70e082`；Rust/Cargo1.96.0、LLVM/Clang21.1.8 |
| frontend 4个精确目标 | 全部各1/1通过；仅raw/base保护，不计final资格成功 |
| CLI 3个精确目标 | 全部真实行为红；每个先采集16次普通build/run human/JSON输出，共48次。各函数的oracle在首个r01失败，其后case不计独立断言执行 |
| 首批实际根因 | single：r01/r02 BlockingDeferred、p34 MissingFact、s04 UnsupportedNode；project basic/constant：r01 MissingFact、r02 UnsupportedNode、p34/s04 ownership invalid argument type。human/JSON均未返回预期源级诊断，stdout均空 |
| 输入冻结 | 1420个源码/配置文件前后NUL摘要同为`6b989d0170c88c266bf45e7cf6dff103083f07d355d8a39ca33041d38a68113b`；每目标argv/status/原日志与48次输出留仓外，未改oracle取绿 |
| 未运行 | 新final API、生产修复、共享回归及本片CI尚未执行；没有故障/插桩或全量云测试 |
| 草案阶段 docs / diff / 库存 / DAG | Python文档检查672页通过，diff通过；当时active0/archive302、draft新增0319 |
| active迁移 | 0319迁入active，archive302不变；draft库存移除0319，四份DAG随生命周期重建 |

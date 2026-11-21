# 所有权细化层终态差距快照

> **性质**：当前实现事实审计 · **状态**：current · **读取时机**：判断 E3 是否只剩薄壳或识别残余函数体 lowering 时 · **唯一真源**：下列精确输入的代码与测试，语言/Phase 权限从属 Guide

## 输入与结论边界

首轮代码输入：`89b57bb81556d0a52e6e02a43469830dba29dfd1`，已包含 main
`51a3aad36bd754c268a05c26c8dd0b80940286cb`；对应 Draft PR #423 的初始候选。
本页是第一轮路由与代表性源码审计，不是全部合法程序的穷尽证明，也不代替后续 head 验收。
初始审计未改生产代码、准入、诊断、ignore 或 Spec 验收条件；后继主干集成见 §7。

结论：refine 的两段式主体与大量家族迁移已落地，但**仍有实际生产兼容 lowering，
不只是物理薄壳**。按 [ADR-0032](../adr/accepted/0032-ownership-refinement-layer.md)
终态合同，全部家族迁完后还应删除迁移预检与旧函数体路径；不能由归档数量推导完成。

## 1. 实际分流与不可按目录删除的基础设施

- `ssa/refine.rs::route` 在 `accepts` 不通过时返回 `Compatibility`；接受后才选 Refine。
  `ForcedPerFamily` 和 `with_per_family_lowering` 仅在 `cfg(test)` 存在。
- `ssa/refine/subset.rs::accepts_with_loop_route` 同时检查 signature、body、drop 支持、循环条件，
  最后仍要求 nullable/migrated、有界 receiver/named/temporary call 或已批准的直线标量触发条件。
  因此“不触发迁移”与“构造本身不受支持”是不同原因。
- `ssa/unit_lower/orchestrate.rs::lower_unit_impl` 的 route/continue 后仍构造
  `UnitExpressionLowerer`；`unit_lower.rs::lower_expression` 实际分派 literal、name、call、
  assignment、if/when 与跳转，不是转发空壳。
- `unit_lower/cfg.rs::{carry_pending_operands, merge_unit_exits}`、`borrow.rs`、
  `call.rs`、`call_lifetimes.rs` 等仍有实体/生命周期状态处理；不能把文件存在等同每个分支都活跃。
- 同目录也混有必须保留或独立迁移的共享基础设施：`constant.rs` 的公开 Unit 入口，
  `type_plan.rs::intern_body_scalar_types` 和 `type_lower.rs` 的类型 intern，
  `closure.rs::{refine_callables, refine_thunk}` 被 refine 编排直接复用。
  不能把删除整个 `unit_lower/` 当作 E3 完成标准。

对 root `unit_lower.rs` 及子树共 25 个 Rust 文件做了发现清单，包含内联测试和混合职责文件；
该物理/定义库存不是“25 个未迁模块”或完成百分比。本页只对下列见证作具体分类。

## 2. 分类 A：E3 终态迁移差距及组合缺口

下表保留首轮审计的路由结论，后继固定用例的状态见 §7–§8。

| ID | 精确见证与当前行为 | 生产落点 / 分类依据 |
|---|---|---|
| A1 | `refine_tests::container_tests::map_cross_function_routing_is_isolated`：初始仅简单标量 helper 不在 refine 集合；合入 SPEC-0454 后，该 helper 也进入 refine，保留 Map/Inout sibling 隔离 | 原有界缺口已由主干 `cc83d9ec` 闭合；仅限已批准的直线标量，不代表取消全部迁移门或完成统一路由 |
| A2 | `loan_region_ssa_tests::receipts::receiver_route::neighboring_receiver_shapes_keep_existing_routes`：Boolean 结果、Borrow 参数、两次调用、非零实参四份固定源的 caller 无 refine 路由，SSA/LLVM 均成功 | `subset/named_route.rs::bounded_receiver_candidate` 只新增受限 trigger；这里指没有其他迁移原因的固定 caller，不声称这些 receiver 构造在任何上下文都不能 refine |
| A3 | `unit_lower_string_tests::lower_frontend_nullable_unit_comparison_tests::nullable_when_read_views_preserve_owned_subject_for_reuse_unit_comparison` 与 `nullable_when_consumes_inner_without_duplicate_owner_unit_comparison`：owned `Node?`、`Box<Token>?`、`Rc<Int>?` 保留 view 或唯一 take，SSA/LLVM 验证成功 | refine `subject_when` 尚不接受 nullable subject；旧 `control.rs` 分派至 `nullable_when.rs`。这是已支持 pointer-like owned nullable 的迁移，不要求新增 loan/place branch ABI |
| A4 | `refine_tests::map_condition_tests::retired_ownership_primitive_boundary_tests` 第一例：nullable `Int?` subject when 与 replace 组合自然进入兼容路径，在真实 replace span 报 UnsupportedNode | 已确认组合拒绝，不把它写成“原来已能成功的程序退化”。inline nullable when 的阶段/ABI与允许范围须先核合同；不能直接扩大为所有 nullable、删除负控或重启旧 replace 实现 |

在该历史基线上，A1 是初始差距及已落地主干的有界修复，A2–A3 是 ADR-0032 终态尚未收口的具体证据；
它们不等于已批准无限扩展 caller/控制流组合。
每个实施切片仍须独立 Spec、失败测试及现有能力/清理负控；A4 尤其需要先明确能力范围。
本轮只识别这些差距，不据总合同自动开放任何新前端事实或后端准入。

## 3. 分类 B：明确延后或本次不能自动计作 E3 欠账

| 范围 | 权威与处理 |
|---|---|
| pointer-like Borrow/Inout/field/element nullable subject 的 proof lowering | [Guide 的 Nullable 阶段交接](../guide/15-conformance-and-staging.md#nullable-的阶段交接)要求后继 nullable-place/loan branch ADR；不能用 owner-only NullableBranch 代替 |
| Elvis、safe call、`as?`、nullable function value、条件借用结果与跨 nullable 的 place-return | 同上明确延后；不因 E3 统一路径目标自动实施 |
| callable Value 参数、字段/容器存储与跨 layout join | [ADR-0034](../adr/accepted/0034-refine-closures-and-callable-views.md)首片范围外，需后续实例化合同/决定，不把已有 callable view 支持外推为全覆盖 |
| dyn/Shareable、协程、自举与未排期语义 | [Guide Phase 边界](../guide/15-conformance-and-staging.md#phase-边界)明确排除 |
| D10/E2 新 owner/borrow 产品的任意形状、任意轮次等推广 | SPEC-0341 的有界产品与 E3 路由有交接，但其全体未完成项不是本轮 E3 自动实施范围；须核各自批准切片 |

例如 `refine_subset_tests::non_nominal_box_payload_is_rejected_before_refinement`
中的 `Box<Int>` 是 frontend L0117 拒绝，不是“缺一个 refine 分支”；不可为提高迁移覆盖率放行。

## 4. 分类 C：测试专用旧路径与容易误判的名字

- `with_per_family_lowering` 是 test-only 强制路径，用来保留旧语义观察与精确拒绝负控。
  `retired_ownership_primitive_boundary_tests` 第二例钉住旧 replace 拒绝；与第一例生产自然分流必须分开。
- `refine_subset_tests::map_for_unrelated_scalar_scope_keeps_legacy_routing` 名字仍含 legacy，
  但当前正文明确 `assert!(routed)` 并验证 Map-for lowering 成功。它证明 sibling 隔离与
  已迁路径，不证明生产还有一套 Map-for；本轮不顺手改名。
- `functions_outside_the_subset_keep_the_per_family_path` 只断言路由未进入 refine，
  对返回的 lowering Result 不作成功断言；不能把其每个输入都计为旧路径成功能力。
- 有 UnsupportedNode 的 guard 可能防止已退休操作重新落入旧实现；删除 guard 不等于完成迁移。

## 5. 已实现与验收未闭合分别记账

- SPEC-0360 的 literal/Group String when 已有实际 subset/build 消费与测试；当前
  `nullable_map_consume_fixture_tests.rs` 中保留 `p41_original_source_runs_through_refinement`
  及分配/清理测试。active Spec 的历史“官方 CI 待核”和旧测试文件路径需要逐项归因核验；
  不能直接说“没有实现”，也不能把某次整个 CI 绿自动填成所有验收已完成。
- SPEC-0392 的层间审查、0341 的 Unit 管线及产品交接仍有各自边界；不合并成一个 E3 完成率。
- 0451–0453 的[34项保留边界](../archive/specs/0453-lang-codegen-remaining-leaf-and-llvm-shells-batch-retirement.md#52-34-项保留清单唯一交付清单)已获用户批准；原全部删除目标未达成，修订合同有界归档。这是尺寸/职责库存，既不穷尽上述生产差距，也不能证明迁移完整。
- 初始代码头 `89b57bb8` 的 [PR #423 CI 43554274292](https://github.com/Halckon/Koven/actions/runs/43554274292)
  已成功结束；它不消除上述有测试锁定的兼容路径，也不自动完成各历史 Spec 的逐项归因。
  本次文档追加后的检查/复用须按新 head 独立核验。完整回归与既有迁移审查以实际选择范围为准，
  旧超时、CI-only 列举及其他 PR 的绿灯不转换成未执行检查通过。

## 6. 本轮证据

代码头 `89b57bb8`；macOS AArch64，Rust 1.96.0、LLVM21，独立 Cargo target。
以下九项普通测试在默认和 `loan-observation-test-support` 下分别为
**9 passed / 0 failed / 0 ignored**，命令退出码均 0：

1. A1、A2 两项路由成功见证。
2. A3 两项 owned nullable view/take 见证。
3. A4/C 共用的 `retired_ownership_primitive_boundary_tests`。
4. `refine_and_legacy_validation_gate_matrix_tests`：合法混合路由与精确拒绝位置。
5. 分类 C 的 Map-for 名称误导例与 subset-only 负控。
6. 分类 B 的非法 Box frontend 拒绝见证。

实际命令为 `cargo test --locked -p lang-codegen --lib [--features loan-observation-test-support] --`
后附以上九项的现有完整 libtest 名字。没有运行 CI-only 故障注入、0360 新增动态计数/原生测试、
frontend 全量或完整 codegen lib；这些九项不证明无限源码空间或全部 SSA 算法已正确。

## 7. 已合主干集成后的有界更新

#423 在 `28747ded` 上普通合并已合 main `cc83d9ec`，保留两侧实现；该主干包含
#424 的直线标量及此前已合的前端修复，不包含未合 #427。A1 只按 SPEC-0454 的
直线 Boolean/整数边界闭合，A2/A3/A4 与其他终态合同不随之关闭。

集成后上列九个普通见证加
`refined_linear_and_legacy_branch_helpers_coexist_in_the_native_program`，默认/feature
各 **10 passed / 0 failed / 0 ignored**，退出 0；额外见证验证 refine main/linear
与 Compatibility branching 的实际 native 调用、完整 stdout/空 stderr。
双配置 codegen all-target Clippy 通过。未重跑完整 LLVM/feature、frontend 全量或
CI-only fault；[SPEC-0453 §4.5](../archive/specs/0453-lang-codegen-remaining-leaf-and-llvm-shells-batch-retirement.md#45-runtime-abort-的-map-result--string-职责恢复)
的 feature 基线观察期限仍不是通过，最新合并候选 CI 另验。

### A2 的 Boolean 结果本地候选

在已同步 `8207b212` 的 E3 局部候选中，只迁移原 A2 见证里「独占持有非
resource 的 `Node` 参数，仅一次零参数共享接收者调用，丢弃 `Boolean` 结果」
的形态。首次回归要求该原始 caller 进入 Refine，原实现返回空路由集合而失败。
最小实现只在原有前端 Protocol 2 观察、Refine 准入和 receipt 结果判断中接纳
`Boolean`，不新增借用或清理事实，也不重写动作生成。

`boolean_receiver_refines_and_preserves_identity_and_owner_release` 在双 Unit 布局
核对原始调用源码、有/无观察产品、真实 SSA 指令及实体身份。独立 native 入口仍调用
同一个 `run`，计数器按分配身份检查它的唯一 owner 释放一次。
其余 A2 三种写法以及相应的 Boolean 结果变体仍由原普通入口负控固定在兼容路径。
这不关闭 A2 其余兼容项，不涉及括号路由，也不代表远端或 Linux 验收。

A2 的借用参数见证继续走 Compatibility。新增
`borrowed_receiver_fallback_forwards_entry_loan_without_owning_release` 保留原
`run(node: Node)` 并附一层 native 调用者，双 Unit 布局中跟踪不同函数内的
loan 身份：`run` 只把导入的共享 loan 传给方法，不建立或释放 owner；外层
调用者在返回后结束原 loan，再释放最初分配的同一个 owner。该测试同时检查
真实 SSA、LLVM 和 native 计数，不扩大 Refine 准入。

A2 的同一 owner 两次共享调用仍留在 Compatibility。
`two_shared_receiver_calls_release_one_owner_after_both_loans_end` 在两个 Unit 布局
分别按原始调用的源码范围跟踪两条不同 loan：第一条结束后才开始第二条，两条都
指向同一参数 owner，且第二条结束后才产生唯一 Drop。外层 native 入口只创建并
转交该 owner，实际执行按分配身份验证只释放一次，不增加 Refine 准入。

A2 的单个显式标量实参也仍走 Compatibility。
`explicit_scalar_receiver_fallback_preserves_callee_argument_and_owner_release`
在双 Unit 布局证明先取得接收者共享 loan，后求出 `1` 并送入独立的标量参数槽；
方法返回该参数，调用返回后结束 loan 并释放原 owner。外层 native 包装转交
同一分配，实际只释放一次，不增加 Refine 准入。


## 8. A2–A4 固定见证复核（2026-11-21）

以下只重分类列出的固定源码，不把相邻语法、借用或 owner 形态一并标成完成。
A3、A4 两个测试模块在主干同步点 `8207b212` 与本地复核树 `570a3633` 中无差异。

| 见证 | 复核后的路径与证据 |
|---|---|
| A2 | 本地候选 `0620936c` 只将既定的 owned Boolean 零参数 caller 迁到 Refine。复核树 `570a3633` 为借用参数、同 owner 两次共享调用、一个显式标量实参三项保留原 Compatibility 路由，并补了两种 Unit 布局中的 SSA/LLVM 及 native owner 释放见证（§7）；其余形态不推论。 |
| A3 | [原始 owned nullable 源的路由与验证](../../crates/lang-codegen/src/ssa/refine_subset_tests/nullable_when.rs)：原普通入口验证 `Node` 12 个、`Box` 6 个、`Rc` 6 个函数实际走 Refine，完整 SSA/LLVM 通过；另一项测试复核三份源的两种 Unit 布局。不覆盖借用位置或其他更宽的组合。 |
| A4 | [原 inline `Int?` + `replace`](../../crates/lang-codegen/src/ssa/refine_tests/map_condition_tests.rs)：普通入口下 `replace_then_select` 实际走 Refine，LLVM 通过，native 输出 `ok`。定点 `UnsupportedNode` 仍保留，仅在测试强制 per-family 路由时被观测；不再将此固定源码的普通路径算作未实现。 |

在 macOS 上使用复核树 `570a3633` 的 `lang-codegen --lib`、feature
`loan-observation-test-support` 精确复跑 A3 两项和 A4 一项，均通过。
这是定向本地复核，不代表生产路径已全部退役或远端 CI 已验收。

另有一项独立可行性探针：仅在显式标量实参见证的 caller 里加入空的
`Int?` 局部绑定作为既有 Refine 路由条件。两种 Unit 布局实际由通用 Refine
实现完成原接收者、实参与每个释放动作，无 Protocol 2 receipt。
在探针基线 `461256d4` 上，原始无附加条件的源码仍走 Compatibility。这证明基本
lowering 能力，不是该形态直接准入或实际 receipt 所需证明的替代。

### A2 单个显式 Int 字面量本地迁移

以 `461256d4` 为基线保留原一条语句的 owned 非 resource caller，仅给
无名称、无模式标记、无括号的单个无后缀 `Int` 字面量补 Refine 准入；调用的
唯一直达参数须为 `own Int`，返回类型仍是 `Int`。基线对同一源码接受但不进入
Refine，作为红测。新路径不改变借用开始/结束或 owner drop 的产生，不增加
Protocol 2 的三个动作和五个 SSA 生命周期指令。

私有校验记录跟踪原字面量范围、值、已降低的 Slot 和实际 SSA 定义/Value，验证
该 Value 紧邻调用且是真实实参。双布局的观察 ON 核对原 Complete2 和 SSA
生命周期端点，真实 OFF 核对同一路由与等价 SSA/LLVM，native 核对同一 owner
唯一释放。独立 anchor 正控仍无 receipt；命名、借用形参、带括号及计算表达式
负控不继承此直达准入。借用接收者和多调用的旧路径不变，未扩大为任意实参。

### A2 借用转发与双调用的独立 Refine 对照

本地测试提交 `62263079`、`a798dd97` 保留原始源码作 Compatibility 对照；
各只增加 `val anchor: Int? = null` 便经普通入口选中 Refine，不改变原样准入。
借用参数 caller 只用导入的 loan 作方法接收者，不创建或退休本地 loan，也不 Drop；
外层才建立、退休借用并释放原 owner。两路径均无后端 receiver receipt。
双调用依序使用同一 owner 的两条不同 loan，中间无 Drop，第二条退休后才唯一 Drop。
原样前端按调用范围分别为 Partial、Complete2，末次局部证书不覆盖第一次；
带声明的对照不符合「仅调用表达式」观察门槛，两条为 Unsupported，均无后端 receipt。
两种 Unit 布局的实际 SSA、LLVM 和按分配身份唯一释放均在 Mac 验证。
前者需要导入 loan 的转发证明，后者需要贯穿两条借用的顺序及唯一 Drop 证明；
都不能以单个局部 Complete2 取代，也不计作生产迁移或兼容退役。

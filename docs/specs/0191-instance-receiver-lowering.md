# SPEC-0191：instance receiver 与静态委托 lowering

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-191` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.34 §34](../guide/01-design-decisions.md#34-显式-instance-receiver-契约与静态分发调用v034) |
| 批准依据 | 2026-08-31 持续 Goal 要求继续按 Phase 推进 guide 对应 Specs，并简化验收；v0.34 已启用且 SPEC-0180/0181 已完成 |
| 前置 Spec | SPEC-0034、0035、0038、0039、0177、0184、0195、0180、0181 `done` |
| 前置 ADR | [ADR-0016](../adr/0016-interprocedural-borrow-abi.md) `accepted` |
| 关联 ADR | ADR-0006、0008、0009 |
| 阻塞项 | Copyable ordinary-class Inout payload assignment 已闭合；MoveOnly field replacement 等待 Phase 3 旧字段 drop/replacement fact；Value interface default 的 MoveOnly concrete `StaticSelf` 等待 Phase 3 条件 receiver-drop fact |
| 影响范围 | `lang-codegen` callable SSA/frontend lowering/LLVM/member native tests；Architecture/Roadmap |
| 语言语义变更 | 否；lower 已验证 receiver facts |

## 1. Goal

完成后，class/value/enum/object/interface 的静态分发 member/default/override 与 Borrow-only
delegate 调用可经 verified SSA、LLVM、object/link/run 执行，receiver mode 与 frontend
所有权效果一致且不生成 vtable、proxy、隐式 copy/retain 或额外 allocation。

## 2. 范围与需求

- callable SSA signature 把隐藏 receiver置于第一参数；Value 使用既有 value/owner ABI，Borrow/
  Inout 使用 ADR-0016 shared/exclusive loan pointer ABI，显式参数顺序随后保持不变。
- member instance key 使用静态 callable target、owner type arguments、callable type arguments；
  从 receiver typed/ownership descriptor lower，不按名称或 AST owner 重新选择。
- lower member body 的 `this`、field place、Borrow/Inout load/store 与 Value owner/drop；receiver
  求值一次，loan/copy/move 在第一个 argument 前发生并与 frontend end/drop facts对应。
- Inout ordinary-class receiver传递现有 handle storage 的 exclusive loan；callee load 同一 handle
  后只修改 payload field。SSA/verifier 与 LLVM 测试拒绝 receiver-binding store、handle 替换或
  把 payload pointer 误当成另一套公开 ABI；inline receiver 继续借用实际 inline storage。
- default/override/`super<I>` 只生成静态 direct call；interface `Self` 在单态化时替换为具体
  owner，不产生 runtime interface value。
- 无状态 object receiver 按静态唯一 value identity lower，可使用临时 ZST addressization 满足
  Borrow ABI；不生成 singleton allocation、全局初始化、guard 或退出析构。
- Borrow-only delegation 生成确定性静态 thunk 或等价直接转发；只投影 delegate field 和转交
  既有 loan/arguments，不生成隐藏 AST、retain、proxy allocation或新 owner。
- SSA/verifier 拒绝 receiver mode/type/loan kind、instance key、ownership plan 与 callee signature
  不一致；verified-before-LLVM 不变。
- 真实 source→object→link→run 覆盖多种 nominal/generic receiver、drop 与调用顺序；DWARF
  仍指向用户 member body，compiler thunk 不伪造源码声明。

## 3. 非目标

- 不实现动态 dispatch/vtable、dyn value、FFI method ABI、bound callable reference或extension。
- 不实现 safe-call、nullable assertion/when receiver、borrow-return或 iterator/provider runtime。
- 不增加非 Borrow interface delegation、公开符号稳定 ABI或跨 compilation-unit member ABI。

## 4. 验收标准

- [x] SSA signature/DirectCall receiver mode 与 verifier 正反矩阵通过；本 Spec 不为已排除的
  bound method value 虚构 `CallableInvoke` receiver source path。
- [ ] class/value/enum/object、generic owner+method、default/override/`super<I>` 静态实例运行正确。
- [ ] Borrow/Inout LLVM pointer ABI、Value owner ABI、receiver-before-arguments 与一次求值被 IR/运行锁定。
- [x] class Inout val-handle native mutation 保持 handle identity；verifier/LLVM 反例拒绝重绑
  receiver、写回另一 handle或使用 payload-only 私有 calling convention。
- [ ] Borrow delegate 与手写转发结果/loan/drop 一致，无 vtable/proxy/retain/额外 allocation。
- [ ] MoveOnly Value receiver 唯一消费、Borrow/Inout 不消费，正常/提前退出 drop 精确。
- [ ] 受影响 `lang-codegen`/CLI 窄测及 workspace Layer 2 静态门禁通过，Architecture/Roadmap/Spec 同步。

## 5. 技术方案与边界

扩展现有 callable signature/instance planner 和 frontend lowerer，而不是建立 method-only IR；
receiver 只是带显式 delivery identity 的第一个 operand。复用 ADR-0016 的 addressization/loan
parameter、ADR-0008 nominal ABI 和 ADR-0009 closure 环境。delegate thunk 使用编译器内部稳定
FunctionId，不形成源码 `DeclarationId` 或用户可见 stack frame。

## 6. 实施计划

1. [x] 扩展 SSA callable/operation/verifier receiver → 验证：model/render 正反矩阵。
2. [ ] 接 frontend member body/call 与 LLVM ABI → 基础 Borrow/Inout/Value/隐式 `this` 已完成；
   非泛型 value class Borrow/Copyable Value 与 ordinary class Borrow/MoveOnly Value 已完成真实
   object/link/run；ordinary-class Inout Copyable payload mutation及无状态 object Borrow receiver
   已完成，generic nominal layout 与 MoveOnly field replacement 继续实施。
3. [ ] 接 default/override/super/delegate 静态转发 → concrete receiver 直接调用有体 Borrow
   default、concrete override 内 `super<I>`、default→`super<Base>` 及 `this.otherDefault()` 的
   concrete `StaticSelf` 传播已完成；default body 内 abstract requirement→本地 concrete override
   已消费 frontend 映射并完成 native 闭环，generic owner/callable slot 重组已由 planner 白盒锁定；
   ancestor requirement→interface replacement/唯一独立 default 已消费双方 owner template 并完成
   native 闭环，`Host<X,Y>: Derived<Y>` 的 owner/callable slot 配方已由 planner 锁定；generic
   nominal native layout 与 Value default 继续实施。非泛型 ordinary-class Inout default 已完成
   exclusive concrete receiver 的 SSA/LLVM 与 native 闭环；MoveOnly Value default 不由 codegen 猜测
   `StaticSelf` 析构，等待 Phase 3 发布条件 receiver-drop fact。Borrow delegate 的首个非泛型、单层、
   ordinary-class 切片已消费 typed/ownership 双重 validated route，以 `SharedHeapFieldLoan` 直接转发
   concrete delegate Borrow receiver；abstract requirement、本地 override、继承/default replacement
   均直接消费 frontend forwarder 的 exact effective target/owner template，interface default 的
   `StaticSelf` 固定为 delegate field concrete type。非泛型 outer/delegate runtime nominal 上的 generic
   interface owner 与 callable slots 已按 forwarder/implementation owner template 精确重组，并由
   default/local override 的 native 闭环锁定；generic runtime nominal、chained/`None` route 继续
   确定性拒绝，禁止静默调用 outer interface default。
4. [ ] 同步 Architecture/Spec并运行 workspace基线。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | receiver callable SSA/verifier 与 LLVM receiver-first operand 基元 | `feat(codegen): model instance receivers (SPEC-0191)` |
| 2 | frontend member/委托接线与 native 闭环 | `feat(codegen): lower member receivers (SPEC-0191)` |
| 3 | bodyful/default Borrow delegation exact-target 接线 | `feat(codegen): lower default delegation (SPEC-0191)` |
| 4 | Inout interface default 的 concrete exclusive ABI 闭环 | `test(codegen): close inout default lowering (SPEC-0191)` |
| 5 | 非泛型 runtime nominal 上的 generic delegation owner/callable 槽位重映射 | `feat(codegen): remap generic delegation slots (SPEC-0191)` |

## 8. 未决问题

- 无；iteration、nullable member forms 与跨 unit ABI 分别保持独立门禁。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 receiver 审计 | 通过 | 既有 callable SSA 只遍历显式参数；ADR-0016 已为未来 receiver 固定 pointer ABI |
| 2026-08-26 候选闭合审计 | 通过 | 移除无 bound method source path 的 CallableInvoke receiver 验收，只保留 DirectCall |
| 2026-08-31 重基审计 | 通过 | v0.34 已重基到完整 v0.33；SPEC-0180/0181 已完成，持续 Goal 的站立授权允许进入 Phase 4 |
| `cargo test -p lang-codegen --lib direct_call` | 5/5 通过 | Layer 1：receiver presence/mode/type、receiver-first render/LLVM 与既有 DirectCall ownership 回归 |
| `cargo test -p lang-codegen --lib` | 231 通过、1 ignored | 共享 DirectCall IR 不变量触发 Layer 3；ignored 为既有 debugserver task-port 权限用例 |
| `cargo clippy -p lang-codegen --lib -- -D warnings` | 通过 | 当前 SSA 切片的 Layer 2 静态门禁 |
| 独立高风险复核 | 通过 | 未发现 receiver/argument 边界、ownership、FunctionAddress 或 LLVM 顺序的 P1/P2 |
| `cargo test -p lang-codegen --lib unit_plan_tests` | 4/4 通过 | Layer 1：member reachability、静态 target、owner+callable type arguments 与既有确定性回归 |
| `cargo test -p lang-codegen --lib unit_lower_receiver_tests` | 12/12 通过 | Layer 1：Borrow/Inout/Value、显式/隐式/reborrow receiver、`return this`、CFG carry、求值顺序、loan end、drop 与 LLVM ABI |
| `cargo test -p lang-codegen --lib shared_reborrow_blocks_parent_end_and_exclusive_call_until_child_end` | 1/1 通过 | Layer 1 负例：derived shared loan 活跃时拒绝 parent end 与 exclusive receiver call |
| `cargo test -p lang-codegen --lib unit_lower_borrow_tests` | 2/2 通过 | Layer 1 回归：既有显式参数 shared loan/forwarding 与 Inout 原子边界不变 |
| `cargo test -p lang-codegen --lib ssa::unit_lower_tests` | 14/14 通过 | Layer 1 回归：顶层 reachability、call、control-flow 与 owner transfer 不变 |
| `cargo test -p lang-codegen --lib` | 245 通过、1 ignored | receiver/reborrow 共享 SSA 与 CFG 不变量触发 Layer 3；ignored 为既有 debugserver task-port 权限用例 |
| `cargo clippy -p lang-codegen --lib -- -D warnings` | 通过 | 基础 source member receiver 切片的 Layer 2 静态门禁；未运行耗时的 `lang-frontend` 全量测试 |
| 独立高风险复核（基础 source member receiver 切片） | 通过 | 针对 CFG/loop receiver carry、合法 reborrow、`return this`、parent/child loan dependency 与跨 edge loan identity 逐项复核；发现项均修复并复核至无 P1/P2 |
| `cargo test -p lang-codegen --lib native::unit_tests` | 3/3 通过 | Layer 1：新增跨文件非泛型 direct member 的 source→object→clang→run；stdout 锁定 receiver→argument→body 与 Copyable Value 重复使用，同一 source 的 verified SSA 另锁定 class Borrow→end→同 owner Value、caller 零 drop/callee 单 drop |
| `cargo test -p lang-codegen --lib unit_lower_receiver_tests` | 12/12 通过 | 与 native 用例并行运行的 receiver SSA/LLVM 回归；未重复运行 `lang-frontend` 全量测试 |
| `cargo clippy -p lang-codegen --lib -- -D warnings` | 通过 | 与 native unit 小集合并行执行的 Layer 2 crate 静态门禁 |
| Phase 2 assignment 门禁审计 | 已解除 | SPEC-0218 已发布普通 `=` 的 target/value/operator/storage-type/control descriptor；ordinary-class Inout payload mutation 的下一切片必须直接消费该事实，不得由 codegen 重推 |
| 独立复核（direct member native 切片） | 通过 | 首轮发现 class 唯一 drop 仅靠 native 成功会假阳性；补同源 SSA owner/loan/drop identity 断言后复核关闭，最终无 P1/P2 |
| `cargo test -p lang-codegen --lib inout_class_receiver_replaces_and_reads_the_same_payload_field --locked --offline` | 通过 | descriptor-driven grouped-`this` replacement 与 bare field read；callee 只持 exact exclusive receiver loan，LLVM 为 handle load→payload GEP→i32 store/load，member body 无 `store ptr` |
| `cargo test -p lang-codegen --lib heap_field_replace_requires_an_active_unshadowed_exclusive_receiver --locked --offline` | 通过 | verifier 正反矩阵覆盖 exclusive success，以及 shared/inactive/active-derived-loan、越界 field、MoveOnly read/replace 拒绝 |
| `cargo test -p lang-codegen --lib divergent_rhs_does_not_emit_an_inout_class_payload_replace --locked --offline` | 通过 | `Nothing` RHS 消费 descriptor 的 non-fallthrough fact，生成 Abort 且不生成 payload read/replace |
| `cargo test -p lang-codegen --lib inout_class_payload_mutation_is_observed_by_a_later_borrow --locked --offline` | 通过 | 真实 source→object→clang→run 输出 `rhs\nobserved\n`；SSA 回链 setter/getter loan 到同一 caller owner |
| `cargo test -p lang-codegen --lib --locked --offline` | 250 通过、1 ignored | 独立终审运行共享 SSA/verifier/LLVM 全 lib 回归；ignored 为既有 debugserver task-port 权限用例，未运行耗时 frontend 全量测试 |
| 最终并行小集合：`unit_lower_receiver_tests` / `verify_ownership_tests` / `native::unit_tests` | 14/14、12/12、4/4 通过 | 以三组职责测试替代重复的 frontend 全量回归；分别锁定 descriptor lowering、loan/field contract 与真实 object/link/run |
| `cargo clippy --workspace --lib --locked --offline -- -D warnings` | 通过 | 与三组定向测试并行执行的 workspace lib 静态门禁 |
| 独立高风险复核（ordinary-class Inout payload 切片） | 通过 | 补 MoveOnly read 明确反例并修正文档后复核至无 P1/P2/P3 |
| `cargo test -p lang-codegen --lib stateless_object_receiver_uses_zst_addressization_without_runtime_storage --locked --offline` | 通过 | grouped/bare object value identity 只在 Borrow receiver context 物化一次 ZST；SSA 无 heap/shared allocation、retain/drop，LLVM 只有临时 addressization、无 malloc/global |
| `cargo test -p lang-codegen --lib stateless_object_borrow_receiver_links_and_runs_without_runtime_storage --locked --offline` | 通过 | 真实 source→object→clang→run 输出 `object\n`，且无状态 object 不建立 runtime singleton storage |
| 最终分层小集合：`multifile_ownership_checking` / `unit_lower_receiver_tests` / `native::unit_tests` | 51/51、15/15、5/5 通过 | 用单个 frontend integration target 加两组 codegen 职责测试覆盖 Phase 3 facts、SSA/LLVM 与真实 link/run；未运行约一小时的 frontend 全量测试 |
| `cargo clippy --workspace --lib --locked --offline -- -D warnings` | 通过 | workspace 库级静态门禁 |
| 独立高风险复核（stateless object Borrow receiver 切片） | 通过 | 复核 object 双命名空间身份、runtime drop 排除边界、ZST receiver-only 门禁及 `L0134` 优先级，最终无 P1/P2/P3 |
| `cargo test -p lang-codegen --lib plans_one_interface_default_instance_per_concrete_static_self --locked --offline` | 通过 | 同一 default target/type arguments 被两个 concrete implementor 使用时形成两个独立实例 key；`static_self` 与普通类型实参保持正交 |
| concrete `StaticSelf` SSA 定向矩阵 | 5/5 通过 | 直接 default、同一 default 的双 concrete symbol、concrete override→`super<I>`、default→`super<Base>` 与 `this.otherDefault()` 均使用 concrete shared-loan target 和静态 DirectCall；无额外 reborrow/vtable/interface runtime type |
| `cargo test -p lang-codegen --lib interface_default_and_super_static_calls_link_and_run --locked --offline` | 通过 | 真实 source→object→clang→run 输出 `default-super\n` |
| 最终并行小集合：`unit_plan_tests` / `unit_lower_receiver_tests` / `native::unit_tests` | 5/5、20/20、6/6 通过 | 分别锁定 bounded concrete-self 实例 identity、receiver SSA/LLVM 与真实 source→object→link→run；不重复 frontend 全量回归 |
| `cargo clippy --workspace --lib --locked --offline -- -D warnings` | 通过 | workspace 库级静态门禁，零 warning |
| `cargo test -p lang-codegen abstract_requirement -- --nocapture` | 3/3 通过 | planner 不再计划无 body requirement，SSA DirectCall 复用原 receiver loan，真实 object/link/run 输出 `abstract-override\n` |
| `cargo test -p lang-codegen remaps_requirement_arguments_to_concrete_owner_and_callable_slots -- --nocapture` | 通过 | 非空 `GenericBase<String>` owner prefix 被替换为 `Host<Int>` owner arguments，`Long` callable argument 按已验证 slot 保留 |
| 定向分层回归：`unit_plan_tests` / `unit_lower_receiver_tests` / `native::unit_tests` / `multifile_ownership_checking` | 7/7、21/21、7/7、51/51 通过 | planner、SSA/LLVM、真实 link/run 与 Phase 3 facts 职责分离；未运行约一小时的 frontend 全量测试 |
| `cargo clippy --workspace --lib -- -D warnings` | 通过 | workspace 库级静态门禁，零 warning |
| 独立高风险复核（abstract requirement→本地 override 切片） | 通过 | 无 P1/P2；发现泛型用例 owner prefix 为空与记录计数两项 P3，补非空 prefix 矩阵并同步 7/7 后关闭 |
| 独立高风险复核（concrete `StaticSelf` direct-default/`super<I>` 切片） | 通过 | 首轮发现双 concrete symbol、`this.otherDefault()` 与完成范围措辞三处 P3，补测试/收窄文档后复核至无 P1/P2/P3 |
| `cargo test -p lang-codegen inherited -- --nocapture` | 3/3 通过 | planner 按双方 owner template 把 `Host<Int,Long>` 的 `Base<String>+Int` 重组为 `Derived<Long>+Int`；SSA 复用 concrete receiver loan；native 覆盖 replacement 与独立唯一 default |
| `cargo test -p lang-codegen rejects_nested_inherited_owner_recipe_before_generic_nominal_layout -- --nocapture` | 通过 | 本切片只开放直接 owner slot；`Derived<List<Y>>` 在 generic nominal layout 完成前以 `UnsupportedNode` fail loud |
| 最终并行职责组：`unit_plan_tests` / `unit_lower_receiver_tests` / `native::unit_tests` / `multifile_ownership_checking` | 9/9、22/22、8/8、51/51 通过 | 分别覆盖实例 recipe、SSA/LLVM、真实 link/run 与 Phase 3 facts；未运行约一小时的 frontend 全量测试 |
| `cargo clippy --workspace --lib -- -D warnings` | 通过 | workspace 库级静态门禁，零 warning |
| 独立高风险复核（inherited effective default 切片） | 通过 | 首轮发现 incompatible unique default 错误通过的 P1 及槽位同值/嵌套 recipe 边界两项 P3；补 L0101 原子拒绝、`[Long,Int]` 正例和 `UnsupportedNode` 负例后复核至无 P1/P2/P3 |
| `cargo test -p lang-codegen shared_heap_field_loan --no-fail-fast`（拆分为两个 exact 用例执行） | 2/2 通过 | verifier 正反矩阵锁定 shared receiver、越界及 parent-before-child end；LLVM 锁定先 load heap handle 再 GEP payload field，且不生成 aggregate value extract |
| `cargo test -p lang-codegen --lib --locked --offline` | 269 通过、1 ignored | 独立复核运行共享 codegen 回归；ignored 为既有 debugserver 权限用例，未运行 frontend 全量测试 |
| 独立高风险复核（heap-owner delegate field loan 基元） | 通过 | 首轮发现既有 inline `SharedFieldLoan` 生命周期被误扩张的 P1 与 LLVM 断言覆盖不足两项 P3；收窄 dependency、改用 MoveOnly delegate owner direct-call 链后复核至无 P1/P2/P3 |
| 最终并行职责组：`unit_plan_tests` / `unit_lower_receiver_tests` / `native::unit_tests` / `multifile_ownership_checking` | 13/13、23/23、9/9、51/51 通过 | exact route、default/generic/chain 拒绝、receiver/argument loan 顺序、真实 source→object→link→run 与 Phase 3 delegation fact 分层覆盖；未运行约一小时的 frontend 全量测试 |
| `cargo clippy --workspace --lib --locked --offline -- -D warnings` | 通过 | 与四组定向测试并行执行的 workspace lib 静态门禁 |
| 独立高风险复核（首个 abstract Borrow delegation route） | 通过 | 首轮发现 bodyful default 绕过 route 的 P1、generic delegate 门禁 P2 与字段/隐藏操作覆盖 P3；改为 route-first、default fail-loud、generic/chain 独立门禁及非零 field 后复核至无 P1/P2/P3 |
| `cargo test -p lang-codegen ssa::unit_plan_tests --locked --offline` | 14/14 通过 | exact forwarder target、replacement default/local override、chain、generic delegate 与 generic interface owner fail-loud |
| `cargo test -p lang-codegen ssa::unit_lower_receiver_tests --locked --offline` | 23/23 通过 | bodyful requirement 仍按 outer loan→heap field loan→DirectCall→逆序 end；无 field value read/copy/retain/proxy allocation |
| `cargo test -p lang-codegen native::unit_tests --locked --offline` | 9/9 通过 | replacement default=2 与 delegate local override=7 同一 source→object→link→run 得 9，排除 outer default=1 误调用 |
| `cargo test -p lang-codegen --lib --locked --offline`（独立复审） | 276 通过、1 ignored | 完整 codegen library 回归；ignored 为既有 debugserver 权限用例，未运行约一小时的 frontend 全量测试 |
| `cargo clippy --workspace --lib --locked --offline -- -D warnings` | 通过 | workspace library Layer 2 静态门禁，零 warning |
| 独立高风险复核（bodyful/default Borrow delegation） | 通过 | 无 P1/P2/P3；确认 exact typed+ownership route、delegate concrete `StaticSelf`、field-loan ABI 及所有未开放 generic/chain 边界 |
| `cargo test -p lang-codegen interface_inout_default_preserves_exclusive_receiver_abi --locked --offline` | 通过 | interface `StaticSelf` 专化为 concrete `Counter`，callee receiver 与 caller operand 均为同类型 exclusive loan，verified SSA/LLVM 通过 |
| `cargo test -p lang-codegen inout_interface_default_links_and_runs --locked --offline` | 通过 | source→object→link→run 中 default 返回 7、concrete payload 保持 5，合计 12 后输出 `inout-default` |
| `cargo test -p lang-codegen ssa::unit_plan_tests --locked --offline` | 15/15 通过 | generic delegation 的 requirement owner prefix 精确替换为 implementation owner template，callable `Long` 后缀原序保留；runtime generic outer/delegate 与 chain 均 fail loud |
| `cargo test -p lang-codegen ssa::unit_lower_receiver_tests --locked --offline` | 24/24 通过 | receiver/field-loan/DirectCall ABI 回归；generic slot 重映射不改变既有 `SharedHeapFieldLoan` 路径 |
| `cargo test -p lang-codegen native::unit_tests::generic_interface_owner_and_callable_delegation_links_and_runs --locked --offline` | 1/1 通过 | `Mapper<String>.map<Long>` 的 default 与 concrete override 分别输出独立 body 标记并返回 7/9，真实 link/run 最后输出 `generic-delegate` |
| `cargo test -p lang-codegen native::unit_tests --locked --offline` | 11/11 通过 | receiver/default/delegation 的完整 native 小模块回归；仍未运行 frontend 全量测试 |
| `cargo clippy --workspace --lib --locked --offline -- -D warnings` | 通过 | 与三组聚焦测试并行执行；未运行约一小时的 frontend 全量测试 |
| 独立高风险复核（generic delegation slots） | 通过 | 首轮发现 generic outer 负例与 native 路由可观察性两项 P3；补 `Host<String>` fail-loud 和 default/override 独立 stdout 标记后复核至无 P1/P2/P3 |

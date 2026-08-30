# SPEC-0199：多文件 compilation-unit native lowering

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-199` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 v0.32 §32 |
| 批准依据 | 2026-08-29 当前持续 Goal 授权在 SPEC-0198 完成后按解锁价值推进后继 Spec |
| 前置 Spec | SPEC-0034、0035、0036、0038、0039、0184、0192、0195、0196、0198 `done` |
| 前置 ADR | ADR-0010、ADR-0020 `accepted` |
| 阻塞项 | 无 |
| 影响范围 | `lang-codegen` unit lowering/SSA/LLVM/object，native integration tests；Architecture/Roadmap |
| 语言语义变更 | 否 |

## 2. Goal

完成后，多个 Koven source unit 的 owner-aware typed program 可在全 unit 上完成 reachability、
单态化和 verified SSA/LLVM lowering，生成并链接一个独立本机 object/executable。

## 3. 范围与需求

- codegen 只消费 SPEC-0198 的 validated unit product；内部 API 显式接收一个已解析
  `DeclarationId` entry，所有跨文件 target 使用同一声明 identity。
- 全 unit 去重 reachable callable、generic instance、type layout 与 drop glue，结果不依赖文件顺序。
- 从 entry 做可达性规划，只 lower 可达 body；实例 key 使用 `DeclarationId + canonical type args`，
  内部函数名包含 package/declaration identity，避免同名 package 碰撞。预算、递归检测和去重均为
  unit-wide。
- 每个 body lowerer 只绑定一个 source view，最终合并为一个 SSA module、一个 LLVM module 和一个
  object；复用现有 native entry wrapper、target preflight 与系统 linker contract。
- native 正例覆盖跨文件 call、constructor/generic、String/Rc/aggregate owner 和正常/提前退出 drop。
- verifier 或 lowering 失败不得写出部分 object/executable。
- 首版只覆盖截至本 Spec 已经 native-closed 的表达式/所有权表面；一般 source `Inout`/receiver
  lowering 不由多文件能力顺带实现。

## 4. 非目标

- 不实现每文件 object、增量缓存、动态链接、公共 package ABI、manifest、source discovery、
  全局 conventional-main 选择或公开多文件 CLI；SPEC-0052 只提供 source snapshot，项目
  entry/CLI 属于 SPEC-0054。
- 不消费 SPEC-0210 的 const-enabled typed unit，也不 lower 跨文件 const use；该能力必须由
  显式消费 0199、0210、0208/0209 及 unit const ownership 产物的后继 Spec 增量发布，不能
  隐式重开本 Spec。

## 5. 验收标准

- [ ] SSA/LLVM 测试证明跨文件 identity、实例与 drop glue 只生成一次且顺序确定。
- [ ] 真实 native build/run 覆盖至少两个 package、exact/alias import 与 MoveOnly 跨文件传递。
- [ ] 非法 unit 在 object 写盘前失败；新 unit object API 通过 sibling temporary + commit 保证目标
  原子更新；输入置换后的规范化 SSA/LLVM 与诊断一致（不要求 object 字节完全相同）。
- [ ] 多 source DWARF 行表保留各自源码定位；codegen/workspace 基线与 Architecture 同步。

剩余切片采用简化三层验收：开发期与提交前只跑受影响的窄测试过滤器；再跑一次 workspace Clippy；
涉及逻辑或共享代码时由 fresh-context 独立评审。受影响 crate 全量仅在公共边界、高风险改动或窄测
无法覆盖时显式追加，不再作为每个切片的固定门禁；尤其不重复约一小时的 `lang-frontend` 全量测试。
共享 Cargo target 的命令顺序执行，文档同步、diff 审计与独立静态复审可并行，不以并发 Cargo 进程
争用 target lock。历史窄测和尚未接线的 LLVM/native matrix 不在每个 SSA 切片重复执行。

## 6. 技术方案与边界

在 frontend unit product 与现有 function-level lowering 之间增加确定的 reachability/instance plan，
例如 `emit_native_unit_object(validated_unit, entry: DeclarationId, output)`。body locator 把 declaration
映射到 source-local item/body；LLVM 仍只接收单个 verified SSA module。真实运行验收可由测试 harness
复用现有系统 linker contract，但本 Spec 不扩张 `kovenc build/run` 的公开单文件输入语义。

## 7. 实施计划

1. [x] 建立 unit reachability/instance plan → 验证：顺序置换、递归、泛型传播与去重测试。
2. [x] 接首条 scalar expression-body verified SSA → 验证：跨 package generic/alias call、
   package identity、死 body 排除、输入置换与 unsupported 原子失败。
3. [x] 扩展 straight-line block/local/explicit return 与 String owner/drop SSA → 验证：逐实参
   Copy/Move/Temporary delivery、FunctionEntry/statement/call/control-transfer drop 与跨文件 owner transfer。
4. [x] 接 Unit-valued `if` owner-aware CFG → 验证：block/edge 参数、正常与提前 `return`、显式/
   隐式 `else` BranchExit、MoveOnly Value delivery 状态转移、分支局部清理与输入置换。
5. [x] 接 Copyable value-valued `if` → 验证：双正常结果 block parameter、`Nothing` 分支、输入
   置换与 MoveOnly 结果 fail-loud 边界。
6. [x] 接 exhaustive Boolean-subject `when` → 验证：true/false entry 源码逆序、原 entry index
   BranchExit、Copyable value/Unit owner 路径、输入置换与 subjectless fail-loud 边界。
7. [x] 接无 jump Unit `while` → 验证：header/body/backedge/false-exit binding 参数、MoveOnly owner
   正常回边、`LoopExit` drop、提前 return、输入置换与 `break` fail-loud 边界。
8. [x] 接 `break` / `continue` 与 bare `loop` → 验证：最近 nested loop target、ControlTransfer
   body-local drop、共同退出、全路径 owner consume、无 break divergence 与 path-specific outer owner fail-loud。
9. [x] 扩展 Boolean `when` entry chain → 验证：subjectful `else`、subjectless 顺序短路、动态
   subject comparison、同 entry 多 condition 只 lower body 一次、原 entry index 与一致 owner 状态合流。
10. [x] 预规划 reachable body-only scalar storage type → 验证：signature-first identity、literal-only
   Boolean `when`、String local owner/drop、泛型 instance substitution、死 body 隔离与输入置换。
11. [x] 接整数 scalar prefix/checked arithmetic/comparison → 验证：5 类算术的 failure result
   `true -> Abort`、6 类比较、一元运算、最小负字面量、跨文件输入置换与 String 边界隔离。
12. [x] 接 Boolean `&&` / `||` short-circuit CFG → 验证：RHS 单次求值、true/false edge、
   Copyable result 与 MoveOnly owner carry、输入置换，以及 RHS-only owner move 的 fail-loud 门禁。
13. [x] 接 String concat/equality binary → 验证：source-order view identity、right-to-left operand
   drop、concat result transfer/return、`!=` 的 equal/drop/not 时序、跨文件调用与输入置换。
14. [x] 接 root name assignment → 验证：String RHS-before-drop/replacement owner transfer、五类整数
   compound checked failure CFG、逐次 binding identity、deferred type fail-loud 与输入置换。
15. [x] 接 concrete non-null `Rc<T>` core → 验证：unit-global shared-owner identity、construction
   Copy/Move/temporary ordered delivery、跨文件 Value transfer、`share()` retain、Copyable `.value`
   payload read、精确 drop、输入置换，以及 MoveOnly payload read 的 lowering fail-loud 边界；
   source-level composite generic `Rc<T>` 当前在 parser/frontend 更早拒绝，尚不能形成 validated unit
   artifact，`resolve_concrete_type` 保留第二道 `UnsupportedNode` 门禁而不伪造测试产物。
16. [x] 接 concrete non-generic `value class` / `class` / intrinsic `Box` aggregate core → 验证：
   unit-global inline/heap layout identity、具名参数源码顺序求值与声明顺序组装、Copy/Move/temporary
   ordered delivery、Copyable field projection、有限 owner 递归、跨文件 owner transfer/drop、输入置换，
   以及 generic nominal、MoveOnly field read/temporary receiver 的 lowering fail-loud 边界。
17. [x] 接 concrete non-generic enum core → 验证：unit-global tagged/payload layout identity、case
   construction ordered delivery、Copyable enum type-test `when` discriminant/smart-cast payload
   projection、MoveOnly root drop、有限 owner 递归、输入置换，以及 generic enum fail-loud 边界。
18. [x] 接 concrete 顺序容器 construction core → 验证：unit-global `Array`/`List`/`MutableList`
   element identity、列表式与空构造、逐元素 Copy/Move/temporary delivery、Unit ZST materialization、
   nested container、跨文件 owner transfer/drop、输入置换，以及 runtime-length initializer 与不支持
   element type 的 fail-loud 边界。
19. [x] 接 shared-Borrow callable core → 验证：Borrow 参数的 function-scoped Shared Loan ABI、
   source-qualified root/temporary loan、已有参数 loan 无嵌套转发、Copyable loan read、混合具名实参的
   源码求值/参数槽位/逆序结束、CallReturn drop 与输入置换；`Inout`、非 root projection 和
   `Borrow(Unit)` 在发布 program 前 fail loud。
20. [x] 接 container element core → 验证：Copyable read、owned/Borrow/temporary receiver shared
   element loan、`Array`/`MutableList` simple 与整数 compound replacement、MoveOnly RHS transfer、
   `AfterReplacement/ReplacedElement` 精确旧元素 drop、checked CFG owner carry 与输入置换；field-backed
   receiver、`size`、temporary compound、一般 `Inout` 与 MoveOnly element read 保持 fail-loud 边界。
21. [x] 接 concrete owned move closure core → 验证：unit-global concrete environment/thunk identity、
   Copy/Move capture、具名局部 transfer/repeated invoke、shared capture view、`Captured`/Named 精确反序
   drop、输入置换与 CFG provenance 恢复；borrowed、temporary/direct、nested、参数化、非 `Unit` 返回、
   body 内 MoveOnly temporary 保持 fail-loud 边界。
22. [x] 接无 capture lambda function-pointer core → 验证：普通与 `move` lambda 共用 canonical
   零参数 `Unit` signature、独立 thunk/`FunctionAddress`、无 environment/`ClosureConstruct`、具名
   transfer/repeated invoke/drop 与输入置换；captured closure 表示和参数化/非 `Unit` 边界不回归。
23. [ ] 扩展 MoveOnly value result、其余 `when`/for 与 closure owner/drop SSA →
   验证：正常和提前退出、结果 owner 转移、复合 drop glue unit-wide 去重。
   - [x] 接 Copyable callable ABI：function pointer/concrete closure 支持 Copyable storage 的
     Borrow/Value 参数与 Unit/Copyable storage 返回，thunk entry 保持 environment-first，调用复用
     source-qualified loan/Value delivery；`Inout`、`Borrow(Unit)`、MoveOnly 参数/返回、跳出实参的
     控制转移及 function-value 实参内部 loop jump 继续 fail loud。
   - [x] 接唯一直接 tail temporary 的 MoveOnly callable result：function pointer/concrete closure thunk
     直接把新 owner 交给 `Return`，caller 取得独立 result owner 并复用既有 binding/return/drop 流；
     concat、分支 result、body local owner、显式 return 与 MoveOnly 参数继续 fail loud，等待 lambda-body
     exit-qualified owner/drop facts。
   - [x] 消费 SPEC-0215 lambda body facts：function pointer/concrete closure thunk 支持 String concat
     composite tail、body-local owner 精确 drop 与显式 return transfer；退出前同时验证无 temporary 或
     MoveOnly named binding 残留。MoveOnly `if` / `when` result 继续原子 fail loud，MoveOnly Value
     参数仍等待 lambda-entry drop point。
   - [x] 消费 SPEC-0216 control result facts：MoveOnly `if` / `when` 各正常 branch tail owner 转交 merge
     result，operand/alternative owner 只按 frontend 精确 point 析构；lambda/named callable、nested
     control、正常/提前退出与输入置换共同锁定。captured closure 的 environment/user loan 与 checked
     arithmetic continuation 均作为显式 block/edge 参数携带，不形成 hidden linear live-in。
24. [ ] 接 LLVM 与多 source DWARF → 验证：规范化 LLVM 顺序置换和源码定位窄测试。
   - [x] 接首条真实 compilation-unit frontend→SSA→LLVM/DWARF 链：跨 package alias call、captured
     closure、environment-first + Borrow pointer ABI、MoveOnly String result/drop、两源 DIFile/
     DISubprogram/DILocation 与输入反序后的完整 LLVM 文本确定性；object/native 仍由后续切片承接。
25. [x] 接单 object 原子写入并完成 native 正反矩阵、Architecture 与 workspace 基线。
   - [x] 接首个 public unit object API：validated identity chain 先于 entry shape 校验，显式
     `DeclarationId` 仅接受非泛型零参数 `Unit` callable；LLVM 只写同目录 `create_new` sibling
     temporary，成功后单次 rename 提交。跨 package alias/captured closure/dynamic String 正例完成
     Mach-O link/run；InvalidEntry、UnsupportedSource、MismatchedAnalysis 与 commit failure 均保留旧目标
     并清理 temporary。该首切片尚未覆盖 aggregate/Rc/constructor 与提前退出组合。
   - [x] 在同一真实可执行 fixture 补齐 native owner/drop matrix：跨文件 named class/value class/Box
     constructor、动态 String 字段、字段投影、`Rc<Int>` construction/share/payload read，以及 consumer
     同时持有 class/Box/Rc 后的正常 Value delivery 与提前 return drop 均完成 Mach-O link/run；不新增
     重复 production API 或分散 link/run 流程。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | unit reachability/instance plan | `feat(codegen): plan multifile instances (SPEC-0199)` |
| 2 | scalar expression-body verified SSA | `feat(codegen): lower multifile scalar calls (SPEC-0199)` |
| 3 | block/return 与跨文件 String owner/drop SSA | `feat(codegen): lower multifile string owners (SPEC-0199)` |
| 4 | Unit-valued `if` owner-aware CFG | `feat(codegen): lower multifile conditionals (SPEC-0199)` |
| 5 | Copyable value-valued `if` | `feat(codegen): lower multifile value conditionals (SPEC-0199)` |
| 6 | exhaustive Boolean-subject `when` | `feat(codegen): lower multifile boolean when (SPEC-0199)` |
| 7 | 无 jump Unit `while` | `feat(codegen): lower multifile while loops (SPEC-0199)` |
| 8 | loop jump 与 bare loop | `feat(codegen): lower multifile loop jumps (SPEC-0199)` |
| 9 | Boolean `when` entry chain | `feat(codegen): lower multifile when chains (SPEC-0199)` |
| 10 | reachable body-only scalar type plan | `feat(codegen): plan multifile body scalar types (SPEC-0199)` |
| 11 | integer scalar operators 与 checked failure CFG | `feat(codegen): lower multifile scalar operators (SPEC-0199)` |
| 12 | Boolean short-circuit owner-aware CFG | `feat(codegen): lower multifile short circuits (SPEC-0199)` |
| 13 | String binary views 与 operand drops | `feat(codegen): lower multifile string binary (SPEC-0199)` |
| 14 | root name owner/scalar assignment | `feat(codegen): lower multifile assignments (SPEC-0199)` |
| 15 | concrete non-null Rc owner-aware verified SSA | `feat(codegen): lower multifile rc owners (SPEC-0199)` |
| 16 | concrete non-generic nominal/Box aggregate core | `feat(codegen): lower multifile nominal aggregates (SPEC-0199)` |
| 17 | concrete non-generic enum core | `feat(codegen): lower multifile enums (SPEC-0199)` |
| 18 | concrete 顺序容器 construction core | `feat(codegen): lower multifile containers (SPEC-0199)` |
| 19 | shared-Borrow callable core | `feat(codegen): lower multifile borrows (SPEC-0199)` |
| 20 | container element core | `feat(codegen): lower multifile container elements (SPEC-0199)` |
| 21 | concrete owned move closure core | `feat(codegen): lower multifile closures (SPEC-0199)` |
| 22 | 无 capture lambda function-pointer core | `feat(codegen): lower multifile function pointers (SPEC-0199)` |
| 23 | 其余现行表面的 owner-aware verified SSA | `feat(codegen): lower multifile units (SPEC-0199)` |
| 24 | LLVM 与 multi-source DWARF | `feat(codegen): lower multifile LLVM (SPEC-0199)` |
| 25 | single-object/native integration 闭环 | `feat(codegen): emit multifile objects (SPEC-0199)` |

## 9. 未决问题

- 多 object/增量 ABI 明确留给后续 ADR。
- SPEC-0216 的 MoveOnly control-tail Consume/transfer 与 alternative drop facts 已由第二十三步第四
  切片消费；剩余 MoveOnly Value lambda 参数、`for` 与未列入现行 storage/ownership 表面的节点仍按
  各自后继切片推进，不从本次 control result 闭环推导额外语义。
- guide 的 `when_condition = expression` 可推出括号表达式 condition，但现行 parser 对
  `(true) ->` 报 L0058/L0065；0199 只 lower 可达的 bare Boolean literal，不在 Phase 4 内修改 Phase 1
  语法。该 parser/guide 漂移由独立 frontend follow-up 锁定并修正。
- 现行 loop drop planner 的 `LoopExit(statement)` 基于 loop-entry state 发布，不区分 while false
  与各 break exit。若所有实际出口都已消费 owner，unit lowerer 在一致状态合流后跳过 stale fact；若
  while false 仍拥有而某 break 已消费，则 exit binding 状态不同并显式 `UnsupportedNode`。后者需要
  frontend 发布 exit-qualified owner/drop facts 后才能形成可验证的可选路径状态，0199 不猜测 drop。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap 审计 | 通过 | 补齐 project build 和跨文件 LSP 之前缺失的 native 层 |
| 2026-08-29 实施前审计 | 通过 | 全部前置 Spec `done`、ADR-0010/0020 `accepted`；0199 比并行的 0187 多解除 SPEC-0054 project build 门禁，先实施 unit reachability/instance plan |
| `cargo test -p lang-codegen unit_plan_tests --lib` | 3 passed | 跨文件可达性、死函数排除、递归去重、泛型传播/去重、输入置换、invalid entry 与 foreign ownership gate |
| `cargo test -p lang-codegen --lib` | 159 passed, 1 ignored | codegen 全量 lib 基线；LLDB sandbox 用例按既有约定 ignored |
| `cargo test --workspace --lib --bins` | 257 passed, 1 ignored | CLI 36、codegen 159、frontend 50、LSP 11、std 1；无失败 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | workspace 静态检查无 warning |
| `cargo test -p lang-codegen unit_lower_tests --lib` | 3 passed | 跨 package generic alias direct call、package-qualified 同名 identity/死 body、unsupported reachable block 原子失败 |
| `cargo test -p lang-codegen --lib` | 162 passed, 1 ignored | scalar unit lowering 后的 codegen 全量 lib 基线；LLDB sandbox 用例按既有约定 ignored |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第二切片复用“受影响窄测试 → 受影响 crate 全量 → workspace 静态门禁”的简化验收层级 |
| `cargo test -p lang-codegen unit_lower_tests --lib` | 5 passed | 新增跨文件 String Move/drop、显式 return transfer 与 loop fail-loud；保留 identity/determinism 基线 |
| `cargo test -p lang-codegen --lib` | 164 passed, 1 ignored | block/String owner 切片后的 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第三切片继续复用简化分层验收，不重复无关 workspace runtime matrix |
| `cargo test -p lang-codegen unit_lower_tests --lib` | 8 passed | Unit `if` CFG、输入置换、提前 return、Move delivery 状态转移、显式/隐式 else 正常合流、BranchExit 与分支局部清理 |
| `cargo test -p lang-codegen --lib` | 167 passed, 1 ignored | Unit `if` CFG 切片后的 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第四切片沿用窄测试 → 受影响 crate 全量 → workspace 静态门禁，不重复 workspace runtime matrix |
| `cargo test -p lang-codegen unit_lower_tests --lib` | 12 passed | Copyable/generic-concrete value `if` 双出口结果合流与槽位顺序、`Nothing` 单出口、输入置换及 MoveOnly 拒绝边界 |
| `cargo test -p lang-codegen --lib` | 171 passed, 1 ignored | Copyable value `if` 切片后的 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第五切片继续采用窄测试 → 受影响 crate 全量 → workspace 静态门禁 |
| `cargo test -p lang-codegen unit_lower_tests --lib` | 15 passed | exhaustive Boolean `when` 逆序 entry 的 true/false edge、原 entry index BranchExit drop、Copyable value/Unit owner、输入置换与 subjectless 拒绝边界 |
| `cargo test -p lang-codegen --lib` | 174 passed, 1 ignored | Boolean `when` 切片后的 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第六切片继续采用窄测试 → 受影响 crate 全量 → workspace 静态门禁 |
| `cargo test -p lang-codegen --lib unit_lower_loop_tests` | 2 passed | 无 jump Unit `while` 的 MoveOnly owner 正常回边、false `LoopExit`、提前 return、输入置换与精确 CFG edge |
| `cargo test -p lang-codegen --lib unit_lower_tests` | 15 passed | 共享 carried-binding CFG 提取后的 if/when 回归，以及 `break` 原子拒绝边界 |
| `cargo test -p lang-codegen --lib` | 176 passed, 1 ignored | 无 jump Unit `while` 切片后的 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第七切片继续采用窄测试 → 受影响 crate 全量 → workspace 静态门禁，不重复无关 runtime matrix |
| `cargo test -p lang-codegen --lib unit_lower_loop_tests` | 7 passed | break/continue ControlTransfer、bare loop、全路径 owner consume、nested 最近 continue/break target 与共同退出 |
| `cargo test -p lang-codegen --lib unit_lower_tests` | 15 passed | if/when/while 回归及 path-specific outer owner move 的 Unsupported 原子边界 |
| `cargo test -p lang-codegen --lib` | 181 passed, 1 ignored | loop jump 与 bare loop 切片后的 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第八切片沿用窄测试 → 受影响 crate 全量 → workspace 静态门禁，不重复无关 runtime matrix |
| `cargo test -p lang-codegen --lib unit_lower_when_tests` | 6 passed | helper-call 求值次数与 false-edge 短路、subjectful else、动态 candidate compare、同 entry 多 condition 单次 body lowering、一致 owner 合流与 implicit synthetic branch index |
| `cargo test -p lang-codegen --lib unit_lower_tests` | 15 passed | 既有 if/Boolean when/while 边界与 subjectless chain 回归 |
| `cargo test -p lang-codegen --lib unit_lower_loop_tests` | 7 passed | loop CFG 与 owner/drop 回归 |
| `cargo test -p lang-codegen --lib` | 187 passed, 1 ignored | Boolean when entry-chain 切片后的 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第九切片继续采用窄测试 → 受影响 crate 全量 → workspace 静态门禁 |
| `cargo test -p lang-codegen --lib type_plan` | 4 passed | 全局 signature-first identity、reachable literal-only Boolean、body-only String owner/drop、空 type map 白盒 generic substitution、dead body type 隔离与输入置换 |
| `cargo test -p lang-codegen --lib unit_lower_when_tests` | 6 passed | body-only Boolean type plan 接入后的 when short-circuit/owner CFG 回归 |
| `cargo test -p lang-codegen --lib` | 191 passed, 1 ignored | reachable body-only scalar type-plan 切片后的 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第十切片沿用 scoped 窄测 → 受影响 crate 全量 → workspace 静态门禁 |
| `cargo test -p lang-codegen --lib unit_lower_scalar_tests` | 4 passed | 5 类 checked 算术、6 类比较、一元运算、最小负字面量、String 显式边界；逐指令锁定 failure result 为 Conditional condition、`true -> Abort`、`false -> continuation` |
| `cargo test -p lang-codegen --lib` | 195 passed, 1 ignored | integer scalar operator 切片后的 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第十一切片继续使用窄测试 → 受影响 crate 全量 → workspace 静态门禁，不重复无关 runtime matrix |
| `cargo test -p lang-codegen --lib unit_lower_short_circuit_tests` | 2 passed | 跨文件 `&&`/`||` 的精确 short edge、RHS 单次 lowering、String owner carried slots、result-first merge、输入置换与 RHS-only owner move 原子拒绝 |
| `cargo test -p lang-codegen --lib` | 197 passed, 1 ignored | Boolean short-circuit 切片后的 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第十二切片继续复用窄测试 → 受影响 crate 全量 → workspace 静态门禁 |
| `cargo test -p lang-codegen --lib unit_lower_string_tests` | 2 passed | literal concat return、operand identity/逆序 drop、跨文件 suffix/identity、两类 equality、精确 `equal -> RHS drop -> not` 与输入置换 |
| `cargo test -p lang-codegen --lib unit_lower_scalar_tests` | 3 passed | String dispatch 接入后的整数 checked/comparison 回归 |
| `cargo test -p lang-codegen --lib` | 198 passed, 1 ignored | String binary 切片后的 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第十三切片继续复用窄测试 → 受影响 crate 全量 → workspace 静态门禁 |
| `cargo test -p lang-codegen --lib unit_lower_assignment_tests` | 3 passed | String replacement 的精确 RHS/drop/transfer、五类整数 compound binding chain/failure edge、非整数 compound 与 deferred mismatch 原子拒绝 |
| `cargo test -p lang-codegen --lib unit_lower` | 42 passed | 用单一 scoped filter 合并 assignment、type-plan、scalar/String 与既有 unit control-flow 回归，减少重复窄命令 |
| `cargo test -p lang-codegen --lib` | 201 passed, 1 ignored | root name assignment 切片后的 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第十四切片继续采用“切片窄测 → scoped umbrella → 受影响 crate 全量 → workspace 静态门禁”，不重复无关 native matrix |
| `cargo test -p lang-codegen --lib unit_lower_rc_tests` | 3 passed | 跨文件 `Rc<Int>` Copy/temporary delivery、retain/Copyable payload read/drop/输入置换，String payload Move 与 Rc owner 唯一转移，以及 MoveOnly payload read 原子拒绝 |
| `cargo test -p lang-codegen --lib unit_lower` | 45 passed | 单一 scoped umbrella 合并 Rc、assignment、type-plan、scalar/String 与 unit control-flow 回归，避免重复运行各历史窄测 |
| `cargo test -p lang-codegen --lib` | 204 passed, 1 ignored | concrete Rc unit-lowering 与 SharedOwner direct-call verifier 接入后的 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第十五切片继续采用“Rc 窄测 → scoped umbrella → codegen 全量 → workspace 静态门禁”，不重复历史 native matrix |
| `cargo test -p lang-codegen --lib unit_lower_aggregate_tests` | 3 passed | 跨文件 value/class/Box construction、反序重新分析与具名实参求值/字段顺序、Copyable projection/drop、`Value -> class -> Value`、`Rc<Class>`、nested Rc 有限 owner 图，以及 generic nominal、MoveOnly field/temporary receiver 原子拒绝 |
| `cargo test -p lang-codegen --lib unit_lower` | 48 passed | scoped umbrella 合并 aggregate、Rc、assignment、type-plan、scalar/String 与 unit control-flow 回归，不逐条重复历史窄测 |
| `cargo test -p lang-codegen --lib` | 207 passed, 1 ignored | concrete non-generic nominal/Box unit-lowering、依赖驱动 owner definition 与既有 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第十六切片沿用“aggregate 窄测 → scoped umbrella → codegen 全量 → workspace 静态门禁”，未重复尚未接线的 multifile native matrix |
| `cargo test -p lang-codegen --lib unit_lower_enum_tests` | 4 passed | 跨文件 enum 构造/Copyable 穷尽 type-test `when`/payload projection、反序重新分析、MoveOnly root drop、有限 enum-class 递归，以及 generic enum/MoveOnly enum subject 拒绝边界 |
| `cargo test -p lang-codegen --lib unit_lower` | 52 passed | scoped umbrella 合并 enum、aggregate、Rc、assignment、type-plan、scalar/String 与 unit control-flow 回归，不逐条重复历史窄测 |
| `cargo test -p lang-codegen --lib` | 211 passed, 1 ignored | concrete non-generic enum TaggedUnion、owner-aware CFG 与既有 codegen 全量 lib 基线 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第十七切片继续采用“enum 窄测 → scoped umbrella → codegen 全量 → workspace 静态门禁”，未重复尚未接线的 multifile native matrix |
| `cargo test -p lang-codegen --lib unit_lower_container_tests` | 2 passed | 跨文件列表式/空构造、Copy/Move/temporary delivery、nested container、Unit `call -> constant -> construct`、反序重新分析，以及 runtime-length/unsupported element 拒绝边界 |
| `cargo test -p lang-codegen --lib unit_lower` | 54 passed | scoped umbrella 合并 container、enum、aggregate、Rc、assignment、type-plan、scalar/String 与 unit control-flow 回归，不逐条重复历史窄测 |
| `cargo test -p lang-codegen --lib` | 213 passed, 1 ignored | concrete 顺序容器 verified SSA 与既有 codegen 全量 lib 基线；LLDB sandbox 用例按既有约定 ignored |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第十八切片继续采用“container 窄测 → scoped umbrella → codegen 全量 → workspace 静态门禁”，不重复尚未接线的 multifile native matrix |
| `cargo test -p lang-codegen --lib unit_lower_borrow_tests` | 2 passed | 跨文件 root/temporary shared loan、Borrow 参数转发、Copyable loan read、混合具名实参槽位与逆序结束、CallReturn drop、输入置换，以及 Inout/非 root/Unit ABI 原子拒绝 |
| `cargo test -p lang-codegen --lib unit_lower` | 56 passed | scoped umbrella 合并 Borrow、container、enum、aggregate、Rc、assignment、type-plan、scalar/String 与 unit control-flow 回归，避免重复历史窄命令 |
| `cargo test -p lang-codegen --lib` | 215 passed, 1 ignored | shared-Borrow callable verified SSA 与既有 codegen 全量 lib 基线；LLDB sandbox 用例按既有约定 ignored |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第十九切片沿用“Borrow 窄测 → scoped umbrella → codegen 全量 → workspace 静态门禁”的简化验收，不重复尚未接线的 multifile native matrix |
| 独立 fresh-context 评审 | 通过 | 发现 `Borrow(Unit)` entry 可绕过 call-site 门禁；修复为 concrete substitution 后、SSA 类型/函数创建前拒绝，并复审确认 generic `T = Unit` 同样闭环 |
| `cargo test -p lang-codegen --lib unit_lower_container_element_tests` | 2 passed | Copyable element read、owned/Borrow/temporary shared loan、simple/compound/MoveOnly replacement、精确 checked owner carry、输入置换与显式 fail-loud 边界 |
| `cargo test -p lang-codegen --lib unit_lower` | 58 passed | scoped umbrella 合并 container element、Borrow、construction、aggregate/Rc/enum、assignment 与 control-flow 回归 |
| `cargo test -p lang-codegen --lib` | 217 passed, 1 ignored | container element verified SSA 与既有 codegen 全量 lib 基线；LLDB sandbox 用例按既有约定 ignored |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第二十切片落实固定四层验收，不重复历史窄命令或尚未接线的 multifile native matrix |
| 独立 fresh-context 评审与复审 | 通过 | 发现 grouped temporary container loan 的 owner identity/drop 消费缺口；修复为按 frontend temporary origin 校验、按 source-qualified loan target 唯一重绑定后，复审确认无新 P1/P2 |
| `cargo test -p lang-codegen --lib unit_lower_closure_tests` | 3 passed | 跨文件 Move closure、输入置换、Copy/Move capture、transfer/repeated invoke/thunk、if/while provenance 与 LoopExit drop，以及 borrowed/temporary/nested/参数化/非 Unit 等原子边界 |
| `cargo test -p lang-codegen --lib unit_lower` | 61 passed | scoped umbrella 合并 closure、container element、Borrow、construction、aggregate/Rc/enum、assignment 与 control-flow 回归 |
| `cargo test -p lang-codegen --lib` | 220 passed, 1 ignored | concrete owned move closure verified SSA 与既有 codegen 全量 lib 基线；LLDB sandbox 用例按既有约定 ignored |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第二十一切片继续采用固定四层验收，不重复历史窄命令或尚未接线的 LLVM/native matrix |
| 独立 fresh-context 评审与复审 | 通过 | 发现 temporary capture-drop、CFG provenance 与 coarse LoopExit stale facts 三个 P2，以及 LoopExit provenance 清理 P3；收窄为具名局部 closure，补齐 branch/loop 状态、live-fact 精确 drop 与 all-breaks-consumed 回归后复审确认无 P1/P2/可观察 P3 |
| `cargo test -p lang-codegen --lib unit_lower_closure_tests` | 4 passed | 普通/`move` 无 capture lambda 的 canonical function pointer、独立 thunk、transfer/repeated invoke/drop、输入置换，以及 captured closure 和既有原子边界回归 |
| `cargo test -p lang-codegen --lib unit_lower` | 62 passed | scoped umbrella 合并 function pointer、closure、container element、Borrow、construction、aggregate/Rc/enum、assignment 与 control-flow 回归 |
| `cargo test -p lang-codegen --lib` | 221 passed, 1 ignored | 无 capture function-pointer verified SSA 与既有 codegen 全量 lib 基线；LLDB sandbox 用例按既有约定 ignored |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第二十二切片继续采用固定四层验收，不重复历史窄命令或尚未接线的 LLVM/native matrix |
| 独立 fresh-context 评审 | 通过 | 复核 Guide/SPEC-0038 两种 callable 表示、普通与 move descriptor、thunk signature、owner transfer/drop 和 captured 回归，未发现 P1/P2/P3 |
| `cargo test -p lang-codegen --lib unit_lower_closure_tests --locked --offline` | 5 passed | Copyable Borrow/Value callable 参数与返回、function pointer/concrete closure thunk、重复调用、输入置换、direct 内部 loop jump 正例，以及 Inout/MoveOnly/跨实参控制转移原子边界 |
| `cargo test -p lang-codegen --lib unit_lower --locked --offline` | 63 passed | scoped umbrella 合并 callable ABI、closure、Borrow/container/aggregate/Rc/enum 与 control-flow 回归 |
| `cargo test -p lang-codegen --lib --locked --offline` | 222 passed, 1 ignored | 参数化 callable verified SSA 与 codegen 全量 lib 基线；LLDB sandbox 用例按既有约定 ignored |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | 通过 | 第二十三步第一切片继续采用固定四层简化验收，不重复历史窄命令或尚未接线的 LLVM/native matrix |
| 独立 fresh-context 评审与三轮复审 | 通过 | 首轮发现后续实参控制转移会遗留 pending loan/callee owner 的 P1；修复为 call-level 原子预检。后续两轮把 direct 内部 loop 与外层 jump、loop body 与 condition/source 边界精确分开，并保留 function-value hidden-linear-live-in 门禁；最终无 P1/P2/P3 |
| `cargo test -p lang-codegen --lib unit_lower_closure_tests --locked --offline` | 6 passed | 直接 MoveOnly callable result、function pointer/concrete closure、跨文件调用、重复调用、result drop、输入置换，以及复杂 MoveOnly tail 原子拒绝 |
| `cargo test -p lang-codegen --lib unit_lower --locked --offline` | 64 passed | scoped umbrella 合并 MoveOnly callable result 与 callable/closure/owner-aware control-flow 回归 |
| `cargo test -p lang-codegen --lib --locked --offline` | 223 passed, 1 ignored | MoveOnly callable result verified SSA 与 codegen 全量 lib 基线；LLDB sandbox 用例按既有约定 ignored |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | 通过 | 第二十三步第二切片沿用固定四层简化验收，不重复尚未接线的 LLVM/native matrix |
| `cargo test -p lang-codegen --lib unit_lower_closure_tests --locked --offline` | 7 passed | SPEC-0215 facts 驱动 composite result、body-local drop、显式 return、返回 owner 不 drop、输入置换与 MoveOnly control result fail-loud |
| `cargo test -p lang-codegen --lib native::unit_tests --locked --offline` | 2 passed | captured closure + body-local String owner 的 object/link/run 正例，以及 MoveOnly `if` result 的 UnsupportedSource 原子负例 |
| `cargo test -p lang-codegen --locked --offline` | 227 passed, 1 ignored | 第二十三步第三切片的 affected-crate 全量基线；LLDB sandbox 用例按既有约定 ignored |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | 通过 | 第二十三步第三切片沿用固定简化验收，覆盖 workspace 全 target/feature |
| `cargo test -q -p lang-codegen --lib unit_lower_control_result_tests` | 1 passed | MoveOnly named `if` alternatives、expression-body `when` composite、nested control、`Nothing` 退出、function pointer/captured closure result、显式 loan edge 与输入置换 |
| `cargo test -q -p lang-codegen --lib unit_lower_tests` | 14 passed | named block/expression result、owner transfer 与既有跨文件 lowering 回归 |
| `cargo test -q -p lang-codegen --lib unit_lower_when_tests` | 6 passed | Boolean entry chain、result merge 与 when owner/drop 回归 |
| `cargo test -q -p lang-codegen --lib unit_lower_closure_tests` | 7 passed | callable ABI、closure body/result/drop 与现行原子边界回归 |
| `cargo test -q -p lang-codegen --lib unit_lower_scalar_tests` | 3 passed | checked arithmetic success/failure CFG 携带 live loan，整数 scalar 回归 |
| `cargo test -q -p lang-codegen --lib unit_lower_short_circuit_tests` | 2 passed | `&&` / `||` true/false edge 的 value/loan 显式携带回归 |
| `cargo test -q -p lang-codegen --lib unit_lower_loop_tests` | 7 passed | `Nothing` 全 diverge control、break/continue、nested loop target 与既有 owner/drop 回归 |
| `cargo test -q -p lang-codegen --lib native::unit_tests` | 2 passed | named/captured MoveOnly control result、branch 内 checked arithmetic 的 object/link/run，以及 MoveOnly Value lambda 参数 UnsupportedSource 负例 |
| `cargo test -q -p lang-codegen --lib unit_lower` | 65 passed | fresh-context 复审补跑 scoped umbrella，覆盖 owner-aware unit lowering 回归，不扩张到 crate/frontend 全量 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 | 第二十三步第四切片采用窄测试过滤器 + workspace 静态门禁，不运行 `lang-frontend` 全量测试 |
| 独立 fresh-context 评审与复审 | 通过 | 初审发现 control block value/loan 参数计数、checked continuation hidden loan、`Nothing` loop gate 三处回归；分别修复并补齐 control/scalar/loop/native 门禁后，最终无 P1/P2/P3 |
| 独立 fresh-context 评审 | 无 P1/P2/P3 | 复核 declare 门禁、thunk/显式 return owner transfer、capture/function pointer、退出不变量、精确 drop 及 native 原子边界 |
| 独立 fresh-context 评审 | 通过 | 复核唯一 direct tail 门禁、thunk Return/caller result owner 转移、function pointer/concrete closure、跨文件确定性及 fail-loud 边界，未发现 P1/P2/P3 |
| `cargo test -p lang-codegen --lib unit_llvm_tests --locked --offline` | 1 passed | 真实 compilation-unit→verified LLVM、跨 package alias/callable/Borrow/String drop、multi-source DWARF 与输入反序全文确定性 |
| `cargo test -p lang-codegen --lib --locked --offline` | 224 passed, 1 ignored | unit LLVM/DWARF 集成与 codegen 全量 lib 基线；LLDB sandbox 用例按既有约定 ignored |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | 通过 | 第二十四步第一切片采用“单一窄测 → crate 全量 → workspace Clippy”的简化验收，不重复历史 SSA 窄测或尚未接线的 native matrix |
| 独立 fresh-context 评审与复审 | 通过 | 首轮发现 source metadata 关联与 Borrow/drop 时序断言不足两个 P2；改为解析 metadata/function/call identity，锁定两源 scope/location、environment-first ABI 与 inspect 后唯一 String drop，复审无 P1/P2/P3 |
| `cargo test -p lang-codegen --lib native::unit_tests --locked --offline` | 2 passed | public unit object 原子替换、跨 package alias/closure/dynamic String link/run，以及 InvalidEntry/UnsupportedSource/MismatchedAnalysis/commit failure 的旧目标保留与 temporary 清理 |
| `cargo test -p lang-codegen --lib --locked --offline` | 226 passed, 1 ignored | unit object/native 集成与 codegen 全量 lib 基线；LLDB sandbox 用例按既有约定 ignored |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | 通过 | 第二十五步第一切片沿用“单一窄测 → crate 全量 → workspace Clippy”的简化验收，不重复历史 SSA/LLVM 窄测或未覆盖的后续 native matrix |
| 独立 fresh-context 评审与复审 | 通过 | 初审发现 entry shape 先于 analysis identity 会造成错误分类漂移的 P2；抽取唯一 `validate_unit_inputs` 并把 compatibility gate 前置，新增 foreign analysis 回归后复审无 P1/P2/P3 |
| `cargo test -p lang-codegen --lib native::unit_tests --locked --offline` | 2 passed | 单一真实可执行 fixture 合并 alias/closure/String、class/value class/Box constructor、Rc retain/payload read，以及同时持有三类 owner 时的正常 Value delivery 与提前 return drop；原子负例继续走 public API |
| `cargo test -p lang-codegen --lib --locked --offline` | 226 passed, 1 ignored | 完整 native matrix 与 codegen 全量 lib 基线；LLDB sandbox 用例按既有约定 ignored |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | 通过 | 第二十五步完成切片继续采用“单一窄测 → crate 全量 → workspace Clippy”，不重复历史 SSA/LLVM 窄测或拆分多个 link/run fixture |
| 独立 fresh-context 评审与复审 | 通过 | 复核 constructor/generic owner、String/Rc/aggregate 正常与提前退出析构，以及 public commit-failure 错误传播、旧目标保持和 temporary 清理；修复 helper-only 覆盖的 P3 后无 P1/P2/P3 |

`Rc<T>` composite generic 没有列入上述 lowering 窄测：现行 source parser 会先发布诊断，因而不存在可合法
传入 SPEC-0199 的 `ValidatedCompilationUnitTypes`。该已知 frontend 实现缺口不由 codegen 测试伪造；若后续
frontend 接通此语法，必须先补 source-level 负例或实例化正例，再决定是否放宽 unit type resolution。

MoveOnly enum subject 没有列入本切片的 `when` 正向表面：SPEC-0198 现行 drop planner 会在 subject
`AfterExpression` 发布 named root drop，而不是在各 `BranchExit` 发布可沿 CFG 重绑定的 owner/drop
facts。unit lowerer 在生成 subject SSA 前返回 `UnsupportedNode`，不搬移或忽略 validated drop point；
待 frontend 发布 branch-qualified subject facts 后再单独放宽。

顺序容器 runtime-length initializer 仍依赖 callable bridge；element read/borrow/replace 仍依赖
source-qualified container operation 与 loan/replacement lowering。两者均未进入第十八切片，lowerer
分别在实参求值前或 type planning 阶段 fail loud。Unit element 已在 verified SSA 中显式物化为
`ScalarConstant::Unit`；其 LLVM value/materialization 由第 24 步统一接入，不把 SSA 进度误写为
native 闭环。

第十九切片先接通后续 callable/container/closure 共用的 shared-Borrow 基础：Borrow 参数在 function
entry 使用 function-scoped Shared Loan；owned root 与 temporary 按唯一的 source-qualified
`UnitLoanFact(call, argument)` 建立同步 loan，调用后只结束本次新建 loan，并按创建逆序结束。已有
Borrow 参数向下游调用直接转发同一 loan，不生成嵌套 begin/end；Copyable Borrow name 通过 active
loan `Read`。源码实参仍按源码顺序 lower，再按 parameter index 组装 DirectCall 槽位，CallReturn drop
发生在 loan 结束后。一般 `Inout`、非 root field/container/Rc projection、MoveOnly Borrow name read
以及 callable ABI 中的 `Borrow(Unit)` 仍是显式边界；其中 `Borrow(Unit)` 在 concrete substitution 后、
任何 SSA 类型或函数发布前拒绝，等待后续 ABI 擦除方案。该切片只闭合 verified SSA，不表示 LLVM、
container element borrow、closure thunk 或 native 已接通。

第二十切片消费 typed element-place、source-qualified loan 与 replacement drop facts，接通 concrete
顺序容器的元素核心。Copyable element 直接形成 `ContainerElementPlace + Read`；Borrow 实参可从
owned root、已有 Borrow 参数或 temporary container 形成 shared element loan；带 group 的 temporary
receiver 先按 frontend temporary origin 核对，再把实际 owner 唯一重绑定到 source-qualified loan target，
使 CallReturn 精确 drop 而不残留同 ValueId 别名。`Array`/`MutableList` simple replacement 先求值
receiver/index/RHS，再执行 `ContainerReplace`；MoveOnly RHS 先按既有
Value delivery 转交，旧元素只在精确 `AfterReplacement/ReplacedElement` fact 存在时由 replace intrinsic
承担 drop。整数 compound replacement 复用 checked failure CFG，并只携带 live MoveOnly bindings，
success block 使用重绑定后的 owner，避免隐藏 linear live-in。source `Int` index 为 i32，verifier 因而
统一接受有符号 i32/i64 container index，同时继续拒绝无符号或其他宽度。field-backed receiver、
`size`、temporary compound、一般 `Inout` 与 MoveOnly element read 仍在发布 program 前 fail loud；
LLVM/native 尚未接线，现行 guide 语义未改变。

第二十一切片消费 SPEC-0198 的 closure capture/drop provenance，并复用 SPEC-0038 的 concrete closure
SSA model。reachable lambda 以所属 function instance 与 source-qualified expression 形成稳定 identity，
environment 字段只接受 owned Copy/Move capture；thunk 通过 shared environment loan 建立逐字段
`SharedFieldLoan`，函数体中的捕获名因而保持 Borrow 视图。具名局部 closure 可移动、重复调用，并在
最后一次使用、BranchExit 或 LoopExit 按 Named owner 与反序 `Captured` facts 精确生成一次 recursive
drop；closure provenance 与普通 owner binding 一起经过 `if`、`when`、短路和 loop 的状态恢复与一致
合流。temporary/direct delivery、borrowed/empty/nested、参数化或非 `Unit` closure、body 内 MoveOnly
temporary 仍在发布 program 前 fail loud；一般 closure surface、LLVM/multi-source DWARF 与 native
继续由后续切片承接，现行 guide 语义未改变。

第二十二切片补齐 Guide §4 与 §27 明确的无 capture callable 表示：普通 lambda 与 `move` lambda
都生成 signature-deduplicated `FunctionPointer`，每个源码 lambda 仍有独立 thunk，并通过
`FunctionAddress` 取得地址；不创建空 environment、`ConcreteClosure`、`ClosureConstruct` 或
`SharedFieldLoan`。function pointer 按 SPEC-0038 继续作为 MoveOnly owner，可绑定、转移、重复
`CallableInvoke`，并由既有 drop fact 唯一 discharge；LLVM 的 function-pointer drop glue 最终为空操作。
结构测试锁定跨文件输入置换、普通/move 两种 surface、canonical type、独立零参数 thunk、无隐藏
environment，以及 captured concrete closure 不回归。参数化、非 `Unit`、temporary/direct delivery
和一般 callable ABI 仍在发布 program 前 fail loud；现行 guide 语义未改变。

第二十三步的第一切片把既有 function-pointer/concrete-closure signature 接到 source lambda 参数与
Copyable 返回。显式 lambda 参数按 source-qualified `LambdaParameter` symbol 绑定到 thunk entry；
captured thunk 保持 environment shared loan 在前，随后按声明顺序接用户参数。Borrow 与 Value
实参复用普通 call 的 `UnitLoanFact` / `UnitValueDeliveryFact`，仍按源码顺序求值、参数下标组装，
只结束本次新建 loan，再处理 callee `AfterExpression` 与 `CallReturn` drop。非 `Unit` lambda body
只把最后一个 element 作为 tail value 返回，其余 element 沿用现有 statement/drop 路径。该切片只
接受可由现有 SSA storage 表示的 Copyable 参数与返回；`Inout`、`Borrow(Unit)`、MoveOnly 参数/返回、
跳出实参的 `return`/`break`/`continue`、function-value 实参内部 loop jump、temporary/direct callable
delivery、LLVM/multi-source DWARF 与 object/native 继续在发布 program 前 fail loud，避免在 frontend
尚无 exit-qualified pending-argument 清理事实或 callable callee edge carry 时猜测 loan/Value/callee
owner 的控制转移析构。direct call 实参内部 loop body 的 break/continue 仍按真实 loop boundary 正常
lower；现行 guide 语义未改变。

第二十三步的第二切片只放行现有 frontend facts 可以完整证明的 MoveOnly callable result：lambda body
最后一个 element 必须是与 callable 返回类型一致、可由现有 SSA storage 表示的唯一直接 temporary。
thunk 将该 owner 直接交给 `Return`，不生成提前 drop；`CallableInvoke` 在 caller 产生独立 result owner，
再由现有 temporary 到 binding/return/drop 流消费。function pointer 与 captured concrete closure 共用该
契约，environment-first thunk ABI 不变。lambda span 内若存在任何其他可表示的 MoveOnly temporary，
或 tail 是 local owner、显式 return、concat/分支等复合结果，仍在发布 program 前 fail loud；这些表面
等待 lambda-body exit-qualified owner/drop facts，不以 codegen 推测替代 frontend 事实。MoveOnly 参数、
一般 MoveOnly `if`/`when` result 与 `for` 也未因此解锁；现行 guide 语义未改变。

第二十三步的第三切片消费 SPEC-0215 发布的独立 lambda body liveness/drop facts，不再用“body 中存在
额外 MoveOnly temporary”这一 AST 扫描近似完整性。thunk 沿普通 expression/statement 路径 lower
String concat operand、body-local owner 与显式 return，并在隐式返回时按 tail expression identity
转移结果 owner；返回前强制 temporary 集合为空，且剩余 named binding 中不得有 concrete MoveOnly
类型。由此 function pointer 与 captured concrete closure 都支持 composite String result，operand 与
local owner 只按 frontend 精确 point 析构，返回 owner 不 drop。MoveOnly `if`/`when` body 因
SPEC-0215 原子回滚 plan 而无法满足退出不变量，继续在 program 发布前 fail loud；MoveOnly Value
参数仍等待 lambda-entry drop point。现行 guide 语义未改变。

第二十三步的第四切片消费 SPEC-0216 control result facts。`if` / `when` result gate 改为检查 concrete
type 是否具有现行 SSA storage，并保留无需 storage 的 `Nothing` 全 diverge 路径，不再用 Copyability
拒绝 String 等 MoveOnly owner；每条正常 branch
完成 tail lowering 后，按 source-qualified expression category 把 Place/Temporary owner 转交 merge
result，再消费 operand、alternative 与 BranchExit 精确 drop facts。merge parameter 顺序固定为
result、live value binding、live loan；named callable、function pointer 与 captured concrete closure
共用该路径，nested control、`Nothing` 分支与输入置换不另设特例。captured thunk 的 environment field
loan/user Borrow 参数及 checked arithmetic 的 success/failure continuation 都通过显式 block/edge 参数
重绑，避免在内层 CFG 形成 hidden linear live-in。public native fixture 同时覆盖跨 package alias、named
MoveOnly `if` result、captured lambda control result 与 branch 内 checked arithmetic；UnsupportedSource
负例改由尚未实现的 MoveOnly Value lambda 参数锁定。现行 guide 语义未改变。

第二十四步的第一切片把真实 compilation-unit product 接到既有 verified LLVM adapter 与 multi-source
DWARF emitter。正向链从两个 package 的 source input 分别执行 name/type/ownership 分析，再生成单一
SSA/LLVM module；fixture 同时覆盖 alias direct call、captured closure thunk、environment-first + user
Borrow pointer ABI、MoveOnly String result、借用结束后的唯一 drop，以及 provider/consumer 各自的
`DIFile`、`DISubprogram` 和代表性 `DILocation`。输入反序后重新分析并比较完整 LLVM 文本，证明 identity、
函数顺序、drop glue 与 debug metadata 均不依赖 source input 顺序。该切片复用现有生产 adapter，没有
新增公开 API；object 原子写入、link/run 与完整 native 正反矩阵仍由第二十五步承接，现行 guide 语义
未改变。

第二十五步的第一切片新增 public `emit_native_unit_object`，显式接收共同 `SourceMap`、source inputs、
validated names/types/ownership、`TypeEnvironment`、resolved `DeclarationId` 与输出路径。入口首先复用
unit plan 的唯一 compatibility gate，稳定拒绝混用 analysis chain；随后只接受非泛型、零参数、精确
`Unit` 的顶层 callable。全部 frontend/SSA/target/LLVM 验证完成后，backend 只写输出同目录、以
`create_new` 抢占的 sibling temporary；成功后单次 rename 发布，任一 backend/commit 失败均由 RAII
清理 temporary，既有目标不被预删或截断。native 正例跨两个 package 运行 alias call、captured closure、
Borrow、动态 String concat/result/drop，并真实生成 Mach-O、Clang link/run；负例锁定 InvalidEntry、
UnsupportedSource、MismatchedAnalysis 与 commit failure 的错误分类及旧目标保留。该子切片尚不等于
SPEC-0199 完整 native matrix；multi-file aggregate/Rc/constructor 与正常/提前退出组合仍由后续子切片
承接，公开 project CLI 仍属于 SPEC-0054，现行 guide 语义未改变。

第二十五步的完成切片不再新增 production surface，而是在上述 public API 的单一真实可执行 fixture 中
补齐剩余 native matrix。provider 同时提供 named class、value class→Box、`Rc<Int>` 与动态 String
construction/operation；consumer 在同一函数内先取得三类 owner，再分别以 `flag=true` 的提前 return
验证本地析构，以 `flag=false` 的跨文件 consuming call 验证 Value delivery、field projection、Rc
retain/payload read 与最终唯一 drop。alias captured closure 与动态 String 返回仍在同一 executable 中运行，
避免为每类 owner 重复 object/link/run。负例继续从 public `emit_native_unit_object` 触发 commit failure，
锁定错误传播、旧目标保持与 sibling temporary 清理。至此第二十五步的 object/native 正反矩阵、文档与
workspace 基线完成；该切片不改变 guide 语义，公开 project CLI 仍属于 SPEC-0054。

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
19. [ ] 扩展 MoveOnly value result、其余 `when`/for、container element operation 与 closure owner/drop SSA →
   验证：正常和提前退出、结果 owner 转移、复合 drop glue unit-wide 去重。
20. [ ] 接 LLVM 与多 source DWARF → 验证：规范化 LLVM 顺序置换和源码定位窄测试。
21. [ ] 接单 object 原子写入并完成 native 正反矩阵、Architecture 与 workspace 基线。

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
| 19 | 其余现行表面的 owner-aware verified SSA | `feat(codegen): lower multifile units (SPEC-0199)` |
| 20 | LLVM 与 multi-source DWARF | `feat(codegen): lower multifile LLVM (SPEC-0199)` |
| 21 | single-object/native integration 闭环 | `feat(codegen): emit multifile objects (SPEC-0199)` |

## 9. 未决问题

- 多 object/增量 ABI 明确留给后续 ADR。
- SPEC-0198 的现行 unit drop planner 会为 MoveOnly control-tail temporary 发布
  `AfterExpression` drop；例如 String value-`if` 两个 literal 均被计划在各自尾表达式后析构，不能作为
  merge result 转交。0199 不得忽略 validated drop facts；MoveOnly control result 必须先由独立 frontend
  follow-up 把 tail 的 Consume/transfer 边界发布正确，再进入 unit SSA。
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
`ScalarConstant::Unit`；其 LLVM value/materialization 由第 20 步统一接入，不把 SSA 进度误写为
native 闭环。

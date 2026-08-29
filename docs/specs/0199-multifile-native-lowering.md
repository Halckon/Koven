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
7. [ ] 扩展 MoveOnly value result、其余 `when`/循环与 aggregate/Rc/container/closure owner/drop SSA →
   验证：正常和提前退出、结果 owner 转移、复合 drop glue unit-wide 去重。
8. [ ] 接 LLVM 与多 source DWARF → 验证：规范化 LLVM 顺序置换和源码定位窄测试。
9. [ ] 接单 object 原子写入并完成 native 正反矩阵、Architecture 与 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | unit reachability/instance plan | `feat(codegen): plan multifile instances (SPEC-0199)` |
| 2 | scalar expression-body verified SSA | `feat(codegen): lower multifile scalar calls (SPEC-0199)` |
| 3 | block/return 与跨文件 String owner/drop SSA | `feat(codegen): lower multifile string owners (SPEC-0199)` |
| 4 | Unit-valued `if` owner-aware CFG | `feat(codegen): lower multifile conditionals (SPEC-0199)` |
| 5 | Copyable value-valued `if` | `feat(codegen): lower multifile value conditionals (SPEC-0199)` |
| 6 | exhaustive Boolean-subject `when` | `feat(codegen): lower multifile boolean when (SPEC-0199)` |
| 7 | 其余现行表面的 owner-aware verified SSA/LLVM | `feat(codegen): lower multifile units (SPEC-0199)` |
| 8 | single-object/native integration 闭环 | `feat(codegen): emit multifile objects (SPEC-0199)` |

## 9. 未决问题

- 多 object/增量 ABI 明确留给后续 ADR。
- SPEC-0198 的现行 unit drop planner 会为 MoveOnly control-tail temporary 发布
  `AfterExpression` drop；例如 String value-`if` 两个 literal 均被计划在各自尾表达式后析构，不能作为
  merge result 转交。0199 不得忽略 validated drop facts；MoveOnly control result 必须先由独立 frontend
  follow-up 把 tail 的 Consume/transfer 边界发布正确，再进入 unit SSA。
- guide 的 `when_condition = expression` 可推出括号表达式 condition，但现行 parser 对
  `(true) ->` 报 L0058/L0065；0199 只 lower 可达的 bare Boolean literal，不在 Phase 4 内修改 Phase 1
  语法。该 parser/guide 漂移由独立 frontend follow-up 锁定并修正。

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

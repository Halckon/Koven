# SPEC-0224：dependent inherited owner recipe lowering

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P4-224` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.34 §34](../guide/01-design-decisions.md#34-显式-instance-receiver-契约与静态分发调用v034) |
| 批准依据 | 2026-09-01 持续 Goal 要求继续按 Phase 推进 guide 对应 Specs、简化验收；SPEC-0191 完成审计隔离该门禁 |
| 前置 Spec | SPEC-0180、0181、0191、0219 `done` |
| 前置 ADR | ADR-0008、0016 `accepted` |
| 关联 Spec | SPEC-0199、0225 |
| 阻塞项 | 无；参数增长型递归 runtime cycle 明确排除到 SPEC-0225 |
| 影响范围 | `lang-codegen` unit planner、nominal layout demand、receiver/default native tests；Architecture/Roadmap |
| 语言语义变更 | 否；只 lower frontend 已选定的 inherited effective implementation |

## 1. Goal

完成后，inherited effective implementation 的 dependent 单参数 ordinary-class owner recipe 可在 concrete
specialization 下确定性实例化；仅用于 callable instance key 的类型不被物化，ABI/body 确实需要的 runtime
layout 则必须来自 exact SPEC-0219 descriptor 并完成 verified SSA/LLVM/native。

## 2. 范围与需求

- planner 继续消费 frontend selected implementation、owner template 与 concrete `StaticSelf`，不得按名称重选。
- 对每个 concrete dependent argument，先分类为 `InstanceKeyOnly` 或 `RuntimeLayoutRequired`；分类由可达
  signature/body operation 决定并写入 plan，不以类型形状猜测。
- `RuntimeLayoutRequired` 必须找到 exact owner-qualified descriptor，并递归实例化字段；
  `InstanceKeyOnly` 不声明 SSA nominal、allocation 或 drop glue。
- 同一 concrete type 若被两个可达路径以不同需求使用，合并为 `RuntimeLayoutRequired`，结果与输入顺序无关。
- self-growing、参数增长型、多参数/value-class/其他 intrinsic recipe 继续 fail loud；参数增长型由 SPEC-0225 承接。

## 3. 非目标

- 不定义递归 nominal SCC、forward declaration 或递归 drop glue；不开放 SPEC-0225 范围。
- 不改变 frontend inherited target、type inference、receiver ownership、runtime ABI 或动态分发边界。

## 4. 验收标准

- [x] planner 白盒覆盖 dependent class 的 instance-key-only 与 runtime-layout-required 两类正例。
- [x] SSA/LLVM 证明前者不物化 nominal identity，后者只物化 exact concrete layout/drop requirements。
- [x] missing/mismatched descriptor、self-growing、参数增长、多参数、value class、intrinsic、固定参数 SCC
  与输入置换矩阵稳定。
- [x] inherited default 的 source→object→link→run 覆盖 dependent Value delivery；既有 override 路径保持独立。
- [x] `unit_plan_tests`、receiver/native 职责测试与 workspace library 静态门禁通过；未跑 frontend 全量测试。
- [x] Architecture/Roadmap/Spec 同步并经独立高风险复核。

## 5. 技术方案与边界

在既有 unit reachability plan 内增加确定性的 runtime-demand 标记，复用 SPEC-0219 concrete field descriptor
与 SPEC-0191 inherited resolver。标记只控制 nominal declaration/layout/drop requirements，不改变 callable
instance key 或 frontend target identity。

## 6. 实施计划

1. [x] 用成对 fixture 锁定 instance-key-only/runtime-demand 分界 → 验证：planner 白盒与输入置换。
2. [x] 接 exact dependent layout 与 fail-loud 门禁 → 验证：SSA/LLVM 正反矩阵。
3. [x] 接 inherited native 闭环并同步文档 → 验证：真实运行、精简回归与独立复核。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | dependent recipe runtime-demand planner 与 native lowering | `feat(codegen): lower dependent inherited owner recipes (SPEC-0224)` |

## 8. 未决问题

- 无。递归参数增长型图由 SPEC-0225 独立决定，不阻塞本 Spec 的非递归 dependent recipe。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-codegen ssa::unit_plan_tests:: --locked --offline` | 通过，29/29 | planner、descriptor、SCC、lambda ABI/enum payload demand；未跑 frontend 全量测试 |
| `cargo test -p lang-codegen ssa::unit_lower_receiver_tests:: --locked --offline` | 通过，46/46 | key-only 与 exact runtime layout 的 SSA/LLVM 职责矩阵 |
| `cargo test -p lang-codegen ssa::unit_lower_closure_tests:: --locked --offline` | 通过，8/8 | closure declaration 后的最终 materialization guard |
| `cargo test -p lang-codegen dependent_class_inherited_owner_recipe_value_roundtrip_links_and_runs --locked --offline` | 通过，1/1 | source→object→link→run |
| `cargo check --workspace --lib --locked --offline` | 通过 | workspace library 静态门禁 |
| `cargo clippy --workspace --lib --locked --offline -- -D warnings` | 通过 | workspace library lint 门禁 |
| `cargo fmt --all -- --check`；`git diff --check` | 通过 | 格式与补丁卫生 |
| 独立高风险复核 | 通过 | 首轮发现 demand 闭包/lambda 晚物化与闭合 SCC 漏洞；修复并补回归后复核 |

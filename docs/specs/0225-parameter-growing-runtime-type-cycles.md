# SPEC-0225：参数增长型 runtime recipe 策略与 lowering

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P4-225` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.34](../guide/00-index.md) 的既有 nominal/generic/receiver 规则 |
| 批准依据 | 2026-09-01 持续 Goal 的站立授权；ADR-0024 已选择确定性拒绝并解除策略阻塞 |
| 前置 Spec | SPEC-0035、0186、0191、0219、0224 `done` |
| 前置 ADR | ADR-0024 `accepted` |
| 关联 Spec | SPEC-0199、0224 |
| 阻塞项 | 无；ADR-0024 已选择 LLVM 前的 declaration-path 确定性拒绝 |
| 影响范围 | `lang-codegen` unit planner、SSA nominal declarations、LLVM type/drop lowering；Architecture/Roadmap |
| 语言语义变更 | 否；若现行 guide 无法保证 finite runtime layout，必须先升级 guide，不能由本 Spec 猜测 |

## 1. Goal

完成后，参数增长型 owner recipe 会按 ADR-0024 在 LLVM 类型构造前、无需展开无限 concrete
arguments 地确定性拒绝；codegen 不会因不断生成新的 specialization key 而发散。

## 2. 范围与需求

- planner 必须按 ADR-0024 的 declaration path 识别增长链并稳定返回 `UnsupportedNode` 与触发边 Span；
  不先生成下一层 concrete key，不使用深度/实例数量上限。
- SSA nominal declaration、drop requirements 与 LLVM module 均不得出现被拒绝链的部分产物。
- `Grow<Int> → Grow<List<Int>> → …` 这类无限且无重复 concrete node 的链必须终止分析并确定性拒绝，
  不能靠递归深度上限伪装支持。
- growth/invalid graph 在 LLVM type construction 前带来源 Span fail loud；target size/alignment/stride
  继续由 SPEC-0186 在取得 LLVM `TargetData` 后、复合类型构造前预检。

## 3. 非目标

- 不开放无限 inline value/enum layout、动态分发、custom allocator、运行时反射或 GC。
- 不修改 frontend generic/copyability 语义，也不把缺失 descriptor 视为可推导。
- 不顺带开放多参数/value-class inherited recipe；它们需要各自可验证需求。

## 4. 验收标准

- [x] ADR 比较确定性拒绝、type erasure、共享 glue 与有限 type graph 方案，明确 ABI、终止与失败分类并已接受。
- [x] 参数增长型 source fixture 与输入置换稳定产生 `UnsupportedNode` 和触发边 Span，且不形成部分 plan。
- [x] 无限 inline、无重复节点的参数增长链与缺 descriptor 在 LLVM type construction 前终止并保留 Span；
  target overflow 按 SPEC-0186 在取得 `TargetData` 后、复合类型构造前终止。
- [x] 非增长有限 recipe 的既有 planner/SSA/LLVM/native 正例保持通过，证明拒绝边界未扩张。
- [x] 受影响 codegen 窄测、workspace library check/clippy、fmt/diff 与独立高风险复核通过。
- [x] Architecture/Roadmap/Spec 同步。

## 5. 技术方案与边界

实现只固化 ADR-0024 的 declaration-path 终止证据与稳定失败，不建立无限 concrete type graph，也不
接受“提高递归深度上限”或按实例数量截断。普通错误与实例上限沿正式 concrete instance frontier
定序；当前 specialized key 即将命中上限时，才以当前及其余 concrete pending frontier 各自的
arguments/`StaticSelf` 播种有限 recipe facts 并统一选择稳定 root；recipe-only 预检不得用抽象
template 顺序覆盖正式 frontier。normal 路径只在正式 concrete call 的 facts-aware probe 已确认
dedicated recipe failure 后汇总同一 frontier，保证是否命中实例上限不改变稳定 witness。

## 6. 实施计划

1. [x] 起草并接受参数增长型 runtime recipe ADR → 验证：替代方案、ABI、终止与失败模型完整。
2. [x] 固化 ADR 选择的 planner 拒绝边界 → 验证：无重复增长链、Span、原子失败与输入置换。
3. [x] 回归非增长 recipe、descriptor 与 target preflight → 验证：既有 SSA/LLVM/native 不退化。
4. [x] 独立复核并同步 Architecture/Spec → 验证：精简 workspace 门禁。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 前置 ADR | `docs(adr): decide parameter-growing runtime recipes` |
| 2 | 终止/拒绝边界及获准的 SSA/LLVM/drop/native | `feat(codegen): handle parameter-growing runtime recipes (SPEC-0225)` |

## 8. 未决问题

- 无。未来开放需由新 ADR 取代 ADR-0024，不在本 Spec 内扩张。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| ADR-0024 方案审计 | 通过 | 比较 declaration-path 拒绝、concrete SCC、深度上限、type erasure、unit-local shared glue 与 pointer-only 例外；记录终止度量、稳定 witness 与 ABI 代价 |
| ADR/Spec 独立文档复核 | 通过 | 首轮发现 target preflight 阶段、验证记录、witness 顺序与 shared-glue 代价表述问题；修正后复核无阻塞 |
| `cargo test -p lang-codegen ssa::unit_plan_tests:: --locked --offline` | 通过，47/47 | growth/fixed SCC、dependent/closed unsupported 优先级、call/frontier 普通错误与 sibling instance-limit 双向优先级、delegation owner field 普通错误优先级、同 template 多 specialization、Declaration/Symbol key 顺序、pending sibling recipe、normal/limit 跨 seed 稳定 root、输入置换、generic substitution、跨 callable/nominal 参数 facts 传播、generic delegation inherited-default 拒绝与 local-override 正例、direct/delegation、exact descriptor 与非增长 recipe；未跑 frontend 全量测试 |
| `cargo test -p lang-codegen ssa::unit_lower_aggregate_tests::rejects_recursive_nullable_owner_until_ssa_declaration_cycles_are_supported --locked --offline` | 通过，1/1 | 无限 inline/owner-cycle 相邻门禁 |
| `cargo test -p lang-codegen llvm::layout_tests::oversized_recursive_shape_fails_deterministically_before_type_lowering --locked --offline` | 通过，1/1 | SPEC-0186 target preflight 回归 |
| `cargo test -p lang-codegen ssa::unit_lower_receiver_tests:: --locked --offline` | 通过，46/46 | 非增长 receiver SSA/LLVM 回归 |
| `cargo test -p lang-codegen dependent_class_inherited_owner_recipe_value_roundtrip_links_and_runs --locked --offline` | 通过，1/1 | 非增长 source→object→link→run |
| `cargo check --workspace --lib --locked --offline`；`cargo clippy --workspace --lib --locked --offline -- -D warnings` | 通过 | workspace library 静态门禁 |
| `cargo fmt --all -- --check`；`git diff --check` | 通过 | 格式与补丁卫生 |
| SPEC-0225 独立高风险复核 | 通过 | declaration-path witness、支持面与 ADR 一致性 |

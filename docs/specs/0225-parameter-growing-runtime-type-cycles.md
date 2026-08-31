# SPEC-0225：参数增长型 runtime recipe 策略与 lowering

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P4-225` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.34](../guide/00-index.md) 的既有 nominal/generic/receiver 规则 |
| 批准依据 | 待递归 runtime type graph ADR 接受后按持续 Goal 审计 |
| 前置 Spec | SPEC-0035、0186、0191、0219 `done` |
| 前置 ADR | 待新增“参数增长型 runtime recipe”ADR并 `accepted` |
| 关联 Spec | SPEC-0199、0224 |
| 阻塞项 | 尚未决定无限 concrete expansion 的确定性拒绝、type erasure、共享 glue 或其他有限表示策略 |
| 影响范围 | `lang-codegen` unit planner、SSA nominal declarations、LLVM type/drop lowering；Architecture/Roadmap |
| 语言语义变更 | 否；若现行 guide 无法保证 finite runtime layout，必须先升级 guide，不能由本 Spec 猜测 |

## 1. Goal

完成后，参数增长型 owner recipe 会按 accepted ADR 选择的有限表示策略完成 lowering，或在无法形成
有限表示时于 LLVM 类型构造前确定性拒绝；codegen 不会因不断生成新的 concrete arguments 而发散。

## 2. 范围与需求

- 先由 accepted ADR 定义参数增长检测与可观察 ABI，明确采用确定性拒绝、type erasure、共享 glue 或
  其他有限表示；本 Spec 不预设增长链必然形成重复节点或有限 SCC。
- 若策略允许 lowering，规划阶段必须区分 pointer/owner 间接递归与 inline 递归，并证明类型声明、
  layout 与 drop glue 生成有限；若策略拒绝，则错误分类和 Span 必须稳定。
- SSA nominal declaration 与 drop requirements 的 identity/预声明方式服从 ADR，LLVM 不依赖遍历偶然顺序。
- `Grow<Int> → Grow<List<Int>> → …` 这类无限且无重复 concrete node 的链必须终止分析并确定性拒绝，
  不能靠递归深度上限伪装支持。
- target size/alignment/stride 继续由 SPEC-0186 预检；invalid graph 在 LLVM 前带来源 Span fail loud。

## 3. 非目标

- 不开放无限 inline value/enum layout、动态分发、custom allocator、运行时反射或 GC。
- 不修改 frontend generic/copyability 语义，也不把缺失 descriptor 视为可推导。
- 不顺带开放多参数/value-class inherited recipe；它们需要各自可验证需求。

## 4. 验收标准

- [ ] ADR 比较确定性拒绝、type erasure、共享 glue 与有限 type graph 方案，明确 ABI、终止与失败分类并已接受。
- [ ] 选择 lowering 时，合法有限表示 recipe 的 planner/SSA/LLVM 正例与输入置换通过；选择拒绝时，
  对应 source fixture 稳定产生已定义失败。
- [ ] 无限 inline、无重复节点的参数增长链、缺 descriptor 与 target overflow 均在 LLVM 前终止并保留 Span。
- [ ] 若 ADR 允许 runtime lowering，合法 recipe 经 source→object→link→run，析构次数精确且代码生成有限。
- [ ] 受影响 codegen 窄测、workspace library check/clippy、fmt/diff 与独立高风险复核通过。
- [ ] Architecture/Roadmap/Spec 同步。

## 5. 技术方案与边界

是否建立 type graph、采用何种节点 identity，以及 LLVM opaque/identified type 或共享 glue 策略都由
前置 ADR 决定。本 Spec 不预选 SCC 解法，也不接受“提高递归深度上限”或按实例数量截断。

## 6. 实施计划

1. [ ] 起草并接受参数增长型 runtime recipe ADR → 验证：替代方案、ABI、终止与失败模型完整。
2. [ ] 实现 ADR 选择的 plan/拒绝边界 → 验证：无重复增长链、正反矩阵和输入置换。
3. [ ] 若允许 lowering，接 SSA/LLVM layout/drop glue 与 native → 验证：有限生成、精确析构、target preflight。
4. [ ] 独立复核并同步 Architecture/Spec → 验证：精简 workspace 门禁。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 前置 ADR | `docs(adr): decide parameter-growing runtime recipes` |
| 2 | 终止/拒绝边界及获准的 SSA/LLVM/drop/native | `feat(codegen): handle parameter-growing runtime recipes (SPEC-0225)` |

## 8. 未决问题

- 前置 ADR 的具体编号与策略尚未确定；在其 accepted 前本 Spec 保持 `draft`。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 待 ADR | 未执行 |  |

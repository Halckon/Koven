# SPEC-0210：跨文件关联常量集成

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P2-210` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 v0.32；候选 [v0.36 §36](../guide/01-design-decisions.md#36-无运行时存储的关联常量与封闭求值v036-候选未启用) |
| 批准依据 | 无；v0.36 尚未启用 |
| 前置 Spec | SPEC-0026 `done`；SPEC-0025/0197 待完成 |
| 前置 ADR | ADR-0020 `accepted` |
| 阻塞项 | v0.36 明确启用；SPEC-0025/0197 `done` |
| 影响范围 | `lang-frontend` compilation-unit const selection/evaluation/tests；Roadmap/Architecture |
| 语言语义变更 | 否；实施启用后的 v0.36 unit integration |

## 2. Goal

完成后，compilation-unit type checker 在统一 DeclarationId/visibility/package facts上复用
SPEC-0026 evaluator，支持跨文件 `import p.Type` 后 `Type.CONST`、绝对 `p.Type.CONST` 与
跨文件 const dependency/cycle，不建立第二套常量语义。

## 3. 范围与需求

- exact import 终端仍只接受 v0.36 定案的顶层声明/函数组；`import p.Type.CONST` 使用 L0148，
  `import p.Type` 后的 `Type.CONST` 才由 associated selector 处理。
- 可见性和 package-qualified target 只消费 SPEC-0025/0197 validated facts；import target 不可见
  继续使用 L0149，成功选择 Type 后 associated const 越界使用 L0154，不按逻辑路径或源码
  文本猜测。
- 跨 source unit 的 dependency graph 使用稳定 DeclarationId；前向 chain 与 cycle 不依赖输入顺序，
  每个 SCC 仍只产生一个 L0157。
- 输出与 0026 使用相同 ConstValue/use descriptor shape，并在 SPEC-0197 的基础 validated unit
  上发布 `ConstEnabledTypedUnit` capability marker（或等价类型状态）。它不回写或重新定义
  0197，也不被既有 0198/0199 自动接受。

## 4. 非目标

- 不实现依赖 package、跨 compilation-unit ABI、associated function、runtime global、unit const
  ownership 或 native multi-file lowering。后两者必须由新后继 Spec 显式消费 0210、0208/0209
  与 0198/0199，不能修改已完成节点的验收含义。

## 5. 验收标准

- [ ] `import p.Type`; `Type.CONST`、`p.Type.CONST` 与跨文件 acyclic chain 正确。
- [ ] `import p.Type.CONST` 精确产生 L0148；invisible imported Type 使用 L0149，private associated
  const 越界使用 L0154。
- [ ] 跨文件 self/two-node/multi-node SCC 每个 cycle 一个 L0157，labels 使用稳定 unit/declaration key。
- [ ] 正逆 source-unit 输入顺序产生相同 facts/diagnostics，单文件 wrapper 行为不变。
- [ ] invalid unit/import/type facts 不追加 const 级联或半成品 descriptor。
- [ ] 基础 validated unit 与 const-enabled capability 不可混用；0198/0199 对后者保持确定性拒绝，
  直到独立 unit const ownership/native 后继完成。
- [ ] Architecture/Roadmap 与 workspace 基线同步。

## 6. 技术方案与边界

为 0026 evaluator 提供 compilation-unit symbol adapter；adapter只映射 DeclarationId、visibility
与 qualified target，不复制 evaluator、类型系统或 import resolver。

## 7. 实施计划

1. [ ] 接 unit declaration/associated target → 验证：visibility/import/qualified 矩阵。
2. [ ] 接跨文件 dependency/evaluation → 验证：chain/cycle/order矩阵。
3. [ ] 同步验收与 Architecture → 验证：frontend、workspace、fmt/clippy 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | unit const facts、测试与完成文档 | `feat(frontend): integrate multifile constants (SPEC-0210)` |

## 9. 未决问题

- unit const ownership/native 应另立最小后继 Spec，显式消费 `0210 + 0208 + 0209 + 0198 + 0199`；
  本 Phase 2 Goal 不提前生成 owner facts 或 object，也不重开 0198/0199。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | exact-import 冲突不阻塞单文件 0026；跨文件 const 必须等待 0025/0197 |

# SPEC-0025：多文件 package/import 名称解析

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P2-025` |
| 所属 Phase | Phase 2（名称解析） |
| 语言规范 | 现行 v0.31；候选 [v0.32 §32](../guide/01-design-decisions.md#32-packageimport-绑定跨文件可见性与-compilation-unitv032-候选未启用) |
| 批准依据 | 无；v0.32 尚未启用 |
| 前置 Spec | SPEC-0015、0018 `done` |
| 前置 ADR | ADR-0005 `accepted`；[ADR-0020](../adr/0020-multifile-compilation-unit.md) 待接受 |
| 阻塞项 | 用户明确启用 v0.32；ADR-0020 `accepted` |
| 影响范围 | `lang-frontend` source/package index、名称解析、诊断、fixtures；Architecture/Roadmap |
| 语言语义变更 | 否；仅可实施未来启用后的 v0.32 |

## 2. Goal

完成后，frontend 能对一个显式 compilation unit 建立确定的 package/declaration identity，按
exact/alias/wildcard import 与 public/internal/private 规则解析所有跨文件名称，并对失败给出
稳定诊断；不读取文件系统或执行跨文件类型检查。

## 3. 范围与需求

- 实施 ADR-0005/0020 的 source-unit key、package 路径校验、全局 declaration index 和稳定 ID。
- 先收集整个 unit 的顶层类型/值声明，再建立每文件 import environment；函数 overload 只按
  候选 §32 允许的 package 绑定形成。
- 实施 exact、alias、wildcard、限定路径、双命名空间优先级与可见性规则。
- 发布跨文件 `ResolvedReference -> DeclarationId` facts，保留 source/target Span。
- 实施 L0146–L0151，并保证文件输入排列不影响产物或诊断。

## 4. 非目标

- 不做跨文件 body 类型/所有权检查、单态化、SSA、LLVM、链接或 LSP workspace 生命周期。
- 不解析 manifest/依赖/lockfile，不实现 re-export、模块初始化或隐式 prelude import。

## 5. 验收标准

- [ ] 正例覆盖同 package、多 root、public/internal exact、alias、wildcard、限定路径与 overload set。
- [ ] 反例精确覆盖 L0146–L0151 的错误码、主/关联 Span 和确定性顺序。
- [ ] 同一文件集合以不同输入顺序运行，package/declaration/reference identity 与诊断完全一致。
- [ ] 单文件现有名称 suite 经 compatibility wrapper 无行为回归。
- [ ] frontend 窄测试、workspace 基线与 Architecture 同步。

## 6. 技术方案与边界

新增不可变 compilation-unit name product；现有单文件 resolver 的 scope/local 逻辑作为每文件
阶段复用。driver 负责 IO，frontend 只消费显式源码。失败不发布可供后续阶段误用的部分 unit。

## 7. 实施计划

1. [ ] 建立稳定 unit/package/declaration index → 验证：顺序置换与冲突测试。
2. [ ] 接 import/visibility/qualified lookup 和 L0146–L0151 → 验证：正反 fixture。
3. [ ] 迁移单文件 wrapper、同步 Architecture → 验证：frontend 与 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | compilation-unit identity 与 package index | `feat(frontend): index compilation units (SPEC-0025)` |
| 2 | import/visibility resolver、诊断与完成文档 | `feat(frontend): resolve package imports (SPEC-0025)` |

## 9. 未决问题

- 无；语言门禁由候选 v0.32 的启用状态表达。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap 审计 | 通过 | Spec 已物化；因 v0.32/ADR-0020 未生效而保持 draft |

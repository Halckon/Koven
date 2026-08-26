# SPEC-0198：跨文件所有权检查

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P3-198` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 v0.31；候选 v0.32 §32 |
| 批准依据 | 无；v0.32 尚未启用 |
| 前置 Spec | SPEC-0029、0030、0032 `done`；SPEC-0197 待完成 |
| 前置 ADR | ADR-0020 待接受 |
| 阻塞项 | v0.32 启用；SPEC-0197 `done`；ADR-0020 `accepted` |
| 影响范围 | `lang-frontend` compilation-unit ownership products、fixtures；Architecture/Roadmap |
| 语言语义变更 | 否 |

## 2. Goal

完成后，每个跨文件 callable/constructor 使用全局 typed identity 获得精确参数、loan、move、drop
和 closure effects，整个 compilation unit 形成可供 codegen 消费的完整 owner-aware typed product。

## 3. 范围与需求

- 每个 body 恰检查一次；跨文件 call/constructor 复用目标声明的既有 mode 与 ownership facts。
- 跨文件 MoveOnly delivery、Borrow/Inout loan、capture、return 与 ASAP drop 规则和单文件一致。
- 诊断携带使用文件与目标声明关联位置，并按 unit 稳定排序。
- 失败 unit 不发布部分 codegen input；单文件 ownership API 保持兼容包装。

## 4. 非目标

- 不改变所有权语义，不实现 SSA/LLVM、跨 compilation-unit ABI、LSP 或项目构建。

## 5. 验收标准

- [ ] 正反例覆盖跨文件 Borrow/Value/Inout、MoveOnly return、constructor、closure 与 drop point。
- [ ] use-after-move/loan 冲突诊断含精确跨文件目标信息且顺序确定。
- [ ] 单文件 ownership suite、frontend/workspace 基线与 Architecture 同步。

## 6. 技术方案与边界

消费 SPEC-0197 的完整 typed unit，按 declaration/body identity 运行现有 ownership checker，并
汇总为不可变 unit product；不重新做名称或类型解析。

## 7. 实施计划

1. [ ] 建立 unit ownership driver 与跨文件 callable facts → 验证：mode/loan 正反矩阵。
2. [ ] 接 move/drop/capture 与确定性诊断 → 验证：跨文件清理 suite。
3. [ ] 同步 Architecture 并跑 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | compilation-unit ownership product | `feat(frontend): check multifile ownership (SPEC-0198)` |

## 9. 未决问题

- 无。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap 审计 | 通过 | 补齐 type facts 与 backend 之间缺失的 Phase 3 层 |

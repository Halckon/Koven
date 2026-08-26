# SPEC-0199：多文件 compilation-unit native lowering

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P4-199` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 v0.32 §32 |
| 批准依据 | 无；等待前置 SPEC-0198 |
| 前置 Spec | SPEC-0034、0035、0036、0038、0039、0184、0192、0195、0196 `done`；SPEC-0198 待完成 |
| 前置 ADR | ADR-0010、ADR-0020 `accepted` |
| 阻塞项 | SPEC-0198 `done` |
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

1. [ ] 建立 unit reachability/instance plan → 验证：顺序置换与去重测试。
2. [ ] 接 SSA/LLVM、单 object 原子写入 → 验证：IR/object 与多 source DWARF 窄测试。
3. [ ] 完成 native 正反矩阵、Architecture 与 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | unit reachability 与 verified SSA | `feat(codegen): lower multifile units (SPEC-0199)` |
| 2 | single-object/native integration 闭环 | `feat(codegen): emit multifile objects (SPEC-0199)` |

## 9. 未决问题

- 无；多 object/增量 ABI 明确留给后续 ADR。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap 审计 | 通过 | 补齐 project build 和跨文件 LSP 之前缺失的 native 层 |

# SPEC-0025：多文件 package/import 名称解析

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P2-025` |
| 所属 Phase | Phase 2（名称解析） |
| 语言规范 | 现行 [v0.32 §32](../guide/01-design-decisions.md#32-packageimport-绑定跨文件可见性与-compilation-unitv032) |
| 批准依据 | 2026-08-26 用户明确启用 v0.32；当前持续 Goal 授权按依赖图推进已完成审计的 Spec |
| 前置 Spec | SPEC-0015、0018 `done` |
| 前置 ADR | ADR-0005、[ADR-0020](../adr/0020-multifile-compilation-unit.md) `accepted` |
| 阻塞项 | 第 1 步无；exact import 是否覆盖 enum case / companion 静态成员须在第 2 步前消除 guide 冲突 |
| 影响范围 | `lang-frontend` source/package index、名称解析、诊断、fixtures；Architecture/Roadmap |
| 语言语义变更 | 否；实施现行 v0.32 |

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
- exact import 在两个命名空间独立绑定，alias 同时作用于两者；发布 import 终端、alias、限定路径
  segment 与普通引用的精确 reference facts。声明记录包含现有 visibility。
- 实施 L0146–L0151，并保证文件输入排列不影响产物或诊断。

## 4. 非目标

- 不做跨文件 body 类型/所有权检查、单态化、SSA、LLVM、链接或 LSP workspace 生命周期。
- 不解析 manifest/依赖/lockfile，不实现 re-export、模块初始化或隐式 prelude import。

## 5. 验收标准

- [ ] 正例覆盖同 package、多 root、public/internal exact、alias、wildcard、限定路径与 overload set。
- [ ] 反例精确覆盖 L0146–L0151 的错误码、主/关联 Span 和确定性顺序。
- [ ] 同一文件集合以不同输入顺序运行，package/declaration/reference identity 与诊断完全一致。
- [ ] 单文件现有名称 suite 经 compatibility wrapper 无行为回归。
- [ ] recovery product 可供诊断消费，但含 error 时无法取得供类型阶段使用的 validated view。
- [ ] frontend 窄测试、workspace 基线与 Architecture 同步。

## 6. 技术方案与边界

在 `lang-frontend` 名称解析门面新增并行 unit API：

- 第 1 步的 `index_compilation_unit(source_map, &[SourceUnitInput])` 接受同一 `SourceMap` 中的
  `ParsedFile`/`SourceId` 与稳定 source key，内部校验并排序，返回 recovery
  `CompilationUnitIndex`；其 validated marker 只证明 Parser、package/path 与跨文件声明冲突
  无 error，类型上不得冒充最终名称产物。
- 第 2 步的 `resolve_compilation_unit_names(index, environment)` 才解析 import 与 body 名称；
  保留现有 `resolve_names`、`NameEnvironment`、`NameResolution` 的精确单文件语义。
- `CompilationUnitNames` 共同拥有 package/declaration/source tables、每文件 local resolution、
  `UnitSymbolId`/`DeclarationId` 与并行的 `UnitReferenceTarget`；不把 package binding 塞进
  compiler-bound `NameEnvironment`，也不修改现有 `ReferenceTarget` 的 exhaustive 枚举。
- recovery product 始终携带诊断；`ValidatedCompilationUnitNames`（或等价不可伪造 marker）才可
  交给 SPEC-0197。`ordered_unit_diagnostics` 使用稳定 source key，旧排序 API 保持不变。

## 7. 实施计划

1. [x] 建立稳定 unit/package/declaration index 与 recovery/validated 门禁 → 验证：顺序置换、
   duplicate key、错误产物消费测试。
2. [ ] 接 import/visibility/qualified lookup 和 L0146–L0151 → 验证：正反 fixture。
3. [ ] 保留单文件 wrapper、同步 Architecture → 验证：`multifile_name_resolution`、既有
   `name_resolution`、frontend 与 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | compilation-unit identity 与 package index | `feat(frontend): index compilation units (SPEC-0025)` |
| 2 | import/visibility resolver、诊断与完成文档 | `feat(frontend): resolve package imports (SPEC-0025)` |

## 9. 未决问题

- v0.32 §32.3 与 grammar §11.1 把 exact import 限于顶层声明，§32.4 的“import target 和静态
  限定名称”又可能允许继续选择 enum case / companion 静态成员。第 1 步只建立与该选择无关的
  unit/package/declaration 基础；第 2 步不得静默选择，须先由 guide 勘误明确范围。
- 现有单文件 resolver 会在所有声明位置跳过名称 `_`，而 guide 只把特定 binding 位置的 `_`
  定义为 discard。第 1 步按普通顶层 Identifier 收集；第 2 步映射 `UnitSymbolId` 前须收窄旧
  skip 行为并增加兼容回归，不能把实现漂移反写为多文件语义。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap 审计 | 通过 | v0.32 已启用、ADR-0020 已接受；实施前仍须核对代码边界与验收矩阵 |
| 2026-08-26 第 1 步实施前审计 | 通过 | 输入/稳定身份、package index 与 validated 门禁不依赖 exact import 未决范围，可独立实施 |
| `cargo test -p lang-frontend --test diagnostic_model production_catalog_contains_exactly_the_published_frontend_codes` | 通过 | L0146–L0151 连续生产目录 |
| `cargo test -p lang-frontend --test compilation_unit_index` | 通过 | 11 个 Stage 1 identity/package/declaration/L0146/L0147/recovery/确定性用例 |
| `cargo test -p lang-frontend --test name_resolution` | 通过 | 13 个既有单文件兼容用例 |
| `cargo test -p lang-frontend` | 通过 | frontend 全量测试与 doc tests |
| `cargo clippy -p lang-frontend --all-targets -- -D warnings` | 通过 | Stage 1 公开 API 与全部 frontend target |
| `cargo test --workspace` | 通过 | workspace 基线；156 个 codegen 测试通过、1 个既有权限相关测试 ignored，其余 target 全通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | workspace 全 target 无 warning |
| `cargo fmt --all -- --check`、`git diff --check` | 通过 | 格式与空白检查 |

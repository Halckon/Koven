# SPEC-0219：compilation-unit 实例限定 runtime 字段布局事实

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P2-219` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.34](../guide/00-index.md)、[名义类型与泛型](../guide/01-design-decisions.md#23-名义类型泛型接口实现与窄化委托v023) |
| 批准依据 | 2026-08-31 持续 Goal 要求继续按 Phase 推进现行 guide 对应 Specs；SPEC-0191 nested generic field recipe 前置审计确认缺少 owner-instance-qualified frontend fact |
| 前置 Spec | SPEC-0020、0177、0197 `done` |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0020 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` compilation-unit typed product；Architecture/Roadmap；解锁 SPEC-0191 |
| 语言语义变更 | 否；只发布现行泛型替换已经确定的 validated frontend 事实 |

## 1. Goal

完成后，无诊断的 compilation-unit typed product 为每个已规范化的具体 ordinary-class owner
实例发布声明限定、参数完整、字段顺序稳定的 runtime layout descriptor，后续 codegen 可直接消费
具体字段类型，不再从全局类型是否存在或某个 construction/call 是否可达推测布局授权。

## 2. 背景

SPEC-0191 已支持参数无关字段和恰为 owner direct type parameter 的 ordinary-class layout；
`List<T>`、`Wrapper<T>` 等嵌套字段需要递归替换 owner 类型参数。Phase 4 若查询全局 canonical
type presence，会把“类型曾被 intern”误当成 owner provenance；若借用 construction descriptor，
又会让布局授权依赖某个构造表达式是否可达。递归类型替换已经是 Phase 2 的既有职责，应由
validated typed product 统一发布结果。

## 3. 范围与需求

- 在完整 compilation-unit body 类型检查结束后冻结 owner 候选快照，为快照中的具体
  `UnitTypeKind::Nominal` ordinary-class 实例物化 `UnitRuntimeFieldLayoutDescriptor`。
- descriptor 精确保存 owner `UnitTypeId`、`DeclarationId`、完整 owner arguments，以及主构造器
  源码字段顺序的 field symbol、template type、递归替换后的 concrete type 与 field `Span`。
- 类型替换复用 frontend 现有规则，覆盖 direct type parameter 和嵌套 nominal/intrinsic/function
  recipe；结果必须是 unit-global canonical `UnitTypeId`，不得把 callable-local 类型参数当作 owner
  参数替换。
- descriptor 以 owner instance 为唯一键，按稳定 `UnitTypeId` 顺序发布；同一实例只发布一次，
  不依赖 construction、call 或 entry reachability，输入文件置换不改变语义结果。
- 字段递归替换可向 canonical type table 加入结果类型，但不得把这些新类型继续加入本次 owner
  候选；`Grow<T>` 包含 `Grow<List<T>>` 等合法 heap 递归必须有限终止。
- descriptor 纳入 recovery product，但任一 signature/body error 都使本集合原子保持为空；只有
  全 unit 无 error 时才能经 `ValidatedCompilationUnitTypes` 交给后续阶段。缺声明、arity 不符、
  残留 owner type parameter 或非 ordinary-class owner 不发布半成品事实。

## 4. 非目标

- 不修改泛型、字段或所有权语言语义，不新增诊断码。
- 不 lower SSA/LLVM，不在本 Spec 开放 SPEC-0191 的 nested recipe。
- 不发布 value class、enum、interface、object 或 callable-local generic 的 runtime layout。
- 不补齐 compilation-unit nullable storage lowering；`T?` 即使能形成 concrete typed fact，Phase 4
  仍须由独立切片验证 nullable 表示与 drop。
- 不以全局 canonical type presence、任意 exact construction descriptor 或 codegen 递归替换替代
  本事实。

## 5. 验收标准

- [x] `Cell<T>` 的 direct `T` 与 `Holder<T>` 的 `List<T>` / `Wrapper<T>` 发布精确 concrete 字段类型。
- [x] 不同 owner actual、跨文件声明/使用与输入置换得到各自唯一、稳定 descriptor。
- [x] 不可达 construction/call 的有无不影响同一个 owner instance 的 descriptor；仅有无关 nested
  concrete type 不会为另一个 owner 实例产生授权。
- [x] arity/provenance/kind/recovery 边界不泄漏 descriptor，validated product 保持原子性。
- [x] 参数增长型 recursive class 的字段可精确替换且物化有限终止，不生成无限 owner closure。
- [x] 运行受影响 frontend model/lib 与 `multifile_type_checking` 定向窄测，以及 Layer 2 workspace
  library check/clippy；不运行约一小时的 `lang-frontend` 全量测试。
- [x] Architecture、Roadmap 与 SPEC-0191 前置边界同步。

## 6. 技术方案与边界

在 `CompilationUnitTypes` 中保存独立 descriptor 集合；body checker 完成所有 source traversal 后，
从 body traversal 结束时的 `UnitTypeTable` 候选快照枚举 concrete ordinary-class owner，并通过 frontend 统一替换函数计算
字段类型。枚举只选择 owner instance，字段结果仍来自 owner declaration signature；因此事实既不
依赖表达式可达性，也不把“某个嵌套类型存在”提升为另一个 owner 的 provenance。替换过程中新增
的 canonical type 只作为字段结果，不递归扩张 owner 候选集合。

descriptor 查询入口接收精确 owner `UnitTypeId`。Phase 4 consumer 只能按正在 lowering 的 owner
实例查询并核对 declaration/arguments，不得退回扫描 construction 或重新执行模板替换。

## 7. 实施计划

1. [x] 增加 runtime field-layout descriptor、稳定查询与 recovery/validated product 接线 → 验证：model/lib 窄测。
2. [x] 在 body type finalization 物化 ordinary-class concrete layouts → 验证：direct/nested/负例/置换矩阵。
3. [x] 同步 Architecture/Roadmap/SPEC-0191 并运行分层验收。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | descriptor、物化、frontend 测试与文档 | `feat(frontend): publish runtime field layouts (SPEC-0219)` |

## 9. 未决问题

- 无；nullable storage 与 Phase 4 消费保持独立门禁。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-31 前置审计 | 通过 | 两轮 SPEC-0191 原型已排除 global type presence 与 construction reachability 作为授权来源；试验代码均撤回 |
| `cargo test -p lang-frontend --test multifile_type_checking runtime_field_layouts --locked --offline` | 2/2 通过 | direct/nested actual、完整 descriptor、strict owner order、输入置换、callable-local template/value class 拒绝、参数增长型递归有限终止与 body-error recovery 原子性；未运行 frontend 全量 |
| `cargo test -p lang-frontend --lib product_queries_use_the_unit_type_space_and_preserve_analysis_identity --locked --offline` | 1/1 通过 | recovery/validated product 使用同一 unit type space 与分析身份 |
| `cargo check --workspace --lib --locked --offline` | 通过 | Layer 2 跨 crate library 编译门禁 |
| `cargo clippy --workspace --lib --locked --offline -- -D warnings` | 通过 | Layer 2 跨 crate public API 静态门禁，零 warning |
| `cargo fmt --all` / `git diff --check` | 通过 | Rust 格式与 whitespace 门禁 |
| 独立高风险复审 | 通过 | 首轮发现“无 TypeParameter”误当 runtime concrete 的 P2；改为统一拒绝 StaticSelf/EnumCase/Capability/IntegerLiteral/Deferred/Error 的递归 predicate，并补完整 descriptor/排序/置换/callable-local 负例后复审无 P1/P2。逐项 poison kind 与 signature-error 白盒用例仅为 P3 测试深度建议 |

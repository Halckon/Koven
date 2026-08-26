# SPEC-0197：跨文件类型检查

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P2-197` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 v0.32 §32 |
| 批准依据 | 2026-08-27 当前持续 Goal 授权先审计 roadmap、再按依赖图推进已完成审计的 Spec |
| 前置 Spec | SPEC-0020、0021、0025、0174、0177 `done` |
| 前置 ADR | ADR-0020 `accepted` |
| 阻塞项 | 无；现行 §23.3 已规定顶层 overload 拒绝重复 shape，L0097 表格遗漏已作纯勘误 |
| 影响范围 | `lang-frontend` compilation-unit type environment/facts、fixtures；Architecture/Roadmap |
| 语言语义变更 | 否 |

## 2. Goal

完成后，所有 source unit 的 public/internal/private 签名与 body 在同一声明身份图上完成类型
检查，跨文件 call、nominal、generic、constructor、enum 与 interface 引用发布可供 ownership
消费的确定 typed facts；private 仍只在声明 source unit 内可引用。

## 3. 范围与需求

- 分离 unit-wide signature collection 与 per-body checking，支持合法递归引用且不依赖文件顺序。
- 所有跨文件类型和 callable target 通过 `DeclarationId` 解析，复用现有 overload、generic
  instance、constructor 和 flow typing 规则。
- unit 只有一个规范化 `TypeTable` / `TypeId` 空间；成员、field、enum case/payload 与类型参数
  通过 `UnitSymbolId` 或 `DeclarationId` 定位，local AST/symbol ID 始终与 source/body 配对。
  进入 unit-global type kind、descriptor 或 instance key 的源码 nominal、type parameter、member、
  field 与 enum case 不得保存未限定的 `SymbolId` / `EnumCaseId`；两个文件中数值相同的 local ID
  必须保持不同身份。
- 同一输入顺序置换产生相同 type/declaration/instance identity 与诊断。
- 任一 signature 或 body type error 都只发布 recovery typed unit，不向 ownership 发布伪完整
  validated typed unit；其他无依赖 body 仍继续检查并聚合本阶段诊断。

## 4. 非目标

- 不新增类型语义，不实现跨文件所有权、codegen、manifest 或 LSP 生命周期。
- 在 SPEC-0210 前，不产生 v0.36 的关联常量诊断、`ConstValue`、依赖图或 materialization fact；
  现行单文件 const 声明与普通 typed use 行为保持不变。基础 validated typed unit 不等于
  const-enabled marker；只由候选 v0.36 定义的选择继续保留既有 deferred fact，不得伪装成成功。

## 5. 验收标准

- [ ] 正例覆盖跨文件函数、名义类型、泛型、constructor、enum/interface、private 同文件使用与
  同 package 递归签名。
- [ ] private/未解析名称保留在 `CompilationUnitNames` recovery product，并因无法取得 validated
  names 而不运行类型阶段、不产生类型级联；类型反例覆盖顶层 package binding 的 L0097 重复
  shape、body 类型错误，以及 primary/label 跨 source 的既有 L0082–L0145 诊断。
- [ ] 两个 source unit 中相同数值的 local `SymbolId` / `EnumCaseId` 不碰撞；顶层 target、unit
  type kind、call/constructor instance 和 per-source fact 都保留 `DeclarationId` / `UnitSymbolId`。
- [ ] 文件输入顺序置换后，`DeclarationId -> signature`、unit type/instance identity、per-source
  facts 和按 stable source key→byte span→code 排序的诊断完全一致；单文件回归 suite 通过。
- [ ] 名称 validated product、source inputs 或 `TypeEnvironment` owner 混用时返回具体内部错误；
  任一 signature/body type error 仍发布 recovery typed diagnostics，但 ownership 不能取得
  validated typed unit。
- [ ] 不发布 v0.36 const capability/诊断；现行 const 基础 typed 行为保持不变。
- [ ] frontend/workspace 基线与 Architecture 同步。

## 6. 技术方案与边界

新增与单文件 API 并行的 unit type-checking 入口，消费 SPEC-0025 的 validated name product；
名称阶段含 error 时只保留 `CompilationUnitNames` recovery product，类型入口不可调用，也不创建
伪造的 typed recovery product。类型阶段自己的 signature/body error 才进入
`CompilationUnitTypes` recovery product，并阻止其 validated view。
它先按 canonical declaration order 构造 unit-global `TypeTable` 和 `UnitSignatureTable`，再检查
各 source/body；`TypeEnvironment` 名称继续专用于 compiler-bound 外部绑定，不能兼任源码 unit
签名表。名称产物显式发布 `DeclarationId -> UnitSymbolId`，类型阶段不得靠 Span 重匹配已知声明。
入口重核 source inputs、name/index/source identity 与 `TypeEnvironment` owner。产物保存
`DeclarationId -> (SourceUnitId, local item/body)` locator、唯一 analysis owner/TypeTable、每文件
typed facts、本阶段 diagnostics 与 validated gate；不在类型阶段重新展开 import，也不重复聚合
名称诊断。既有单文件 API 与事实必须精确兼容。
后继 SPEC-0210 在此基础上发布独立的 const-enabled capability marker（或等价类型状态），而不是
回写或重新定义本 Spec 已发布的基础 validated product。

## 7. 实施计划

1. [x] 补齐 declaration→unit symbol 名称事实，建立防碰撞 unit type/signature identity → 验证：
   mixed-input、相同 local ID 与输入置换矩阵。
2. [x] 收集跨文件 nominal/callable/field/enum/interface 签名与图诊断 → 验证：递归、L0097、
   generic/constructor/interface 正反矩阵。
3. [ ] 接 body、overload/generic/constructor facts与 recovery/validated gate → 验证：
   `multifile_type_checking` 及既有
   `type_checking`、`type_callable`、`type_copyability`、`type_containers` suite。
   - [x] local/unit 类型表复用同一插入有序、结构去重核心，并为 unit body 建立与单文件一致的
     builtin、Signed/Unsigned integer literal、Error 初始种子；两类公开 TypeId 仍保持隔离。
4. [ ] 同步 Architecture 并跑 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | declaration/unit type identity 与 unit-wide signatures | `feat(frontend): collect unit type signatures (SPEC-0197)` |
| 2 | interface/member/capability/layout signature graph 与 provenance 门禁 | `feat(frontend): complete unit signature graphs (SPEC-0197)` |
| 3 | body typed facts、validated gate 与完成文档 | `feat(frontend): type check compilation units (SPEC-0197)` |

## 9. 未决问题

- 无。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap 审计 | 通过 | 补齐 0025 与 ownership/codegen 之间缺失的 Phase 2 层 |
| 2026-08-27 实施前审计 | 通过 | SPEC-0025 done、ADR-0020 accepted；收紧 unit identity、all-error validated gate、private/const/诊断与 mixed-input 验收 |
| 2026-08-27 L0097 guide 审计 | 通过 | §23.3 已明确顶层 overload 同样拒绝重复 shape；诊断表“成员作用域”遗漏已作不改变语义的纯勘误 |
| `cargo test -p lang-frontend --test multifile_type_signatures --locked --offline` | 通过 | 7 tests；递归签名、输入置换、local ID 防碰撞、alpha/mode L0097、arity、mixed analysis boundary、interface closure/bound/cycle |
| `cargo test -p lang-frontend --test multifile_type_signatures --test multifile_type_signature_provenance --test multifile_type_capability_graph --test multifile_type_member_graph --test multifile_type_signature_determinism --locked --offline` | 通过 | 22 tests；provenance、L0094/L0098–L0105/L0115/L0116/L0141、invariant branch 抑制、delegation plan、instance/companion scope、全 source-local identity 与跨 source 诊断置换 |
| `cargo test -p lang-frontend --test multifile_name_resolution --locked --offline` | 通过 | 10 tests；declaration→unit symbol 名称事实与旧多文件名称语义无回归 |
| `cargo test -p lang-frontend --locked --offline` | 通过 | 新旧 frontend 全量通过，含现有单文件 type/ownership 回归 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 完整 signature graph API、测试与既有 frontend targets 无 warning |
| `cargo test --workspace --locked --offline` | 通过 | 完整 signature graph 合入后 workspace 全量通过；1 个既有 sandbox/CI LLDB 权限测试保持 ignored |
| `cargo test -p lang-frontend type_checking::canonical::tests::local_and_unit_seed_kinds_have_the_same_exact_tail_order --locked --offline -- --exact` | 通过 | 白盒锁定两类表的 Signed/Unsigned/Error 精确尾部顺序与长度 |
| `cargo test -p lang-frontend --test canonical_type_tables --locked --offline` | 通过 | 公开产物锁定 local/unit builtin identity 与完整初始类型表长度 |

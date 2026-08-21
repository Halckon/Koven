# SPEC-0067: 检查 callable 调用与实参契约

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P2-067` |
| 所属 Phase | Phase 2 |
| 语言规范 | [现行 v0.25 callable 契约](../guide/05-grammar-calls-lambda.md#callable-参数契约与调用匹配) |
| 批准依据 | 用户在当前持续 Goal 中要求继续分阶段实施 Specs，并授权简化重复验收环节 |
| 前置 Spec | SPEC-0019、SPEC-0020、SPEC-0022 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` callable model / type checker、L0119–L0124、Phase 2 fixture、Architecture |
| 语言语义变更 | 否；实现现行 guide 已封闭的 callable 契约 |

## 1. Goal

完成后，Phase 2 能为单态具名函数、预声明函数、实例 member 与普通函数值选择唯一 callable，
按源码顺序检查实参映射、类型和参数模式，并输出稳定的 call descriptor 与 place/temporary
分类供 Phase 3 使用。

## 2. 背景

SPEC-0020 已收集源码 callable 签名与 member owner，但调用、member 与 overload 选择仍统一
标为 deferred。现行 guide 已封闭位置/命名映射和 `Value` / `Borrow` / `Inout` 矩阵，本 Spec
只落地这些 Phase 2 事实，不提前判断移动、借用冲突或独占访问的动态所有权前提。

## 3. 范围与需求

- callable 参数元数据保存稳定名称、模式、类型与声明位置；外部单态签名允许参数名称缺失。
- 直接名称调用按名称阶段的源码/外部 overload set 取候选；member 调用按 receiver 的名义
  instance、接口 closure 与泛型 owner substitution 取候选；普通函数值由其 `Function` 类型调用。
- 位置实参只能位于首个命名实参之前；命名实参精确匹配稳定参数名；拒绝未知、重复、缺失、
  多余实参及函数值命名实参。operand 始终只按源码顺序检查一次。
- `Value` 接受无 marker，`Borrow` 接受无 marker 或 `borrow`，`Inout` 只接受 `&`；`&` operand
  在本 Phase 必须具有 place 类别，是否可变及当前是否可独占留给 Phase 3。
- 唯一结构候选向实参传播 expected type；多个候选先对实参定型再按 assignability 过滤，仍有
  多个时报稳定歧义，不按参数模式之外的隐式偏好择一。
- typed 产物按 call expression identity 保存选中的 source/external/function-value target、返回
  类型及源码实参到参数下标映射，并可查询每个 expression 的 place/temporary 类别。
- L0119–L0124 分别覆盖非 callable target、非法命名映射、数量不符、模式不符、无匹配 overload
  与歧义；普通 argument 类型不匹配继续复用 L0084。

## 4. 非目标

- 不实现泛型 callable 的调用点类型实参推导、单态化准备、constructor 或 callable reference
  overload 选择；这些节点保持精确 deferred，不伪造成功 descriptor。
- 不实现 move/copy、借用活跃区、可变性、use-after-move、ASAP 析构或跨线程约束。
- 不实现 index contract、safe-call nullable lifting、companion/关联成员、跨文件 package/import。
- 不新增依赖、crate、语法或按 API 名称硬编码的参数例外。

## 5. 验收标准

- [x] source/external overload、member 与函数值调用产生稳定类型和 call descriptor。
- [x] 位置/命名映射覆盖合法混排，以及未知、重复、位置后置、缺失和多余反例。
- [x] 参数模式矩阵与 place/temporary 分类覆盖无 marker、`borrow`、`&place`、`&temporary`。
- [x] argument expected type、无匹配和歧义按 L0084/L0123/L0124 稳定诊断，候选/实参不重复求值。
- [x] 泛型调用、callable reference、safe member 与所有权前提保持明确 deferred 边界。
- [x] Rust 窄测、Phase 2 pass/fail fixture及一次 workspace 标准基线通过；Architecture、guide
      路线图和本 Spec 验收记录同步为实际事实。

## 6. 技术方案与边界

- `call.rs` 收敛调用选择和表达式类别产物，`model.rs` 只接入参数名称与 typed file 字段；
  选择算法放入独立 `checker/callable.rs`，`expression.rs` 仅分派并维护通用 expression cache。
- 候选、映射与失败原因均使用源码顺序的 `Vec` / 有序集合；不依赖随机 hash 迭代。
- member 候选复用现有 nominal descriptor、interface closure、callable descriptor 与
  `substitute_type`，不建立第二套 class graph。
- 单候选才向 child 传播 expected type；多候选只检查一次无 expected operand，再过滤，避免
  回滚 AST 或重复产生诊断。

## 7. 实施计划

1. [x] 扩展 callable typed model 与诊断目录 → 验证：model/diagnostic 窄测。
2. [x] 实现 source/external/function-value/member 候选与实参映射 → 验证：callable Rust 窄测。
3. [x] 接入 place/temporary 与 call descriptor、补 fixture → 验证：frontend 受影响测试。
4. [x] 同步 Architecture、guide、Spec 验收记录并运行一次 workspace 基线 → 验证：实际退出状态。
5. [x] 暂存本 Spec 独立范围并检查 staged diff → 验证：无跨 Spec 或无关改动。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | callable model/选择/测试、Architecture 与 done 验收 | `feat(frontend): check callable calls (SPEC-0067)` |

## 9. 未决问题

- 无；泛型调用推导与所有权前提已按现行路线图明确延后。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test type_callable --test type_checking --test type_copyability --locked --offline` | 通过 | 43 passed；callable 专测、既有类型检查与 Copyable 回归 |
| Phase 2 fixture | 通过 | `type-pass` / `type-fail` 各 5 个；callable fail 精确核对 L0084、L0119、L0121、L0122 byte Span |
| workspace Cargo 基线 | 通过 | fmt、check、Clippy `-D warnings`、373 tests、`cargo build -p lang-cli` 全部成功；全量测试首轮发现并修正旧诊断目录上界后重跑通过 |
| 文档链接、路线图与 `git diff --check` | 通过 | callable guide 标题、SPEC-0067 状态、Architecture 与下一候选一致；无 whitespace error |

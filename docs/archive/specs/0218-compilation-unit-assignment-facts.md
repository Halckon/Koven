# SPEC-0218：compilation-unit 普通替换赋值类型事实

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P2-218` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.34](../guides/v0.34-pre-restructure/00-index.md)、[表达式语法](../guides/v0.34-pre-restructure/03-grammar-core.md#4-运算符层级与结合性)与[所有权赋值边界](../guides/v0.34-pre-restructure/01-design-decisions.md#26-调用期借用与-asap-析构点v026) |
| 批准依据 | 2026-08-31 持续 Goal 要求继续按 Phase 推进现行 guide 对应 Specs；SPEC-0191 审计确认一般 assignment typed deferred 是 ordinary-class Inout payload lowering 的真实前置 |
| 前置 Spec | SPEC-0019、0020、0197 `done` |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0020 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` compilation-unit body types；Architecture/Roadmap；解锁 SPEC-0191 |
| 语言语义变更 | 否；移除现行普通 `=` assignment 的 deferred 实现边界 |

## 1. Goal

完成后，无诊断的 compilation-unit 普通替换赋值发布 source-qualified target/value/operator/type
descriptor，赋值表达式精确为 `Unit`，供后续 ownership/codegen 直接消费，不再要求新 consumer
从 AST 和相邻类型重新推导赋值契约。

## 2. 背景

SPEC-0197 已遍历一般赋值并在 RHS 后清除 smart-cast fact，但仍把结果保留为
`Deferred(Assignment)`；SPEC-0199 因而只能对 root-name 标量/owner 赋值执行窄重验。
SPEC-0191 的 ordinary-class Inout payload mutation 需要已验证的字段 target、operator 与 RHS
类型，不能在 Phase 4 猜测这些 Phase 2 事实。

## 3. 范围与需求

- 对非 container 的普通 `=` 检查 target 与 RHS；RHS 使用 target place 的声明/storage 类型作为
  expected type，而不是使用可能经 smart cast 收窄的 expression type；
  类型不相容复用 L0084，并保持 expected 来源 label。
- `=` 接受现行 assignable 关系。`+=`、`-=`、`*=`、`/=`、`%=` 的 target 单次求值、旧值读取
  与 RHS 顺序尚未由现行 guide 封闭，本 Spec 保持这五类表达式为 `Deferred(Assignment)`。
- 成功赋值表达式类型固定为 unit-global `Unit`；控制效果合并 target/RHS 的 `falls_through`，
  smart-cast fact 仍只在 RHS 检查完成后 kill。
- 成功时发布源码稳定的 `UnitAssignmentDescriptor`，保存 assignment、target、value、operator 与
  target 的声明/storage 类型及合并后的 `falls_through`；失败或 deferred 节点不发布半成品
  descriptor。RHS 可以是 assignable subtype 或 `Nothing`，因此不重复保存“同一 RHS 类型”。
- descriptor 纳入 overload/lambda trial 的完整快照与 recovery/validated product；公开查询按
  `UnitExpressionId`，输入置换不改变事实或诊断顺序。

## 4. 非目标

- 不改变 container element assignment 已有 descriptor、可变性与诊断。
- 不把 `val`/`var`、field mutability、loan、move、旧值 drop/replacement 提前到 Phase 2；普通
  immutable-place 的稳定诊断码与唯一阶段仍等待 guide 封闭，这些事实继续由 validated ownership
  产物决定。
- 不定义或实现五种复合赋值的类型/求值语义；不得从 Kotlin、Rust 或现有 ownership traversal
  反推语言规则。
- 不 lower SSA/LLVM，不实现 MoveOnly field replacement、property setter、operator overload 或
  新的复合赋值类型。

## 5. 验收标准

- [x] root name、group、裸/显式 `this` field 的普通 `=` 发布稳定 descriptor，表达式类型为 `Unit`。
- [x] RHS 类型不匹配产生 L0084；错误与五种 deferred compound assignment 不发布 descriptor。
- [x] RHS `Nothing` 与 smart-cast kill 的控制/顺序事实保持正确，container assignment 回归不变。
- [x] 输入置换、trial 回滚及 recovery→validated 原子性有测试。
- [x] 受影响 frontend 窄测与 Layer 2 workspace library 门禁通过；不运行约一小时的 frontend 全量测试。
- [x] Architecture、Roadmap、SPEC-0191 前置边界同步。

## 6. 技术方案与边界

在既有 `CompilationUnitTypeParts` 与 `CompilationUnitTypes` 增加按源码顺序保存的 descriptor；
`BodyChecker::check_assignment` 继续复用统一 expected-type 检查和 flow-fact kill，
不新增 assignment 专用类型系统或可变 place checker。trial 已克隆完整 parts，因此新事实随既有
事务状态自然回滚。私有 `expression_falls_through` cache 与 expression type 同步记录控制效果，
避免公开 descriptor 禁止覆盖的 deferred compound 在 memoized 查询时把 `Nothing` RHS 恢复成
可继续路径。

## 7. 实施计划

1. [x] 增加 descriptor/model/query/validated product 接线 → 验证：model 单元与现有 product 身份测试。
2. [x] 完成普通替换赋值类型检查与原子发布 → 验证：multifile type 正反/顺序/置换窄矩阵。
3. [x] 同步 Architecture/Roadmap/SPEC-0191 并运行分层验收。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | assignment typed descriptor、检查、测试与文档 | `feat(frontend): publish assignment type facts (SPEC-0218)` |

## 9. 未决问题

- 无；MoveOnly replacement 与 class payload SSA 继续由 ownership facts 和 SPEC-0191 承接。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-31 前置审计 | 通过 | 一般 assignment 仍为 `Deferred(Assignment)`；ownership 已保持 RHS-first 与字段可变性，codegen 缺失 validated typed contract |
| `cargo test -p lang-frontend --test multifile_type_checking assignment` | 7/7 通过 | storage smart cast、group/field/`this`、`Nothing`、L0084、contextual rollback、trial/input permutation 与 container 回归；未运行 frontend 全量 |
| `cargo test -p lang-frontend --lib product_queries_use_the_unit_type_space_and_preserve_analysis_identity` | 1/1 通过 | descriptor recovery/product/query/analysis identity 接线 |
| `cargo test -p lang-frontend --test multifile_ownership_checking unit_asap_drop_facts_cover_return_temporary_replacement_and_control_edges` | 1/1 通过 | Phase 3 replacement/drop 回归 |
| `cargo test -p lang-codegen --lib unit_lower_assignment_tests` | 3/3 通过 | root replacement、既有 checked compound 实现回归与 undefined compound 边界 |
| `cargo check --workspace --lib` | 通过 | Layer 2 workspace library 编译门禁 |
| `cargo clippy --workspace --lib -- -D warnings` | 通过 | Layer 2 workspace library 静态门禁 |
| `cargo fmt --all -- --check` / `git diff --check` | 通过 | 格式与 whitespace 门禁 |
| 独立高风险复审 | 通过 | 发现 contextual mismatch descriptor 泄漏与 compound+`Nothing` memoized control 两项 P2；均修复后复审至无 P1/P2，白盒 cache test 仅为 P3 |
